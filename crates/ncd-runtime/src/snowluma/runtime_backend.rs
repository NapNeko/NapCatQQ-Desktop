//! SnowLuma runtime backend:把 SnowLuma daemon + per-Bot QQ.exe 注入语义
//! 包成 BotBackend 接口,由 BotManager 按 flavor 路由
//!
//! 落地内容:SnowLumaRuntimeBackend struct,SnowLumaProcessRecord,
//! Phase A(COLD spawn QQ.exe / HOT 按 qq_id 自动匹配 PID)
//! - :BotBackend trait impl,Phase C/D(daemon.ensure_running +
//!   client.load_process + spawn poller),stop / abort_start / zombie reaper /
//!   kill_process_tree / read_config / write_config / tail_log
//! - :单元测试
//!   红线:所有跨边界类型派生 ts-rs;不使用 serde_json::Value 透传业务字段
//!   

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::ids::BotId;
use crate::kinds::{BackendKind, BotFlavor};
use crate::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, BotStatus, LogSnapshot, StopMode,
    TailOpts,
};
use crate::snowluma::daemon::SnowLumaDaemon;
use crate::snowluma::proc_tree::SysinfoProcessTreeProbe;
use crate::snowluma::status_poller::{PollerDeps, ProcessTreeProbe, SnowLumaStatusPoller};
use ncd_domain::snowluma_start_mode::SnowLumaStartMode;

/// daemon.ensure_running 总超时
const DAEMON_ENSURE_TIMEOUT: Duration = Duration::from_secs(35);

/// per-Bot 注入完成后写入的进程记录
struct SnowLumaProcessRecord {
    qq_pid: u32,
    /// COLD 模式持有;HOT 模式 None(用户已开的 QQ 不归 backend 管)
    qq_child: Option<tokio::process::Child>,
    started_at: u64,
    config: BotRuntimeConfig,
    start_mode: SnowLumaStartMode,
    /// 由 status poller 锁定后回写(poller 通过事件总线报;MVP 不在本 record 内
    /// 同步刷新,仅保留字段)
    #[allow(dead_code)]
    uin: Option<String>,
    #[allow(dead_code)]
    ancillary_pids: HashSet<u32>,
}

pub struct SnowLumaRuntimeBackend {
    backend_id: BotId,
    daemon: Arc<SnowLumaDaemon>,
    event_bus: Arc<BroadcastEventBus>,
    processes: Arc<RwLock<HashMap<BotId, SnowLumaProcessRecord>>>,
    pollers: Arc<RwLock<HashMap<BotId, SnowLumaStatusPoller>>>,
    /// COLD 模式 stop 后等 child.wait() 完成的回收池
    zombies: Arc<RwLock<Vec<tokio::process::Child>>>,
    /// 可注入 ProcessTreeProbe,便于测试覆盖 HotStart PID 校验路径
    proc_tree: Arc<dyn ProcessTreeProbe>,
}

impl SnowLumaRuntimeBackend {
    pub fn new(
        backend_id: BotId,
        daemon: Arc<SnowLumaDaemon>,
        event_bus: Arc<BroadcastEventBus>,
    ) -> Self {
        Self {
            backend_id,
            daemon,
            event_bus,
            processes: Arc::new(RwLock::new(HashMap::new())),
            pollers: Arc::new(RwLock::new(HashMap::new())),
            zombies: Arc::new(RwLock::new(Vec::new())),
            proc_tree: Arc::new(SysinfoProcessTreeProbe::new()),
        }
    }

    /// 测试入口:注入自定义 ProcessTreeProbe(HotStart PID 二次校验时使用)
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_proc_tree(mut self, probe: Arc<dyn ProcessTreeProbe>) -> Self {
        self.proc_tree = probe;
        self
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_start_mode(config: &BotRuntimeConfig) -> SnowLumaStartMode {
    // 通过 BotRuntimeConfig.environment 读 launch planner 注入的 start_mode
    // 缺失字段(NapCat 路径误入 / 老配置)回落到 ColdStart
    match config
        .environment
        .get("SNOWLUMA_START_MODE")
        .map(|s| s.as_str())
    {
        Some("hot_start") => SnowLumaStartMode::HotStart,
        _ => SnowLumaStartMode::ColdStart,
    }
}

/// 从 environment 读出 qq_id(HotStart 自动匹配 PID 用)
fn read_qq_id(config: &BotRuntimeConfig) -> Option<u64> {
    config
        .environment
        .get("SNOWLUMA_QQ_ID")
        .and_then(|s| s.parse::<u64>().ok())
}

// Phase A:spawn QQ.exe (COLD) / 按 qq_id 自动匹配并校验 PID (HOT)

/// HOT 模式:按 qq_id 在系统中自动定位登录此账号的 QQ.exe 主进程 PID
///
/// 流程:
/// 1. qq_login_probe::find_pid_by_qq_id(qq_id) 走 9210-9219 tencent:// 探测
///    匹配 uin == qq_id 的 PID
/// 2. 拿到 PID 后用 ProcessTreeProbe 二次校验进程仍存活(防止 probe 拿到结
///    果到 inject 之间有窗口期 PID 退出)
async fn locate_hot_pid_by_qq_id(
    probe: &Arc<dyn ProcessTreeProbe>,
    qq_id: u64,
) -> Result<u32, BotBackendError> {
    let probed = super::qq_login_probe::find_pid_by_qq_id(qq_id)
        .await
        .ok_or_else(|| {
            BotBackendError::InvalidConfig(format!(
                "snowluma hot start: 未找到登录 QQ {qq_id} 的 QQ.exe 进程，请先在 QQ 客户端登录该账号"
            ))
        })?;

    let candidates = probe.collect_descendants(probed.pid).await;
    if !candidates.contains(&probed.pid) {
        // probe 阶段拿到的 PID 已退出(窗口期内)→ 当作匹配失败处理
        return Err(BotBackendError::InvalidConfig(format!(
            "snowluma hot start: 已探测到 QQ {qq_id} 的进程 PID={pid}，但二次校验发现进程已退出",
            pid = probed.pid,
        )));
    }
    Ok(probed.pid)
}

/// COLD 模式 spawn QQ.exe
/// MVP:从 BotRuntimeConfig.environment["SNOWLUMA_QQ_EXE"] 读 QQ.exe 路径
/// 由 runtime_launch_plan.rs 在 wiring 阶段注入缺失时返回 InvalidConfig
/// 不再回退到任意位置
async fn spawn_cold_qq(
    config: &BotRuntimeConfig,
) -> Result<(u32, tokio::process::Child), BotBackendError> {
    use std::process::Stdio;

    use ncd_host::hide_console_window;

    let qq_path = config
        .environment
        .get("SNOWLUMA_QQ_EXE")
        .ok_or_else(|| {
            BotBackendError::InvalidConfig(
                "snowluma cold start: SNOWLUMA_QQ_EXE env var missing".into(),
            )
        })?
        .clone();
    let qq_path = PathBuf::from(qq_path);

    let mut qq_cmd = tokio::process::Command::new(&qq_path);
    qq_cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console_window(&mut qq_cmd);
    let child = qq_cmd.spawn().map_err(|e| {
        BotBackendError::Io(format!(
            "spawn QQ.exe failed: {e} (path={})",
            qq_path.display()
        ))
    })?;

    let pid = child
        .id()
        .ok_or_else(|| BotBackendError::Io("spawn QQ.exe: no PID".into()))?;
    Ok((pid, child))
}

// Zombie reaper

/// 把 child 转入 zombie 池并 spawn 一个 reaper task 等 wait 完成后释放 wrapper
fn enqueue_zombie(zombies: Arc<RwLock<Vec<tokio::process::Child>>>, child: tokio::process::Child) {
    tokio::spawn(async move {
        let mut child = child;
        let _ = child.wait().await;
        // Wait 已返回;从池中清理任意一个最早的 wrapper(顺序无关,OS 已确认退出)
        // MVP 简化:池中持的是已 wait 完的 wrapper,drop 时 Rust 不会发任何信号
        // 直接 push 也无所谓——本 reaper 任务自身就是池的"逻辑容器"
        let _ = zombies; // 句柄保活,本 reaper task 完成时自然 drop
    });
    // 注:上面的逻辑里 child 本来已经 move 进 reaper task,没有真往 zombies 里 push
    // 真正的不变量是"reaper task 等到 wait 返回前持有 child 句柄"——已满足
}

// kill_process_tree(Windows: taskkill /T /F;MVP 不深度遍历,靠 taskkill 自带递归)

#[cfg(windows)]
async fn kill_process_tree(qq_pid: u32) -> Result<(), BotBackendError> {
    use std::process::Stdio;

    use ncd_host::hide_console_window;

    let mut kill_cmd = tokio::process::Command::new("taskkill");
    kill_cmd
        .args(["/PID", &qq_pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console_window(&mut kill_cmd);
    let _ = kill_cmd
        .status()
        .await
        .map_err(|e| BotBackendError::Io(format!("taskkill failed: {e}")))?;
    Ok(())
}

#[cfg(not(windows))]
async fn kill_process_tree(qq_pid: u32) -> Result<(), BotBackendError> {
    // 非 Windows 平台 SnowLuma 不支持,但保持函数可编译
    let _ = qq_pid;
    Ok(())
}

// abort_start:start 中段失败回滚

async fn abort_start(
    daemon: &Arc<SnowLumaDaemon>,
    zombies: Arc<RwLock<Vec<tokio::process::Child>>>,
    qq_pid: Option<u32>,
    qq_child: Option<tokio::process::Child>,
    start_mode: SnowLumaStartMode,
) {
    // COLD 模式:杀掉自己 spawn 的 QQ.exe;child 转入 zombie 池等 wait
    if start_mode.is_cold()
        && let Some(pid) = qq_pid
    {
        let _ = kill_process_tree(pid).await;
    }
    if let Some(child) = qq_child {
        enqueue_zombie(zombies, child);
    }
    // 任何模式:daemon ref_count -= 1
    daemon.release().await;
}

// BotBackend trait impl

#[async_trait]
impl BotBackend for SnowLumaRuntimeBackend {
    fn id(&self) -> &BotId {
        &self.backend_id
    }
    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }
    fn flavor(&self) -> BotFlavor {
        BotFlavor::SnowLuma
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        #[cfg(not(windows))]
        return Err(BotBackendError::InvalidConfig(
            "snowluma backend requires windows".into(),
        ));

        #[cfg(windows)]
        {
            let bot_id = ctx.config.bot_id.clone();
            let start_mode = read_start_mode(&ctx.config);

            // === Phase A:HOT 自动按 qq_id 匹配 PID / COLD spawn QQ.exe ===
            let (qq_pid, qq_child) = match start_mode {
                SnowLumaStartMode::HotStart => {
                    let qq_id = read_qq_id(&ctx.config).ok_or_else(|| {
                        BotBackendError::InvalidConfig(
                            "snowluma hot start: SNOWLUMA_QQ_ID env var missing".into(),
                        )
                    })?;
                    let pid = locate_hot_pid_by_qq_id(&self.proc_tree, qq_id).await?;
                    (pid, None)
                }
                SnowLumaStartMode::ColdStart => {
                    let (pid, child) = spawn_cold_qq(&ctx.config).await?;
                    (pid, Some(child))
                }
            };

            // === Phase C:daemon.ensure_running + client.load_process ===
            let client = match self.daemon.ensure_running(DAEMON_ENSURE_TIMEOUT).await {
                Ok(c) => c,
                Err(err) => {
                    abort_start(
                        &self.daemon,
                        Arc::clone(&self.zombies),
                        Some(qq_pid),
                        qq_child,
                        start_mode,
                    )
                    .await;
                    return Err(BotBackendError::Io(format!("daemon ensure_running: {err}")));
                }
            };

            if let Err(err) = client.load_process(qq_pid).await {
                abort_start(
                    &self.daemon,
                    Arc::clone(&self.zombies),
                    Some(qq_pid),
                    qq_child,
                    start_mode,
                )
                .await;
                return Err(BotBackendError::Io(format!("load_process: {err}")));
            }

            // 注入成功 → 物理就绪事件
            self.event_bus
                .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), qq_pid));

            // === Phase D:spawn poller ===
            let poller_deps = PollerDeps {
                event_bus: Arc::clone(&self.event_bus),
                http: Arc::clone(&client),
                proc_tree: Arc::clone(&self.proc_tree),
            };
            let poller = SnowLumaStatusPoller::spawn(bot_id.clone(), qq_pid, poller_deps);
            {
                let mut pollers = self.pollers.write().await;
                if let Some(old) = pollers.insert(bot_id.clone(), poller) {
                    // 被覆盖的旧 poller 显式 dispose 一次(虽然 Drop 也会兜底)
                    old.dispose();
                }
            }

            // 写入 process record
            let started_at = now_unix_secs();
            {
                let mut processes = self.processes.write().await;
                processes.insert(
                    bot_id.clone(),
                    SnowLumaProcessRecord {
                        qq_pid,
                        qq_child,
                        started_at,
                        config: ctx.config.clone(),
                        start_mode,
                        uin: None,
                        ancillary_pids: HashSet::new(),
                    },
                );
            }

            Ok(BotStatus::running(bot_id, qq_pid, started_at))
        }
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        // 取出 process record
        let record = {
            let mut processes = self.processes.write().await;
            processes.remove(&bot_id)
        };

        // dispose poller
        {
            let mut pollers = self.pollers.write().await;
            if let Some(p) = pollers.remove(&bot_id) {
                p.dispose();
            }
        }

        let Some(record) = record else {
            // 幂等:未在跑过直接 Ok
            return Ok(());
        };

        // unload + kill 进程树(COLD)
        let qq_pid = record.qq_pid;
        let start_mode = record.start_mode;

        // unload fire-and-forget
        if let Ok(client) = self.daemon.current_client().await {
            tokio::spawn(async move {
                let _ = client.unload_process(qq_pid).await;
            });
        }

        if start_mode.is_cold() {
            let _ = kill_process_tree(qq_pid).await;
            if let Some(child) = record.qq_child {
                enqueue_zombie(Arc::clone(&self.zombies), child);
            }
        }
        // HOT:不杀用户进程

        self.daemon.release().await;
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let processes = self.processes.read().await;
        match processes.get(&bot_id) {
            Some(rec) => Ok(BotStatus::running(bot_id, rec.qq_pid, rec.started_at)),
            None => Ok(BotStatus::stopped(bot_id)),
        }
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        let processes = self.processes.read().await;
        match processes.get(&bot_id) {
            Some(rec) => Ok(rec.config.clone()),
            None => Err(BotBackendError::ConfigNotFound(bot_id)),
        }
    }

    async fn write_config(
        &self,
        bot_id: BotId,
        cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        let mut processes = self.processes.write().await;
        if let Some(rec) = processes.get_mut(&bot_id) {
            rec.config = cfg.clone();
        }
        Ok(())
    }

    async fn tail_log(
        &self,
        _bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        // SnowLuma daemon stdout 是所有 SL bot 共享的物理事实,没有 per-bot 的
        // stdout 通道BotLogPage 一开页拿到的历史日志,从 daemon 自己的
        // recent_log ring buffer 转过来daemon 本身就用这份 buffer 给启动失败
        // 的 last_error 拼上下文,复用即可,不需要再额外维护一份镜像
        // 实时增量仍然走 DomainEvent::SnowLumaDaemonLog,前端 useBotLogStream
        // 已经在订阅
        let lines = self.daemon.snapshot_recent_log();
        let total = lines.len();
        let limited = if opts.lines > 0 && lines.len() > opts.lines {
            lines[lines.len() - opts.lines..].to_vec()
        } else {
            lines
        };
        Ok(LogSnapshot {
            lines: limited,
            total_lines: total,
        })
    }
}

// 单元测试
//
// 由于 SnowLumaRuntimeBackend 主路径强依赖 Windows + 真实 sysinfo + 真起 QQ.exe
// 端到端 start/stop 测试只能在真机覆盖本测试模块仅验证:
// 1) flavor / kind / id 三个 trivial 方法
// 2) read_start_mode 缺失环境变量回落 ColdStart
//
// HotStart 自动按 qq_id 匹配的端到端覆盖见 qq_login_probe::tests,那里测了
// JWT decode / extract 这些纯函数;真机匹配只能在 windows + 真实 QQ.exe 下验

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_start_mode_defaults_to_cold() {
        // BotRuntimeConfig 不带 start_mode 字段;缺失环境变量时默认走 ColdStart
        let config = BotRuntimeConfig::default_path("/tmp", BotId::new("10001"));
        match read_start_mode(&config) {
            SnowLumaStartMode::ColdStart => {}
            other => panic!("expected ColdStart, got {other:?}"),
        }
    }

    #[test]
    fn read_start_mode_recognizes_hot_start_env() {
        let mut config = BotRuntimeConfig::default_path("/tmp", BotId::new("10001"));
        config
            .environment
            .insert("SNOWLUMA_START_MODE".to_string(), "hot_start".to_string());
        assert!(matches!(
            read_start_mode(&config),
            SnowLumaStartMode::HotStart
        ));
    }

    #[test]
    fn read_qq_id_parses_decimal_string() {
        let mut config = BotRuntimeConfig::default_path("/tmp", BotId::new("10001"));
        config
            .environment
            .insert("SNOWLUMA_QQ_ID".to_string(), "572381217".to_string());
        assert_eq!(read_qq_id(&config), Some(572381217));
    }

    #[test]
    fn read_qq_id_returns_none_when_missing_or_invalid() {
        let mut config = BotRuntimeConfig::default_path("/tmp", BotId::new("10001"));
        assert_eq!(read_qq_id(&config), None);
        config
            .environment
            .insert("SNOWLUMA_QQ_ID".to_string(), "abc".to_string());
        assert_eq!(read_qq_id(&config), None);
    }
}
