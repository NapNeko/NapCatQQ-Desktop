//! 远端 SnowLuma BotBackend

use std::collections::{BTreeMap, HashMap, HashSet};
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
    BotBackend, BotBackendError, BotStartCtx, BotStatus, LogSnapshot, StopMode, TailOpts,
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

#[derive(Clone)]
struct PendingSpawn {
    pid: u32,
    start_time: Option<String>,
    host: Arc<dyn Host>,
}

#[derive(Clone)]
struct CompletedStart {
    status: BotStatus,
    host: Arc<dyn Host>,
    tracked: TrackedQq,
    mode: SnowLumaStartMode,
    client: Arc<dyn SnowLumaWebUiClient>,
}

struct ReusablePoller {
    tracked: TrackedQq,
    client: Arc<dyn SnowLumaWebUiClient>,
    host: Arc<dyn Host>,
}

pub struct ReconciledQqObservation {
    bot_id: BotId,
    backend_id: BotId,
    host: Arc<dyn Host>,
    pid: Option<u32>,
    reusable: Option<ReusablePoller>,
}

impl ReconciledQqObservation {
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
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
use super::orchestrator::{bot_cold_start_with_login, remember_remote_bot_pid};
use super::tunnel::RemoteSnowLumaTunnelRegistry;
use crate::snowluma::log_noise::prepare_snowluma_bot_history_lines;
use crate::snowluma::status_poller::{PollerDeps, SnowLumaStatusPoller};

use super::config::{render_native_snowluma_config_on_host, resolve_start_mode};
use super::daemon::RemoteSnowLumaDaemon;
use super::helpers::read_remote_log_tail_lines;
use super::inject::{inject_via_tunnel, remote_qq_running_pid_with_hint};

#[path = "qr_relogin.rs"]
mod qr_relogin;

#[path = "qq_process.rs"]
mod qq_process;

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
    reconcile_metrics: Mutex<HashMap<BotId, tokio::task::JoinHandle<()>>>,
    /// 本机 `state/snowluma`，用来读全局 WebUI 固定密码覆盖
    snowluma_data_root: Option<PathBuf>,
    login_sessions: Mutex<HashMap<BotId, LoginSession>>,
    login_clients: Mutex<HashMap<BotId, Arc<dyn SnowLumaWebUiClient>>>,
    login_pids: Mutex<HashMap<BotId, TrackedQq>>,
    hot_detached: Mutex<HashSet<BotId>>,
    pending_spawns: std::sync::Mutex<HashMap<BotId, PendingSpawn>>,
    completed_starts: Mutex<HashMap<BotId, CompletedStart>>,
    qr_activity: Mutex<HashMap<BotId, Arc<QrActivity>>>,
}

impl Drop for RemoteSnowLumaBackend {
    fn drop(&mut self) {
        for (_, task) in self.reconcile_metrics.get_mut().drain() {
            task.abort();
        }
    }
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
            reconcile_metrics: Mutex::new(HashMap::new()),
            snowluma_data_root: None,
            login_sessions: Mutex::new(HashMap::new()),
            login_clients: Mutex::new(HashMap::new()),
            login_pids: Mutex::new(HashMap::new()),
            hot_detached: Mutex::new(HashSet::new()),
            pending_spawns: std::sync::Mutex::new(HashMap::new()),
            completed_starts: Mutex::new(HashMap::new()),
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

    pub async fn reconcile_running_pid(
        &self,
        bot_id: &BotId,
        allow_single: bool,
    ) -> Result<Option<u32>, BotBackendError> {
        self.observe_reconciled_running(bot_id, allow_single)
            .await
            .map(|observed| observed.pid)
    }

    pub async fn observe_reconciled_running(
        &self,
        bot_id: &BotId,
        allow_single: bool,
    ) -> Result<ReconciledQqObservation, BotBackendError> {
        if self.hot_detached.lock().await.contains(bot_id) {
            return Ok(ReconciledQqObservation {
                bot_id: bot_id.clone(),
                backend_id: self.backend_id.clone(),
                host: self.daemon.current_host().await,
                pid: None,
                reusable: None,
            });
        }
        self.probe_running_observation(bot_id, allow_single).await
    }

    async fn explicit_start_pid(&self, bot_id: &BotId) -> Result<Option<u32>, BotBackendError> {
        if self.hot_detached.lock().await.contains(bot_id) {
            let tracked = self.login_pids.lock().await.get(bot_id).cloned();
            if let Some(tracked) = tracked {
                let host = self.daemon.current_host().await;
                let qq_id = bot_id
                    .as_str()
                    .parse::<u64>()
                    .map_err(|_| BotBackendError::InvalidConfig("QQ 账号无效".into()))?;
                return match qq_process::inspect(
                    host.as_ref(),
                    &self.daemon.layout().qq_bin,
                    qq_id,
                    tracked.pid,
                    &tracked.start_time,
                )
                .await?
                {
                    qq_process::Identity::Verified => Ok(Some(tracked.pid)),
                    _ => Err(BotBackendError::Io(
                        "原热启动 QQ 进程已退出或变化，已拒绝接管其它进程".into(),
                    )),
                };
            }
        }
        self.probe_running_pid(bot_id, false).await
    }

    async fn probe_running_pid(
        &self,
        bot_id: &BotId,
        allow_single: bool,
    ) -> Result<Option<u32>, BotBackendError> {
        self.probe_running_observation(bot_id, allow_single)
            .await
            .map(|observed| observed.pid)
    }

    async fn probe_running_observation(
        &self,
        bot_id: &BotId,
        allow_single: bool,
    ) -> Result<ReconciledQqObservation, BotBackendError> {
        let probe = self.daemon.in_current_session(async {
            let qq_id = bot_id
                .as_str()
                .parse::<u64>()
                .map_err(|_| BotBackendError::InvalidConfig("QQ 账号无效".into()))?;
            let host = self.daemon.current_host().await;
            let observed = |pid, reusable| ReconciledQqObservation {
                bot_id: bot_id.clone(),
                backend_id: self.backend_id.clone(),
                host: Arc::clone(&host),
                pid,
                reusable,
            };
            let tracked = self.login_pids.lock().await.get(bot_id).cloned();
            if let Some(tracked) = tracked {
                if let Some(reusable) = self.observe_reusable_poller(bot_id, tracked.pid).await {
                    return Ok(observed(Some(tracked.pid), Some(reusable)));
                }
                match qq_process::inspect(
                    host.as_ref(),
                    self.daemon.layout().qq_bin.as_str(),
                    qq_id,
                    tracked.pid,
                    &tracked.start_time,
                )
                .await?
                {
                    qq_process::Identity::Verified => return Ok(observed(Some(tracked.pid), None)),
                    qq_process::Identity::Absent => {}
                    qq_process::Identity::Replaced => {
                        return Err(BotBackendError::Io(
                            "已绑定的 QQ 进程身份已变化，请刷新状态后重试".into(),
                        ));
                    }
                }
            }
            let hint = self.daemon.paths().pid_bot_path(bot_id.as_str());
            let Some(pid) = remote_qq_running_pid_with_hint(
                host.as_ref(),
                qq_id,
                Some(hint.as_str()),
                Some(self.daemon.layout().qq_bin.as_str()),
                allow_single,
            )
            .await?
            else {
                if self
                    .pending_spawns
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .contains_key(bot_id)
                {
                    return Err(BotBackendError::Io(
                        "QQ 启动操作的进程归属尚未确认，不能确认已停止".into(),
                    ));
                }
                return Ok(observed(None, None));
            };
            let birth = login_helper::process_start_time(host.as_ref(), pid)
                .await
                .map_err(|error| BotBackendError::Io(error.to_string()))?;
            match qq_process::inspect(
                host.as_ref(),
                self.daemon.layout().qq_bin.as_str(),
                qq_id,
                pid,
                &birth,
            )
            .await?
            {
                qq_process::Identity::Verified => {}
                qq_process::Identity::Absent => return Ok(observed(None, None)),
                qq_process::Identity::Replaced => {
                    return Err(BotBackendError::Io(
                        "QQ 进程在探测期间发生变化，请重试".into(),
                    ));
                }
            }
            self.login_pids.lock().await.insert(
                bot_id.clone(),
                TrackedQq {
                    pid,
                    start_time: birth,
                },
            );
            Ok(observed(Some(pid), None))
        });
        tokio::time::timeout(Duration::from_secs(15), probe)
            .await
            .map_err(|_| BotBackendError::Io("获取目标 QQ 状态超时，请重试".into()))?
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
        if let Some(task) = self.reconcile_metrics.lock().await.remove(bot_id) {
            task.abort();
        }
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
        for (_, task) in self.reconcile_metrics.lock().await.drain() {
            task.abort();
        }
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
        self.daemon.release_bot(bot_id).await;
    }

    async fn reuse_reconciled_poller(&self, bot_id: &BotId, pid: u32) -> bool {
        self.observe_reusable_poller(bot_id, pid).await.is_some()
    }

    async fn reusable_is_current(&self, bot_id: &BotId, reusable: &ReusablePoller) -> bool {
        if self.hot_detached.lock().await.contains(bot_id)
            || !self.pollers.lock().await.contains_key(bot_id)
            || self.login_pids.lock().await.get(bot_id) != Some(&reusable.tracked)
        {
            return false;
        }
        let host = self.daemon.current_host().await;
        Arc::ptr_eq(&host, &reusable.host)
            && self
                .login_clients
                .lock()
                .await
                .get(bot_id)
                .is_some_and(|current| Arc::ptr_eq(current, &reusable.client))
    }

    async fn observe_reusable_poller(&self, bot_id: &BotId, pid: u32) -> Option<ReusablePoller> {
        if !self.pollers.lock().await.contains_key(bot_id) {
            return None;
        }
        let Some(tracked) = self.login_pids.lock().await.get(bot_id).cloned() else {
            return None;
        };
        if tracked.pid != pid {
            return None;
        }
        let Some(client) = self.login_clients.lock().await.get(bot_id).cloned() else {
            return None;
        };
        let host = self.daemon.current_host().await;
        let (birth, processes) = tokio::join!(
            login_helper::process_start_time(host.as_ref(), pid),
            client.list_processes(),
        );
        let verified = birth.is_ok_and(|birth| birth == tracked.start_time)
            && processes.is_ok_and(|processes| {
                processes.iter().any(|process| {
                    process.pid == pid
                        && !matches!(
                            process.status,
                            crate::snowluma::webui_client::HookProcessStatus::Error
                                | crate::snowluma::webui_client::HookProcessStatus::Disconnected
                        )
                })
            });
        let reusable = ReusablePoller {
            tracked,
            client,
            host,
        };
        if verified && self.reusable_is_current(bot_id, &reusable).await {
            Some(reusable)
        } else {
            None
        }
    }

    pub async fn attach_reconciled_observation(
        &self,
        bot_id: BotId,
        observation: ReconciledQqObservation,
        config: &BotConfig,
    ) -> Result<(), BotBackendError> {
        self.daemon
            .in_current_session(async {
                let host = self.daemon.current_host().await;
                if observation.bot_id != bot_id
                    || observation.backend_id != self.backend_id
                    || !Arc::ptr_eq(&host, &observation.host)
                {
                    return Err(BotBackendError::Io("本轮 QQ 观测已失效，请重新刷新".into()));
                }
                let Some(pid) = observation.pid else {
                    return Ok(());
                };
                if let Some(reusable) = observation.reusable
                    && self.reusable_is_current(&bot_id, &reusable).await
                {
                    return Ok(());
                }
                self.attach_reconciled_running(bot_id, pid, config).await
            })
            .await
    }

    async fn prepare_reconcile_metrics(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        host: Arc<dyn Host>,
    ) {
        let Some(injector) = self.metrics_injector.clone() else {
            return;
        };
        let daemon = Arc::clone(&self.daemon);
        let config = config.clone();
        let id = bot_id.clone();
        let (done, completed) = tokio::sync::oneshot::channel();
        let mut tasks = self.reconcile_metrics.lock().await;
        tasks.retain(|_, task| !task.is_finished());
        let task = tokio::spawn(async move {
            let _ = daemon
                .in_current_session(async {
                    if !Arc::ptr_eq(&host, &daemon.current_host().await) {
                        return Ok(());
                    }
                    if let Some(env) = injector
                        .prepare(host.as_ref(), daemon.remote_home(), id.as_str(), &config)
                        .await
                    {
                        daemon.apply_metrics_node_env(Some(env)).await?;
                    }
                    Ok(())
                })
                .await;
            let _ = done.send(());
        });
        if let Some(previous) = tasks.insert(bot_id.clone(), task) {
            previous.abort();
        }
        drop(tasks);
        // Optional metrics uploads must not hold login recovery behind slow SFTP.
        let _ = tokio::time::timeout(Duration::from_secs(2), completed).await;
    }

    /// 供 bootstrap reconcile 取日志 follow 路径,不暴露整个 daemon
    pub fn daemon_paths(&self) -> &SnowLumaRemotePaths {
        self.daemon.paths()
    }

    pub fn qq_bin(&self) -> &str {
        &self.daemon.layout().qq_bin
    }

    /// 只恢复已归因到此 Bot 的 QQ 会话，WebUI 不可用时也可重新核对进程。
    pub async fn prepare_novnc_for_bot(
        &self,
        bot_id: &BotId,
        allow_single_main_fallback: bool,
    ) -> Result<super::tunnel::RemoteSnowLumaTunnelEndpoints, BotBackendError> {
        let qq_id = bot_id
            .as_str()
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or_else(|| BotBackendError::InvalidConfig("无效的 QQ 账号".into()))?;
        self.daemon
            .in_current_session(async {
                let host = self.daemon.current_host().await;
                let tracked = self.login_pids.lock().await.get(bot_id).cloned();
                let tracked = match tracked {
                    Some(tracked)
                        if login_helper::process_start_time(host.as_ref(), tracked.pid)
                            .await
                            .ok()
                            .as_deref()
                            == Some(tracked.start_time.as_str()) =>
                    {
                        Some(tracked)
                    }
                    _ => None,
                };
                let tracked = match tracked {
                    Some(tracked) => tracked,
                    None => {
                        let pid_file = self.daemon.paths().pid_bot_path(bot_id.as_str());
                        let pid = remote_qq_running_pid_with_hint(
                            host.as_ref(),
                            qq_id,
                            Some(&pid_file),
                            Some(self.qq_bin()),
                            allow_single_main_fallback,
                        )
                        .await?
                        .ok_or_else(|| {
                            BotBackendError::Io(
                                "未找到可归属于此 Bot 的 QQ 主进程，请确认 Bot 已启动并刷新运行状态"
                                    .into(),
                            )
                        })?;
                        let start_time = login_helper::process_start_time(host.as_ref(), pid)
                            .await
                            .map_err(|_| {
                                BotBackendError::Io(
                                    "无法读取目标 QQ 进程身份：进程已退出或当前 SSH 用户无读取权限"
                                        .into(),
                                )
                            })?;
                        TrackedQq { pid, start_time }
                    }
                };
                self.daemon
                    .ensure_novnc_ready(tracked.pid, &tracked.start_time, qq_id)
                    .await
            })
            .await
    }

    /// 接管存活 QQ；WebUI 失败返回调用方，QQ 运行态由 reconcile 单独保留。
    pub async fn attach_reconciled_running(
        &self,
        bot_id: BotId,
        pid: u32,
        config: &BotConfig,
    ) -> Result<(), BotBackendError> {
        if self.hot_detached.lock().await.contains(&bot_id) {
            return Ok(());
        }
        self.daemon
            .in_current_session(async {
                // SSH 重连可能换了本地端口；旧 poller 存在不代表仍能访问新的隧道。
                let started = Instant::now();
                if self.reuse_reconciled_poller(&bot_id, pid).await {
                    return Ok(());
                }
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

                self.prepare_reconcile_metrics(&bot_id, config, Arc::clone(&host))
                    .await;

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
                let birth = login_helper::process_start_time(host.as_ref(), pid)
                    .await
                    .map_err(|error| BotBackendError::Io(error.to_string()))?;
                let webui_pid = self.daemon.webui_process_pid(pid, &birth).await?;
                let http = match inject_via_tunnel(&endpoints, pid, webui_pid).await {
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
                    proc_tree: Arc::new(
                        crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(pid),
                    ),
                    expected_uin: Some(qq_id),
                };
                self.replace_poller(bot_id.clone(), pid, poller_deps).await;
                tracing::info!(
                    target: "ncd_backend_snowluma::remote",
                    %bot_id,
                    elapsed_ms = started.elapsed().as_millis(),
                    "远端 SnowLuma 登录状态探测已恢复"
                );
                Ok(())
            })
            .await
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
        self.start_cancellable(ctx, CancellationToken::new()).await
    }

    async fn start_cancellable(
        &self,
        ctx: &BotStartCtx,
        cancel: CancellationToken,
    ) -> Result<BotStatus, BotBackendError> {
        let cancel = cancel.child_token();
        super::startup_cancel::check(Some(&cancel))?;
        let original_host = self.daemon.current_host().await;
        let submitted = std::sync::Mutex::new(None::<PendingSpawn>);
        let completed = std::sync::Mutex::new(None::<CompletedStart>);
        let previous_client = self
            .login_clients
            .lock()
            .await
            .get(&ctx.config.bot_id)
            .cloned();
        let staged_client = std::sync::Mutex::new(previous_client);
        let injected_here = AtomicBool::new(false);
        let record_client = |client| {
            *staged_client
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(client);
        };
        let record_spawn = |pid, start_time| {
            *submitted.lock().unwrap_or_else(|error| error.into_inner()) = Some(PendingSpawn {
                pid,
                start_time,
                host: Arc::clone(&original_host),
            });
        };
        let start = self.daemon.in_current_session(async {
        super::startup_cancel::check(Some(&cancel))?;
        if !Arc::ptr_eq(&original_host, &self.daemon.current_host().await) {
            return Err(BotBackendError::Io("启动主机连接已更新，请重试".into()));
        }
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
        let start_mode = resolve_start_mode(config);
        if start_mode.is_cold() && self.hot_detached.lock().await.contains(&bot_id) {
            return Err(BotBackendError::InvalidConfig(
                "保留的 QQ 属于热启动会话，请以热启动恢复".into(),
            ));
        }
        let known_running_pid = super::startup_cancel::read_only(Some(&cancel), self.explicit_start_pid(&bot_id)).await?;
        super::startup_cancel::check(Some(&cancel))?;
        self.start_modes.lock().await.insert(bot_id.clone(), start_mode);

        // 指标资产先上传，再随 node 第一次拉起带上 env，避免先听口再 SIGTERM 重启
        // 导致隧道打到已死的旧 WebUI 口、界面卡在启动中。
        let host = Arc::clone(&original_host);
        let metrics_env = if let Some(inj) = &self.metrics_injector {
            let home = self.daemon.remote_home();
            inj.prepare(host.as_ref(), home, bot_id.as_str(), config)
                .await
        } else {
            None
        };
        super::startup_cancel::check(Some(&cancel))?;

        // 首次接管可换密，先移除本 Bot 的旧凭据；其他 Bot 的 lease 由 daemon 保护。
        self.dispose_poller(&bot_id).await;
        let override_pwd = if config.bot.webui_password_takeover {
            self.webui_password_override()
        } else {
            None
        };
        self.daemon
            .ensure_running_for_bot_cancellable(
                &bot_id,
                metrics_env,
                config.bot.webui_password_takeover,
                override_pwd.as_deref(),
                &cancel,
            )
            .await?;
        super::startup_cancel::check(Some(&cancel))?;

        let started = async {
            let paths = self.daemon.paths();
            if let Err(e) =
                render_native_snowluma_config_on_host(host.as_ref(), &bot_id, config, paths).await
            {
                return Err(e);
            }
            super::startup_cancel::check(Some(&cancel))?;

            let layout = self.daemon.layout();

            let pid = match start_mode {
                SnowLumaStartMode::HotStart => {
                    if let Some(pid) = known_running_pid {
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

                    super::startup_cancel::check(Some(&cancel))?;
                    let already_running = known_running_pid.is_some();
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
                    super::startup_cancel::check(Some(&cancel))?;
                    match super::orchestrator::bot_cold_start_cancellable(
                        host.as_ref(),
                        layout,
                        &qq_id_str,
                        &qq_id_str,
                        prepared.as_ref(),
                        known_running_pid,
                        Some(&cancel),
                        Some(&record_spawn),
                    )
                    .await
                    {
                        Ok(pid) => {
                            let pending = submitted.lock().unwrap_or_else(|error| error.into_inner()).clone();
                            if let Some(PendingSpawn { start_time: Some(birth), .. }) = pending {
                                self.login_pids.lock().await.insert(bot_id.clone(), TrackedQq { pid, start_time: birth });
                            }
                            super::startup_cancel::check(Some(&cancel))?;
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
                            super::startup_cancel::check(Some(&cancel))?;
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
            let birth = login_helper::process_start_time(host.as_ref(), pid)
                .await
                .map_err(|error| BotBackendError::Io(error.to_string()))?;
            if qq_process::inspect(host.as_ref(), &layout.qq_bin, qq_id, pid, &birth).await?
                != qq_process::Identity::Verified
            {
                return Err(BotBackendError::Io("启动后的 QQ 进程身份未能确认".into()));
            }
            self.login_pids.lock().await.insert(
                bot_id.clone(), TrackedQq { pid, start_time: birth.clone() },
            );
            super::startup_cancel::check(Some(&cancel))?;
            let webui_pid = self.daemon.webui_process_pid(pid, &birth).await?;
            let http = match super::inject::inject_via_tunnel_cancellable(&endpoints, pid, webui_pid, Some(&cancel), Some(&record_client), Some(&injected_here)).await {
                Ok(c) => c,
                Err(e) => return Err(e),
            };
            self.login_clients.lock().await.insert(bot_id.clone(), Arc::clone(&http));
            super::startup_cancel::check(Some(&cancel))?;

            self.event_bus
                .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), pid));
            self.start_modes
                .lock()
                .await
                .insert(bot_id.clone(), start_mode);

            let poller_deps = PollerDeps {
                event_bus: Arc::clone(&self.event_bus),
                http: Arc::clone(&http),
                proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(
                    pid,
                )),
                expected_uin: Some(qq_id_str.clone()),
            };
            self.replace_poller(bot_id.clone(), pid, poller_deps).await;
            super::startup_cancel::check(Some(&cancel))?;
            self.hot_detached.lock().await.remove(&bot_id);
            self.daemon.confirm_bot_started(&bot_id).await;
            super::startup_cancel::check(Some(&cancel))?;

            self.event_bus
                .publish(DomainEvent::SnowLumaDockerEndpointsReady {
                    bot_id: bot_id.clone(),
                });

            let status = BotStatus::running(bot_id.clone(), pid, 0);
            *completed.lock().unwrap_or_else(|error| error.into_inner()) = Some(CompletedStart {
                status: status.clone(),
                host: Arc::clone(&original_host),
                tracked: TrackedQq { pid, start_time: birth },
                mode: start_mode,
                client: http,
            });
            Ok(status)
        }
        .await;
        started
        });
        tokio::pin!(start);
        let result = tokio::select! {
            result = &mut start => result,
            _ = tokio::time::sleep(Duration::from_secs(180)) => {
                cancel.cancel();
                start.await
            }
        };
        let owned = submitted
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let saved_client = staged_client
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let finalized = self.daemon.on_current_host(&original_host, async {
          if result.is_err() {
            if let Some(client) = saved_client.as_ref() {
                self.login_clients.lock().await.insert(ctx.config.bot_id.clone(), Arc::clone(client));
            }
            if let Some(owned) = owned.as_ref() {
                self.pending_spawns.lock().unwrap_or_else(|error| error.into_inner()).insert(ctx.config.bot_id.clone(), owned.clone());
                let Some(birth) = owned.start_time.as_ref() else {
                    return Err(BotBackendError::Io(format!("启动未完成；本次 QQ PID {} 的身份尚未确认，请刷新后重试停止", owned.pid)));
                };
                self.login_pids.lock().await.insert(ctx.config.bot_id.clone(), TrackedQq { pid: owned.pid, start_time: birth.clone() });
                let qq_id = ctx.config.bot_id.as_str().parse::<u64>().map_err(|_| BotBackendError::Io("QQ 账号无效".into()))?;
                qq_process::stop(owned.host.as_ref(), &self.daemon.layout().qq_bin, qq_id, owned.pid, birth).await?;
                self.pending_spawns.lock().unwrap_or_else(|error| error.into_inner()).remove(&ctx.config.bot_id);
                self.login_pids.lock().await.remove(&ctx.config.bot_id);
                self.dispose_poller(&ctx.config.bot_id).await;
            } else if cancel.is_cancelled() {
                let hot = ctx.bot_config.as_ref().is_some_and(|config| !resolve_start_mode(config).is_cold());
                if injected_here.load(Ordering::SeqCst) || (hot && saved_client.is_some()) {
                    let client = saved_client.as_ref().ok_or_else(|| BotBackendError::Io("取消启动后缺少卸载客户端，状态尚未确认".into()))?;
                    let tracked = self.login_pids.lock().await.get(&ctx.config.bot_id).cloned()
                        .ok_or_else(|| BotBackendError::Io("取消启动后 QQ 归属尚未确认".into()))?;
                    let unloaded = client.unload_process(tracked.pid).await
                        .map_err(|error| BotBackendError::Io(format!("取消启动后未能确认卸载：{error}")))?;
                    if unloaded.pid != tracked.pid || unloaded.injected == Some(true)
                        || !matches!(unloaded.status, crate::snowluma::webui_client::HookProcessStatus::Available | crate::snowluma::webui_client::HookProcessStatus::Disconnected)
                    {
                        return Err(BotBackendError::Io("取消启动后目标 QQ 尚未确认卸载".into()));
                    }
                }
                if hot {
                    self.start_modes.lock().await.insert(ctx.config.bot_id.clone(), SnowLumaStartMode::HotStart);
                    self.hot_detached.lock().await.insert(ctx.config.bot_id.clone());
                    self.dispose_poller(&ctx.config.bot_id).await;
                }
            }
          } else {
            self.pending_spawns.lock().unwrap_or_else(|error| error.into_inner()).remove(&ctx.config.bot_id);
            let proof = completed.lock().unwrap_or_else(|error| error.into_inner()).clone()
                .ok_or_else(|| BotBackendError::Io("启动成功但缺少原进程归属记录".into()))?;
            self.completed_starts.lock().await.insert(ctx.config.bot_id.clone(), proof);
          }
          Ok(())
        }).await?;
        if finalized.is_none() {
            // An old startup owns only its captured Host and freshly spawned QQ.
            if let Some(owned) = owned {
                let birth = owned.start_time.as_deref().ok_or_else(|| {
                    BotBackendError::Io("连接已更新，旧启动的 QQ 归属尚未确认".into())
                })?;
                let qq_id = ctx
                    .config
                    .bot_id
                    .as_str()
                    .parse::<u64>()
                    .map_err(|_| BotBackendError::Io("QQ 账号无效".into()))?;
                qq_process::stop(
                    owned.host.as_ref(),
                    &self.daemon.layout().qq_bin,
                    qq_id,
                    owned.pid,
                    birth,
                )
                .await?;
            }
            return Err(BotBackendError::Io(
                "启动连接已更新，请刷新当前状态后重试".into(),
            ));
        }
        if result.is_err() {
            self.daemon
                .abandon_start_on_host(&original_host, &ctx.config.bot_id)
                .await;
        }
        result
    }

    async fn cleanup_unacknowledged_start(
        &self,
        started: &BotStatus,
    ) -> Result<(), BotBackendError> {
        let proof = self
            .completed_starts
            .lock()
            .await
            .get(&started.bot_id)
            .cloned()
            .filter(|proof| proof.status == *started && started.pid == Some(proof.tracked.pid))
            .ok_or_else(|| {
                BotBackendError::Io("未确认启动的原进程归属已不可用，已拒绝清理其它会话".into())
            })?;
        let qq_id = started
            .bot_id
            .as_str()
            .parse::<u64>()
            .map_err(|_| BotBackendError::InvalidConfig("QQ 账号无效".into()))?;
        let cleaned = self
            .daemon
            .on_current_host(&proof.host, async {
                let same_process =
                    self.login_pids.lock().await.get(&started.bot_id) == Some(&proof.tracked);
                let same_mode =
                    self.start_modes.lock().await.get(&started.bot_id) == Some(&proof.mode);
                let same_client = self
                    .login_clients
                    .lock()
                    .await
                    .get(&started.bot_id)
                    .is_some_and(|client| Arc::ptr_eq(client, &proof.client));
                if !same_process || !same_mode || !same_client {
                    return Err(BotBackendError::Io(
                        "未确认启动的会话已被替换，已拒绝清理当前会话".into(),
                    ));
                }
                if proof.mode.is_cold() {
                    qq_process::stop(
                        proof.host.as_ref(),
                        &self.daemon.layout().qq_bin,
                        qq_id,
                        proof.tracked.pid,
                        &proof.tracked.start_time,
                    )
                    .await?;
                } else {
                    let unloaded = proof
                        .client
                        .unload_process(proof.tracked.pid)
                        .await
                        .map_err(|error| {
                            BotBackendError::Io(format!("未确认启动的 QQ 尚未确认卸载：{error}"))
                        })?;
                    if unloaded.pid != proof.tracked.pid
                        || unloaded.injected == Some(true)
                        || !matches!(
                            unloaded.status,
                            crate::snowluma::webui_client::HookProcessStatus::Available
                                | crate::snowluma::webui_client::HookProcessStatus::Disconnected
                        )
                    {
                        return Err(BotBackendError::Io(
                            "未确认启动的目标 QQ 尚未确认卸载".into(),
                        ));
                    }
                }
                self.dispose_poller(&started.bot_id).await;
                if proof.mode.is_cold() {
                    self.login_pids.lock().await.remove(&started.bot_id);
                    self.start_modes.lock().await.remove(&started.bot_id);
                    self.hot_detached.lock().await.remove(&started.bot_id);
                } else {
                    self.hot_detached
                        .lock()
                        .await
                        .insert(started.bot_id.clone());
                }
                self.completed_starts.lock().await.remove(&started.bot_id);
                Ok(())
            })
            .await?;
        if cleaned.is_some() {
            self.daemon
                .abandon_start_on_host(&proof.host, &started.bot_id)
                .await;
            return Ok(());
        }
        if !proof.mode.is_cold() {
            return Err(BotBackendError::Io(
                "原热启动连接已更换，无法安全确认卸载，已保留当前会话".into(),
            ));
        }
        qq_process::stop(
            proof.host.as_ref(),
            &self.daemon.layout().qq_bin,
            qq_id,
            proof.tracked.pid,
            &proof.tracked.start_time,
        )
        .await
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        if self.hot_detached.lock().await.contains(&bot_id) {
            return Ok(());
        }
        let qq_id = bot_id
            .as_str()
            .parse::<u64>()
            .map_err(|_| BotBackendError::InvalidConfig("QQ 账号无效".into()))?;
        let start_mode = self
            .start_modes
            .lock()
            .await
            .get(&bot_id)
            .copied()
            .unwrap_or(SnowLumaStartMode::ColdStart);
        let client = self.login_clients.lock().await.get(&bot_id).cloned();
        if let Some(task) = self.reconcile_metrics.lock().await.remove(&bot_id) {
            task.abort();
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), self.close_qr_login(&bot_id)).await;
        let host = self.daemon.current_host().await;
        let stopped = self.daemon.in_current_session(async {
            let mut tracked = self.login_pids.lock().await.get(&bot_id).cloned();
            if tracked.is_none() && start_mode.is_cold() {
                let status = self.status(bot_id.clone()).await?;
                if status.state != ncd_domain::bot_actor::BotActorState::Stopped {
                    tracked = self.login_pids.lock().await.get(&bot_id).cloned();
                    if tracked.is_none() {
                        return Err(BotBackendError::Io(
                            "尚未绑定目标 QQ 进程，已拒绝停止".into(),
                        ));
                    }
                }
            }
            if let Some(tracked) = tracked.as_ref() {
                if start_mode.is_cold() {
                    qq_process::stop(
                        host.as_ref(),
                        self.daemon.layout().qq_bin.as_str(),
                        qq_id,
                        tracked.pid,
                        &tracked.start_time,
                    )
                    .await?;
                } else if let Some(client) = client.as_ref() {
                    match qq_process::inspect(
                        host.as_ref(),
                        self.daemon.layout().qq_bin.as_str(),
                        qq_id,
                        tracked.pid,
                        &tracked.start_time,
                    )
                    .await?
                    {
                        qq_process::Identity::Verified => {
                            // The cached client translates the host PID into its verified namespace.
                            let unloaded = client.unload_process(tracked.pid).await.map_err(|error| {
                                BotBackendError::Io(format!(
                                    "卸载目标 QQ 的 SnowLuma 失败：{error}"
                                ))
                            })?;
                            if unloaded.pid != tracked.pid
                                || unloaded.injected == Some(true)
                                || !matches!(unloaded.status,
                                    crate::snowluma::webui_client::HookProcessStatus::Available
                                    | crate::snowluma::webui_client::HookProcessStatus::Disconnected)
                            {
                                return Err(BotBackendError::Io("目标 QQ 的 SnowLuma 尚未确认卸载".into()));
                            }
                        }
                        qq_process::Identity::Absent => {}
                        qq_process::Identity::Replaced => {
                            return Err(BotBackendError::Io(
                                "目标 QQ 进程已变化，已拒绝操作其它进程".into(),
                            ));
                        }
                    }
                } else {
                    return Err(BotBackendError::Io("WebUI 连接未就绪，无法确认已卸载目标 QQ".into()));
                }
            }
            Ok(tracked)
        });
        let tracked = tokio::time::timeout(Duration::from_secs(25), stopped)
            .await
            .map_err(|_| BotBackendError::Io("停止目标 QQ 超时，请刷新状态后重试".into()))??;
        // Rejected stops retain the working status client; shared teardown starts only after success.
        self.dispose_poller(&bot_id).await;
        if start_mode.is_cold() {
            {
                let mut pids = self.login_pids.lock().await;
                if pids.get(&bot_id) == tracked.as_ref() {
                    pids.remove(&bot_id);
                }
            }
            self.start_modes.lock().await.remove(&bot_id);
            self.pending_spawns
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&bot_id);
        } else {
            self.hot_detached.lock().await.insert(bot_id.clone());
        }
        let session = self.login_sessions.lock().await.remove(&bot_id);
        if let Some(session) = session {
            let _ = tokio::time::timeout(Duration::from_secs(2), session.remove_all(host.as_ref()))
                .await;
        }
        // A slow shared-daemon cleanup must not obscure confirmed QQ termination.
        if tokio::time::timeout(Duration::from_secs(5), self.daemon.release_bot(&bot_id))
            .await
            .is_err()
        {
            tracing::warn!(target: "ncd_backend_snowluma::remote", %bot_id,
                "目标 QQ 已停止，共享 SnowLuma 清理超时，保留远端进程");
        }
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        match self.reconcile_running_pid(&bot_id, false).await? {
            Some(pid) => Ok(BotStatus::running(bot_id, pid, 0)),
            None => Ok(BotStatus::stopped(bot_id)),
        }
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

#[cfg(test)]
#[path = "backend_reconcile_tests.rs"]
mod reconcile_tests;

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
