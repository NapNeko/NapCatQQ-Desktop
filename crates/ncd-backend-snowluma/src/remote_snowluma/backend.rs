//! 远端 SnowLuma BotBackend

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use crate::snowluma::SnowLumaWebUiClient;
use async_trait::async_trait;
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::kinds::BackendKind;
use ncd_domain::{
    BotConfig, BotFlavor, BotId, SnowLumaQrLoginSnapshot, SnowLumaQrLoginState, SnowLumaStartMode,
};
use ncd_host::{Host, HostPath};
use ncd_traits::events::{BroadcastEventBus, EventBus};
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, BotStatus, LogSnapshot, StopMode,
    TailOpts,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

struct QrActivity {
    cancel: CancellationToken,
    operation: Mutex<()>,
    scanned: AtomicBool,
    login_detected: AtomicBool,
    opened_at: Instant,
    terminal: Mutex<Option<SnowLumaQrLoginSnapshot>>,
    cleanup_failed: AtomicBool,
    refresh_pending: AtomicBool,
    capture_failures: AtomicU32,
    recovery_started: AtomicBool,
    close_revision: AtomicU32,
}

#[derive(Clone, PartialEq)]
struct TrackedQq {
    pid: u32,
    start_time: String,
}

fn closed_snapshot(
    previous: Option<SnowLumaQrLoginSnapshot>,
    mut closed: SnowLumaQrLoginSnapshot,
) -> SnowLumaQrLoginSnapshot {
    closed.qrcode_payload = None;
    if let Some(mut previous) = previous.filter(|value| {
        matches!(
            value.state,
            SnowLumaQrLoginState::LoggedIn | SnowLumaQrLoginState::AccountMismatch
        )
    }) {
        previous.qrcode_payload = None;
        previous.observer_removed = closed.observer_removed;
        return previous;
    }
    closed
}

impl QrActivity {
    async fn can_renew(&self) -> bool {
        (self.cancel.is_cancelled() || self.opened_at.elapsed() >= Duration::from_secs(600))
            && !self.cleanup_failed.load(Ordering::SeqCst)
            && !self.scanned.load(Ordering::SeqCst)
            && !self.login_detected.load(Ordering::SeqCst)
            && self
                .terminal
                .lock()
                .await
                .as_ref()
                .is_none_or(can_retry_qr_snapshot)
    }

    fn new() -> Self {
        Self {
            cancel: CancellationToken::new(),
            operation: Mutex::new(()),
            scanned: AtomicBool::new(false),
            login_detected: AtomicBool::new(false),
            opened_at: Instant::now(),
            terminal: Mutex::new(None),
            cleanup_failed: AtomicBool::new(false),
            refresh_pending: AtomicBool::new(false),
            capture_failures: AtomicU32::new(0),
            recovery_started: AtomicBool::new(false),
            close_revision: AtomicU32::new(0),
        }
    }

    fn capture_result(&self, mut snapshot: SnowLumaQrLoginSnapshot) -> SnowLumaQrLoginSnapshot {
        if snapshot.state == SnowLumaQrLoginState::Expired {
            self.scanned.store(false, Ordering::SeqCst);
        } else if self.scanned.load(Ordering::SeqCst)
            && !snapshot.state.is_terminal()
            && snapshot.state != SnowLumaQrLoginState::LoginDetected
        {
            snapshot.state = SnowLumaQrLoginState::Scanned;
            snapshot.qrcode_payload = None;
        }
        if snapshot.state == SnowLumaQrLoginState::Starting {
            if self.capture_failures.fetch_add(1, Ordering::SeqCst) >= 3 {
                snapshot.state = SnowLumaQrLoginState::Unavailable;
                snapshot.message = Some("暂时无法获取二维码，请重试或打开 QQ 界面".into());
            }
        } else {
            self.capture_failures.store(0, Ordering::SeqCst);
        }
        snapshot
    }
}

fn can_retry_qr_snapshot(snapshot: &SnowLumaQrLoginSnapshot) -> bool {
    snapshot.observer_removed
        && matches!(
            snapshot.state,
            SnowLumaQrLoginState::Cancelled | SnowLumaQrLoginState::Unavailable
        )
}

use super::layout::SnowLumaRemotePaths;
use super::login_helper::{self, LoginSession};
use super::orchestrator::{
    bot_cold_start_with_login, bot_pid_if_running, bot_stop, remember_remote_bot_pid,
};
use super::tunnel::RemoteSnowLumaTunnelRegistry;
use crate::snowluma::log_noise::prepare_snowluma_bot_history_lines;
use crate::snowluma::status_poller::{PollerDeps, SnowLumaStatusPoller};

use super::config::{render_native_snowluma_config_on_host, resolve_start_mode};
use super::daemon::RemoteSnowLumaDaemon;
use super::helpers::{read_remote_log_tail, read_remote_log_tail_lines};
use super::inject::{inject_via_tunnel, remote_qq_running_pid_with_hint};

#[path = "qr_relogin.rs"]
mod qr_relogin;

/// 远端 SL 指标探针：上传资产并返回应 export 进共享 node 的 env
#[async_trait]
pub trait RemoteSlMetricsInjector: Send + Sync {
    async fn prepare(
        &self,
        host: &dyn Host,
        home: &str,
        bot_id: &str,
        config: &BotConfig,
    ) -> Option<BTreeMap<String, String>>;
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
    /// Shared coordinator for flipping the selected QQ tree entry point.
    /// Passed from BotManager so that NC and SL cold starts on the same server_id
    /// serialize their package.json main changes.
    qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
    metrics_injector: Option<Arc<dyn RemoteSlMetricsInjector>>,
    /// 本机 `state/snowluma`，用来读全局 WebUI 固定密码覆盖
    snowluma_data_root: Option<PathBuf>,
    login_sessions: Mutex<HashMap<BotId, LoginSession>>,
    login_clients: Mutex<HashMap<BotId, Arc<dyn SnowLumaWebUiClient>>>,
    login_pids: Mutex<HashMap<BotId, TrackedQq>>,
    qr_activity: Mutex<HashMap<BotId, Arc<QrActivity>>>,
}

impl RemoteSnowLumaBackend {
    /// 供 ncd-runtime facade 层统一构造远端后端实例。
    pub fn new(
        backend_id: impl Into<BotId>,
        daemon: Arc<RemoteSnowLumaDaemon>,
        event_bus: Arc<BroadcastEventBus>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
    ) -> Self {
        Self::new_with_metrics(
            backend_id,
            daemon,
            event_bus,
            tunnels,
            qq_entry_coordinator,
            None,
        )
    }

    pub fn new_with_metrics(
        backend_id: impl Into<BotId>,
        daemon: Arc<RemoteSnowLumaDaemon>,
        event_bus: Arc<BroadcastEventBus>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
        metrics_injector: Option<Arc<dyn RemoteSlMetricsInjector>>,
    ) -> Self {
        Self {
            backend_id: backend_id.into(),
            daemon,
            event_bus,
            tunnels,
            start_modes: Arc::new(Mutex::new(HashMap::new())),
            pollers: Arc::new(Mutex::new(HashMap::new())),
            qq_entry_coordinator,
            metrics_injector,
            snowluma_data_root: None,
            login_sessions: Mutex::new(HashMap::new()),
            login_clients: Mutex::new(HashMap::new()),
            login_pids: Mutex::new(HashMap::new()),
            qr_activity: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_snowluma_data_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.snowluma_data_root = Some(path.into());
        self
    }

    pub fn qr_login_enabled(&self) -> bool {
        self.snowluma_data_root.as_ref().is_some_and(|root| {
            crate::snowluma::session::load_snowluma_app_config(root).remote_qr_login_enabled
        })
    }

    pub async fn qr_login_snapshot(
        &self,
        bot_id: &BotId,
        refresh: bool,
    ) -> SnowLumaQrLoginSnapshot {
        if !self.qr_login_enabled() {
            let mut result = SnowLumaQrLoginSnapshot::unavailable("扫码登录已关闭");
            result.state = SnowLumaQrLoginState::Disabled;
            return result;
        }
        let Some(mut tracked) = self.login_pids.lock().await.get(bot_id).cloned() else {
            return SnowLumaQrLoginSnapshot::unavailable(
                "尚未确认此 Bot 的 QQ 进程，请打开 QQ 界面",
            );
        };
        if self.start_modes.lock().await.get(bot_id) != Some(&SnowLumaStartMode::ColdStart) {
            return SnowLumaQrLoginSnapshot::unavailable("热启动请使用 QQ 界面继续登录");
        }
        let host = self.daemon.current_host().await;
        let mut activity = self
            .qr_activity
            .lock()
            .await
            .entry(bot_id.clone())
            .or_insert_with(|| Arc::new(QrActivity::new()))
            .clone();
        let serial_activity = Arc::clone(&activity);
        let request_close_revision = activity.close_revision.load(Ordering::SeqCst);
        let _serial_operation = serial_activity.operation.lock().await;
        if self
            .qr_activity
            .lock()
            .await
            .get(bot_id)
            .is_none_or(|current| !Arc::ptr_eq(current, &serial_activity))
        {
            let mut result = SnowLumaQrLoginSnapshot::unavailable("扫码会话已更新");
            result.state = SnowLumaQrLoginState::Starting;
            return result;
        }
        let cancelled = |pid| {
            let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("扫码已关闭");
            snapshot.state = SnowLumaQrLoginState::Cancelled;
            snapshot.qq_pid = Some(pid);
            snapshot
        };
        if serial_activity.close_revision.load(Ordering::SeqCst) != request_close_revision {
            return cancelled(tracked.pid);
        }
        let mut relogin_bound = false;
        if refresh || activity.refresh_pending.load(Ordering::SeqCst) {
            let prepared = self
                .prepare_qr_relogin(bot_id, &tracked, &activity, request_close_revision)
                .await;
            if serial_activity.close_revision.load(Ordering::SeqCst) != request_close_revision {
                if let Ok(qr_relogin::QrReloginPreparation::Bound(_, renewed)) = &prepared {
                    renewed.cancel.cancel();
                }
                return cancelled(tracked.pid);
            }
            match prepared {
                Ok(qr_relogin::QrReloginPreparation::Bound(current, renewed)) => {
                    tracked = current;
                    activity = renewed;
                    relogin_bound = true;
                }
                Ok(qr_relogin::QrReloginPreparation::Unchanged) => {}
                Ok(qr_relogin::QrReloginPreparation::Waiting) => {
                    activity.refresh_pending.store(true, Ordering::SeqCst);
                    let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("正在恢复 QQ 扫码通道");
                    snapshot.state = SnowLumaQrLoginState::Starting;
                    return snapshot;
                }
                Err(error) => {
                    tracing::warn!(bot_id = %bot_id, %error, "QQ 扫码重登通道恢复失败");
                    return SnowLumaQrLoginSnapshot::unavailable(
                        "扫码通道恢复失败，请重新获取或打开 QQ 界面",
                    );
                }
            }
        }
        let pid = tracked.pid;
        let blank = |state| SnowLumaQrLoginSnapshot {
            v: 1,
            state,
            qq_pid: Some(pid),
            qrcode_payload: None,
            uin: None,
            observer_removed: true,
            message: None,
        };
        if self.login_pids.lock().await.get(bot_id) != Some(&tracked)
            || self
                .qr_activity
                .lock()
                .await
                .get(bot_id)
                .is_none_or(|current| !Arc::ptr_eq(current, &activity))
        {
            return blank(SnowLumaQrLoginState::Starting);
        }
        if refresh && activity.can_renew().await {
            let mut activities = self.qr_activity.lock().await;
            if activities
                .get(bot_id)
                .is_none_or(|current| !Arc::ptr_eq(current, &serial_activity))
            {
                return blank(SnowLumaQrLoginState::Cancelled);
            }
            if serial_activity.close_revision.load(Ordering::SeqCst) != request_close_revision {
                return blank(SnowLumaQrLoginState::Cancelled);
            }
            activity = Arc::new(QrActivity::new());
            activities.insert(bot_id.clone(), activity.clone());
        }
        let _renewed_operation = if Arc::ptr_eq(&activity, &serial_activity) {
            None
        } else {
            Some(activity.operation.lock().await)
        };
        if let Some(terminal) = activity.terminal.lock().await.clone() {
            return terminal;
        }
        if refresh {
            activity.refresh_pending.store(true, Ordering::SeqCst);
        }
        if activity.cancel.is_cancelled()
            || activity.opened_at.elapsed() >= Duration::from_secs(600)
        {
            activity.cancel.cancel();
            let snapshot = blank(SnowLumaQrLoginState::Cancelled);
            *activity.terminal.lock().await = Some(snapshot.clone());
            return snapshot;
        }
        let Some(client) = self.login_clients.lock().await.get(bot_id).cloned() else {
            return blank(SnowLumaQrLoginState::Starting);
        };
        let before = match client.list_processes().await {
            Ok(processes) => processes,
            Err(_) => return blank(SnowLumaQrLoginState::Starting),
        };
        if !self
            .qr_request_current(bot_id, &activity, &client, &tracked)
            .await
        {
            return blank(SnowLumaQrLoginState::Cancelled);
        }
        if let Some(mut gated) = crate::qr_guard::process_gate(
            before.iter().find(|p| p.pid == pid),
            pid,
            bot_id.as_str(),
        ) {
            if gated.state == SnowLumaQrLoginState::LoginDetected {
                activity.login_detected.store(true, Ordering::SeqCst);
            }
            if gated.state == SnowLumaQrLoginState::Starting
                && activity.login_detected.load(Ordering::SeqCst)
            {
                gated.state = SnowLumaQrLoginState::LoginDetected;
            }
            if matches!(
                gated.state,
                SnowLumaQrLoginState::LoggedIn | SnowLumaQrLoginState::AccountMismatch
            ) {
                activity.cancel.cancel();
                *activity.terminal.lock().await = Some(gated.clone());
                if let Some(session) = self.login_sessions.lock().await.get(bot_id).cloned() {
                    session.remove_assets(host.as_ref()).await;
                }
            }
            return gated;
        }
        if activity.login_detected.load(Ordering::SeqCst) {
            return blank(SnowLumaQrLoginState::LoginDetected);
        }
        let scanned = activity.scanned.load(Ordering::SeqCst);
        let mut session = self
            .login_sessions
            .lock()
            .await
            .get(bot_id)
            .cloned()
            .filter(|session| {
                session.pid() == pid && session.start_time() == Some(tracked.start_time.as_str())
            });
        let needs_binding = session.as_ref().is_none_or(LoginSession::needs_renewal)
            || (!relogin_bound && !Arc::ptr_eq(&activity, &serial_activity));
        if needs_binding {
            // Only the user's open/refresh request may create a channel for an already running QQ.
            if !activity.refresh_pending.load(Ordering::SeqCst) {
                return SnowLumaQrLoginSnapshot::unavailable("请刷新二维码或打开 QQ 界面");
            }
            let recovered = if let Some(mut saved) = session {
                saved
                    .renew_with_cancel(host.as_ref(), activity.cancel.clone())
                    .await
                    .map(|_| Some(saved))
            } else {
                LoginSession::attach_running(
                    host.as_ref(),
                    self.daemon.paths(),
                    self.daemon.layout(),
                    bot_id,
                    pid,
                    &tracked.start_time,
                    activity.cancel.clone(),
                )
                .await
            };
            if !self
                .qr_request_current(bot_id, &activity, &client, &tracked)
                .await
            {
                return blank(SnowLumaQrLoginState::Cancelled);
            }
            let restored = match recovered {
                Ok(Some(session))
                    if session.pid() == pid
                        && session.start_time() == Some(tracked.start_time.as_str()) =>
                {
                    session
                }
                Err(BotBackendError::Io(message))
                    if message == login_helper::DESKTOP_SESSION_UNAVAILABLE =>
                {
                    activity.refresh_pending.store(false, Ordering::SeqCst);
                    return SnowLumaQrLoginSnapshot::unavailable(message);
                }
                _ => {
                    activity.refresh_pending.store(false, Ordering::SeqCst);
                    return SnowLumaQrLoginSnapshot::unavailable(
                        "当前 QQ 未提供可用扫码控件，请打开 QQ 界面",
                    );
                }
            };
            // Keep registration inside the activity lock so a simultaneous close cannot miss it.
            self.login_sessions
                .lock()
                .await
                .insert(bot_id.clone(), restored.clone());
            session = Some(restored);
        }
        let Some(session) = session.filter(|session| {
            session.pid() == pid && session.start_time() == Some(tracked.start_time.as_str())
        }) else {
            return SnowLumaQrLoginSnapshot::unavailable("QQ 进程已变化，请重新打开扫码");
        };
        if !self
            .qr_request_current(bot_id, &activity, &client, &tracked)
            .await
        {
            return blank(SnowLumaQrLoginState::Cancelled);
        }
        let mut snapshot = match session
            .request_with_cancel(
                host.as_ref(),
                if activity.refresh_pending.swap(false, Ordering::SeqCst) && !scanned {
                    "refresh"
                } else {
                    "snapshot"
                },
                activity.cancel.clone(),
                scanned,
            )
            .await
        {
            Ok(snapshot) => snapshot,
            Err(_) => {
                return SnowLumaQrLoginSnapshot::unavailable("扫码通道不可用，请打开 QQ 界面");
            }
        };
        snapshot = activity.capture_result(snapshot);
        activity
            .cleanup_failed
            .store(!snapshot.observer_removed, Ordering::SeqCst);
        if activity.cancel.is_cancelled() {
            return blank(SnowLumaQrLoginState::Cancelled);
        }
        if !self
            .qr_request_current(bot_id, &activity, &client, &tracked)
            .await
        {
            return blank(SnowLumaQrLoginState::Cancelled);
        }
        let after = match client.list_processes().await {
            Ok(processes) => processes,
            Err(_) => return blank(SnowLumaQrLoginState::Starting),
        };
        if let Some(gated) =
            crate::qr_guard::process_gate(after.iter().find(|p| p.pid == pid), pid, bot_id.as_str())
        {
            snapshot = gated;
        }
        if !self
            .qr_request_current(bot_id, &activity, &client, &tracked)
            .await
        {
            return blank(SnowLumaQrLoginState::Cancelled);
        }
        match snapshot.state {
            SnowLumaQrLoginState::Scanned => {
                activity.scanned.store(true, Ordering::SeqCst);
                snapshot.qrcode_payload = None;
            }
            SnowLumaQrLoginState::LoginDetected => {
                activity.login_detected.store(true, Ordering::SeqCst);
                snapshot.qrcode_payload = None;
            }
            state if state.is_terminal() => {
                snapshot.qrcode_payload = None;
                activity.cancel.cancel();
                *activity.terminal.lock().await = Some(snapshot.clone());
                if snapshot.observer_removed {
                    session.remove_assets(host.as_ref()).await;
                }
            }
            _ => {}
        }
        snapshot
    }

    async fn remember_login_pid(&self, bot_id: &BotId, host: &dyn Host, pid: u32) {
        match login_helper::process_start_time(host, pid).await {
            Ok(start_time) => {
                self.login_pids
                    .lock()
                    .await
                    .insert(bot_id.clone(), TrackedQq { pid, start_time });
            }
            Err(_) => {
                self.login_pids.lock().await.remove(bot_id);
            }
        }
    }

    async fn qr_request_current(
        &self,
        bot_id: &BotId,
        activity: &Arc<QrActivity>,
        client: &Arc<dyn SnowLumaWebUiClient>,
        tracked: &TrackedQq,
    ) -> bool {
        if activity.cancel.is_cancelled()
            || activity.opened_at.elapsed() >= Duration::from_secs(600)
            || !self.qr_login_enabled()
        {
            return false;
        }
        self.qr_activity
            .lock()
            .await
            .get(bot_id)
            .is_some_and(|current| Arc::ptr_eq(current, activity))
            && self
                .login_clients
                .lock()
                .await
                .get(bot_id)
                .is_some_and(|current| Arc::ptr_eq(current, client))
            && self
                .login_pids
                .lock()
                .await
                .get(bot_id)
                .is_some_and(|current| current == tracked)
            && self.start_modes.lock().await.get(bot_id) == Some(&SnowLumaStartMode::ColdStart)
    }

    pub async fn close_qr_login(&self, bot_id: &BotId) -> Result<(), BotBackendError> {
        let activity = self
            .qr_activity
            .lock()
            .await
            .entry(bot_id.clone())
            .or_insert_with(|| Arc::new(QrActivity::new()))
            .clone();
        activity.close_revision.fetch_add(1, Ordering::SeqCst);
        activity.cancel.cancel();
        let _operation = activity.operation.lock().await;
        if self
            .qr_activity
            .lock()
            .await
            .get(bot_id)
            .is_none_or(|current| !Arc::ptr_eq(current, &activity))
        {
            return Ok(());
        }
        let session = self.login_sessions.lock().await.get(bot_id).cloned();
        if let Some(session) = session {
            let host = self.daemon.current_host().await;
            let mut snapshot = session.request(host.as_ref(), "close").await?;
            if activity.cleanup_failed.load(Ordering::SeqCst) {
                snapshot.state = SnowLumaQrLoginState::CleanupFailed;
                snapshot.observer_removed = false;
            }
            let previous = activity.terminal.lock().await.clone();
            let snapshot = closed_snapshot(previous, snapshot);
            *activity.terminal.lock().await = Some(snapshot.clone());
            if !snapshot.observer_removed {
                return Err(BotBackendError::Io("扫码观察器未能移除".into()));
            }
            session.remove_assets(host.as_ref()).await;
        } else {
            let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("扫码已关闭");
            snapshot.state = SnowLumaQrLoginState::Cancelled;
            *activity.terminal.lock().await = Some(snapshot);
        }
        Ok(())
    }

    pub async fn close_all_qr_logins(&self) {
        let mut ids: std::collections::BTreeSet<_> =
            self.login_sessions.lock().await.keys().cloned().collect();
        ids.extend(self.qr_activity.lock().await.keys().cloned());
        for id in ids {
            let _ = self.close_qr_login(&id).await;
        }
    }

    fn webui_password_override(&self) -> Option<String> {
        let root = self.snowluma_data_root.as_ref()?;
        let cfg = crate::snowluma::session::load_snowluma_app_config(root);
        let trimmed = cfg.webui_password_override.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    async fn dispose_poller(&self, bot_id: &BotId) {
        let mut pollers = self.pollers.lock().await;
        if let Some(poller) = pollers.remove(bot_id) {
            poller.dispose();
        }
        self.login_clients.lock().await.remove(bot_id);
        if let Some(activity) = self.qr_activity.lock().await.get(bot_id) {
            activity.cancel.cancel();
        }
    }

    pub async fn detach_login_pollers(&self) {
        let mut pollers = self.pollers.lock().await;
        for (bot_id, poller) in pollers.drain() {
            poller.dispose();
            self.event_bus
                .publish(DomainEvent::snowluma_login_probe_unavailable(bot_id));
        }
        self.login_clients.lock().await.clear();
        for activity in self.qr_activity.lock().await.values() {
            activity.cancel.cancel();
        }
    }

    async fn replace_poller(&self, bot_id: BotId, pid: u32, deps: PollerDeps) {
        let mut pollers = self.pollers.lock().await;
        if let Some(old) = pollers.remove(&bot_id) {
            old.dispose();
        }
        self.login_clients
            .lock()
            .await
            .insert(bot_id.clone(), Arc::clone(&deps.http));
        pollers.insert(
            bot_id.clone(),
            SnowLumaStatusPoller::spawn(bot_id, pid, deps),
        );
    }

    pub async fn detach_status_poller(&self, bot_id: &BotId) {
        self.dispose_poller(bot_id).await;
        self.login_clients.lock().await.remove(bot_id);
    }

    /// 供 bootstrap reconcile 取日志 follow 路径,不暴露整个 daemon
    pub fn daemon_paths(&self) -> &SnowLumaRemotePaths {
        self.daemon.paths()
    }

    pub fn qq_bin(&self) -> &str {
        &self.daemon.layout().qq_bin
    }

    /// 接管存活 QQ；WebUI 失败返回调用方，QQ 运行态由 reconcile 单独保留。
    pub async fn attach_reconciled_running(
        &self,
        bot_id: BotId,
        pid: u32,
        config: &BotConfig,
    ) -> Result<(), BotBackendError> {
        // SSH 重连可能换了本地端口；旧 poller 存在不代表仍能访问新的隧道。
        self.dispose_poller(&bot_id).await;
        self.login_clients.lock().await.remove(&bot_id);
        let qq_id = config.bot.qq_id.to_string();
        let host = self.daemon.current_host().await;
        self.remember_login_pid(&bot_id, host.as_ref(), pid).await;
        if let Ok(Some(session)) = LoginSession::recover(
            host.as_ref(),
            self.daemon.paths(),
            &bot_id,
            pid,
            &self.daemon.layout().node_bin,
        )
        .await
        {
            self.login_sessions
                .lock()
                .await
                .insert(bot_id.clone(), session);
            if !self.qr_login_enabled() {
                let _ = self.close_qr_login(&bot_id).await;
            }
        }
        if let Err(err) =
            remember_remote_bot_pid(host.as_ref(), self.daemon.paths(), &qq_id, pid).await
        {
            tracing::warn!(
                target: "ncd_runtime::remote_snowluma",
                bot_id = %bot_id,
                %err,
                "reconcile: 未能写下 pid_bot（仍按进程认运行）"
            );
        }

        self.start_modes
            .lock()
            .await
            .insert(bot_id.clone(), resolve_start_mode(config));

        // Desktop 重开后重新注册已存活账号，恢复缺失 node 时仍预加载共享探针。
        if let Some(injector) = &self.metrics_injector {
            if let Some(env) = injector
                .prepare(
                    host.as_ref(),
                    self.daemon.remote_home(),
                    bot_id.as_str(),
                    config,
                )
                .await
            {
                self.daemon.apply_metrics_node_env(Some(env)).await?;
            }
        }

        if let Err(err) = self.daemon.ensure_running_for_reconcile_bot(&bot_id).await {
            tracing::warn!(
                target: "ncd_runtime::remote_snowluma",
                bot_id = %bot_id,
                %err,
                "reconcile: SnowLuma 隧道未建立，跳过 WebUI 注入"
            );
            self.event_bus
                .publish(DomainEvent::snowluma_login_probe_unavailable(bot_id));
            return Err(err);
        }
        let Some(endpoints) = self.daemon.tunnel_endpoints().await else {
            tracing::info!(
                target: "ncd_runtime::remote_snowluma",
                bot_id = %bot_id,
                "reconcile: 无 WebUI 隧道（node 未在或端口未解析），仅同步运行态"
            );
            self.event_bus
                .publish(DomainEvent::snowluma_login_probe_unavailable(bot_id));
            return Err(BotBackendError::Io("SnowLuma WebUI 隧道尚未建立".into()));
        };
        let http = match inject_via_tunnel(&endpoints, pid).await {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(
                    target: "ncd_runtime::remote_snowluma",
                    bot_id = %bot_id,
                    pid,
                    %err,
                    "reconcile: WebUI 注入失败（密码未知或端口刚换），Bot 仍标为运行中"
                );
                self.event_bus
                    .publish(DomainEvent::snowluma_login_probe_unavailable(bot_id));
                return Err(err);
            }
        };
        self.event_bus
            .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), pid));
        self.event_bus
            .publish(DomainEvent::SnowLumaDockerEndpointsReady {
                bot_id: bot_id.clone(),
            });

        let poller_deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http: Arc::clone(&http),
            proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(
                pid,
            )),
            expected_uin: Some(qq_id),
        };
        self.replace_poller(bot_id, pid, poller_deps).await;
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

    async fn qr_login_snapshot(
        &self,
        bot_id: &BotId,
        refresh: bool,
    ) -> Result<SnowLumaQrLoginSnapshot, BotBackendError> {
        Ok(RemoteSnowLumaBackend::qr_login_snapshot(self, bot_id, refresh).await)
    }

    async fn close_qr_login(&self, bot_id: &BotId) -> Result<(), BotBackendError> {
        RemoteSnowLumaBackend::close_qr_login(self, bot_id).await
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let config = ctx
            .bot_config
            .as_ref()
            .ok_or_else(|| BotBackendError::ConfigNotFound(ctx.config.bot_id.clone()))?;
        let qq_id = config.bot.qq_id;
        let qq_id_str = qq_id.to_string();
        let bot_id = ctx.config.bot_id.clone();
        if let Some(activity) = self.qr_activity.lock().await.remove(&bot_id) {
            activity.cancel.cancel();
        }
        self.login_clients.lock().await.remove(&bot_id);
        self.login_pids.lock().await.remove(&bot_id);
        let start_mode = resolve_start_mode(config);

        // 指标资产先上传，再随 node 第一次拉起带上 env，避免先听口再 SIGTERM 重启
        // 导致隧道打到已死的旧 WebUI 口、界面卡在启动中。
        let host = self.daemon.current_host().await;
        let metrics_env = if let Some(inj) = &self.metrics_injector {
            let home = self.daemon.remote_home();
            inj.prepare(host.as_ref(), home, bot_id.as_str(), config)
                .await
        } else {
            None
        };

        // 首次接管可换密，先移除本 Bot 的旧凭据；其他 Bot 的 lease 由 daemon 保护。
        self.dispose_poller(&bot_id).await;
        let override_pwd = if config.bot.webui_password_takeover {
            self.webui_password_override()
        } else {
            None
        };
        self.daemon
            .ensure_running_for_bot(
                &bot_id,
                metrics_env,
                config.bot.webui_password_takeover,
                override_pwd.as_deref(),
            )
            .await?;

        let started = async {
            let paths = self.daemon.paths();
            if let Err(e) =
                render_native_snowluma_config_on_host(host.as_ref(), &bot_id, config, paths).await
            {
                return Err(e);
            }

            let layout = self.daemon.layout();

            let pid = match start_mode {
                SnowLumaStartMode::HotStart => {
                    if let Some(pid) = remote_qq_running_pid_with_hint(
                        host.as_ref(),
                        qq_id,
                        Some(&paths.pid_bot_path(&qq_id_str)),
                        Some(&layout.qq_bin),
                        false,
                    )
                    .await?
                    {
                        pid
                    } else {
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
                    let install_base = HostPath::from_posix(layout.qq_install_base.clone());
                    if let Err(e) = self
                        .qq_entry_coordinator
                        .ensure_for_native(host.as_ref(), self.daemon.server_id(), &install_base)
                        .await
                    {
                        return Err(BotBackendError::InvalidConfig(format!(
                            "SnowLuma 冷启动前确保纯净 QQ 入口失败: {e}"
                        )));
                    }

                    let already_running = bot_pid_if_running(host.as_ref(), paths, &qq_id_str)
                        .await?
                        .is_some();
                    let prepared = if self.qr_login_enabled() && !already_running {
                        match login_helper::prepare(host.as_ref(), paths, layout, &bot_id, qq_id).await
                        {
                            Ok(prepared) => prepared,
                            Err(_) => {
                                tracing::warn!(bot_id = %bot_id, "扫码辅助不可用，继续使用 QQ 图形界面");
                                None
                            }
                        }
                    } else {
                        None
                    };
                    match bot_cold_start_with_login(
                        host.as_ref(),
                        layout,
                        &qq_id_str,
                        &qq_id_str,
                        prepared.as_ref(),
                    )
                    .await
                    {
                        Ok(pid) => {
                            if let Some(prepared) = prepared {
                                let mut session = prepared.session;
                                match session.bind(host.as_ref(), pid).await {
                                    Ok(()) => {
                                        self.login_sessions
                                            .lock()
                                            .await
                                            .insert(bot_id.clone(), session);
                                    }
                                    Err(_) => {
                                        let _ = session.request(host.as_ref(), "close").await;
                                        tracing::warn!(bot_id = %bot_id, "无法保存扫码会话绑定，已请求关闭扫码通道");
                                    }
                                }
                            } else if let Ok(Some(session)) = LoginSession::recover(
                                host.as_ref(),
                                paths,
                                &bot_id,
                                pid,
                                &layout.node_bin,
                            )
                            .await
                            {
                                self.login_sessions
                                    .lock()
                                    .await
                                    .insert(bot_id.clone(), session);
                            }
                            pid
                        }
                        Err(e) => {
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
                        let _ = self.close_qr_login(&bot_id).await;
                        let _ = bot_stop(host.as_ref(), paths, &qq_id_str).await;
                        if let Ok(tail) =
                            read_remote_log_tail(host.as_ref(), &paths.log_bot_path(&qq_id_str), 40)
                                .await
                        {
                            if !tail.trim().is_empty() {
                                return Err(BotBackendError::Io(format!(
                                    "{e}\n--- 启动日志末尾 (bot log) ---\n{tail}"
                                )));
                            }
                        }
                    }
                    return Err(e);
                }
            };

            self.event_bus
                .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), pid));
            self.start_modes
                .lock()
                .await
                .insert(bot_id.clone(), start_mode);
            self.remember_login_pid(&bot_id, host.as_ref(), pid).await;

            let poller_deps = PollerDeps {
                event_bus: Arc::clone(&self.event_bus),
                http,
                proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(
                    pid,
                )),
                expected_uin: Some(qq_id_str.clone()),
            };
            self.replace_poller(bot_id.clone(), pid, poller_deps).await;

            self.event_bus
                .publish(DomainEvent::SnowLumaDockerEndpointsReady {
                    bot_id: bot_id.clone(),
                });

            Ok(BotStatus::running(bot_id.clone(), pid, 0))
        }
        .await;
        if started.is_err() {
            self.daemon.release_bot(&bot_id).await;
        }
        started
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        let _ = self.close_qr_login(&bot_id).await;
        self.login_clients.lock().await.remove(&bot_id);
        self.login_pids.lock().await.remove(&bot_id);
        let qq_id_str = bot_id.as_str();
        let paths = self.daemon.paths();
        let start_mode = self
            .start_modes
            .lock()
            .await
            .remove(&bot_id)
            .unwrap_or(SnowLumaStartMode::ColdStart);

        // 先停 poller：否则停 QQ / 释放 daemon 期间它还会拿旧 token 打登录。
        self.dispose_poller(&bot_id).await;
        if start_mode.is_cold() {
            let host = self.daemon.current_host().await;
            let _ = bot_stop(host.as_ref(), paths, qq_id_str).await;
        }
        if let Some(session) = self.login_sessions.lock().await.remove(&bot_id) {
            let host = self.daemon.current_host().await;
            session.remove_all(host.as_ref()).await;
        }
        self.daemon.release_bot(&bot_id).await;
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let qq_id = bot_id.as_str();
        let paths = self.daemon.paths();
        if let Ok(qq_id_u) = qq_id.parse::<u64>() {
            let host = self.daemon.current_host().await;
            let pid_file = paths.pid_bot_path(qq_id);
            if let Some(pid) = remote_qq_running_pid_with_hint(
                host.as_ref(),
                qq_id_u,
                Some(pid_file.as_str()),
                Some(self.daemon.layout().qq_bin.as_str()),
                false,
            )
            .await?
            {
                return Ok(BotStatus::running(bot_id, pid, 0));
            }
        }
        Ok(BotStatus::stopped(bot_id))
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
        let paths = self.daemon.paths();
        let want = if opts.lines > 0 { opts.lines } else { 1000 };
        // saturating_mul 后下界 800 ≤ 上界 20_000，clamp 不会 panic
        let raw_n = want.saturating_mul(5).clamp(800, 20_000);
        let host = self.daemon.current_host().await;
        // 历史快照与实时跟随共用同一解析：外来安装的真实日志在 framework 自带
        // logs 目录，布局路径可能不存在（空快照会让页面在无增量时永远空白）
        let resolved = super::probe::resolve_remote_snowluma_log_targets(
            host.as_ref(),
            paths,
            &paths.log_bot_path(qq_id),
        )
        .await;
        let same_source = resolved.bot == resolved.daemon;
        let daemon_raw: Vec<String> =
            read_remote_log_tail_lines(host.as_ref(), &resolved.daemon, raw_n)
                .await
                .unwrap_or_default();
        let bot_raw: Vec<String> = if same_source {
            // 两源同文件时只走 daemon 侧（带会话裁剪与 UIN 收窄），避免整段重复
            Vec::new()
        } else {
            read_remote_log_tail_lines(host.as_ref(), &resolved.bot, raw_n)
                .await
                .unwrap_or_default()
        };
        let mut lines = prepare_snowluma_bot_history_lines(bot_raw, daemon_raw, qq_id);
        let total = lines.len();
        if lines.len() > want {
            lines = lines.split_off(lines.len() - want);
        }
        Ok(LogSnapshot {
            lines,
            total_lines: total,
        })
    }
}

#[cfg(all(test, windows))]
#[path = "backend_tests.rs"]
mod tests;

#[cfg(all(test, windows))]
#[path = "qr_relogin_tests.rs"]
mod qr_relogin_tests;

#[cfg(test)]
mod qr_close_tests {
    use super::*;

    #[test]
    fn confirmation_remains_latched_until_explicit_expiry() {
        let activity = QrActivity::new();
        activity.scanned.store(true, Ordering::SeqCst);
        let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("test");
        snapshot.state = SnowLumaQrLoginState::WaitingForQr;
        snapshot.qrcode_payload = Some("stale".into());
        let waiting = activity.capture_result(snapshot.clone());
        assert_eq!(waiting.state, SnowLumaQrLoginState::Scanned);
        assert!(waiting.qrcode_payload.is_none());
        snapshot.state = SnowLumaQrLoginState::Expired;
        assert_eq!(
            activity.capture_result(snapshot).state,
            SnowLumaQrLoginState::Expired
        );
        assert!(!activity.scanned.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn refresh_renews_an_expired_lease_before_its_first_cancelled_reply() {
        let mut activity = QrActivity::new();
        assert!(!activity.can_renew().await);
        activity.opened_at = Instant::now() - Duration::from_secs(601);
        assert!(activity.can_renew().await);
        activity.scanned.store(true, Ordering::SeqCst);
        assert!(!activity.can_renew().await);
        activity.scanned.store(false, Ordering::SeqCst);
        activity.login_detected.store(true, Ordering::SeqCst);
        assert!(!activity.can_renew().await);
        activity.login_detected.store(false, Ordering::SeqCst);
        activity.cleanup_failed.store(true, Ordering::SeqCst);
        assert!(!activity.can_renew().await);
    }

    #[test]
    fn transient_capture_failure_retries_then_stops_and_success_resets_budget() {
        let activity = QrActivity::new();
        let mut pending = SnowLumaQrLoginSnapshot::unavailable("retry");
        pending.state = SnowLumaQrLoginState::Starting;
        for _ in 0..3 {
            assert_eq!(
                activity.capture_result(pending.clone()).state,
                SnowLumaQrLoginState::Starting
            );
        }
        assert_eq!(
            activity.capture_result(pending.clone()).state,
            SnowLumaQrLoginState::Unavailable
        );
        let mut ready = pending.clone();
        ready.state = SnowLumaQrLoginState::WaitingForQr;
        activity.capture_result(ready);
        assert_eq!(
            activity.capture_result(pending).state,
            SnowLumaQrLoginState::Starting
        );
    }

    #[test]
    fn explicit_retry_requires_completed_observer_and_never_reopens_confirmation() {
        let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("retry");
        assert!(can_retry_qr_snapshot(&snapshot));
        snapshot.observer_removed = false;
        assert!(!can_retry_qr_snapshot(&snapshot));
        snapshot.observer_removed = true;
        for state in [
            SnowLumaQrLoginState::Scanned,
            SnowLumaQrLoginState::LoginDetected,
            SnowLumaQrLoginState::LoggedIn,
            SnowLumaQrLoginState::AccountMismatch,
            SnowLumaQrLoginState::CleanupFailed,
        ] {
            snapshot.state = state;
            assert!(!can_retry_qr_snapshot(&snapshot));
        }
    }

    #[test]
    fn closing_does_not_reopen_verified_login_or_wrong_account() {
        for state in [
            SnowLumaQrLoginState::LoggedIn,
            SnowLumaQrLoginState::AccountMismatch,
        ] {
            let mut previous = SnowLumaQrLoginSnapshot::unavailable("fixture");
            previous.state = state;
            previous.qrcode_payload = Some("private".into());
            let mut closed = SnowLumaQrLoginSnapshot::unavailable("closed");
            closed.state = SnowLumaQrLoginState::Cancelled;
            let result = closed_snapshot(Some(previous), closed);
            assert_eq!(result.state, state);
            assert!(result.qrcode_payload.is_none());
        }
    }
}
