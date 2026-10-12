//! 远端主机共享 SnowLuma daemon（图形栈 + node + 隧道）

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::BotId;
use ncd_domain::domain_event::DomainEvent;
use ncd_host::Host;
use ncd_traits::events::{BroadcastEventBus, EventBus};
use ncd_traits::runtime_backend::BotBackendError;
use tokio::sync::Mutex;

use super::layout::{RemoteSnowLumaLayout, SnowLumaRemotePaths, probe_remote_snowluma_layout};
use super::orchestrator::{
    daemon_start, daemon_stop, remote_daemon_already_ready, write_status_daemon_json,
};
use super::probe::{
    resolve_remote_novnc_port, resolve_remote_vnc_secret_from_x11vnc, resolve_remote_webui_port,
    resolve_remote_webui_secret_near_snowluma_dir, wait_remote_webui_ready,
};
use super::stack::restart_node_with_env;
use super::tunnel::{RemoteSnowLumaTunnelEndpoints, RemoteSnowLumaTunnelRegistry};
use crate::snowluma::daemon::DaemonState;

use super::config::{ensure_remote_daemon_prereqs, take_over_remote_webui_credentials};
use super::helpers::read_remote_file_trimmed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebuiPortPlan {
    /// 刚拉起的栈已经等过 WebUI，直接用返回口
    UseKnown(u16),
    /// node 刚重启，只等这一次
    WaitAfterRestart,
    /// 栈已在跑且没重启，快速解析现有口
    ResolveExisting,
}

pub(crate) fn webui_port_plan(
    stack_just_started: bool,
    node_restarted: bool,
    known_port: Option<u16>,
) -> WebuiPortPlan {
    // 冷拉栈已经在 ensure_stack_running 里等过 WebUI。接管/metrics 只是启动参数，
    // 不能当成「已有 node 被重启」再空等 90s。
    if stack_just_started {
        if let Some(port) = known_port {
            return WebuiPortPlan::UseKnown(port);
        }
        return WebuiPortPlan::WaitAfterRestart;
    }
    if node_restarted {
        return WebuiPortPlan::WaitAfterRestart;
    }
    WebuiPortPlan::ResolveExisting
}

#[derive(Default)]
struct DaemonLeases {
    anonymous: u32,
    bots: HashSet<BotId>,
}

impl DaemonLeases {
    fn count(&self) -> u32 {
        self.anonymous
            .saturating_add(u32::try_from(self.bots.len()).unwrap_or(u32::MAX))
    }

    fn acquire(&mut self, bot_id: Option<&BotId>) {
        if let Some(bot_id) = bot_id {
            self.bots.insert(bot_id.clone());
        } else {
            self.anonymous = self.anonymous.saturating_add(1);
        }
    }

    fn release(&mut self, bot_id: Option<&BotId>) -> bool {
        if let Some(bot_id) = bot_id {
            self.bots.remove(bot_id)
        } else if self.anonymous > 0 {
            self.anonymous -= 1;
            true
        } else {
            false
        }
    }

    fn has_other_bots(&self, bot_id: &BotId) -> bool {
        self.bots.iter().any(|active| active != bot_id)
    }
}

/// 单台远端主机共享的 SL daemon(单例图形栈 + node);多 Bot 共用,按 qq_id 分别启停 QQ
pub struct RemoteSnowLumaDaemon {
    pub(crate) host: Mutex<Arc<dyn Host>>,
    layout: RemoteSnowLumaLayout,
    server_id: String,
    leases: Mutex<DaemonLeases>,
    /// 同一 SSH 主机上多 Bot 并发 start 时,整段启栈(Xvfb/x11vnc/node)单飞,避免抢 5900 等端口
    stack_bootstrap: Mutex<()>,
    tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    event_bus: Arc<BroadcastEventBus>,
    tunnel_eps: Mutex<Option<RemoteSnowLumaTunnelEndpoints>>,
    tunnel_remote_ports: Mutex<Option<(u16, u16)>>,
    /// 仅在共享 node 真正启动或显式重载时更新，其他 Bot 启动保留现有探针。
    metrics_node_env: Mutex<Option<BTreeMap<String, String>>>,
}

impl RemoteSnowLumaDaemon {
    pub async fn new(
        server_id: String,
        host: Arc<dyn Host>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        event_bus: Arc<BroadcastEventBus>,
    ) -> Result<Self, BotBackendError> {
        let layout = probe_remote_snowluma_layout(host.as_ref()).await?;
        Ok(Self::from_layout(
            server_id, host, tunnels, event_bus, layout,
        ))
    }

    pub fn from_layout(
        server_id: String,
        host: Arc<dyn Host>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        event_bus: Arc<BroadcastEventBus>,
        layout: RemoteSnowLumaLayout,
    ) -> Self {
        Self {
            host: Mutex::new(host),
            layout,
            server_id,
            leases: Mutex::new(DaemonLeases::default()),
            stack_bootstrap: Mutex::new(()),
            tunnels,
            event_bus,
            tunnel_eps: Mutex::new(None),
            tunnel_remote_ports: Mutex::new(None),
            metrics_node_env: Mutex::new(None),
        }
    }

    pub async fn current_host(&self) -> Arc<dyn Host> {
        Arc::clone(&*self.host.lock().await)
    }

    /// SSH 重连后 ServerManager 会换新 Host，并把旧 session 毒死。
    /// daemon / 隧道若仍握旧句柄，accept 循环会每 2s 打一次 poisoned。
    pub async fn replace_host(&self, host: Arc<dyn Host>) {
        let _stack_guard = self.stack_bootstrap.lock().await;
        {
            let mut guard = self.host.lock().await;
            if Arc::ptr_eq(&*guard, &host) {
                return;
            }
            *guard = host;
        }
        self.tunnels.drop_server(&self.server_id).await;
        *self.tunnel_eps.lock().await = None;
        *self.tunnel_remote_ports.lock().await = None;
        tracing::info!(
            target: "ncd_backend_snowluma::remote",
            server_id = %self.server_id,
            "SSH 重连后已替换 SnowLuma daemon host，并拆掉旧隧道"
        );
    }

    /// 应用 metrics env 到共享 node：与当前已应用 env 不同则重启 node 并等 WebUI。
    /// 重启后必须按新端口重开隧道，否则 inject 会打到已死的旧口。
    pub async fn apply_metrics_node_env(
        &self,
        env: Option<BTreeMap<String, String>>,
    ) -> Result<(), BotBackendError> {
        let _stack_guard = self.stack_bootstrap.lock().await;
        {
            let guard = self.metrics_node_env.lock().await;
            if *guard == env {
                return Ok(());
            }
        }
        let host = self.current_host().await;
        let layout = &self.layout;
        restart_node_with_env(host.as_ref(), layout, env.as_ref()).await?;
        let webui_port =
            wait_remote_webui_ready(host.as_ref(), &layout.paths, Duration::from_secs(60)).await?;
        self.open_tunnels(webui_port, None).await?;
        *self.metrics_node_env.lock().await = env;
        Ok(())
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

    pub async fn host(&self) -> Arc<dyn Host> {
        self.current_host().await
    }

    pub fn layout(&self) -> &RemoteSnowLumaLayout {
        &self.layout
    }

    pub async fn ensure_running(&self) -> Result<(), BotBackendError> {
        self.ensure_running_ex(false, None, None).await
    }

    /// 接管刚覆盖 webui.json 之后：已在跑的 node 仍持旧哈希，必须重启再开隧道。
    pub async fn ensure_running_after_webui_takeover(&self) -> Result<(), BotBackendError> {
        self.ensure_running_ex(true, None, None).await
    }

    pub async fn ensure_running_with_metrics(
        &self,
        restart_node: bool,
        metrics_env: Option<BTreeMap<String, String>>,
        webui_password: Option<String>,
    ) -> Result<(), BotBackendError> {
        self.ensure_running_ex(restart_node, metrics_env, webui_password)
            .await
    }

    pub async fn has_other_bot_leases(&self, bot_id: &BotId) -> bool {
        self.leases.lock().await.has_other_bots(bot_id)
    }

    pub async fn ensure_running_for_bot(
        &self,
        bot_id: &BotId,
        metrics_env: Option<BTreeMap<String, String>>,
        takeover_requested: bool,
        password_override: Option<&str>,
    ) -> Result<(), BotBackendError> {
        let _stack_guard = self.stack_bootstrap.lock().await;
        let takeover = takeover_requested && !self.has_other_bot_leases(bot_id).await;
        let webui_password = if takeover {
            let host = self.current_host().await;
            Some(
                take_over_remote_webui_credentials(
                    host.as_ref(),
                    &self.layout.paths,
                    password_override,
                )
                .await?,
            )
        } else {
            None
        };
        self.ensure_running_locked(takeover, metrics_env, webui_password, Some(bot_id))
            .await
    }

    async fn ensure_running_ex(
        &self,
        restart_node: bool,
        metrics_env: Option<BTreeMap<String, String>>,
        webui_password: Option<String>,
    ) -> Result<(), BotBackendError> {
        let _stack_guard = self.stack_bootstrap.lock().await;
        self.ensure_running_locked(restart_node, metrics_env, webui_password, None)
            .await
    }

    async fn ensure_running_locked(
        &self,
        restart_node: bool,
        metrics_env: Option<BTreeMap<String, String>>,
        webui_password: Option<String>,
        bot_id: Option<&BotId>,
    ) -> Result<(), BotBackendError> {
        let host = self.current_host().await;
        let creds = ensure_remote_daemon_prereqs(
            host.as_ref(),
            &self.layout.home,
            &self.layout.paths,
            &self.layout.qq_bin,
        )
        .await?;

        let stack_up = remote_daemon_already_ready(host.as_ref(), &self.layout.paths).await?;
        let restart_node = restart_node || (creds.wrote && stack_up);
        if !stack_up || restart_node {
            self.event_bus
                .publish(DomainEvent::snowluma_daemon_state_changed(
                    DaemonState::Starting,
                    self.leases.lock().await.count(),
                    None,
                    Some(self.server_id.clone()),
                ));
        } else if *self.metrics_node_env.lock().await != metrics_env {
            // 指标输出路径按 Bot 不同；为它重启会断掉其他 QQ 的共享 SL 通道。
            tracing::info!(
                target: "ncd_backend_snowluma::remote",
                server_id = %self.server_id,
                "复用共享 SnowLuma node，保留现有指标环境，新环境在共享 node 重载时应用"
            );
        }
        let known_port = if !stack_up {
            Some(daemon_start(host.as_ref(), &self.layout, metrics_env.as_ref()).await?)
        } else {
            if restart_node {
                tracing::info!(
                    target: "ncd_backend_snowluma::remote",
                    server_id = %self.server_id,
                    "接管 WebUI 密码后重启已运行的 SnowLuma node，使新哈希生效"
                );
                restart_node_with_env(host.as_ref(), &self.layout, metrics_env.as_ref()).await?;
            }
            None
        };
        let node_restarted = stack_up && restart_node;
        let webui_port = match webui_port_plan(!stack_up, node_restarted, known_port) {
            WebuiPortPlan::UseKnown(port) => port,
            WebuiPortPlan::WaitAfterRestart => {
                wait_remote_webui_ready(host.as_ref(), &self.layout.paths, Duration::from_secs(90))
                    .await?
            }
            WebuiPortPlan::ResolveExisting => {
                resolve_remote_webui_port(host.as_ref(), &self.layout.paths).await?
            }
        };
        write_status_daemon_json(host.as_ref(), &self.layout.paths, true, true, webui_port).await?;

        let tunnel_password = webui_password
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                let t = creds.plaintext.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(creds.plaintext.clone())
                }
            });
        self.open_tunnels(webui_port, tunnel_password.as_deref())
            .await?;
        if !stack_up || restart_node {
            *self.metrics_node_env.lock().await = metrics_env;
        }

        let mut guard = self.leases.lock().await;
        guard.acquire(bot_id);
        let rc = guard.count();
        drop(guard);

        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Ready,
                rc,
                None,
                Some(self.server_id.clone()),
            ));
        Ok(())
    }

    pub async fn release(&self) {
        self.release_lease(None).await;
    }

    pub async fn release_bot(&self, bot_id: &BotId) {
        self.release_lease(Some(bot_id)).await;
    }

    async fn release_lease(&self, bot_id: Option<&BotId>) {
        let _stack_guard = self.stack_bootstrap.lock().await;
        let mut guard = self.leases.lock().await;
        if !guard.release(bot_id) {
            return;
        }
        let rc = guard.count();
        let stop_daemon = rc == 0;
        drop(guard);

        if stop_daemon {
            self.tunnels.release(&self.server_id).await;
            let host = self.current_host().await;
            let _ = daemon_stop(host.as_ref(), &self.layout.paths).await;
            *self.tunnel_eps.lock().await = None;
            *self.tunnel_remote_ports.lock().await = None;
            self.event_bus
                .publish(DomainEvent::snowluma_daemon_state_changed(
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
        let _stack_guard = self.stack_bootstrap.lock().await;
        if self.tunnel_eps.lock().await.take().is_some() {
            self.tunnels.release(&self.server_id).await;
        }
        *self.tunnel_remote_ports.lock().await = None;
    }

    /// 导入/冷启动 reconcile：只接管已在跑的 node，不新拉图形栈、不写死 WebUI 端口。
    /// node 不在则跳过隧道（QQ 进程仍可由调用方标成运行中）。
    pub async fn ensure_running_for_reconcile(&self) -> Result<(), BotBackendError> {
        self.ensure_running_for_reconcile_lease(None).await
    }

    pub async fn ensure_running_for_reconcile_bot(
        &self,
        bot_id: &BotId,
    ) -> Result<(), BotBackendError> {
        self.ensure_running_for_reconcile_lease(Some(bot_id)).await
    }

    async fn ensure_running_for_reconcile_lease(
        &self,
        bot_id: Option<&BotId>,
    ) -> Result<(), BotBackendError> {
        let _stack_guard = self.stack_bootstrap.lock().await;
        let host = self.current_host().await;
        if !remote_daemon_already_ready(host.as_ref(), &self.layout.paths).await? {
            tracing::info!(
                target: "ncd_runtime::remote_snowluma",
                server_id = %self.server_id,
                "reconcile: 远端未发现已运行的 SnowLuma node，跳过 WebUI 隧道"
            );
            return Ok(());
        }

        let webui_port = resolve_remote_webui_port(host.as_ref(), &self.layout.paths).await?;
        let _ = write_status_daemon_json(host.as_ref(), &self.layout.paths, true, true, webui_port)
            .await;
        self.open_tunnels(webui_port, None).await?;

        let mut guard = self.leases.lock().await;
        if bot_id.is_some() || guard.count() == 0 {
            guard.acquire(bot_id);
        }
        let rc = guard.count();
        drop(guard);

        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Ready,
                rc,
                None,
                Some(self.server_id.clone()),
            ));
        Ok(())
    }

    async fn open_tunnels(
        &self,
        webui_port: u16,
        webui_password: Option<&str>,
    ) -> Result<(), BotBackendError> {
        let host = self.current_host().await;
        let webui_plain = match webui_password.map(str::trim).filter(|s| !s.is_empty()) {
            Some(pwd) => pwd.to_string(),
            None => {
                let primary =
                    read_remote_file_trimmed(host.as_ref(), &self.layout.paths.webui_secret)
                        .await
                        .unwrap_or_default();
                if primary.is_empty() {
                    // 外来安装（systemd 自启等）常把 webui.secret 放在 snowluma_dir 父目录，
                    // 不在 Desktop 假设的 {workspace}/webui.secret 下；以实际文件为准回退。
                    let fallback = resolve_remote_webui_secret_near_snowluma_dir(
                        host.as_ref(),
                        &self.layout.paths.snowluma_dir,
                    )
                    .await?;
                    match fallback {
                        Some(p) => {
                            tracing::info!(
                                target: "ncd_backend_snowluma::remote",
                                server_id = %self.server_id,
                                fallback = %p,
                                "webui.secret 不在预期路径，已回退到 snowluma_dir 父目录"
                            );
                            read_remote_file_trimmed(host.as_ref(), &p)
                                .await
                                .unwrap_or_default()
                        }
                        None => String::new(),
                    }
                } else {
                    primary
                }
            }
        };
        let mut vnc_plain = read_remote_file_trimmed(host.as_ref(), &self.layout.paths.vnc_secret)
            .await
            .unwrap_or_default();
        if vnc_plain.is_empty() {
            // x11vnc -passwdfile 指向的才是实际 VNC 密码文件；外来启动脚本可能把它放在
            // 任意目录，从进程 cmdline 读最可靠。
            if let Some(actual) = resolve_remote_vnc_secret_from_x11vnc(host.as_ref()).await? {
                tracing::info!(
                    target: "ncd_backend_snowluma::remote",
                    server_id = %self.server_id,
                    fallback = %actual,
                    "vnc.secret 不在预期路径，已从 x11vnc -passwdfile 回退"
                );
                vnc_plain = read_remote_file_trimmed(host.as_ref(), &actual)
                    .await
                    .unwrap_or_default();
            }
        }

        // noVNC 口动态探测：外来图形栈的 websockify 常不在默认 6081
        let novnc_port = resolve_remote_novnc_port(host.as_ref()).await;

        let previous_ports = *self.tunnel_remote_ports.lock().await;
        let previous_eps = self.tunnel_eps.lock().await.clone();
        if let Some(previous) = previous_eps.as_ref() {
            if previous_ports == Some((webui_port, novnc_port))
                && (webui_plain.is_empty() || previous.webui_password == webui_plain)
                && (vnc_plain.is_empty() || previous.vnc_password == vnc_plain)
            {
                return Ok(());
            }
        }

        let opened = self
            .tunnels
            .acquire(
                &self.server_id,
                host.as_ref(),
                webui_plain,
                vnc_plain,
                webui_port,
                novnc_port,
            )
            .await;
        let eps = match opened {
            Ok(eps) => eps,
            Err(error) => {
                *self.tunnel_eps.lock().await = None;
                *self.tunnel_remote_ports.lock().await = None;
                return Err(BotBackendError::Io(format!("SnowLuma SSH 隧道: {error}")));
            }
        };
        // daemon 持有一个 bundle 引用；端口变动时 registry 已替换旧 bundle。
        if previous_eps.is_some() && previous_ports == Some((webui_port, novnc_port)) {
            self.tunnels.release(&self.server_id).await;
        }
        *self.tunnel_eps.lock().await = Some(eps);
        *self.tunnel_remote_ports.lock().await = Some((webui_port, novnc_port));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webui_port_plan_uses_known_port_when_stack_just_started() {
        assert_eq!(
            webui_port_plan(true, false, Some(5103)),
            WebuiPortPlan::UseKnown(5103)
        );
    }

    #[test]
    fn webui_port_plan_waits_only_after_restart() {
        assert_eq!(
            webui_port_plan(false, true, None),
            WebuiPortPlan::WaitAfterRestart
        );
    }

    #[test]
    fn webui_port_plan_takeover_on_cold_start_does_not_wait_again() {
        assert_eq!(
            webui_port_plan(true, true, Some(5099)),
            WebuiPortPlan::UseKnown(5099)
        );
    }

    #[test]
    fn webui_port_plan_resolves_existing_when_stack_already_up() {
        assert_eq!(
            webui_port_plan(false, false, None),
            WebuiPortPlan::ResolveExisting
        );
    }

    #[test]
    fn reconciled_bots_each_hold_one_shared_daemon_lease() {
        let mut leases = DaemonLeases::default();
        for id in ["bot-a", "bot-b", "bot-c"] {
            leases.acquire(Some(&BotId::new(id)));
        }
        assert_eq!(leases.count(), 3);
    }

    #[test]
    fn repeated_reconcile_does_not_duplicate_a_bot_lease() {
        let mut leases = DaemonLeases::default();
        let bot = BotId::new("bot-a");
        for _ in 0..3 {
            leases.acquire(Some(&bot));
        }
        assert_eq!(leases.count(), 1);
    }

    #[test]
    fn stopping_one_reconciled_bot_keeps_the_other_two_leases() {
        let mut leases = DaemonLeases::default();
        let stopped = BotId::new("bot-a");
        leases.acquire(Some(&stopped));
        leases.acquire(Some(&BotId::new("bot-b")));
        leases.acquire(Some(&BotId::new("bot-c")));
        assert!(leases.release(Some(&stopped)));
        assert_eq!(leases.count(), 2);
        assert!(leases.has_other_bots(&stopped));
    }

    #[test]
    fn failed_start_returns_only_its_own_lease() {
        let mut leases = DaemonLeases::default();
        let existing = BotId::new("existing");
        let failed = BotId::new("failed");
        leases.acquire(Some(&existing));
        leases.acquire(Some(&failed));
        assert!(leases.release(Some(&failed)));
        assert!(!leases.release(Some(&failed)));
        assert_eq!(leases.count(), 1);
        assert!(leases.bots.contains(&existing));
    }

    #[test]
    fn bot_cleanup_does_not_consume_anonymous_preflight_leases() {
        let mut leases = DaemonLeases::default();
        let bot = BotId::new("bot-a");
        leases.acquire(None);
        leases.acquire(Some(&bot));
        assert!(leases.release(Some(&bot)));
        assert_eq!(leases.count(), 1);
        assert!(leases.release(None));
        assert_eq!(leases.count(), 0);
    }
}
