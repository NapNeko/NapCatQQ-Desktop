//! 远端主机共享 SnowLuma daemon（图形栈 + node + 隧道）

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::BotId;
use ncd_domain::domain_event::DomainEvent;
use ncd_host::{Host, HostCommand};
use ncd_traits::events::{BroadcastEventBus, EventBus};
use ncd_traits::runtime_backend::BotBackendError;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::layout::{RemoteSnowLumaLayout, SnowLumaRemotePaths, probe_remote_snowluma_layout};
use super::orchestrator::{daemon_stop, remote_daemon_already_ready, write_status_daemon_json};
use super::probe::{
    ManagedNodeIdentity, managed_node_identity, remote_qq_main_running, resolve_remote_novnc_port,
    resolve_remote_vnc_secret_from_x11vnc, wait_remote_webui_ready,
};
use super::stack::{recover_node_for_reconcile, restart_node_with_env};
use super::tunnel::{RemoteSnowLumaTunnelEndpoints, RemoteSnowLumaTunnelRegistry};
use crate::snowluma::daemon::DaemonState;

use super::config::{ensure_remote_daemon_prereqs_cancellable, take_over_remote_webui_credentials};
#[path = "novnc_target.rs"]
mod novnc_target;

#[path = "webui.rs"]
mod webui;

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
    pending_bots: HashSet<BotId>,
}

impl DaemonLeases {
    fn count(&self) -> u32 {
        self.anonymous
            .saturating_add(u32::try_from(self.bots.len()).unwrap_or(u32::MAX))
    }

    fn acquire(&mut self, bot_id: Option<&BotId>) {
        if let Some(bot_id) = bot_id {
            self.bots.insert(bot_id.clone());
            self.pending_bots.remove(bot_id);
        } else {
            self.anonymous = self.anonymous.saturating_add(1);
        }
    }

    fn acquire_start(&mut self, bot_id: Option<&BotId>) {
        if let Some(bot_id) = bot_id {
            if self.bots.insert(bot_id.clone()) {
                self.pending_bots.insert(bot_id.clone());
            }
        } else {
            self.acquire(None);
        }
    }

    fn discard_pending(&mut self) {
        for bot_id in self.pending_bots.drain() {
            self.bots.remove(&bot_id);
        }
    }

    fn release(&mut self, bot_id: Option<&BotId>) -> bool {
        if let Some(bot_id) = bot_id {
            self.pending_bots.remove(bot_id);
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

#[derive(Clone)]
struct ReadyNode {
    identity: ManagedNodeIdentity,
    startup_validated: bool,
}

/// 单台远端主机共享的 SL daemon(单例图形栈 + node);多 Bot 共用,按 qq_id 分别启停 QQ
pub struct RemoteSnowLumaDaemon {
    pub(crate) host: Mutex<Arc<dyn Host>>,
    layout: RemoteSnowLumaLayout,
    server_id: String,
    leases: Mutex<DaemonLeases>,
    /// 同一 SSH 主机上多 Bot 并发 start 时,整段启栈(Xvfb/x11vnc/node)单飞,避免抢 5900 等端口
    stack_bootstrap: Mutex<()>,
    session: Mutex<CancellationToken>,
    tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    event_bus: Arc<BroadcastEventBus>,
    tunnel_eps: Mutex<Option<RemoteSnowLumaTunnelEndpoints>>,
    tunnel_remote_ports: Mutex<Option<(u16, u16)>>,
    tunnel_remote_webui_host: Mutex<Option<String>>,
    tunnel_secret_paths: Mutex<Vec<String>>,
    webui_tunnel: Mutex<Option<webui::WebuiTunnel>>,
    webui_endpoint: Mutex<Option<super::probe::ResolvedWebuiEndpoint>>,
    /// 仅在共享 node 真正启动或显式重载时更新，其他 Bot 启动保留现有探针。
    metrics_node_env: Mutex<Option<BTreeMap<String, String>>>,
    /// 成功状态只属于当前连接代；接管状态不能替代完整启动预检。
    ready_node_identity: Mutex<Option<ReadyNode>>,
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
            session: Mutex::new(CancellationToken::new()),
            tunnels,
            event_bus,
            tunnel_eps: Mutex::new(None),
            tunnel_remote_ports: Mutex::new(None),
            tunnel_remote_webui_host: Mutex::new(None),
            tunnel_secret_paths: Mutex::new(Vec::new()),
            webui_tunnel: Mutex::new(None),
            webui_endpoint: Mutex::new(None),
            metrics_node_env: Mutex::new(None),
            ready_node_identity: Mutex::new(None),
        }
    }

    pub async fn current_host(&self) -> Arc<dyn Host> {
        Arc::clone(&*self.host.lock().await)
    }

    async fn resolve_webui_port(&self, host: &dyn Host) -> Result<u16, BotBackendError> {
        let endpoint = super::probe::resolve_remote_webui_endpoint(host, &self.layout).await?;
        let port = endpoint.port;
        *self.webui_endpoint.lock().await = Some(endpoint);
        Ok(port)
    }

    pub(super) async fn webui_process_pid(
        &self,
        pid: u32,
        start_time: &str,
    ) -> Result<u32, BotBackendError> {
        let host = self.current_host().await;
        if self.webui_endpoint.lock().await.is_none() {
            self.resolve_webui_port(host.as_ref()).await?;
        }
        let endpoint = self
            .webui_endpoint
            .lock()
            .await
            .clone()
            .ok_or_else(|| BotBackendError::Io("尚未确认 SnowLuma WebUI 所属 Node".into()))?;
        super::webui_process_pid::resolve(
            host.as_ref(),
            &self.layout,
            &endpoint.node,
            pid,
            start_time,
        )
        .await
    }

    pub(super) async fn webui_uses_pid_namespace(&self) -> bool {
        self.webui_endpoint
            .lock()
            .await
            .as_ref()
            .is_some_and(|endpoint| endpoint.node.ns_pids.len() > 1)
    }

    pub(crate) async fn in_current_session<T>(
        &self,
        action: impl Future<Output = Result<T, BotBackendError>>,
    ) -> Result<T, BotBackendError> {
        let session = self.session.lock().await.clone();
        tokio::select! {
            biased;
            _ = session.cancelled() => Err(BotBackendError::Io(
                "远端 SnowLuma 连接已失效，请重试".into(),
            )),
            result = action => result,
        }
    }

    /// SSH 重连后 ServerManager 会换新 Host，并把旧 session 毒死。
    /// daemon / 隧道若仍握旧句柄，accept 循环会每 2s 打一次 poisoned。
    pub async fn replace_host_if_current(
        &self,
        host: Arc<dyn Host>,
        is_current: impl Future<Output = bool> + Send,
    ) -> Result<(), BotBackendError> {
        let mut session = self.session.lock().await;
        // Validate under the same gate that publishes a replacement. A delayed old
        // router request must never cancel a newer connection that already won.
        if !is_current.await {
            return Err(BotBackendError::Io(
                "远端 SnowLuma 连接已更新，请重试".into(),
            ));
        }
        if Arc::ptr_eq(&*self.host.lock().await, &host) {
            return Ok(());
        }
        session.cancel();
        let _stack_guard = self.stack_bootstrap.lock().await;
        *self.host.lock().await = host;
        self.tunnels.drop_server(&self.server_id).await;
        *self.tunnel_eps.lock().await = None;
        *self.tunnel_remote_ports.lock().await = None;
        *self.tunnel_remote_webui_host.lock().await = None;
        self.tunnel_secret_paths.lock().await.clear();
        self.webui_tunnel.lock().await.take();
        self.webui_endpoint.lock().await.take();
        *self.ready_node_identity.lock().await = None;
        self.leases.lock().await.discard_pending();
        *session = CancellationToken::new();
        tracing::info!(
            target: "ncd_backend_snowluma::remote",
            server_id = %self.server_id,
            "SSH 重连后已替换 SnowLuma daemon host，并拆掉旧隧道"
        );
        Ok(())
    }

    #[cfg(test)]
    async fn replace_host(&self, host: Arc<dyn Host>) {
        self.replace_host_if_current(host, async { true })
            .await
            .unwrap();
    }

    /// 记住共享 node 下次启动的探针环境；已有 node 只靠注册表热更新。
    /// 旧 node 尚未预加载时也不主动重启，以免中断其它账号的通信。
    pub async fn apply_metrics_node_env(
        &self,
        env: Option<BTreeMap<String, String>>,
    ) -> Result<(), BotBackendError> {
        let _stack_guard = self.stack_bootstrap.lock().await;
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

    pub(crate) async fn on_current_host<T>(
        &self,
        expected: &Arc<dyn Host>,
        action: impl Future<Output = Result<T, BotBackendError>>,
    ) -> Result<Option<T>, BotBackendError> {
        let session = self.session.lock().await;
        if session.is_cancelled() || !Arc::ptr_eq(expected, &self.current_host().await) {
            return Ok(None);
        }
        action.await.map(Some)
    }

    pub(crate) async fn abandon_start_on_host(&self, expected: &Arc<dyn Host>, bot_id: &BotId) {
        let session = self.session.lock().await;
        if !session.is_cancelled() && Arc::ptr_eq(expected, &self.current_host().await) {
            self.leases.lock().await.release(Some(bot_id));
        }
    }

    pub async fn ensure_running(&self) -> Result<(), BotBackendError> {
        self.ensure_running_ex(false, None, None).await
    }

    pub async fn has_other_bot_leases(&self, bot_id: &BotId) -> bool {
        self.leases.lock().await.has_other_bots(bot_id)
    }

    pub(crate) async fn confirm_bot_started(&self, bot_id: &BotId) {
        self.leases.lock().await.pending_bots.remove(bot_id);
    }

    pub async fn ensure_running_for_bot(
        &self,
        bot_id: &BotId,
        metrics_env: Option<BTreeMap<String, String>>,
        takeover_requested: bool,
        password_override: Option<&str>,
    ) -> Result<(), BotBackendError> {
        self.ensure_running_for_bot_cancellable(
            bot_id,
            metrics_env,
            takeover_requested,
            password_override,
            &CancellationToken::new(),
        )
        .await
    }

    pub async fn ensure_running_for_bot_cancellable(
        &self,
        bot_id: &BotId,
        metrics_env: Option<BTreeMap<String, String>>,
        takeover_requested: bool,
        password_override: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<(), BotBackendError> {
        self.in_current_session(async {
            let _stack_guard = super::startup_cancel::read_only(Some(cancel), async {
                Ok(self.stack_bootstrap.lock().await)
            })
            .await?;
            super::startup_cancel::check(Some(cancel))?;
            let result = async {
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
                // A completed credential pair must be followed by its node replacement before cancellation.
                self.ensure_running_locked(
                    takeover,
                    metrics_env,
                    webui_password,
                    Some(bot_id),
                    Some(cancel),
                )
                .await
            }
            .await;
            if result.is_err() {
                if cancel.is_cancelled() {
                    *self.ready_node_identity.lock().await = None;
                } else {
                    self.clear_local_startup().await;
                }
            }
            result
        })
        .await
    }

    async fn ensure_running_ex(
        &self,
        restart_node: bool,
        metrics_env: Option<BTreeMap<String, String>>,
        webui_password: Option<String>,
    ) -> Result<(), BotBackendError> {
        self.in_current_session(async {
            let _stack_guard = self.stack_bootstrap.lock().await;
            let result = self
                .ensure_running_locked(restart_node, metrics_env, webui_password, None, None)
                .await;
            if result.is_err() {
                self.clear_local_startup().await;
            }
            result
        })
        .await
    }

    async fn ensure_running_locked(
        &self,
        restart_node: bool,
        metrics_env: Option<BTreeMap<String, String>>,
        webui_password: Option<String>,
        bot_id: Option<&BotId>,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), BotBackendError> {
        if !restart_node {
            super::startup_cancel::check(cancel)?;
        }
        let host = self.current_host().await;
        if !restart_node
            && webui_password.is_none()
            && self.reuse_running_node(host.as_ref(), bot_id, true).await
        {
            if metrics_env.is_some() {
                *self.metrics_node_env.lock().await = metrics_env;
            }
            super::startup_cancel::check(cancel)?;
            return Ok(());
        }
        // A failed repair must never leave a previous success eligible for reuse.
        *self.ready_node_identity.lock().await = None;
        let creds = ensure_remote_daemon_prereqs_cancellable(
            host.as_ref(),
            &self.layout.home,
            &self.layout.paths,
            &self.layout.qq_bin,
            if restart_node { None } else { cancel },
        )
        .await?;
        if !restart_node && !creds.wrote {
            super::startup_cancel::check(cancel)?;
        }

        let stack_up = if restart_node || creds.wrote {
            remote_daemon_already_ready(host.as_ref(), &self.layout).await?
        } else {
            super::startup_cancel::read_only(
                cancel,
                remote_daemon_already_ready(host.as_ref(), &self.layout),
            )
            .await?
        };
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
            // 新环境按主机固定；旧 node 未预加载时留到完整服务下次启动。
            tracing::info!(
                target: "ncd_backend_snowluma::remote",
                server_id = %self.server_id,
                "复用共享 SnowLuma node；账号注册热更新，探针环境在共享 node 下次启动时应用"
            );
        }
        let known_port = if !stack_up {
            // No Node exists to reload; a completed credential pair needs no spawn.
            super::startup_cancel::check(cancel)?;
            Some(
                super::orchestrator::daemon_start_cancellable(
                    host.as_ref(),
                    &self.layout,
                    metrics_env.as_ref(),
                    cancel,
                )
                .await?,
            )
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
        super::startup_cancel::check(cancel)?;
        let node_restarted = stack_up && restart_node;
        let webui_port = match webui_port_plan(!stack_up, node_restarted, known_port) {
            WebuiPortPlan::UseKnown(_) => {
                super::startup_cancel::read_only(cancel, self.resolve_webui_port(host.as_ref()))
                    .await?
            }
            WebuiPortPlan::WaitAfterRestart => {
                super::startup_cancel::read_only(
                    cancel,
                    wait_remote_webui_ready(
                        host.as_ref(),
                        &self.layout.paths,
                        Duration::from_secs(90),
                    ),
                )
                .await?;
                super::startup_cancel::read_only(cancel, self.resolve_webui_port(host.as_ref()))
                    .await?
            }
            WebuiPortPlan::ResolveExisting => {
                super::startup_cancel::read_only(cancel, self.resolve_webui_port(host.as_ref()))
                    .await?
            }
        };
        super::startup_cancel::check(cancel)?;
        write_status_daemon_json(host.as_ref(), &self.layout.paths, true, true, webui_port).await?;
        super::startup_cancel::check(cancel)?;

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
        super::startup_cancel::check(cancel)?;
        if metrics_env.is_some() || !stack_up || restart_node {
            *self.metrics_node_env.lock().await = metrics_env;
        }

        let secret_paths = self.tunnel_secret_paths.lock().await.clone();
        let identity =
            managed_node_identity(host.as_ref(), &self.layout.paths, None, &secret_paths)
                .await
                .map(|identity| ReadyNode {
                    identity,
                    startup_validated: true,
                });
        *self.ready_node_identity.lock().await = identity;

        let mut guard = self.leases.lock().await;
        super::startup_cancel::check(cancel)?;
        guard.acquire_start(bot_id);
        let rc = guard.count();
        drop(guard);
        super::startup_cancel::check(cancel)?;

        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Ready,
                rc,
                None,
                Some(self.server_id.clone()),
            ));
        Ok(())
    }

    /// Batch starts share configuration, credentials and tunnels. Rechecking all of
    /// them (including scrypt) under the bootstrap lock adds one full SSH sequence
    /// per account. A single process-identity probe validates this existing session.
    async fn reuse_running_node(
        &self,
        host: &dyn Host,
        bot_id: Option<&BotId>,
        require_startup_validation: bool,
    ) -> bool {
        let Some(expected) = self.ready_node_identity.lock().await.clone() else {
            return false;
        };
        let secret_paths = self.tunnel_secret_paths.lock().await.clone();
        if (require_startup_validation && !expected.startup_validated)
            || self.leases.lock().await.count() == 0
            || self.tunnel_eps.lock().await.is_none()
            || self
                .tunnels
                .endpoints_for_server(&self.server_id)
                .await
                .is_none()
            || managed_node_identity(
                host,
                &self.layout.paths,
                Some(expected.identity.pid),
                &secret_paths,
            )
            .await
            .as_ref()
                != Some(&expected.identity)
        {
            return false;
        }
        let mut leases = self.leases.lock().await;
        if require_startup_validation {
            leases.acquire_start(bot_id);
        } else if bot_id.is_some() {
            leases.acquire(bot_id);
        }
        let count = leases.count();
        drop(leases);
        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Ready,
                count,
                None,
                Some(self.server_id.clone()),
            ));
        true
    }

    pub async fn release(&self) {
        self.release_lease(None).await;
    }

    pub async fn release_bot(&self, bot_id: &BotId) {
        self.release_lease(Some(bot_id)).await;
    }

    async fn release_lease(&self, bot_id: Option<&BotId>) {
        let _ = self
            .in_current_session(async {
                let _stack_guard = self.stack_bootstrap.lock().await;
                let mut guard = self.leases.lock().await;
                if !guard.release(bot_id) {
                    return Ok(());
                }
                let rc = guard.count();
                let stop_daemon = rc == 0;
                drop(guard);

                if stop_daemon {
                    self.clear_local_startup().await;
                    let host = self.current_host().await;
                    // 冷启动接管尚未完成时，本地租约数不包含所有远端 QQ。
                    match remote_qq_main_running(host.as_ref(), &self.layout.qq_bin).await {
                        Ok(false) => {}
                        Ok(true) => {
                            tracing::info!(server_id = %self.server_id,
                        "远端仍有 QQ 主进程，保留共享 SnowLuma daemon");
                            return Ok(());
                        }
                        Err(err) => {
                            tracing::warn!(server_id = %self.server_id, %err,
                        "无法确认远端 QQ 已全部退出，保留共享 SnowLuma daemon");
                            return Ok(());
                        }
                    }
                    let _ = daemon_stop(host.as_ref(), &self.layout.paths).await;
                    self.event_bus
                        .publish(DomainEvent::snowluma_daemon_state_changed(
                            DaemonState::Stopped,
                            0,
                            None,
                            Some(self.server_id.clone()),
                        ));
                }
                Ok(())
            })
            .await;
    }

    pub async fn tunnel_endpoints(&self) -> Option<RemoteSnowLumaTunnelEndpoints> {
        let mut endpoints = self.tunnel_eps.lock().await.clone();
        if let Some(endpoint) = endpoints.as_mut() {
            if let Some(webui) = self.webui_tunnel.lock().await.as_ref() {
                endpoint.webui_local_port = webui.handle.local_port();
                endpoint.webui_password = webui.password.clone();
            }
        }
        endpoints
    }

    /// 显式打开 QQ 界面时补齐缺失的 VNC 服务，保留正在运行的 X server / QQ。
    pub async fn ensure_novnc_ready(
        &self,
        qq_pid: u32,
        qq_start_time: &str,
        qq_id: u64,
    ) -> Result<RemoteSnowLumaTunnelEndpoints, BotBackendError> {
        if qq_pid == 0
            || qq_id == 0
            || qq_start_time.is_empty()
            || !qq_start_time.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(BotBackendError::InvalidConfig(
                "尚未确认目标 QQ 进程身份，请刷新 Bot 运行状态".into(),
            ));
        }
        let started = std::time::Instant::now();
        let mut stage = "等待图形会话";
        let prepare = self.in_current_session(async {
            let lock_started = std::time::Instant::now();
            let _stack_guard = self.stack_bootstrap.lock().await;
            tracing::info!(
                target: "ncd_backend_snowluma::remote",
                elapsed_ms = lock_started.elapsed().as_millis() as u64,
                "noVNC 图形会话等待耗时"
            );
            let host = self.current_host().await;
            let paths = &self.layout.paths;
            stage = "验证 QQ 图形会话";
            let metadata_started = std::time::Instant::now();
            let target = novnc_target::inspect(
                host.as_ref(),
                &self.layout.qq_bin,
                qq_pid,
                qq_start_time,
                qq_id,
            )
            .await;
            tracing::info!(
                target: "ncd_backend_snowluma::remote",
                elapsed_ms = metadata_started.elapsed().as_millis() as u64,
                "noVNC QQ 图形会话验证耗时"
            );
            let target = target?;
            let target = serde_json::to_string(&target)
                .map_err(|_| BotBackendError::Io("无法传递 QQ 图形会话身份".into()))?;
            stage = "恢复 noVNC 服务";
            let recovery_started = std::time::Instant::now();
            let command = HostCommand::new("python3")
                .arg("-c")
                .arg(include_str!("../../resources/novnc-recovery.py"))
                .arg(&paths.workspace_dir)
                .arg(&paths.runtime_dir)
                .arg(&paths.log_dir)
                .arg(&paths.vnc_secret)
                .arg(&self.layout.qq_bin)
                .arg(target)
                .timeout(Duration::from_secs(15));
            let output = host.run_to_string(command).await;
            tracing::info!(
                target: "ncd_backend_snowluma::remote",
                elapsed_ms = recovery_started.elapsed().as_millis() as u64,
                "noVNC 服务恢复耗时"
            );
            let output = output.map_err(|error| BotBackendError::Io(error.to_string()))?;
            if !output.success() {
                return Err(BotBackendError::Io(format!(
                    "恢复 QQ 界面失败：{}",
                    output.stderr.trim()
                )));
            }
            #[derive(serde::Deserialize)]
            struct Ready {
                port: std::num::NonZeroU16,
            }
            let ready: Ready = serde_json::from_str(output.stdout.trim())
                .map_err(|_| BotBackendError::Io("远端 noVNC 检查未返回有效端口".into()))?;
            stage = "验证 WebUI 端口";
            let endpoint_started = std::time::Instant::now();
            let webui_port = self.resolve_webui_port(host.as_ref()).await?;
            tracing::info!(
                target: "ncd_backend_snowluma::remote",
                elapsed_ms = endpoint_started.elapsed().as_millis() as u64,
                "noVNC WebUI 端口验证耗时"
            );
            stage = "读取当前凭据并准备 SSH 隧道";
            self.open_tunnels_with_novnc_port(webui_port, None, Some(ready.port.get()))
                .await?;
            self.tunnel_endpoints()
                .await
                .ok_or_else(|| BotBackendError::Io("noVNC 隧道未就绪".into()))
        });
        let prepared = tokio::time::timeout(Duration::from_secs(30), prepare).await;
        tracing::info!(
            target: "ncd_backend_snowluma::remote",
            elapsed_ms = started.elapsed().as_millis() as u64,
            "noVNC 打开准备总耗时"
        );
        prepared.map_err(|_| BotBackendError::Io(format!("打开 QQ 界面超时（{stage}），请重试")))?
    }

    /// 桌面退出:只拆掉本机 SSH 隧道,不 stop 远端 daemon / QQ
    pub async fn detach_local_sessions(&self) {
        let mut session = self.session.lock().await;
        session.cancel();
        let _stack_guard = self.stack_bootstrap.lock().await;
        self.clear_local_startup().await;
        self.leases.lock().await.discard_pending();
        *session = CancellationToken::new();
    }

    async fn clear_local_startup(&self) {
        self.webui_tunnel.lock().await.take();
        self.webui_endpoint.lock().await.take();
        if self.tunnel_eps.lock().await.take().is_some() {
            self.tunnels.release(&self.server_id).await;
        } else {
            self.tunnels.drop_server(&self.server_id).await;
        }
        *self.tunnel_remote_ports.lock().await = None;
        *self.tunnel_remote_webui_host.lock().await = None;
        self.tunnel_secret_paths.lock().await.clear();
        *self.ready_node_identity.lock().await = None;
    }

    /// 接管存活 QQ；缺失时只恢复 node，保留 QQ 使用中的图形会话和配置。
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
        self.in_current_session(async {
            let _stack_guard = self.stack_bootstrap.lock().await;
            // WebUI 接管失败也不代表 QQ 已退出，仍须占用共享 daemon。
            if bot_id.is_some() {
                let mut leases = self.leases.lock().await;
                leases.acquire(bot_id);
            }
            let host = self.current_host().await;
            if self.reuse_running_node(host.as_ref(), bot_id, false).await {
                return Ok(());
            }
            *self.ready_node_identity.lock().await = None;
            let result = self.attach_reconciled_daemon(bot_id.is_some()).await;
            if result.is_err() {
                self.clear_local_startup().await;
            }
            let rc = {
                let mut leases = self.leases.lock().await;
                if result.is_ok() && bot_id.is_none() && leases.count() == 0 {
                    leases.acquire(None);
                }
                leases.count()
            };
            // WebUI 失败不能发 Crashed：runtime 会据此把仍存活的 QQ 一并标成崩溃。
            if result.is_ok() {
                self.event_bus
                    .publish(DomainEvent::snowluma_daemon_state_changed(
                        DaemonState::Ready,
                        rc,
                        None,
                        Some(self.server_id.clone()),
                    ));
            }
            result
        })
        .await
    }

    async fn attach_reconciled_daemon(
        &self,
        recover_missing_node: bool,
    ) -> Result<(), BotBackendError> {
        let host = self.current_host().await;
        let webui_port = if !remote_daemon_already_ready(host.as_ref(), &self.layout).await? {
            if !recover_missing_node {
                return Err(BotBackendError::Io("远端 SnowLuma node 未运行".into()));
            }
            tracing::info!(
                target: "ncd_runtime::remote_snowluma",
                server_id = %self.server_id,
                "reconcile: QQ 仍在运行，恢复缺失的 SnowLuma node"
            );
            let env = self.metrics_node_env.lock().await.clone();
            recover_node_for_reconcile(host.as_ref(), &self.layout, env.as_ref()).await?;
            self.resolve_webui_port(host.as_ref()).await?
        } else {
            self.resolve_webui_port(host.as_ref()).await?
        };
        let _ = write_status_daemon_json(host.as_ref(), &self.layout.paths, true, true, webui_port)
            .await;
        self.open_tunnels(webui_port, None).await?;
        let secret_paths = self.tunnel_secret_paths.lock().await.clone();
        let identity =
            managed_node_identity(host.as_ref(), &self.layout.paths, None, &secret_paths)
                .await
                .map(|identity| ReadyNode {
                    identity,
                    startup_validated: false,
                });
        *self.ready_node_identity.lock().await = identity;

        Ok(())
    }

    async fn open_tunnels(
        &self,
        webui_port: u16,
        webui_password: Option<&str>,
    ) -> Result<(), BotBackendError> {
        self.open_tunnels_with_novnc_port(webui_port, webui_password, None)
            .await
    }

    async fn open_tunnels_with_novnc_port(
        &self,
        webui_port: u16,
        webui_password: Option<&str>,
        novnc_port: Option<u16>,
    ) -> Result<(), BotBackendError> {
        let host = self.current_host().await;
        let webui_host = self
            .webui_endpoint
            .lock()
            .await
            .as_ref()
            .filter(|endpoint| endpoint.port == webui_port)
            .map(|endpoint| endpoint.host.clone())
            .unwrap_or_else(|| "127.0.0.1".to_owned());
        let credential_started = std::time::Instant::now();
        let credentials = tokio::try_join!(
            async {
                match webui_password.map(str::trim).filter(|s| !s.is_empty()) {
                    Some(pwd) => Ok((pwd.to_string(), Vec::new())),
                    None => self.read_current_webui_credentials(host.as_ref()).await,
                }
            },
            webui::read_credential_file(host.as_ref(), &self.layout.paths.vnc_secret, "VNC 凭据"),
        );
        tracing::info!(
            target: "ncd_backend_snowluma::remote",
            elapsed_ms = credential_started.elapsed().as_millis() as u64,
            "SnowLuma 隧道凭据读取与校验耗时"
        );
        let ((webui_plain, mut secret_paths), vnc_bytes) = credentials?;
        {
            let mut webui = self.webui_tunnel.lock().await;
            if let Some(tunnel) = webui.as_mut() {
                if tunnel.remote_port == webui_port && tunnel.remote_host == webui_host {
                    tunnel.password = webui_plain.clone();
                } else {
                    *webui = None;
                }
            }
        }
        let mut vnc_plain = vnc_bytes
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
            .unwrap_or_default();
        if vnc_plain.is_empty() {
            // x11vnc -passwdfile 指向的才是实际 VNC 密码文件；外来启动脚本可能把它放在
            // 任意目录，从进程 cmdline 读最可靠。
            if let Some(actual) = resolve_remote_vnc_secret_from_x11vnc(host.as_ref()).await? {
                secret_paths.push(actual.clone());
                tracing::info!(
                    target: "ncd_backend_snowluma::remote",
                    server_id = %self.server_id,
                    fallback = %actual,
                    "vnc.secret 不在预期路径，已从 x11vnc -passwdfile 回退"
                );
                vnc_plain = webui::read_credential_file(host.as_ref(), &actual, "VNC 备用凭据")
                    .await?
                    .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
                    .unwrap_or_default();
            }
        }

        // noVNC 口动态探测：外来图形栈的 websockify 常不在默认 6081
        let novnc_port = match novnc_port {
            Some(port) => port,
            None => resolve_remote_novnc_port(host.as_ref()).await,
        };

        let previous_ports = *self.tunnel_remote_ports.lock().await;
        let same_host =
            self.tunnel_remote_webui_host.lock().await.as_deref() == Some(webui_host.as_str());
        let previous_eps = self.tunnel_eps.lock().await.clone();
        let had_bundle = self
            .tunnels
            .endpoints_for_server(&self.server_id)
            .await
            .is_some();
        if let Some(previous) = previous_eps.as_ref() {
            if had_bundle
                && same_host
                && previous_ports == Some((webui_port, novnc_port))
                && previous.webui_password == webui_plain
                && (vnc_plain.is_empty() || previous.vnc_password == vnc_plain)
            {
                *self.tunnel_secret_paths.lock().await = secret_paths;
                return Ok(());
            }
        }

        let current_webui_password = webui_plain.clone();
        let tunnel_started = std::time::Instant::now();
        let opened = self
            .tunnels
            .acquire_at(
                &self.server_id,
                host.as_ref(),
                webui_plain,
                vnc_plain,
                &webui_host,
                webui_port,
                novnc_port,
            )
            .await;
        tracing::info!(
            target: "ncd_backend_snowluma::remote",
            elapsed_ms = tunnel_started.elapsed().as_millis() as u64,
            "SnowLuma 隧道准备耗时"
        );
        let mut eps = match opened {
            Ok(eps) => eps,
            Err(error) => {
                *self.tunnel_eps.lock().await = None;
                *self.tunnel_remote_ports.lock().await = None;
                *self.tunnel_remote_webui_host.lock().await = None;
                return Err(BotBackendError::Io(format!("SnowLuma SSH 隧道: {error}")));
            }
        };
        eps.webui_password = current_webui_password;
        // daemon 持有一个 bundle 引用；端口变动时 registry 已替换旧 bundle。
        if had_bundle
            && same_host
            && previous_eps.is_some()
            && previous_ports == Some((webui_port, novnc_port))
        {
            self.tunnels.release(&self.server_id).await;
        }
        *self.tunnel_eps.lock().await = Some(eps);
        *self.tunnel_remote_ports.lock().await = Some((webui_port, novnc_port));
        *self.tunnel_remote_webui_host.lock().await = Some(webui_host);
        *self.tunnel_secret_paths.lock().await = secret_paths;
        Ok(())
    }
}

#[cfg(test)]
#[path = "daemon_recovery_tests.rs"]
mod recovery_tests;

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
