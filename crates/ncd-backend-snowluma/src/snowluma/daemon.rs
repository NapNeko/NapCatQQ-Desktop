//! SnowLuma daemon 主体红线:不引入 serde_json::Value,跨边界 enum 派生 ts-rs

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::process::Command;
use tokio::sync::{Mutex, Notify, broadcast};
use tracing::info;

use ncd_domain::domain_event::DomainEvent;
use ncd_traits::events::{BroadcastEventBus, EventBus};

use crate::snowluma::error::{SnowLumaDaemonError, SnowLumaWebUiError};
use crate::snowluma::log_noise::SnowLumaLogNoiseFilter;
use crate::snowluma::session::{load_snowluma_app_config, render_daemon_globals};
use crate::snowluma::webui_client::SnowLumaWebUiClient;
use ncd_host::hide_console_window;

// DaemonState 已下沉到 ncd-domain，此处 re-export 保持向后兼容
pub use ncd_domain::daemon_state::DaemonState;

// SnowLumaWebUiClientFactory

/// SnowLuma WebUI 客户端工厂 trait
/// SnowLumaDaemon::ensure_running 在 starter 路径里:
/// 1. 渲染 webui.json + runtime.json
/// 2. spawn node.exe entry.js
/// 3. 调本 trait 的 create(password) 拿到一个新 Arc<dyn SnowLumaWebUiClient>
/// 4. 在该 client 上调 wait_ready + login
///
/// 把客户端构造抽到 trait 后,SnowLumaDaemon 单测可以注入
/// MockSnowLumaWebUiClientFactory,不依赖真实 reqwest / wiremock
#[async_trait]
pub trait SnowLumaWebUiClientFactory: Send + Sync {
    /// 用 daemon 当前生效的密码与 WebUI 端口构造一个新的 WebUI client
    /// password / port 来自 render_daemon_globals(已读 app-config.json)
    async fn create(
        &self,
        password: String,
        port: u16,
    ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError>;
}

/// SnowLumaDaemon 内部受 tokio::sync::Mutex 保护的可变状态
struct DaemonInner {
    state: DaemonState,
    /// 当前 ensure_running 引用计数;持久 daemon 模型下仅作监控信号
    /// 不再驱动 terminate
    ref_count: u32,
    node_pid: Option<u32>,
    /// tokio::process::Child:starter 路径写入;shutdown / watch_exit
    /// 取出后做 wait / kill
    node_child: Option<tokio::process::Child>,
    webui_client: Option<Arc<dyn SnowLumaWebUiClient>>,
    /// 最近一次启动失败 / crash 的原因;并发 caller 等 ready_notify 唤醒后
    /// 取这个字段决定返回什么 error variant
    last_error: Option<String>,
}

impl DaemonInner {
    fn new() -> Self {
        Self {
            state: DaemonState::Stopped,
            ref_count: 0,
            node_pid: None,
            node_child: None,
            webui_client: None,
            last_error: None,
        }
    }
}

// SnowLumaDaemon

/// SnowLuma 全局 daemon
/// 一个 App 进程内仅一份实例,由 Tauri setup 阶段构造并注入
/// BotManager多 SnowLuma flavor Bot 通过 ensure_running 共享同一份
/// node.exe + WebUI client
/// 并发安全约束:
/// - 内部可变状态全部由 inner: Mutex<DaemonInner> 守护
/// - ready_notify 用于在 starter 完成(成 / 败)后唤醒所有 Starting 期 waiter
/// - dead_flag: AtomicBool 由 starter 在准备启动时清零,由 watcher
///   在子进程退出时置位;wait_ready 的 dead_check 闭包据此 fast-fail
/// - log_tx:daemon 共享 stdout 行级广播通道(容量 10000);任何 SL flavor
///   BotLogPage 通过 subscribe_logs 订阅
pub struct SnowLumaDaemon {
    snowluma_data_root: PathBuf,
    runtime_root: PathBuf,
    event_bus: Arc<BroadcastEventBus>,
    http: Arc<dyn SnowLumaWebUiClientFactory>,
    inner: Mutex<DaemonInner>,
    ready_notify: Notify,
    log_tx: broadcast::Sender<String>,
    dead_flag: Arc<AtomicBool>,
    /// 最近 N 行 stdout/stderr ring buffer,启动失败时被拼进 last_error
    /// 让卡片上能直接看到 node 报的错——broadcast 没订阅者时也不会丢
    recent_log: Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
}

/// stdout 广播容量订阅滞后会被 broadcast 通道 overwrite
const LOG_CHANNEL_CAPACITY: usize = 10_000;

/// 启动期 last_error 拼接的最近日志行数
/// daemon stdout 由所有 SL bot 共享;当 BotLogPage 开页时也是从这里拿初始
/// 历史,所以这个容量等于 SL bot 看到的最大历史窗口设到 1000 行能覆盖一次
/// 启动到登录 + 几次配置 hot reload 的输出,再多对内存压力开始显现,按需调
const RECENT_LOG_CAPACITY: usize = 1000;

/// wait_ready 单轮总超时
const WAIT_READY_TIMEOUT: Duration = Duration::from_secs(30);

/// shutdown 等子进程退出的总超时
const SHUTDOWN_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

impl SnowLumaDaemon {
    /// 构造一个全新的,状态为 Stopped 的 daemon 实例
    /// 不做任何 IO;不 spawn 任何 task真正的 spawn 发生在首个
    /// ensure_running 调用
    pub fn new(
        snowluma_data_root: PathBuf,
        runtime_root: PathBuf,
        event_bus: Arc<BroadcastEventBus>,
        http: Arc<dyn SnowLumaWebUiClientFactory>,
    ) -> Arc<Self> {
        let (log_tx, _initial_rx) = broadcast::channel(LOG_CHANNEL_CAPACITY);
        Arc::new(Self {
            snowluma_data_root,
            runtime_root,
            event_bus,
            http,
            inner: Mutex::new(DaemonInner::new()),
            ready_notify: Notify::new(),
            log_tx,
            dead_flag: Arc::new(AtomicBool::new(false)),
            recent_log: Arc::new(std::sync::Mutex::new(
                std::collections::VecDeque::with_capacity(RECENT_LOG_CAPACITY),
            )),
        })
    }

    /// 取最近的日志行快照(按时间顺序),用于把 node 启动失败前的 stderr 拼进
    /// 用户可见的错误消息也供 BotLogPage 一开页时拉历史用容量上限是
    /// [RECENT_LOG_CAPACITY](1000 行),按时间顺序返回
    pub fn snapshot_recent_log(&self) -> Vec<String> {
        match self.recent_log.lock() {
            Ok(buf) => buf.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    /// 任意 tokio 任务可调;并发 caller 安全
    /// - Ready → ref_count += 1,复用现有 client
    /// - Stopped → 自我提升为 starter,驱动启动序列;成功后切到 Ready 并返回
    ///   client;任何环节失败立即回滚到 Stopped 并返回错误
    /// - Starting → ref_count += 1,等 ready_notify 在 timeout 内唤醒
    ///   唤醒后按最终态决定返回 client / 错误
    /// - Crashed → 直接返回 SnowLumaDaemonError::Crashed(last_error)
    /// - Stopping → 直接返回 SnowLumaDaemonError::Stopping
    pub async fn ensure_running(
        self: &Arc<Self>,
        timeout: Duration,
    ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaDaemonError> {
        info!(target: "ncd_runtime::snowluma_daemon", "正在确保 SnowLuma 守护进程已就绪");
        // === 1. 状态决策 ===
        let role = {
            let mut inner = self.inner.lock().await;
            match inner.state {
                DaemonState::Ready => {
                    inner.ref_count = inner.ref_count.saturating_add(1);
                    let client = inner.webui_client.clone().ok_or_else(|| {
                        SnowLumaDaemonError::Crashed(
                            "daemon Ready 但 webui_client 为空（不变量违反）".into(),
                        )
                    })?;
                    return Ok(client);
                }
                DaemonState::Stopped => {
                    inner.state = DaemonState::Starting;
                    inner.ref_count = 1;
                    inner.last_error = None;
                    self.dead_flag.store(false, Ordering::Release);
                    // 启动新一轮前清空旧的 recent log,避免拼回上一次的 node 退出原因
                    if let Ok(mut buf) = self.recent_log.lock() {
                        buf.clear();
                    }
                    StarterRole::Starter
                }
                DaemonState::Starting => {
                    inner.ref_count = inner.ref_count.saturating_add(1);
                    StarterRole::Waiter
                }
                DaemonState::Crashed => {
                    // 自愈:用户主动点启动按钮 = 明确意图,重置成 Stopped 重新跑
                    // starter;旧的 last_error 已记录过事件,不必抛回
                    inner.state = DaemonState::Starting;
                    inner.ref_count = 1;
                    inner.last_error = None;
                    inner.node_pid = None;
                    inner.node_child = None;
                    inner.webui_client = None;
                    self.dead_flag.store(false, Ordering::Release);
                    if let Ok(mut buf) = self.recent_log.lock() {
                        buf.clear();
                    }
                    StarterRole::Starter
                }
                DaemonState::Stopping => {
                    return Err(SnowLumaDaemonError::Stopping);
                }
            }
        };

        match role {
            StarterRole::Starter => self.run_starter(timeout).await,
            StarterRole::Waiter => self.wait_for_ready(timeout).await,
        }
    }

    pub async fn current_webui_client(&self) -> Option<Arc<dyn SnowLumaWebUiClient>> {
        let inner = self.inner.lock().await;
        if inner.state != DaemonState::Ready {
            return None;
        }
        inner.webui_client.clone()
    }

    /// starter 路径:渲染配置 → spawn node.exe → 构造 client → wait_ready → login
    /// 任意失败立即调 rollback_to_stopped 重置内部状态,清 ref_count 并发事件
    async fn run_starter(
        self: &Arc<Self>,
        _timeout: Duration,
    ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaDaemonError> {
        // 发出 Starting 事件
        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Starting,
                1,
                None,
                Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
            ));

        // === 2. 渲染全局配置(读 app-config.json → runtime.json + webui.json)===
        let app_cfg = load_snowluma_app_config(&self.snowluma_data_root);
        let webui_port = app_cfg.webui_port;
        let pwd_override = {
            let t = app_cfg.webui_password_override.trim();
            if t.is_empty() { None } else { Some(t) }
        };
        let password = match render_daemon_globals(
            &self.snowluma_data_root,
            &self.runtime_root,
            pwd_override,
            webui_port,
        ) {
            Ok(pwd) => pwd,
            Err(err) => return Err(self.rollback_to_stopped(err).await),
        };

        // === 3. spawn node.exe entry.js ===
        // TODO( / SnowLuma 安装布局对齐): 实际入口应来自 PathProbe /
        // runtime_launch_plan;当前 task 阶段无 wiring,按"runtime_root 下的
        // entry.js"硬编码一个占位入口,正式落地由 wiring task 修正
        let node_exe = self.runtime_root.join("node.exe");
        let entry_js = resolve_daemon_entry(&self.runtime_root);
        let mut node_cmd = Command::new(&node_exe);
        node_cmd
            .arg(&entry_js)
            .current_dir(&self.runtime_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_console_window(&mut node_cmd);
        let child_result = node_cmd.spawn();
        let mut child = match child_result {
            Ok(c) => c,
            Err(err) => {
                let mapped = SnowLumaDaemonError::Spawn(format!(
                    "spawn {} {} failed: {err}",
                    node_exe.display(),
                    entry_js.display()
                ));
                return Err(self.rollback_to_stopped(mapped).await);
            }
        };
        let node_pid = child.id();

        // : spawn stdout reader(订阅 log_tx + 发布 SnowLumaDaemonLog)
        // 把 stdout 句柄交给独立 tokio 任务,逐行 ANSI/控制字符清洗后既走
        // log_tx 让前端 BotLogPage 订阅,也走 event_bus 发 SnowLumaDaemonLog
        // EOF 时任务自然结束
        if let Some(stdout) = child.stdout.take() {
            spawn_stdout_reader(
                stdout,
                self.log_tx.clone(),
                Arc::clone(&self.event_bus),
                Arc::clone(&self.recent_log),
            );
        }
        // 同时转发 stderr:node entry 报错(如入口找不到 / 端口冲突)会写到 stderr
        // 不读会让用户的 BotLogPage 完全没线索
        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_reader(
                stderr,
                self.log_tx.clone(),
                Arc::clone(&self.event_bus),
                Arc::clone(&self.recent_log),
            );
        }

        // 把 child 立即移到 inner.node_child,并起一个 early watcher:
        // 让 wait_ready 期间的 node 早退能 fast-fail 而不是傻等 30s
        // 这与 Ready 之后的 watcher 是同一份 watch_exit —— 它会自己根据
        // 当前 state 判断该转 Stopped 还是 Crashed
        {
            let mut inner = self.inner.lock().await;
            inner.node_pid = node_pid;
            inner.node_child = Some(child);
        }
        let weak_for_early = Arc::downgrade(self);
        tokio::spawn(watch_exit(weak_for_early));

        // === 4. 构造 WebUI client + wait_ready + login ===
        let client = match self.http.create(password, webui_port).await {
            Ok(c) => c,
            Err(err) => {
                // node 子进程已经 spawn 出来了;rollback 路径会清 inner.node_child
                // 并 watcher 自己捕到 wait → 切 Stopped,无需在此再 kill
                self.kill_inner_child().await;
                let mapped: SnowLumaDaemonError = err.into();
                return Err(self.rollback_to_stopped(mapped).await);
            }
        };

        // dead_check 闭包:从 dead_flag 读取;watcher 在 child 退出时置位
        let dead_flag = self.dead_flag.clone();
        let dead_check: Box<dyn Fn() -> bool + Send + Sync> =
            Box::new(move || dead_flag.load(Ordering::Acquire));

        if let Err(err) = client.wait_ready(WAIT_READY_TIMEOUT, dead_check).await {
            self.kill_inner_child().await;
            let mapped: SnowLumaDaemonError = err.into();
            return Err(self.rollback_to_stopped(mapped).await);
        }
        // wait_ready 也可能因 dead_flag=true 短路返回 Ok;此时进程其实已死
        // 在调 login 前先确认进程仍在
        if self.dead_flag.load(Ordering::Acquire) {
            let mapped = SnowLumaDaemonError::Crashed("node.exe exited before WebUI ready".into());
            return Err(self.rollback_to_stopped(mapped).await);
        }

        if let Err(err) = client.login().await {
            self.kill_inner_child().await;
            let mapped: SnowLumaDaemonError = err.into();
            return Err(self.rollback_to_stopped(mapped).await);
        }

        // === 5. 切到 Ready,发布事件,唤醒所有 waiter ===
        let ref_snapshot = {
            let mut inner = self.inner.lock().await;
            inner.state = DaemonState::Ready;
            inner.webui_client = Some(client.clone());
            inner.last_error = None;
            inner.ref_count
        };
        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Ready,
                ref_snapshot,
                None,
                Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
            ));
        self.ready_notify.notify_waiters();

        Ok(client)
    }

    /// 工具:从 inner 中取走 child 并 start_kill,让在跑的 watcher 立即收到 wait
    /// 返回 + 进入 Crashed/Stopped 转换当 child 不在 inner 时是 no-op
    async fn kill_inner_child(&self) {
        let child = {
            let mut inner = self.inner.lock().await;
            inner.node_child.take()
        };
        if let Some(mut child) = child {
            let _ = child.start_kill();
            // 不在此处 await wait——交给已经 spawn 的 watcher 处理(它持有
            // 自己的 child 句柄路径:watch_exit 会从 inner.node_child.take()
            // 但本函数已经 take 走了,watcher 那边 take 拿到 None 直接 return
            // 既然如此,本函数也得自己 wait,避免 zombie
            let _ = tokio::time::timeout(SHUTDOWN_WAIT_TIMEOUT, child.wait()).await;
            // 显式置 dead_flag,让任何还在跑的 wait_ready 立即结束
            self.dead_flag.store(true, Ordering::Release);
        }
    }

    /// waiter 路径:等 starter 通过 ready_notify 通知最终态
    /// 实现策略:循环 tokio::time::timeout(remaining, ready_notify.notified())
    /// + 每轮重新读 inner 状态命中 Ready/Stopped/Crashed/Stopping 立即按最终态
    ///   返回;超时则返回 StartTimeoutstate 仍为 Starting(spurious wake)
    ///   则继续下一轮
    async fn wait_for_ready(
        &self,
        timeout: Duration,
    ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaDaemonError> {
        let started_at = Instant::now();
        loop {
            // 先 sync 检查一次状态:starter 可能在我们 await ready_notify 之前
            // 就完成了(在我们 ref_count++ 之后,注册 notified 之前);这种情况下
            // 等 ready_notify 会一直等到超时提前快照可以短路
            {
                let inner = self.inner.lock().await;
                match inner.state {
                    DaemonState::Ready => {
                        return inner.webui_client.clone().ok_or_else(|| {
                            SnowLumaDaemonError::Crashed(
                                "daemon Ready 但 webui_client 为空（不变量违反）".into(),
                            )
                        });
                    }
                    DaemonState::Stopped => {
                        let reason = inner
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "snowluma daemon stopped before ready".into());
                        return Err(SnowLumaDaemonError::Crashed(reason));
                    }
                    DaemonState::Crashed => {
                        let reason = inner
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "snowluma daemon crashed".into());
                        return Err(SnowLumaDaemonError::Crashed(reason));
                    }
                    DaemonState::Stopping => {
                        return Err(SnowLumaDaemonError::Stopping);
                    }
                    DaemonState::Starting => {}
                }
            }

            let elapsed = started_at.elapsed();
            if elapsed >= timeout {
                return Err(SnowLumaDaemonError::StartTimeout(timeout));
            }
            let remaining = timeout - elapsed;
            // 即使等不到 notify(race / spurious),timeout 兜底;下轮重新读状态
            let _ = tokio::time::timeout(remaining, self.ready_notify.notified()).await;
        }
    }

    /// starter 失败回滚:把内部状态压回 Stopped + 清 ref_count + 发事件 + 唤醒
    /// waiter,最后把传入的错误透传给调用方
    async fn rollback_to_stopped(&self, err: SnowLumaDaemonError) -> SnowLumaDaemonError {
        // 给 stdout/stderr reader 一点时间 flush 缓冲(node 退出后 pipe EOF 还需要
        // 几个 io::poll 才能让 reader 把所有行喂进 recent_log)
        tokio::time::sleep(Duration::from_millis(150)).await;
        // 把最近 50 行 stdout/stderr(短时间窗内 node 退出前的输出)拼到 reason 末尾
        // 让前端卡片能直接看到 node 报的错
        let recent = self.snapshot_recent_log();
        let reason = if recent.is_empty() {
            err.to_string()
        } else {
            // 取最近 ≤10 行避免 reason 过长(卡片 UI 显示有限)
            let tail: Vec<&String> = recent
                .iter()
                .rev()
                .take(10)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let log_text = tail
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" / ");
            format!("{} | recent log: {}", err, log_text)
        };
        {
            let mut inner = self.inner.lock().await;
            inner.state = DaemonState::Stopped;
            inner.ref_count = 0;
            inner.node_pid = None;
            inner.node_child = None;
            inner.webui_client = None;
            inner.last_error = Some(reason.clone());
        }
        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Stopped,
                0,
                Some(reason),
                Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
            ));
        self.ready_notify.notify_waiters();
        err
    }

    /// ref_count -= 1;持久 daemon 模型下不触发 terminate
    pub async fn release(&self) {
        let mut inner = self.inner.lock().await;
        if inner.ref_count > 0 {
            inner.ref_count -= 1;
        }
    }

    /// 显式优雅关闭
    /// 仅 Ready / Starting 状态下生效;其它状态早 return(幂等)
    /// 1. 切 Stopping + 取出 client / child
    /// 2. client.logout() fire-and-forget(忽略错误)
    /// 3. child.start_kill() + tokio::time::timeout(5s, child.wait())
    ///    超时则 child.kill().await
    /// 4. 切 Stopped + 清 ref_count + 清字段 + 发事件 + 唤醒 waiter
    pub async fn shutdown(&self) {
        let (client, child_opt) = {
            let mut inner = self.inner.lock().await;
            if !matches!(inner.state, DaemonState::Ready | DaemonState::Starting) {
                return;
            }
            inner.state = DaemonState::Stopping;
            (inner.webui_client.take(), inner.node_child.take())
        };

        // logout fire-and-forget
        if let Some(c) = client {
            let _ = c.logout().await;
        }

        if let Some(mut child) = child_opt {
            let _ = child.start_kill();
            // 5s 内等子进程退出;超时 → 强 kill 兜底
            match tokio::time::timeout(SHUTDOWN_WAIT_TIMEOUT, child.wait()).await {
                Ok(_) => {}
                Err(_) => {
                    let _ = child.kill().await;
                }
            }
        }

        {
            let mut inner = self.inner.lock().await;
            inner.state = DaemonState::Stopped;
            inner.ref_count = 0;
            inner.node_pid = None;
            inner.node_child = None;
            inner.webui_client = None;
            inner.last_error = None;
        }
        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Stopped,
                0,
                None,
                Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
            ));
        self.ready_notify.notify_waiters();
    }

    /// 当前 daemon 状态快照
    pub async fn state(&self) -> DaemonState {
        self.inner.lock().await.state
    }

    /// 当前 ref_count 快照
    pub async fn ref_count(&self) -> u32 {
        self.inner.lock().await.ref_count
    }

    /// 订阅 daemon 共享的 node.exe stdout 行流
    /// spawn 的 stdout reader 把行清洗后 log_tx.send(..),订阅者
    /// 通过本方法拿 Receiver 收行
    pub fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.log_tx.subscribe()
    }

    /// 仅在 Ready 状态下返回当前生效的 WebUI client clone;其它状态返回相应
    /// 错误(runtime_backend stop 路径 / 健康检查会用到)
    pub async fn current_client(
        &self,
    ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaDaemonError> {
        let inner = self.inner.lock().await;
        match inner.state {
            DaemonState::Ready => inner.webui_client.clone().ok_or_else(|| {
                SnowLumaDaemonError::Crashed(
                    "daemon Ready 但 webui_client 为空（不变量违反）".into(),
                )
            }),
            DaemonState::Stopping => Err(SnowLumaDaemonError::Stopping),
            DaemonState::Crashed => {
                let reason = inner
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "snowluma daemon crashed".into());
                Err(SnowLumaDaemonError::Crashed(reason))
            }
            DaemonState::Stopped | DaemonState::Starting => Err(SnowLumaDaemonError::Crashed(
                format!("daemon not ready (state={:?})", inner.state),
            )),
        }
    }
}

/// ensure_running 状态决策内部用的小 enum
enum StarterRole {
    Starter,
    Waiter,
}

/// spawn 一个长生命周期 tokio 任务,逐行读取 SnowLuma node.exe 的 stdout
/// 按 做 ANSI / 控制字符清洗后:
/// 1. 通过 log_tx(broadcast 容量 10000)广播给所有 subscribe_logs 订阅者
///    SL flavor BotLogPage 会消费这个 channel
/// 2. 通过 bus.publish(DomainEvent::snowluma_daemon_log(line)) 发到全局事件总线
///    供前端通用日志面板 / 调试通道订阅(events.rs 中已落地的
///    SnowLumaDaemonLog { line } variant)
///
/// 空行(清洗后为 "")直接丢弃,避免污染日志面板EOF(子进程关闭 stdout)
/// 时 next_line 返回 Ok(None),循环自然退出,任务结束;watch_exit
/// 会负责子进程退出后的状态切换,本 reader 不发 Crashed 事件
fn spawn_stdout_reader(
    stdout: tokio::process::ChildStdout,
    log_tx: broadcast::Sender<String>,
    bus: Arc<BroadcastEventBus>,
    recent: Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
) {
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut filter = SnowLumaLogNoiseFilter::new();
        while let Ok(Some(raw)) = lines.next_line().await {
            let Some(cleaned) = filter.process_line(&raw) else {
                continue;
            };
            push_recent(&recent, &cleaned);
            let _ = log_tx.send(cleaned.clone());
            bus.publish(DomainEvent::snowluma_daemon_log(cleaned));
        }
    });
}

/// 与 spawn_stdout_reader 同语义,专门转发 stderr前缀加 [stderr] 便于排查
fn spawn_stderr_reader(
    stderr: tokio::process::ChildStderr,
    log_tx: broadcast::Sender<String>,
    bus: Arc<BroadcastEventBus>,
    recent: Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
) {
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let mut filter = SnowLumaLogNoiseFilter::new();
        while let Ok(Some(raw)) = lines.next_line().await {
            let Some(cleaned) = filter.process_line(&raw) else {
                continue;
            };
            let line = format!("[stderr] {cleaned}");
            push_recent(&recent, &line);
            let _ = log_tx.send(line.clone());
            bus.publish(DomainEvent::snowluma_daemon_log(line));
        }
    });
}

fn push_recent(recent: &Arc<std::sync::Mutex<std::collections::VecDeque<String>>>, line: &str) {
    if let Ok(mut buf) = recent.lock() {
        if buf.len() >= RECENT_LOG_CAPACITY {
            buf.pop_front();
        }
        buf.push_back(line.to_string());
    }
}

/// 从 runtime_root 推断 daemon 入口脚本路径
/// SnowLuma 的真实入口在不同打包版本下命名不一(entry.js / index.js /
/// dist/index.js / napcat.mjs 等)这里按一组候选名按顺序探测
/// 命中第一个真实文件就用;都不存在则保守地回落到 entry.js 让 spawn 报错时
/// 携带能识别的路径文本(用户能立即知道是哪条)
fn resolve_daemon_entry(runtime_root: &std::path::Path) -> std::path::PathBuf {
    const CANDIDATES: &[&str] = &[
        // SnowLuma 实际打包入口(rolldown 输出):rolldown bundle 的 ESM 入口
        "index.mjs",
        // 历史 / 兼容 fallback
        "entry.js",
        "index.js",
        "main.js",
        "dist/index.js",
        "dist/main.js",
        "snowluma.mjs",
    ];
    for name in CANDIDATES {
        let candidate = runtime_root.join(name);
        if candidate.is_file() {
            return candidate;
        }
    }
    runtime_root.join("index.mjs")
}

/// 监听 node.exe 子进程退出,按 把 daemon 状态机推进到
/// Stopped(intentional shutdown 路径)或 Crashed(意外退出路径)
///
/// 为什么用 Weak<SnowLumaDaemon>:daemon 本身持 Arc<Self> 并把 child 句柄
/// 通过 inner.node_child 持有;如果 watcher 也持 Arc<SnowLumaDaemon>,会与
/// daemon 内部任何持 watcher 句柄的对象(未来扩展)形成循环引用当下虽未触发
/// leak,但用 Weak 是显式约定,便于将来 JoinHandle 落到 inner 时不踩坑
///
/// 流程:
/// 1. daemon.upgrade() 失败 → daemon 已被 drop,直接 return
/// 2. 锁 inner 取出 node_child(take);若为 None —— 通常是 starter 失败
///    回滚 / 手动 shutdown 已经把 child 取走 —— 直接 return
/// 3. child.wait().await 阻塞等子进程退出,捕获 ExitStatus
/// 4. dead_flag.store(true):任何还在等 wait_ready 的轮询会 fast-fail
/// 5. 重新锁 inner:根据当前 state 判断 was_intentional:
///    - Stopping / Stopped → 视为 intentional,目标态 Stopped,不写
///      last_error(不污染下次 ensure_running 的错误信号)
///    - 其它(Ready / Starting / Crashed)→ 视为意外退出,目标态 Crashed
///      last_error = Some(format!("node.exe exited: {exit:?}"))
/// 6. 清空 node_pid(child 已不在);快照 state / ref_count / last_error
/// 7. drop lock 后再发 SnowLumaDaemonStateChanged{state, ref_count, reason}
///    避免在事件订阅者处理回调时持有内部 mutex
/// 8. ready_notify.notify_waiters():解开任何因 starter 死掉但 ready_notify
///    没被 starter 喊到的 waiter(fail-safe)
async fn watch_exit(daemon: Weak<SnowLumaDaemon>) {
    let Some(daemon) = daemon.upgrade() else {
        return;
    };

    // 取出 child;starter / shutdown / rollback 都可能先一步把 child 拿走
    let child = {
        let mut inner = daemon.inner.lock().await;
        inner.node_child.take()
    };
    let Some(mut child) = child else {
        return;
    };

    // 阻塞等子进程退出child.wait() 不需要 mutable lock,节省锁占用
    let exit = child.wait().await;

    daemon.dead_flag.store(true, Ordering::Release);

    // 给 stdout/stderr reader 一点时间把缓冲行 flush 进 ring buffer
    // 然后把最近 ≤10 行拼进 last_error,让前端卡片能直接看到 node 死前抱怨
    tokio::time::sleep(Duration::from_millis(150)).await;
    let recent = daemon.snapshot_recent_log();

    let (state_snapshot, ref_snapshot, reason_snapshot) = {
        let mut inner = daemon.inner.lock().await;
        let was_intentional = matches!(inner.state, DaemonState::Stopping | DaemonState::Stopped);
        if was_intentional {
            inner.state = DaemonState::Stopped;
        } else {
            inner.state = DaemonState::Crashed;
            let base = format!("node.exe exited: {exit:?}");
            let full = if recent.is_empty() {
                base
            } else {
                let tail: Vec<&String> = recent
                    .iter()
                    .rev()
                    .take(10)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let log_text = tail
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ");
                format!("{} | recent log: {}", base, log_text)
            };
            inner.last_error = Some(full);
        }
        inner.node_pid = None;
        (inner.state, inner.ref_count, inner.last_error.clone())
    };

    daemon
        .event_bus
        .publish(DomainEvent::snowluma_daemon_state_changed(
            state_snapshot,
            ref_snapshot,
            reason_snapshot,
            Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
        ));
    daemon.ready_notify.notify_waiters();
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // DaemonState 字面量稳定性(继承自 ,签名不变所以保留)
    // -----------------------------------------------------------------------

    #[test]
    fn daemon_state_serializes_as_snake_case() {
        // 防字面量漂移: / 9.1 的前端事件 payload state 字段
        // 严格依赖这五个 snake_case 字面量;这里 lock 一遍
        assert_eq!(
            serde_json::to_string(&DaemonState::Stopped).unwrap(),
            "\"stopped\""
        );
        assert_eq!(
            serde_json::to_string(&DaemonState::Starting).unwrap(),
            "\"starting\""
        );
        assert_eq!(
            serde_json::to_string(&DaemonState::Ready).unwrap(),
            "\"ready\""
        );
        assert_eq!(
            serde_json::to_string(&DaemonState::Stopping).unwrap(),
            "\"stopping\""
        );
        assert_eq!(
            serde_json::to_string(&DaemonState::Crashed).unwrap(),
            "\"crashed\""
        );
    }

    #[test]
    fn daemon_state_round_trips_through_serde() {
        for state in [
            DaemonState::Stopped,
            DaemonState::Starting,
            DaemonState::Ready,
            DaemonState::Stopping,
            DaemonState::Crashed,
        ] {
            let encoded = serde_json::to_string(&state).expect("serialize");
            let decoded: DaemonState = serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn daemon_state_derives_copy_and_eq() {
        // Copy / Eq 派生健康度: 要求 Copy(被 DomainEvent 内联)
        // 这里通过类型推断断言
        let s = DaemonState::Ready;
        let copied = s; // 触发 Copy
        assert_eq!(s, copied);
    }

    // -----------------------------------------------------------------------
    // SnowLumaDaemon smoke 测试( deliverable)
    //
    // 完整 starter / waiter / rollback / crash / shutdown 行为覆盖由
    // 接手;本 task 仅验证:
    // 1) new 不做任何 IO 即可构造,初始 state == Stopped
    // 2) 初始 ref_count == 0
    // -----------------------------------------------------------------------

    /// 最小 stub factory:smoke 测试用,永远返回错误,不会被实际调用
    struct StubFactory;

    #[async_trait]
    impl SnowLumaWebUiClientFactory for StubFactory {
        async fn create(
            &self,
            _password: String,
            _port: u16,
        ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::Http {
                endpoint: "<stub>".into(),
                cause: "stub factory must not be invoked in smoke tests".into(),
            })
        }
    }

    fn build_smoke_daemon() -> Arc<SnowLumaDaemon> {
        let snowluma_data_root = PathBuf::from("/tmp/snowluma-smoke");
        let runtime_root = PathBuf::from("/tmp/snowluma-runtime-smoke");
        let event_bus = Arc::new(BroadcastEventBus::default());
        let factory: Arc<dyn SnowLumaWebUiClientFactory> = Arc::new(StubFactory);
        SnowLumaDaemon::new(snowluma_data_root, runtime_root, event_bus, factory)
    }

    #[tokio::test]
    async fn daemon_initial_state_is_stopped() {
        let daemon = build_smoke_daemon();
        assert_eq!(daemon.state().await, DaemonState::Stopped);
    }

    #[tokio::test]
    async fn daemon_ref_count_is_zero_initially() {
        let daemon = build_smoke_daemon();
        assert_eq!(daemon.ref_count().await, 0);
    }

    /// 编译期断言:SnowLumaWebUiClientFactory 直接以 Arc<dyn SnowLumaWebUiClient>
    /// 暴露 client( 占位的关联类型已经在 移除)这里通过让
    /// 一个 stub factory 真正参与 Arc::new 装箱来锁定签名形态
    #[tokio::test]
    async fn factory_trait_uses_dyn_client_directly() {
        let factory: Arc<dyn SnowLumaWebUiClientFactory> = Arc::new(StubFactory);
        // 调一次 create 拿到 Result,确认返回类型恰好是 Arc<dyn SnowLumaWebUiClient>
        let result: Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError> =
            factory.create("x".into(), 5099).await;
        assert!(result.is_err(), "stub factory always errors");
    }

    /// subscribe_logs 应当直接返回一个 broadcast Receiver;不需要 daemon 已启动
    #[tokio::test]
    async fn daemon_subscribe_logs_does_not_require_start() {
        let daemon = build_smoke_daemon();
        let _rx: broadcast::Receiver<String> = daemon.subscribe_logs();
    }

    // -----------------------------------------------------------------------
    // spawn_stdout_reader smoke 测试( deliverable)
    //
    // 通过真起一个会立刻 echo 一行的小子进程,把 ChildStdout 喂给
    // spawn_stdout_reader,断言:
    // 1) log_tx 订阅者收到清洗后的行
    // 2) event_bus 上能收到 SnowLumaDaemonLog variant
    // 3) 子进程退出后 reader 任务自然结束(无 panic,无 hang)
    //
    // SnowLuma daemon 本身仅 Windows 落地,但 spawn_stdout_reader 自身是平台无关的
    // 所以本 smoke 测试在两平台都跑:Windows 用 cmd /C echo,其它走 echo
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn spawn_stdout_reader_forwards_cleaned_line() {
        use ncd_domain::domain_event::DomainEventKind;
        use ncd_traits::events::EventFilter;
        use std::process::Stdio;

        let event_bus = Arc::new(BroadcastEventBus::default());
        let mut bus_sub =
            event_bus.subscribe(EventFilter::kind(DomainEventKind::SnowLumaDaemonLog));
        let (log_tx, mut log_rx) = broadcast::channel::<String>(16);

        // 用一个一定能立即 echo 一行并退出的小子进程:
        // - Windows: cmd /C echo hello-snowluma
        // - 其它: /bin/sh -c "echo hello-snowluma"
        // echo 行尾换行让 lines.next_line() 立刻拿到一整行
        #[cfg(windows)]
        let mut child = tokio::process::Command::new("cmd")
            .args(["/C", "echo hello-snowluma"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn cmd /C echo");
        #[cfg(not(windows))]
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "echo hello-snowluma"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sh -c echo");

        let stdout = child.stdout.take().expect("child stdout should be piped");
        let recent: Arc<std::sync::Mutex<std::collections::VecDeque<String>>> =
            Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        spawn_stdout_reader(stdout, log_tx.clone(), Arc::clone(&event_bus), recent);

        // 1) log_tx 订阅者拿到清洗后的行
        let received = tokio::time::timeout(Duration::from_secs(5), log_rx.recv())
            .await
            .expect("log_tx send should arrive within 5s")
            .expect("log channel still open");
        assert_eq!(received, "hello-snowluma");

        // 2) event_bus 上的 SnowLumaDaemonLog variant 也命中
        let bus_event = tokio::time::timeout(Duration::from_secs(5), bus_sub.next())
            .await
            .expect("event bus should publish within 5s")
            .expect("event subscription still open");
        match bus_event {
            DomainEvent::SnowLumaDaemonLog { line } => assert_eq!(line, "hello-snowluma"),
            other => panic!("expected SnowLumaDaemonLog, got {other:?}"),
        }

        // 3) 子进程退出收尾,reader 任务靠 EOF 自然结束
        let _ = child.wait().await;
    }

    // -----------------------------------------------------------------------
    // deliverable
    //
    // MockSnowLumaWebUiClient + MockFactory:复用 / 3.2 的 trait 边界
    // 注入测试 stub,不走真实 reqwest / wiremock
    //
    // 测试覆盖:
    // - daemon_spawn_failure_rolls_back_to_stopped:empty runtime_root → spawn
    // 失败 → state 回到 Stopped + ref_count == 0 + 收到 Starting/Stopped
    // 两次状态事件
    // - daemon_concurrent_callers_share_starting_then_all_fail:3 个并发
    // ensure_running,starter spawn 失败时其它 waiter 也拿到 Crashed-ish 错误
    // - daemon_release_is_safe_when_ref_count_is_zero:release 多次不 panic,
    // ref_count 始终 == 0
    // - daemon_shutdown_from_stopped_is_noop:build 一个新 daemon 调 shutdown
    // 不 panic,状态保持 Stopped
    // - 属性测试 daemon_release_never_underflows:随机 0..50 次 release
    // ref_count 始终 == 0,不 panic
    //
    // Follow-up:watch_exit 真实 child 监听路径无法在 Mock 层构造(child 是
    // tokio::process::Child 句柄),留给真起 node.exe 的端到端集成测试覆盖
    // watch_exit_emits_crashed_when_child_unexpectedly_exits 行为
    // -----------------------------------------------------------------------

    use ncd_domain::domain_event::DomainEventKind;
    use ncd_traits::events::EventFilter;
    use proptest::prelude::*;
    use tempfile::tempdir;
    use tokio::sync::Mutex as TokioMutex;

    /// MockSnowLumaWebUiClient:用 Arc<TokioMutex<MockBehavior>> 控制每个
    /// trait 方法的返回值其它 daemon 测试不需要的方法返回简单 Ok / 空
    /// 注:当前 的几个落地测试(spawn 失败 / 并发 / release / shutdown)
    /// 都跑不到 wait_ready / login,因为 spawn 阶段就已失败;mock 仍保留以
    /// 便后续 task 在 Ready 路径覆盖中复用,并满足 trait 完备性编译要求
    #[derive(Default)]
    struct MockBehavior {
        wait_ready_result: Option<Result<(), SnowLumaWebUiError>>,
        login_result: Option<Result<(), SnowLumaWebUiError>>,
        logout_calls: u32,
    }

    struct MockSnowLumaWebUiClient {
        behavior: Arc<TokioMutex<MockBehavior>>,
    }

    #[async_trait]
    impl SnowLumaWebUiClient for MockSnowLumaWebUiClient {
        async fn wait_ready(
            &self,
            _timeout: Duration,
            _dead_check: Box<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<(), SnowLumaWebUiError> {
            let behavior = self.behavior.lock().await;
            match &behavior.wait_ready_result {
                Some(Ok(())) | None => Ok(()),
                Some(Err(e)) => Err(clone_webui_error(e)),
            }
        }

        async fn login(&self) -> Result<(), SnowLumaWebUiError> {
            let behavior = self.behavior.lock().await;
            match &behavior.login_result {
                Some(Ok(())) | None => Ok(()),
                Some(Err(e)) => Err(clone_webui_error(e)),
            }
        }

        async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
            let mut behavior = self.behavior.lock().await;
            behavior.logout_calls = behavior.logout_calls.saturating_add(1);
            Ok(())
        }

        async fn list_processes(
            &self,
        ) -> Result<Vec<crate::snowluma::webui_client::HookProcessInfo>, SnowLumaWebUiError>
        {
            Ok(Vec::new())
        }

        async fn list_qq_instances(
            &self,
        ) -> Result<Vec<crate::snowluma::webui_client::OneBotInstanceInfo>, SnowLumaWebUiError>
        {
            Ok(Vec::new())
        }

        async fn probe_process_login_info(
            &self,
            _pid: u32,
        ) -> Result<Option<crate::snowluma::webui_client::QqPortLoginInfo>, SnowLumaWebUiError>
        {
            Ok(None)
        }

        async fn load_process(
            &self,
            _pid: u32,
        ) -> Result<crate::snowluma::webui_client::HookProcessInfo, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::ServerRejected {
                endpoint: "<mock>/load".into(),
                message: "mock not configured".into(),
            })
        }

        async fn unload_process(
            &self,
            _pid: u32,
        ) -> Result<crate::snowluma::webui_client::HookProcessInfo, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::ServerRejected {
                endpoint: "<mock>/unload".into(),
                message: "mock not configured".into(),
            })
        }

        async fn get_auth_state(
            &self,
        ) -> Result<crate::snowluma::webui_client::AuthState, SnowLumaWebUiError> {
            Ok(crate::snowluma::webui_client::AuthState::default())
        }

        async fn get_agreements(
            &self,
        ) -> Result<crate::snowluma::webui_client::AgreementsPayload, SnowLumaWebUiError> {
            Ok(crate::snowluma::webui_client::AgreementsPayload {
                version: "mock".into(),
                consent_required: false,
                documents: Vec::new(),
            })
        }

        async fn record_agreement_consent(&self, _version: &str) -> Result<(), SnowLumaWebUiError> {
            Ok(())
        }

        async fn update_onebot_config(
            &self,
            _uin: &str,
            _config: &serde_json::Value,
        ) -> Result<bool, SnowLumaWebUiError> {
            Ok(true)
        }
    }

    /// SnowLumaWebUiError 不实现 Clone(含 BTreeMap 字段);测试里需要
    /// 在多次调用之间复用同一个错误模板,这里手写一份字面 clone
    fn clone_webui_error(e: &SnowLumaWebUiError) -> SnowLumaWebUiError {
        match e {
            SnowLumaWebUiError::Status {
                endpoint,
                status,
                message,
            } => SnowLumaWebUiError::Status {
                endpoint: endpoint.clone(),
                status: *status,
                message: message.clone(),
            },
            SnowLumaWebUiError::Timeout { endpoint } => SnowLumaWebUiError::Timeout {
                endpoint: endpoint.clone(),
            },
            SnowLumaWebUiError::Http { endpoint, cause } => SnowLumaWebUiError::Http {
                endpoint: endpoint.clone(),
                cause: cause.clone(),
            },
            SnowLumaWebUiError::Decode { endpoint, message } => SnowLumaWebUiError::Decode {
                endpoint: endpoint.clone(),
                message: message.clone(),
            },
            SnowLumaWebUiError::NotReady(d, errs) => SnowLumaWebUiError::NotReady(*d, errs.clone()),
            SnowLumaWebUiError::LoginFailed(msg) => SnowLumaWebUiError::LoginFailed(msg.clone()),
            SnowLumaWebUiError::ServerRejected { endpoint, message } => {
                SnowLumaWebUiError::ServerRejected {
                    endpoint: endpoint.clone(),
                    message: message.clone(),
                }
            }
        }
    }

    /// 返回 MockSnowLumaWebUiClient 的工厂;shared 行为容器让测试用例可在
    /// daemon 构造之后再调整 mock 返回值
    struct MockFactory {
        behavior: Arc<TokioMutex<MockBehavior>>,
    }

    #[async_trait]
    impl SnowLumaWebUiClientFactory for MockFactory {
        async fn create(
            &self,
            _password: String,
            _port: u16,
        ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError> {
            Ok(Arc::new(MockSnowLumaWebUiClient {
                behavior: Arc::clone(&self.behavior),
            }))
        }
    }

    /// 构造 daemon + tempdir 持有句柄runtime_root 为空目录,spawn 时找不到
    /// node.exe 必然失败 —— 这是测试 spawn 失败回滚路径的关键
    /// 同时 snowluma_data_root 也用 tempdir,保证 render_daemon_globals
    /// 写 session.json / runtime.json / webui.json 不污染真实数据根
    /// 返回 (daemon, runtime_dir, snowluma_dir, behavior);测试函数应把 dir
    /// 句柄绑到 _runtime_dir / _snowluma_dir 保活到测试结束
    fn build_test_daemon() -> (
        Arc<SnowLumaDaemon>,
        tempfile::TempDir,
        tempfile::TempDir,
        Arc<TokioMutex<MockBehavior>>,
    ) {
        let runtime_dir = tempdir().expect("tempdir runtime");
        let snowluma_dir = tempdir().expect("tempdir snowluma_data");
        let event_bus = Arc::new(BroadcastEventBus::default());
        let behavior = Arc::new(TokioMutex::new(MockBehavior::default()));
        let factory: Arc<dyn SnowLumaWebUiClientFactory> = Arc::new(MockFactory {
            behavior: Arc::clone(&behavior),
        });
        let daemon = SnowLumaDaemon::new(
            snowluma_dir.path().to_path_buf(),
            runtime_dir.path().to_path_buf(),
            event_bus,
            factory,
        );
        (daemon, runtime_dir, snowluma_dir, behavior)
    }

    /// runtime_root 不含 node.exe → Command::spawn 失败 → starter 路径在
    /// step 3 早 fail → rollback_to_stopped → state == Stopped,ref_count == 0,
    /// last_error 非空事件序列:先 Starting,再 Stopped(reason=Some)
    #[tokio::test]
    async fn daemon_spawn_failure_rolls_back_to_stopped() {
        let (daemon, _runtime_dir, _snowluma_dir, _behavior) = build_test_daemon();
        let mut sub = daemon.event_bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaDaemonStateChanged,
        ));

        let result = daemon.ensure_running(Duration::from_secs(5)).await;
        assert!(
            result.is_err(),
            "spawn must fail without node.exe in runtime_root"
        );

        assert_eq!(daemon.state().await, DaemonState::Stopped);
        assert_eq!(daemon.ref_count().await, 0);

        // 收到 Starting 事件
        let first = tokio::time::timeout(Duration::from_secs(2), sub.next())
            .await
            .expect("starting event within 2s")
            .expect("subscription open");
        match first {
            DomainEvent::SnowLumaDaemonStateChanged {
                state, ref_count, ..
            } => {
                assert_eq!(state, DaemonState::Starting);
                assert_eq!(ref_count, 1);
            }
            other => panic!("expected Starting state event, got {other:?}"),
        }

        // 紧接着 Stopped 事件,reason 携带回滚原因
        let second = tokio::time::timeout(Duration::from_secs(2), sub.next())
            .await
            .expect("stopped event within 2s")
            .expect("subscription open");
        match second {
            DomainEvent::SnowLumaDaemonStateChanged {
                state,
                ref_count,
                reason,
                ..
            } => {
                assert_eq!(state, DaemonState::Stopped);
                assert_eq!(ref_count, 0);
                assert!(reason.is_some(), "rollback should carry reason");
            }
            other => panic!("expected Stopped state event, got {other:?}"),
        }
    }

    /// 3 个并发 caller 同时调 ensure_running:starter 路径 spawn 失败 →
    /// rollback_to_stopped → ref_count 归零 + ready_notify 唤醒 waiter三个
    /// caller 全部得到 Err;最终 state == Stopped,ref_count == 0
    /// 这是 并发 caller 行为的反向覆盖(正向 Ready 路径需要真实
    /// node.exe,留给端到端集成测试)
    #[tokio::test]
    async fn daemon_concurrent_callers_share_starting_then_all_fail() {
        let (daemon, _runtime_dir, _snowluma_dir, _behavior) = build_test_daemon();

        let d1 = Arc::clone(&daemon);
        let d2 = Arc::clone(&daemon);
        let d3 = Arc::clone(&daemon);
        let h1 = tokio::spawn(async move { d1.ensure_running(Duration::from_secs(5)).await });
        let h2 = tokio::spawn(async move { d2.ensure_running(Duration::from_secs(5)).await });
        let h3 = tokio::spawn(async move { d3.ensure_running(Duration::from_secs(5)).await });

        let r1 = h1.await.expect("join h1");
        let r2 = h2.await.expect("join h2");
        let r3 = h3.await.expect("join h3");

        assert!(r1.is_err(), "caller 1 should err");
        assert!(r2.is_err(), "caller 2 should err");
        assert!(r3.is_err(), "caller 3 should err");

        // 全部失败后应回到稳态 Stopped,ref_count 归零(rollback 强制清零)
        assert_eq!(daemon.state().await, DaemonState::Stopped);
        assert_eq!(daemon.ref_count().await, 0);
    }

    /// release 在 ref_count == 0 时是安全的:多次调用不 panic,不 underflow
    /// ref_count 仍然 == 0(saturating_sub 语义)
    #[tokio::test]
    async fn daemon_release_is_safe_when_ref_count_is_zero() {
        let daemon = build_smoke_daemon();
        for _ in 0..10 {
            daemon.release().await;
            assert_eq!(daemon.ref_count().await, 0);
        }
    }

    /// 新构造的 daemon state == Stopped;shutdown 在 Stopped 状态早 return
    /// 不 panic,不发任何状态事件,状态保持 Stopped
    #[tokio::test]
    async fn daemon_shutdown_from_stopped_is_noop() {
        let daemon = build_smoke_daemon();
        daemon.shutdown().await;
        assert_eq!(daemon.state().await, DaemonState::Stopped);
        assert_eq!(daemon.ref_count().await, 0);
    }

    // -------------------------------------------------------------------
    // 属性测试:随机生成 0..50 次 release 操作序列,断言 ref_count 始终 == 0
    // 且不 panic(saturating_sub 单调上界)Validates:
    // 的 release 安全性(即便没有 ensure_running 配对调用,也不能 underflow)
    // -------------------------------------------------------------------

    proptest! {
    #![proptest_config(ProptestConfig {
    cases: 16,
    ..ProptestConfig::default()
    })]

    #[test]
    fn daemon_release_never_underflows(call_count in 0u32..50) {
    // proptest case 内部跑 tokio runtime;用 current_thread 即可
    let rt = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .expect("build rt");
    rt.block_on(async {
    let daemon = build_smoke_daemon();
    for _ in 0..call_count {
    daemon.release().await;
    }
    let final_ref = daemon.ref_count().await;
    prop_assert_eq!(final_ref, 0);
    Ok(())
    }).expect("proptest async block");
    }
    }
}
