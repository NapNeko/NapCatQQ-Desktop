//! 远端 SnowLuma「直接运行」:内联 shell 编排 + SSH 隧道 WebUI 注入(无上传 launcher 文件)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ncd_domain::{BackendType, BotConfig, BotFlavor, BotId, DeploymentType, SnowLumaStartMode};
use ncd_host::{Host, HostCommand, HostPath};
use serde_json::Value;
use tokio::sync::Mutex;

use ncd_deploy::backend_config_renderer::render_snowluma_docker_config_payloads;
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::ids::BotId;
use ncd_domain::kinds::BackendKind;
use ncd_traits::events::{BroadcastEventBus, EventBus};
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, BotStatus, LogSnapshot, StopMode,
    TailOpts,
};
use crate::remote_snowluma_layout::{
    RemoteSnowLumaLayout, SnowLumaRemotePaths, DEFAULT_WEBUI_PORT, napcat_layout_qq_executable,
    probe_remote_snowluma_layout,
};
use crate::remote_snowluma_orchestrator::{
    bot_cold_start, bot_stop, daemon_start, daemon_stop, remote_daemon_already_ready,
    resolve_remote_bash, wait_webui_tcp, write_status_daemon_json,
};
use crate::remote_snowluma_tunnel::{
    RemoteSnowLumaTunnelEndpoints, RemoteSnowLumaTunnelRegistry,
};
use crate::snowluma::daemon::DaemonState;
use crate::snowluma::error::SnowLumaWebUiError;
use crate::snowluma::session::{build_webui_json_payload, generate_strong_password};
use crate::snowluma::status_poller::{PollerDeps, SnowLumaStatusPoller};
use crate::snowluma::webui_client::{HookProcessStatus, ReqwestSnowLumaWebUiClient, SnowLumaWebUiClient};
use serde_json::json;

async fn host_file_nonempty(host: &dyn Host, path: &str) -> bool {
    match host.read_file(&HostPath::from_posix(path)).await {
        Ok(b) => !b.is_empty(),
        Err(_) => false,
    }
}

async fn read_remote_file_trimmed(host: &dyn Host, path: &str) -> Result<String, BotBackendError> {
    let bytes = host
        .read_file(&HostPath::from_posix(path))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(String::from_utf8_lossy(&bytes).trim().to_string())
}

async fn read_remote_log_tail(host: &dyn Host, path: &str, max_lines: usize) -> Result<String, BotBackendError> {
    let bytes = match host.read_file(&HostPath::from_posix(path)).await {
        Ok(b) => b,
        Err(_) => return Ok(String::new()),
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let start = if lines.len() > max_lines { lines.len() - max_lines } else { 0 };
    Ok(lines[start..].join("\n"))
}

/// daemon 启动前:全局 config,VNC/WebUI 明文密钥,图形栈探测
async fn ensure_remote_daemon_prereqs(
    host: &dyn Host,
    home: &str,
    paths: &SnowLumaRemotePaths,
) -> Result<(), BotBackendError> {
    host.create_dir_all(&HostPath::from_posix(&paths.config_dir))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;

    let runtime_json = serde_json::to_vec_pretty(&json!({ "webuiPort": DEFAULT_WEBUI_PORT }))
        .map_err(|e| BotBackendError::Json(e.to_string()))?;
    host.write_file(
        &HostPath::from_posix(format!("{}/runtime.json", paths.config_dir)),
        &runtime_json,
    )
    .await
    .map_err(|e| BotBackendError::Io(e.to_string()))?;

    let webui_plain = if host_file_nonempty(host, &paths.webui_secret).await {
        let bytes = host
            .read_file(&HostPath::from_posix(&paths.webui_secret))
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
        String::from_utf8_lossy(&bytes).trim().to_string()
    } else {
        let pwd = generate_strong_password(16);
        host.write_file(&HostPath::from_posix(&paths.webui_secret), pwd.as_bytes())
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
        pwd
    };

    if webui_plain.is_empty() {
        return Err(BotBackendError::InvalidConfig(
            "远端 webui.secret 为空，无法启动 SnowLuma daemon".into(),
        ));
    }

    let webui_payload = build_webui_json_payload(&webui_plain, false)
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    let webui_json = serde_json::to_vec_pretty(&webui_payload)
        .map_err(|e| BotBackendError::Json(e.to_string()))?;
    host.write_file(
        &HostPath::from_posix(format!("{}/webui.json", paths.config_dir)),
        &webui_json,
    )
    .await
    .map_err(|e| BotBackendError::Io(e.to_string()))?;

    if !host_file_nonempty(host, &paths.vnc_secret).await {
        let vnc_pwd = generate_strong_password(8);
        host.write_file(&HostPath::from_posix(&paths.vnc_secret), vnc_pwd.as_bytes())
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }

    let stack_check = HostCommand::new("sh").arg("-c").arg(
        "command -v Xvfb >/dev/null && command -v x11vnc >/dev/null && \
         command -v websockify >/dev/null && command -v dbus-launch >/dev/null",
    );
    let stack = host
        .run_to_string(stack_check)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !stack.success() {
        return Err(BotBackendError::InvalidConfig(
            "远端缺少 SnowLuma 图形栈（需要 Xvfb、x11vnc、websockify、dbus-launch）。\
             请先在远端安装依赖（或参考 legacy install_snowluma 脚本）。"
                .into(),
        ));
    }

    resolve_remote_bash(host).await?;

    let qq = napcat_layout_qq_executable(home);
    let qq_check = HostCommand::new("sh")
        .arg("-c")
        .arg(format!("test -x '{}'", qq.replace('\'', "'\"'\"'")));
    let qq_out = host
        .run_to_string(qq_check)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !qq_out.success() {
        return Err(BotBackendError::InvalidConfig(format!(
            "远端未找到可执行的 QQ（组件页应已安装到 {qq}）。请先在同一 SSH 主机安装 QQ 组件。"
        )));
    }

    Ok(())
}

pub async fn render_native_snowluma_config_on_host(
    host: &dyn Host,
    bot_id: &BotId,
    config: &BotConfig,
    paths: &SnowLumaRemotePaths,
) -> Result<(), BotBackendError> {
    if config.bot.backend_type != BackendType::SnowLuma {
        return Err(BotBackendError::InvalidConfig(
            "render_native_snowluma_config_on_host 仅支持 SnowLuma".into(),
        ));
    }
    host.create_dir_all(&HostPath::from_posix(&paths.config_dir))
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    let config_dir = &paths.config_dir;
    let mut existing = HashMap::new();
    let file_name = format!("onebot_{}.json", bot_id.as_str());
    let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
    if let Ok(bytes) = host.read_file(&path).await {
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            existing.insert(file_name.clone(), value);
        }
    }
    for item in render_snowluma_docker_config_payloads(bot_id, config, &existing) {
        let bytes = serde_json::to_vec_pretty(&item.payload)
            .map_err(|e| BotBackendError::Json(e.to_string()))?;
        let p = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
        host.write_file(&p, &bytes)
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }
    Ok(())
}

fn resolve_start_mode(config: &BotConfig) -> SnowLumaStartMode {
    config
        .bot
        .snowluma_start_mode
        .unwrap_or(SnowLumaStartMode::ColdStart)
}

/// 远端 Native + SnowLuma + SSH 主机
pub fn is_remote_native_snowluma_config(config: &BotConfig) -> bool {
    config.bot.backend_type == BackendType::SnowLuma
        && config.bot.deployment_type == DeploymentType::Native
        && matches!(
            config.bot.runtime_target,
            ncd_domain::RuntimeTarget::Server(_)
        )
}

/// 远端是否已有匹配 qq_id 的 qq 进程(热启动 attach / bootstrap reconcile 用)
pub async fn remote_qq_running_pid(host: &dyn Host, qq_id: u64) -> Result<Option<u32>, BotBackendError> {
    let script = format!(
        r#"pgrep -f "qq --no-sandbox -q {qq_id}$" 2>/dev/null | head -n 1"#
    );
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        return Ok(None);
    }
    let line = out.stdout.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    line.parse()
        .map(Some)
        .map_err(|_| BotBackendError::InvalidConfig(format!("invalid pgrep pid: {line}")))
}

async fn inject_via_tunnel(
    endpoints: &RemoteSnowLumaTunnelEndpoints,
    qq_pid: u32,
) -> Result<Arc<dyn SnowLumaWebUiClient>, BotBackendError> {
    let client = ReqwestSnowLumaWebUiClient::new(endpoints.webui_local_port, endpoints.webui_password.clone())
        .map_err(|e: SnowLumaWebUiError| BotBackendError::Io(e.to_string()))?;
    client
        .wait_ready(Duration::from_secs(90), Box::new(|| false))
        .await
        .map_err(|e| BotBackendError::Io(format!("SnowLuma WebUI wait_ready: {e}")))?;
    client
        .login()
        .await
        .map_err(|e| BotBackendError::Io(format!("SnowLuma WebUI login: {e}")))?;

    // 关键修复:冷启动刚 spawn QQ 后,SnowLuma daemon 侧的扫描器需要时间把该 PID 识别为 Available
    // 立即 load_process 极大概率拿到 "server rejected"(success=false,error 常为空)
    // 这里 best-effort 等到列表里出现该 PID 且状态不是 Error/Disconnected 再去 load;超时也不阻塞,
    // 由 load_process 自己失败 + 事后 list_processes 快照一起给出带上下文的错误,便于诊断
    wait_process_available(&client, qq_pid, Duration::from_secs(25)).await;

    // 尝试 loadSnowLuma 对已注入的进程返回 success=false + "already loaded" 类消息;
    // 对刚 spawn 尚未被扫描到的返回 success=false + 空 error
    // 我们把 "already" 当成注入成功(调用方后续会建 poller 并依赖状态轮询推进到 Online)
    // 其它拒绝场景:若 error 为空则抓一次当前快照,把该 pid 的真实状态(Available / Error / 不存在)带进错误
    let load_res = client.load_process(qq_pid).await;
    match load_res {
        Ok(_) => {
            // 成功注入(或 daemon 确认可注入)
        }
        Err(SnowLumaWebUiError::ServerRejected { ref message, .. }) => {
            let m = message.to_lowercase();
            if m.contains("already") {
                // 视为已注入成功继续用这个已登录的 client 建 poller 即可
                // 为了诊断友好,记录一条 info 级别的观察(不作为错误返回)
                tracing::info!(
                    target: "ncd_runtime::remote_snowluma",
                    pid = qq_pid,
                    "load_process reported already loaded/injected; proceeding with existing client"
                );
            } else {
                // 非 "already" 的拒绝:丰富错误上下文
                let mut msg = format!("SnowLuma load_process: {load_res:?}");
                // 上面 load_res 已被消费,这里重新构造可读消息;实际上我们用原始 e
                // 重新取一次 e 的 Display 更准:
                let e = SnowLumaWebUiError::ServerRejected {
                    endpoint: format!("/api/processes/{qq_pid}/load"),
                    message: message.clone(),
                };
                msg = format!("SnowLuma load_process: {e}");
                if message.trim().is_empty() {
                    match client.list_processes().await {
                        Ok(list) => {
                            if let Some(p) = list.into_iter().find(|p| p.pid == qq_pid) {
                                msg.push_str(&format!(
                                    "; observed pid {} in /api/processes: status={:?} error={:?}",
                                    p.pid, p.status, p.error
                                ));
                            } else {
                                msg.push_str("; pid not present in current /api/processes snapshot");
                            }
                        }
                        Err(list_err) => {
                            msg.push_str(&format!("; list_processes after rejection also failed: {list_err}"));
                        }
                    }
                }
                return Err(BotBackendError::Io(msg));
            }
        }
        Err(e) => {
            // 其它 WebUI 错误(超时,网络,decode 等),同样尝试补快照(仅对 ServerRejected 空消息有意义,这里只透传)
            return Err(BotBackendError::Io(format!("SnowLuma load_process: {e}")));
        }
    }

    Ok(Arc::new(client) as Arc<dyn SnowLumaWebUiClient>)
}

/// 等待 SnowLuma 的 /api/processes 列表出现指定 pid
///
/// 冷启动时我们在远端 DISPLAY 下用 nohup 刚 spawn 了 QQ 进程,SnowLuma daemon
/// 内部的扫描器(很可能按固定周期遍历 /proc 或遍历 DISPLAY 下的窗口/进程)需要时间
/// 才能把这个 pid 标记为 "Available"(可注入)直接在 spawn 后立刻 load_process
/// 很容易遇到 "server rejected"(success=false,error 常为空)
///
/// 本函数只做 best-effort 等待:
/// - 出现 Available / Loaded / Online / Connecting / Loading 就返回(上层会尝试 load 或容忍已注入)
/// - 明确看到 Error / Disconnected 立即返回(上层会把错误状态一起报出来)
/// - 超时不抛错,让上层 load + 事后 list 快照统一产生带上下文的错误,便于用户/日志诊断
async fn wait_process_available(client: &dyn SnowLumaWebUiClient, pid: u32, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            return;
        }
        if let Ok(list) = client.list_processes().await {
            if let Some(p) = list.into_iter().find(|p| p.pid == pid) {
                match p.status {
                    HookProcessStatus::Error | HookProcessStatus::Disconnected => return,
                    HookProcessStatus::Available
                    | HookProcessStatus::Loaded
                    | HookProcessStatus::Online
                    | HookProcessStatus::Connecting
                    | HookProcessStatus::Loading => return,
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
}

/// 单台远端主机共享的 SL daemon(单例图形栈 + node);多 Bot 共用,按 qq_id 分别启停 QQ
pub struct RemoteSnowLumaDaemon {
    host: Arc<dyn Host>,
    layout: RemoteSnowLumaLayout,
    server_id: String,
    refcount: Mutex<u32>,
    /// 同一 SSH 主机上多 Bot 并发 start 时,整段启栈(Xvfb/x11vnc/node)单飞,避免抢 5900 等端口
    stack_bootstrap: Mutex<()>,
    tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    event_bus: Arc<BroadcastEventBus>,
    tunnel_eps: Mutex<Option<RemoteSnowLumaTunnelEndpoints>>,
}

impl RemoteSnowLumaDaemon {
    pub async fn new(
        server_id: String,
        host: Arc<dyn Host>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        event_bus: Arc<BroadcastEventBus>,
    ) -> Result<Self, BotBackendError> {
        let layout = probe_remote_snowluma_layout(host.as_ref()).await?;
        Ok(Self {
            host,
            layout,
            server_id,
            refcount: Mutex::new(0),
            stack_bootstrap: Mutex::new(()),
            tunnels,
            event_bus,
            tunnel_eps: Mutex::new(None),
        })
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn remote_home(&self) -> &str {
        &self.layout.home
    }

    pub fn paths(&self) -> &SnowLumaRemotePaths {
        &self.layout.paths
    }

    pub fn layout(&self) -> &RemoteSnowLumaLayout {
        &self.layout
    }

    pub async fn ensure_running(&self) -> Result<(), BotBackendError> {
        let _stack_guard = self.stack_bootstrap.lock().await;

        self.event_bus.publish(DomainEvent::snowluma_daemon_state_changed(
            DaemonState::Starting,
            0,
            None,
            Some(self.server_id.clone()),
        ));

        ensure_remote_daemon_prereqs(
            self.host.as_ref(),
            &self.layout.home,
            &self.layout.paths,
        )
        .await?;

        let stack_up = remote_daemon_already_ready(self.host.as_ref(), &self.layout.paths).await?;
        if !stack_up {
            daemon_start(self.host.as_ref(), &self.layout).await?;
            wait_webui_tcp(
                self.host.as_ref(),
                DEFAULT_WEBUI_PORT,
                Duration::from_secs(90),
            )
            .await?;
        } else {
            wait_webui_tcp(
                self.host.as_ref(),
                DEFAULT_WEBUI_PORT,
                Duration::from_secs(30),
            )
            .await?;
        }
        write_status_daemon_json(self.host.as_ref(), &self.layout.paths, true, true).await?;

        let webui_plain =
            read_remote_file_trimmed(self.host.as_ref(), &self.layout.paths.webui_secret).await?;
        let vnc_plain =
            read_remote_file_trimmed(self.host.as_ref(), &self.layout.paths.vnc_secret).await?;
        if webui_plain.is_empty() {
            return Err(BotBackendError::InvalidConfig(
                "远端 webui.secret 为空，无法建立 SnowLuma WebUI 隧道".into(),
            ));
        }

        let eps = self
            .tunnels
            .acquire(
                &self.server_id,
                self.host.as_ref(),
                webui_plain,
                vnc_plain,
            )
            .await
            .map_err(|e| BotBackendError::Io(format!("SnowLuma SSH 隧道: {e}")))?;
        *self.tunnel_eps.lock().await = Some(eps);

        let mut guard = self.refcount.lock().await;
        *guard = guard.saturating_add(1);
        let rc = *guard;
        drop(guard);

        self.event_bus.publish(DomainEvent::snowluma_daemon_state_changed(
            DaemonState::Ready,
            rc,
            None,
            Some(self.server_id.clone()),
        ));
        Ok(())
    }

    pub async fn release(&self) {
        let mut guard = self.refcount.lock().await;
        if *guard == 0 {
            return;
        }
        *guard -= 1;
        let rc = *guard;
        let stop_daemon = rc == 0;
        drop(guard);

        self.tunnels.release(&self.server_id).await;

        if stop_daemon {
            let _ = daemon_stop(self.host.as_ref(), &self.layout.paths).await;
            *self.tunnel_eps.lock().await = None;
            self.event_bus.publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Stopped,
                0,
                None,
                Some(self.server_id.clone()),
            ));
        }
    }

    pub async fn tunnel_endpoints(&self) -> Option<RemoteSnowLumaTunnelEndpoints> {
        self.tunnel_eps.lock().await.clone()
    }

    /// 桌面退出:只拆掉本机 SSH 隧道,不 stop 远端 daemon / QQ
    pub async fn detach_local_sessions(&self) {
        *self.tunnel_eps.lock().await = None;
        self.tunnels.release(&self.server_id).await;
    }

    /// 冷启动 reconcile:远端进程已在跑时只补隧道与状态文件,不重复 daemon start shell
    pub async fn ensure_running_for_reconcile(&self) -> Result<(), BotBackendError> {
        ensure_remote_daemon_prereqs(
            self.host.as_ref(),
            &self.layout.home,
            &self.layout.paths,
        )
        .await?;

        if !crate::remote_snowluma_orchestrator::remote_daemon_already_ready(
            self.host.as_ref(),
            &self.layout.paths,
        )
        .await?
        {
            return Err(BotBackendError::InvalidConfig(
                "bootstrap reconcile: 远端 SnowLuma daemon 未就绪，请手动启动 Bot".into(),
            ));
        }

        write_status_daemon_json(self.host.as_ref(), &self.layout.paths, true, true).await?;

        let webui_plain =
            read_remote_file_trimmed(self.host.as_ref(), &self.layout.paths.webui_secret).await?;
        let vnc_plain =
            read_remote_file_trimmed(self.host.as_ref(), &self.layout.paths.vnc_secret).await?;
        if webui_plain.is_empty() {
            return Err(BotBackendError::InvalidConfig(
                "远端 webui.secret 为空，无法建立 SnowLuma WebUI 隧道".into(),
            ));
        }

        let eps = self
            .tunnels
            .acquire(
                &self.server_id,
                self.host.as_ref(),
                webui_plain,
                vnc_plain,
            )
            .await
            .map_err(|e| BotBackendError::Io(format!("SnowLuma SSH 隧道: {e}")))?;
        *self.tunnel_eps.lock().await = Some(eps);

        let mut guard = self.refcount.lock().await;
        if *guard == 0 {
            *guard = 1;
        }
        drop(guard);

        self.event_bus.publish(DomainEvent::snowluma_daemon_state_changed(
            DaemonState::Ready,
            *self.refcount.lock().await,
            None,
            Some(self.server_id.clone()),
        ));
        Ok(())
    }
}

/// 远端 SnowLuma BotBackend(内联编排,非本机 SnowLumaDaemon)
pub struct RemoteSnowLumaBackend {
    backend_id: BotId,
    daemon: Arc<RemoteSnowLumaDaemon>,
    event_bus: Arc<BroadcastEventBus>,
    #[allow(dead_code)]
    tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    start_modes: Arc<Mutex<HashMap<BotId, SnowLumaStartMode>>>,
    pollers: Arc<Mutex<HashMap<BotId, SnowLumaStatusPoller>>>,
    /// Shared coordinator for flipping the common ~/Napcat/opt/QQ tree entry point.
    /// Passed from BotManager so that NC and SL cold starts on the same server_id
    /// serialize their package.json main changes.
    qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
}

impl RemoteSnowLumaBackend {
    /// 内部构造器
    ///
    /// 只应由同 crate 内的 BotManager 调用
    /// 使用 pub(crate) 而非 pub,以避免把 pub(crate) 的 RemoteQqEntryCoordinator
    /// 通过公开 API 泄露出去(这正是编译器 private_interfaces 警告的来源)
    ///
    /// 外部 crate 不应直接构造此类型,所有 backend 都应由 BotManager::backend_for_config
    /// 统一创建
    pub(crate) fn new(
        backend_id: impl Into<BotId>,
        daemon: Arc<RemoteSnowLumaDaemon>,
        event_bus: Arc<BroadcastEventBus>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
    ) -> Self {
        Self {
            backend_id: backend_id.into(),
            daemon,
            event_bus,
            tunnels,
            start_modes: Arc::new(Mutex::new(HashMap::new())),
            pollers: Arc::new(Mutex::new(HashMap::new())),
            qq_entry_coordinator,
        }
    }

    /// 冷启动后再开桌面:远端 QQ 仍在跑时恢复隧道注入与 status poller
    pub async fn attach_reconciled_running(
        &self,
        bot_id: BotId,
        pid: u32,
        config: &BotConfig,
    ) -> Result<(), BotBackendError> {
        self.daemon.ensure_running_for_reconcile().await?;
        let endpoints = self
            .daemon
            .tunnel_endpoints()
            .await
            .ok_or_else(|| BotBackendError::Io("SnowLuma 隧道未建立".into()))?;
        let http = inject_via_tunnel(&endpoints, pid).await?;
        self.event_bus
            .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), pid));
        self.event_bus
            .publish(DomainEvent::SnowLumaDockerEndpointsReady {
                bot_id: bot_id.clone(),
            });
        self.start_modes
            .lock()
            .await
            .insert(bot_id.clone(), resolve_start_mode(config));

        if self.pollers.lock().await.contains_key(&bot_id) {
            return Ok(());
        }
        let poller_deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http,
            proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(pid)),
        };
        let poller = SnowLumaStatusPoller::spawn(bot_id.clone(), pid, poller_deps);
        self.pollers.lock().await.insert(bot_id, poller);
        Ok(())
    }
}

#[async_trait]
impl BotBackend for RemoteSnowLumaBackend {
    fn id(&self) -> &BotId {
        &self.backend_id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::RemoteSsh
    }

    fn flavor(&self) -> BotFlavor {
        BotFlavor::SnowLuma
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let config = ctx
            .bot_config
            .as_ref()
            .ok_or_else(|| BotBackendError::ConfigNotFound(ctx.config.bot_id.clone()))?;
        let qq_id = config.bot.qq_id;
        let qq_id_str = qq_id.to_string();
        let bot_id = ctx.config.bot_id.clone();
        let start_mode = resolve_start_mode(config);

        self.daemon.ensure_running().await?;

        let paths = self.daemon.paths();
        if let Err(e) = render_native_snowluma_config_on_host(
            self.daemon.host.as_ref(),
            &bot_id,
            config,
            paths,
        )
        .await
        {
            self.daemon.release().await;
            return Err(e);
        }

        let host = self.daemon.host.as_ref();
        let layout = self.daemon.layout();

        let pid = match start_mode {
            SnowLumaStartMode::HotStart => {
                if let Some(pid) = remote_qq_running_pid(host, qq_id).await? {
                    pid
                } else {
                    self.daemon.release().await;
                    return Err(BotBackendError::InvalidConfig(format!(
                        "SnowLuma 热启动：远端未找到已登录 QQ {qq_id} 的进程（qq --no-sandbox -q {qq_id}）。\
                         请先在远端 Xvfb 上启动 QQ，或改为冷启动。"
                    )));
                }
            }
            SnowLumaStartMode::ColdStart => {
                // Ensure the shared remote QQ tree is in vanilla mode *before* we launch
                // a plain QQ for the SnowLuma daemon to inject into. This is serialized
                // per server_id via the coordinator so that a concurrent NC bot start on
                // the same host cannot race the package.json write.
                let install_base = HostPath::from_posix(format!("{}/Napcat", layout.home));
                if let Err(e) = self
                    .qq_entry_coordinator
                    .ensure_for_native(self.daemon.host.as_ref(), self.daemon.server_id(), &install_base)
                    .await
                {
                    self.daemon.release().await;
                    return Err(BotBackendError::InvalidConfig(format!(
                        "SnowLuma 冷启动前确保纯净 QQ 入口失败: {e}"
                    )));
                }

                match bot_cold_start(host, layout, &qq_id_str, &qq_id_str).await {
                    Ok(pid) => pid,
                    Err(e) => {
                        self.daemon.release().await;
                        return Err(e);
                    }
                }
            }
        };

        let endpoints = self
            .daemon
            .tunnel_endpoints()
            .await
            .ok_or_else(|| BotBackendError::Io("SnowLuma 隧道未建立".into()))?;
        let http = match inject_via_tunnel(&endpoints, pid).await {
            Ok(c) => c,
            Err(e) => {
                if start_mode.is_cold() {
                    let _ = bot_stop(host, paths, &qq_id_str).await;
                    // 失败时带上 bot 启动日志尾部,便于诊断 "QQ 进程立即退出 / 缺库 / ptrace / DISPLAY 问题"
                    if let Ok(tail) = read_remote_log_tail(host, &paths.log_bot_path(&qq_id_str), 40).await {
                        if !tail.trim().is_empty() {
                            return Err(BotBackendError::Io(format!(
                                "{e}\n--- 启动日志末尾 (bot log) ---\n{tail}"
                            )));
                        }
                    }
                }
                self.daemon.release().await;
                return Err(e);
            }
        };

        self.event_bus
            .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), pid));
        self.start_modes
            .lock()
            .await
            .insert(bot_id.clone(), start_mode);

        let poller_deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http,
            proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(pid)),
        };
        let poller = SnowLumaStatusPoller::spawn(bot_id.clone(), pid, poller_deps);
        self.pollers.lock().await.insert(bot_id.clone(), poller);

        self.event_bus
            .publish(DomainEvent::SnowLumaDockerEndpointsReady {
                bot_id: bot_id.clone(),
            });

        Ok(BotStatus::running(bot_id, pid, 0))
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        let qq_id_str = bot_id.as_str();
        let paths = self.daemon.paths();
        let start_mode = self
            .start_modes
            .lock()
            .await
            .remove(&bot_id)
            .unwrap_or(SnowLumaStartMode::ColdStart);

        if start_mode.is_cold() {
            let _ = bot_stop(self.daemon.host.as_ref(), paths, qq_id_str).await;
        }
        if let Some(poller) = self.pollers.lock().await.remove(&bot_id) {
            poller.dispose();
        }
        self.daemon.release().await;
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let qq_id = bot_id.as_str();
        let paths = self.daemon.paths();
        let start_mode = self
            .start_modes
            .lock()
            .await
            .get(&bot_id)
            .copied()
            .unwrap_or(SnowLumaStartMode::ColdStart);

        if start_mode.is_hot() {
            if let Ok(qq_id_u) = qq_id.parse::<u64>() {
                if let Some(pid) = remote_qq_running_pid(self.daemon.host.as_ref(), qq_id_u).await?
                {
                    return Ok(BotStatus::running(bot_id, pid, 0));
                }
            }
            return Ok(BotStatus::stopped(bot_id));
        }

        let status_path = paths.status_bot_path(qq_id);
        match self
            .daemon
            .host
            .read_file(&HostPath::from_posix(&status_path))
            .await
        {
            Ok(bytes) => {
                let status: Value = serde_json::from_slice(&bytes)
                    .map_err(|e| BotBackendError::Json(e.to_string()))?;
                let running = status.get("running").and_then(|v| v.as_bool()) == Some(true);
                if running {
                    let pid = status
                        .get("pid")
                        .and_then(|v| v.as_u64())
                        .map(|p| p as u32)
                        .unwrap_or(0);
                    return Ok(BotStatus::running(bot_id, pid, 0));
                }
                Ok(BotStatus::stopped(bot_id))
            }
            Err(_) => Ok(BotStatus::stopped(bot_id)),
        }
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        Err(BotBackendError::ConfigNotFound(bot_id))
    }

    async fn write_config(
        &self,
        _bot_id: BotId,
        _cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        Ok(())
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        let qq_id = bot_id.as_str();
        let path = self.daemon.paths().log_bot_path(qq_id);
        let bytes = match self
            .daemon
            .host
            .read_file(&HostPath::from_posix(&path))
            .await
        {
            Ok(b) => b,
            Err(_) => {
                return Ok(LogSnapshot {
                    lines: Vec::new(),
                    total_lines: 0,
                });
            }
        };
        let text = String::from_utf8_lossy(&bytes);
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let total = lines.len();
        if opts.lines > 0 && lines.len() > opts.lines {
            lines = lines.split_off(lines.len() - opts.lines);
        }
        Ok(LogSnapshot { lines, total_lines: total })
    }
}