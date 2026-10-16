//! 稳定用户会话中的 Bot 管理，控制服务重启不释放 QQ 所有权。
mod config;
mod logs;
mod napcat;
mod schedule;
mod state;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use ncd_backend_snowluma::{
    ReqwestSnowLumaWebUiClientFactory, SnowLumaDaemon, SnowLumaRuntimeBackend,
};
use ncd_domain::windows_bot::{WindowsBotEndpoint, WindowsBotEventBatch, WindowsBotSpec};
use ncd_domain::windows_remote::{WindowsAgentAction, WindowsAgentError, WindowsAgentResult};
use ncd_domain::{BackendType, BotId, BotStatus, DomainEvent, StopMode};
use ncd_traits::events::{BroadcastEventBus, EventBus, EventFilter};
use ncd_traits::runtime_backend::{BotBackend, BotRuntimeConfig, BotStartCtx, TailOpts};
use tokio::sync::Mutex;

struct ManagedBot {
    backend: Arc<dyn BotBackend>,
    spec: WindowsBotSpec,
    started: BotStatus,
    identity: Option<(u32, u64, u32)>,
    next_restart: Option<chrono::DateTime<chrono::Local>>,
}

#[derive(Default)]
struct EventHistory {
    generation: String,
    sequence: u64,
    entries: VecDeque<(u64, DomainEvent)>,
    napcat_endpoints: HashMap<BotId, WindowsBotEndpoint>,
}

pub struct BotService {
    root: PathBuf,
    bus: Arc<BroadcastEventBus>,
    bots: Mutex<HashMap<String, ManagedBot>>,
    daemons: Mutex<HashMap<String, Arc<SnowLumaDaemon>>>,
    operation: Mutex<()>,
    initialized: AtomicBool,
    history: Arc<Mutex<EventHistory>>,
    recovery: Mutex<HashMap<String, (u8, std::time::Instant)>>,
    offline: Arc<Mutex<std::collections::HashSet<BotId>>>,
    restart_budget: Mutex<HashMap<String, (std::time::Instant, u8)>>,
}

impl BotService {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            bus: Arc::new(BroadcastEventBus::default()),
            bots: Mutex::new(HashMap::new()),
            daemons: Mutex::new(HashMap::new()),
            operation: Mutex::new(()),
            initialized: AtomicBool::new(false),
            history: Arc::new(Mutex::new(EventHistory {
                generation: uuid::Uuid::new_v4().to_string(),
                ..Default::default()
            })),
            recovery: Mutex::new(HashMap::new()),
            offline: Arc::new(Mutex::new(std::collections::HashSet::new())),
            restart_budget: Mutex::new(HashMap::new()),
        }
    }

    pub async fn resume(&self) {
        let _operation = self.operation.lock().await;
        self.initialize().await;
    }

    pub async fn retry_pending(&self) {
        let _operation = self.operation.lock().await;
        let candidates: Vec<_> = self
            .bots
            .lock()
            .await
            .iter()
            .map(|(id, record)| {
                (
                    id.clone(),
                    record.backend.clone(),
                    record.spec.clone(),
                    record.next_restart,
                    record.identity,
                )
            })
            .collect();
        let offline = std::mem::take(&mut *self.offline.lock().await);
        for (id, backend, spec, due, identity) in candidates {
            if config::check_intent(&self.root, &spec).await.is_err() {
                continue;
            }
            let Ok(status) = backend.status(BotId::new(&id)).await else {
                continue;
            };
            let crashed = !status.state.is_active();
            let scheduled = due.is_some_and(|due| chrono::Local::now() >= due);
            let invalidated =
                spec.config.bot.offline_auto_restart && offline.contains(&BotId::new(&id));
            if !crashed && !scheduled && !invalidated {
                continue;
            }
            let may_stop = match verify_stop_identity(identity) {
                Ok(may_stop) => may_stop,
                Err(error) => {
                    tracing::warn!(%error, "暂缓恢复身份不明的托管 Bot");
                    continue;
                }
            };
            if !scheduled {
                let mut budgets = self.restart_budget.lock().await;
                let (since, attempts) = budgets
                    .entry(id.clone())
                    .or_insert((std::time::Instant::now(), 0));
                if since.elapsed() >= std::time::Duration::from_secs(600) {
                    *since = std::time::Instant::now();
                    *attempts = 0;
                }
                if *attempts >= 3 {
                    continue;
                }
                *attempts += 1;
            }
            if may_stop {
                if let Err(error) = backend.stop(BotId::new(&id), StopMode::Force).await {
                    tracing::warn!(%error, "停止待恢复 Bot 失败");
                    continue;
                }
            }
            if let Err(error) = state::clear(&self.root, &id) {
                tracing::warn!(%error, "清理已退出 Bot 运行身份失败");
                continue;
            }
            self.bots.lock().await.remove(&id);
            if let Err(error) = self.start(&spec, false).await {
                tracing::warn!(%error, "恢复托管 Bot 失败");
            }
        }
        self.restore_pending().await;
    }

    pub async fn handle(&self, action: &WindowsAgentAction) -> Option<WindowsAgentResult> {
        if !matches!(
            action,
            WindowsAgentAction::BotStart(_)
                | WindowsAgentAction::BotConfigure(_)
                | WindowsAgentAction::BotStop { .. }
                | WindowsAgentAction::BotStatus { .. }
                | WindowsAgentAction::BotTail { .. }
                | WindowsAgentAction::BotQr { .. }
                | WindowsAgentAction::BotCloseQr { .. }
                | WindowsAgentAction::BotEvents { .. }
                | WindowsAgentAction::BotEndpoint { .. }
        ) {
            return None;
        }
        let _operation = self.operation.lock().await;
        self.initialize().await;
        Some(match self.dispatch(action).await {
            Ok(value) => value,
            Err(message) => WindowsAgentResult::Error(WindowsAgentError {
                code: "bot_operation_failed".into(),
                message,
                retryable: false,
            }),
        })
    }

    async fn initialize(&self) {
        if self.initialized.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut subscription = self.bus.subscribe(EventFilter::all());
        let history = Arc::clone(&self.history);
        let event_bus = Arc::clone(&self.bus);
        let offline = Arc::clone(&self.offline);
        let log_root = self.root.clone();
        tokio::spawn(async move {
            let mut napcat_pollers = HashMap::new();
            while let Some(event) = subscription.next().await {
                let logged = match &event {
                    DomainEvent::BotLogAppended { bot_id, line, .. }
                        if config::valid_bot_id(bot_id.as_str()).is_ok() =>
                    {
                        logs::append(&log_root, bot_id.as_str(), line).await
                    }
                    DomainEvent::SnowLumaDaemonLog { line } => {
                        logs::append(&log_root, "snowluma-daemon", line).await
                    }
                    _ => Ok(()),
                };
                if let Err(error) = logged {
                    tracing::warn!(%error, "持久化会话日志失败");
                }
                match &event {
                    DomainEvent::NapCatWebuiAvailable {
                        bot_id,
                        port,
                        token,
                        ..
                    } => {
                        if let Ok(http) = ncd_backend_napcat::ReqwestNapCatWebUiClient::new() {
                            let poller = ncd_backend_napcat::NapCatLoginPoller::spawn(
                                bot_id.clone(),
                                *port,
                                token.clone(),
                                ncd_backend_napcat::PollerConfig::default(),
                                ncd_backend_napcat::PollerDeps {
                                    event_bus: Arc::clone(&event_bus),
                                    http: Arc::new(http),
                                    notifier: Arc::new(ncd_backend_napcat::NoopOfflineNotifier),
                                    restart_handle: Arc::new(PassiveRestart),
                                },
                            );
                            napcat_pollers.insert(bot_id.clone(), poller);
                        }
                    }
                    DomainEvent::BotProcessExited { bot_id, .. } => {
                        napcat_pollers.remove(bot_id);
                    }
                    DomainEvent::NapCatLoginInvalidated { bot_id, .. } => {
                        offline.lock().await.insert(bot_id.clone());
                    }
                    _ => {}
                }
                let mut history = history.lock().await;
                match &event {
                    DomainEvent::NapCatWebuiAvailable {
                        bot_id,
                        port,
                        token,
                        ..
                    } => {
                        history.napcat_endpoints.insert(
                            bot_id.clone(),
                            WindowsBotEndpoint {
                                backend: BackendType::NapCat,
                                host_port: *port,
                                secret: token.clone(),
                            },
                        );
                    }
                    DomainEvent::BotProcessExited { bot_id, .. } => {
                        history.napcat_endpoints.remove(bot_id);
                    }
                    _ => {}
                }
                history.sequence = history.sequence.saturating_add(1);
                let sequence = history.sequence;
                history.entries.push_back((sequence, event));
                while history.entries.len() > 2048 {
                    history.entries.pop_front();
                }
            }
        });
        self.restore_pending().await;
    }

    async fn restore_pending(&self) {
        let Ok(mut directory) = tokio::fs::read_dir(self.root.join("session-bots")).await else {
            return;
        };
        while let Ok(Some(entry)) = directory.next_entry().await {
            if entry.path().extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = entry
                .path()
                .file_stem()
                .and_then(|id| id.to_str())
                .unwrap_or("")
                .to_string();
            if config::valid_bot_id(&id).is_err() || self.bots.lock().await.contains_key(&id) {
                continue;
            }
            {
                let mut retry = self.recovery.lock().await;
                let (attempt, due) = retry
                    .entry(id.clone())
                    .or_insert((0, std::time::Instant::now()));
                if *attempt >= 6 || *due > std::time::Instant::now() {
                    continue;
                }
                *due = std::time::Instant::now()
                    + std::time::Duration::from_secs(5 * (1u64 << *attempt));
                *attempt += 1;
            }
            let restored = async {
                let bytes = tokio::fs::read(entry.path())
                    .await
                    .map_err(|e| e.to_string())?;
                let spec: WindowsBotSpec =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                self.start(&spec, false).await.map(|_| ())
            }
            .await;
            if let Err(error) = restored {
                tracing::warn!(%error, "恢复用户会话 Bot 失败，保留启动意图供显式重试");
            }
        }
    }

    async fn dispatch(&self, action: &WindowsAgentAction) -> Result<WindowsAgentResult, String> {
        match action {
            WindowsAgentAction::BotStart(spec) => self
                .start(spec, true)
                .await
                .map(WindowsAgentResult::BotStatus),
            WindowsAgentAction::BotStatus { bot_id } => {
                let backend = self.backend(bot_id).await;
                let record = self.bots.lock().await.get(bot_id).map(|record| {
                    (
                        record.started.pid,
                        record.started.started_at,
                        record.identity,
                    )
                });
                let mut status = match backend {
                    Some(backend) => backend
                        .status(BotId::new(bot_id))
                        .await
                        .map_err(|e| e.to_string())?,
                    None => BotStatus::stopped(BotId::new(bot_id)),
                };
                status.extra.remove("agent_generation");
                if status.state.is_active() && status.pid.is_none_or(|pid| pid == 0) {
                    if let Some((pid, started_at, _)) = record {
                        status.pid = pid;
                        status.started_at = started_at;
                    }
                }
                if !status.state.is_active() {
                    return Ok(WindowsAgentResult::BotStatus(status));
                }
                let expected = record.and_then(|(_, _, identity)| identity);
                if expected.is_some_and(|(pid, _, _)| status.pid != Some(pid)) {
                    return Ok(invalidated_status(
                        status,
                        "backend PID no longer matches the managed process",
                    ));
                }
                let observed = match status.pid.filter(|pid| *pid > 0) {
                    Some(pid) => crate::native::process_identity(pid)
                        .map(|identity| identity.map(|(created, session)| (pid, created, session))),
                    None => Err("managed process PID is not available".into()),
                };
                if let Ok(Some(identity)) = observed.as_ref() {
                    if expected.is_none_or(|expected| expected == *identity) {
                        if expected.is_none() || !state::is_recorded(&self.root, bot_id) {
                            state::record(&self.root, bot_id, Some(*identity))?;
                        }
                        if let Some(record) = self.bots.lock().await.get_mut(bot_id) {
                            record.identity = Some(*identity);
                        }
                    }
                }
                Ok(validate_status_identity(status, expected, observed))
            }
            WindowsAgentAction::BotConfigure(spec) => {
                self.configure(spec).await?;
                Ok(WindowsAgentResult::Done)
            }
            WindowsAgentAction::BotStop { bot_id } => {
                config::valid_bot_id(bot_id)?;
                let path = self
                    .root
                    .join("session-bots")
                    .join(format!("{bot_id}.json"));
                match tokio::fs::remove_file(path).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(format!("保存停止意图失败：{error}")),
                }
                if let Some(backend) = self.backend(bot_id).await {
                    let identity = self
                        .bots
                        .lock()
                        .await
                        .get(bot_id)
                        .and_then(|record| record.identity);
                    if verify_stop_identity(identity)? {
                        backend
                            .stop(BotId::new(bot_id), StopMode::Force)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    state::clear(&self.root, bot_id)?;
                    self.bots.lock().await.remove(bot_id);
                }
                self.recovery.lock().await.remove(bot_id);
                Ok(WindowsAgentResult::Done)
            }
            WindowsAgentAction::BotTail { bot_id, lines } => {
                config::valid_bot_id(bot_id)?;
                let source = self
                    .bots
                    .lock()
                    .await
                    .get(bot_id)
                    .map(|record| record.spec.config.bot.backend_type);
                if let Some(snapshot) =
                    logs::tail_bot(&self.root, bot_id, source, (*lines as usize).min(2000)).await
                {
                    return Ok(WindowsAgentResult::BotLog(snapshot));
                }
                let backend = self.backend(bot_id).await.ok_or("Bot 未运行")?;
                let log = backend
                    .tail_log(
                        BotId::new(bot_id),
                        TailOpts {
                            lines: (*lines as usize).min(2000),
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(WindowsAgentResult::BotLog(log))
            }
            WindowsAgentAction::BotQr { bot_id, refresh } => {
                if self.bots.lock().await.get(bot_id).is_some_and(|record| {
                    record.spec.config.bot.backend_type == BackendType::SnowLuma
                        && !record.spec.remote_qr_login_enabled
                }) {
                    let mut snapshot =
                        ncd_domain::SnowLumaQrLoginSnapshot::unavailable("已关闭远端扫码辅助");
                    snapshot.state = ncd_domain::SnowLumaQrLoginState::Disabled;
                    return Ok(WindowsAgentResult::QrLogin(snapshot));
                }
                let backend = self.backend(bot_id).await.ok_or("Bot 未运行")?;
                backend
                    .qr_login_snapshot(&BotId::new(bot_id), *refresh)
                    .await
                    .map(WindowsAgentResult::QrLogin)
                    .map_err(|e| e.to_string())
            }
            WindowsAgentAction::BotCloseQr { bot_id } => {
                if let Some(backend) = self.backend(bot_id).await {
                    backend
                        .close_qr_login(&BotId::new(bot_id))
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Ok(WindowsAgentResult::Done)
            }
            WindowsAgentAction::BotEndpoint { bot_id } => {
                let spec = self
                    .bots
                    .lock()
                    .await
                    .get(bot_id)
                    .map(|record| record.spec.clone())
                    .ok_or("Bot 未运行")?;
                if spec.config.bot.backend_type == BackendType::SnowLuma {
                    #[derive(serde::Deserialize)]
                    struct Runtime {
                        #[serde(rename = "webuiPort")]
                        port: u16,
                    }
                    let path = PathBuf::from(&spec.snowluma_dir).join("config/runtime.json");
                    let bytes = tokio::fs::read(path)
                        .await
                        .map_err(|error| error.to_string())?;
                    let runtime: Runtime =
                        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                    Ok(WindowsAgentResult::BotEndpoint(WindowsBotEndpoint {
                        backend: BackendType::SnowLuma,
                        host_port: runtime.port,
                        secret: spec.webui_password,
                    }))
                } else {
                    let history = self.history.lock().await;
                    let endpoint = history
                        .napcat_endpoints
                        .get(&BotId::new(bot_id))
                        .cloned()
                        .ok_or("NapCat WebUI 尚未就绪")?;
                    Ok(WindowsAgentResult::BotEndpoint(endpoint))
                }
            }
            WindowsAgentAction::BotEvents {
                bot_id,
                after,
                generation,
            } => {
                config::valid_bot_id(bot_id)?;
                let history = self.history.lock().await;
                if *after == u64::MAX {
                    return Ok(WindowsAgentResult::BotEvents(WindowsBotEventBatch {
                        generation: history.generation.clone(),
                        next: history.sequence,
                        events: Vec::new(),
                        event_sequences: Vec::new(),
                    }));
                }
                let mut events = Vec::new();
                let mut event_sequences = Vec::new();
                let effective_after = if *after > history.sequence
                    || generation
                        .as_ref()
                        .is_some_and(|generation| generation != &history.generation)
                {
                    0
                } else {
                    *after
                };
                let mut next = effective_after;
                let mut bytes = 0;
                for (sequence, event) in &history.entries {
                    if *sequence <= effective_after {
                        continue;
                    }
                    if event.bot_id().is_some_and(|id| id.as_str() == bot_id)
                        || matches!(
                            event,
                            DomainEvent::SnowLumaDaemonStateChanged { .. }
                                | DomainEvent::SnowLumaDaemonLog { .. }
                        )
                    {
                        let size = serde_json::to_vec(event)
                            .map_err(|error| error.to_string())?
                            .len();
                        if bytes + size > 512 * 1024 || events.len() >= 128 {
                            break;
                        }
                        bytes += size;
                        events.push(event.clone());
                        event_sequences.push(*sequence);
                    }
                    next = *sequence;
                }
                Ok(WindowsAgentResult::BotEvents(WindowsBotEventBatch {
                    generation: history.generation.clone(),
                    next,
                    events,
                    event_sequences,
                }))
            }
            _ => Err("不支持的 Bot 操作".into()),
        }
    }

    async fn backend(&self, bot_id: &str) -> Option<Arc<dyn BotBackend>> {
        self.bots
            .lock()
            .await
            .get(bot_id)
            .map(|record| Arc::clone(&record.backend))
    }

    async fn configure(&self, spec: &WindowsBotSpec) -> Result<(), String> {
        config::validate_spec(&self.root, spec)?;
        config::check_intent(&self.root, spec).await?;
        let existing = self
            .bots
            .lock()
            .await
            .get(&spec.bot_id)
            .map(|record| record.spec.clone())
            .ok_or("Bot 未由当前会话托管，未改写恢复记录")?;
        if existing.config.bot.backend_type != spec.config.bot.backend_type {
            return Err("协议端切换需要重启 Bot".into());
        }
        let mut updated = spec.clone();
        updated.webui_password = existing.webui_password;
        updated.webui_port = existing.webui_port;
        config::render(&updated).await?;
        config::check_intent(&self.root, &updated).await?;
        config::write_json(
            &self
                .root
                .join("session-bots")
                .join(format!("{}.json", spec.bot_id)),
            &updated,
        )
        .await?;
        if let Some(record) = self.bots.lock().await.get_mut(&spec.bot_id) {
            if record.spec.config.bot.auto_restart_schedule
                != updated.config.bot.auto_restart_schedule
            {
                record.next_restart = schedule::next(
                    &updated.config.bot.auto_restart_schedule,
                    chrono::Local::now(),
                );
            }
            record.spec = updated;
        }
        Ok(())
    }

    async fn start(&self, spec: &WindowsBotSpec, persist: bool) -> Result<BotStatus, String> {
        config::validate_spec(&self.root, spec)?;
        config::check_intent(&self.root, spec).await?;
        if let Some(backend) = self.backend(&spec.bot_id).await {
            let status = backend
                .status(BotId::new(&spec.bot_id))
                .await
                .map_err(|e| e.to_string())?;
            if status.state.is_active() {
                self.configure(spec).await?;
                return Ok(status);
            }
        }
        let mut spec = spec.clone();
        if spec.config.bot.backend_type == BackendType::SnowLuma {
            if let Some(existing) = self.bots.lock().await.values().find(|record| {
                record.spec.config.bot.backend_type == BackendType::SnowLuma
                    && record
                        .spec
                        .snowluma_dir
                        .eq_ignore_ascii_case(&spec.snowluma_dir)
            }) {
                spec.webui_password = existing.spec.webui_password.clone();
                spec.webui_port = existing.spec.webui_port;
            }
        }
        config::render(&spec).await?;
        let backend: Arc<dyn BotBackend> = match spec.config.bot.backend_type {
            BackendType::NapCat => Arc::new(napcat::NapCatBackend::new(
                spec.clone(),
                Arc::clone(&self.bus),
            )?),
            BackendType::SnowLuma => {
                let mut daemons = self.daemons.lock().await;
                let key = spec.snowluma_dir.to_ascii_lowercase();
                let daemon = Arc::clone(daemons.entry(key).or_insert_with(|| {
                    let daemon = SnowLumaDaemon::new(
                        PathBuf::from(&spec.data_root).join("snowluma"),
                        PathBuf::from(&spec.snowluma_dir),
                        Arc::clone(&self.bus),
                        Arc::new(ReqwestSnowLumaWebUiClientFactory),
                    );
                    daemon.set_node_bin_override(Some(PathBuf::from(&spec.node_executable)));
                    daemon.set_metrics_child_env(Some(spec.metrics_environment.clone()));
                    daemon
                }));
                Arc::new(SnowLumaRuntimeBackend::new(
                    BotId::new("windows-snowluma"),
                    daemon,
                    Arc::clone(&self.bus),
                ))
            }
        };
        let bot_id = BotId::new(&spec.bot_id);
        let mut runtime = BotRuntimeConfig::default_path(&self.root, bot_id.clone());
        runtime.flavor = ncd_domain::BotFlavor::from(spec.config.bot.backend_type);
        runtime.environment.extend(spec.metrics_environment.clone());
        runtime
            .environment
            .insert("SNOWLUMA_QQ_EXE".into(), spec.qq_executable.clone());
        runtime
            .environment
            .insert("SNOWLUMA_QQ_ID".into(), spec.bot_id.clone());
        runtime.environment.insert(
            "SNOWLUMA_LOCAL_QR_LOGIN_ENABLED".into(),
            if spec.remote_qr_login_enabled {
                "1"
            } else {
                "0"
            }
            .into(),
        );
        runtime.environment.insert(
            "SNOWLUMA_START_MODE".into(),
            match spec.config.bot.snowluma_start_mode {
                Some(ncd_domain::SnowLumaStartMode::HotStart) => "hot_start",
                _ => "cold_start",
            }
            .into(),
        );
        let status = backend
            .start(&BotStartCtx {
                config: runtime,
                bot_config: Some(spec.config.clone()),
            })
            .await
            .map_err(|e| e.to_string())?;
        if let Err(error) = config::check_intent(&self.root, &spec).await {
            let _ = backend.stop(bot_id, StopMode::Force).await;
            return Err(error);
        }
        if persist {
            let path = self
                .root
                .join("session-bots")
                .join(format!("{}.json", spec.bot_id));
            if let Err(error) = config::write_json(&path, &spec).await {
                let _ = backend.stop(bot_id, StopMode::Force).await;
                return Err(error);
            }
        }
        let identity = status.pid.filter(|pid| *pid > 0).and_then(|pid| {
            crate::native::process_identity(pid)
                .ok()
                .flatten()
                .map(|(created, session)| (pid, created, session))
        });
        if let Err(error) =
            logs::set_source(&self.root, &spec.bot_id, spec.config.bot.backend_type).await
        {
            tracing::warn!(%error, "保存 Bot 日志来源失败");
        }
        self.recovery.lock().await.remove(&spec.bot_id);
        let next_restart =
            schedule::next(&spec.config.bot.auto_restart_schedule, chrono::Local::now());
        let state_id = spec.bot_id.clone();
        self.bots.lock().await.insert(
            spec.bot_id.clone(),
            ManagedBot {
                backend,
                spec,
                started: status.clone(),
                identity,
                next_restart,
            },
        );
        state::record(&self.root, &state_id, identity)?;
        Ok(status)
    }
}

fn verify_stop_identity(identity: Option<(u32, u64, u32)>) -> Result<bool, String> {
    let observed = match identity {
        Some((pid, _, _)) => crate::native::process_identity(pid),
        None => Err("缺少已验证的托管进程身份".into()),
    };
    validate_stop_identity(identity, observed)
}

fn validate_stop_identity(
    expected: Option<(u32, u64, u32)>,
    observed: Result<Option<(u64, u32)>, String>,
) -> Result<bool, String> {
    let (_, created, session) = expected.ok_or("缺少已验证的托管进程身份，拒绝停止")?;
    match observed {
        Ok(Some(actual)) if actual == (created, session) => Ok(true),
        Ok(None) => Ok(false),
        Ok(Some(_)) => Err("进程身份已改变，已清除启动意图并拒绝停止其他进程".into()),
        Err(error) => Err(format!(
            "进程身份无法确认，已清除启动意图并暂缓停止：{error}"
        )),
    }
}

fn invalidated_status(status: BotStatus, reason: &str) -> WindowsAgentResult {
    let mut stopped = BotStatus::stopped(status.bot_id);
    stopped
        .extra
        .insert("agent_identity_reason".into(), reason.into());
    WindowsAgentResult::BotStatus(stopped)
}

fn validate_status_identity(
    mut status: BotStatus,
    expected: Option<(u32, u64, u32)>,
    observed: Result<Option<(u32, u64, u32)>, String>,
) -> WindowsAgentResult {
    status.extra.remove("agent_generation");
    match observed {
        Ok(Some(actual)) if expected.is_none_or(|identity| identity == actual) => {
            let (pid, created, session) = actual;
            status.extra.insert(
                "agent_generation".into(),
                format!("{session}:{pid}:{created}").into(),
            );
            WindowsAgentResult::BotStatus(status)
        }
        Ok(Some(_)) => {
            invalidated_status(status, "managed process creation time or session changed")
        }
        Ok(None) => invalidated_status(status, "managed process has exited"),
        Err(message) => WindowsAgentResult::Error(WindowsAgentError {
            code: "ProcessIdentityUnknown".into(),
            message,
            retryable: true,
        }),
    }
}

struct PassiveRestart;
#[async_trait::async_trait]
impl ncd_backend_napcat::RestartHandle for PassiveRestart {
    async fn restart_bot(&self, _: &BotId) {}
}

pub fn has_managed_bots(root: &std::path::Path) -> Result<bool, String> {
    let users = root.join("users");
    if !users.exists() {
        return Ok(false);
    }
    for user in std::fs::read_dir(users).map_err(|error| error.to_string())? {
        let user = user.map_err(|error| error.to_string())?.path();
        if state::blocks_uninstall(&user)? {
            return Ok(true);
        }
        let path = user.join("session-bots");
        if !path.exists() {
            continue;
        }
        for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.extension().is_some_and(|ext| ext == "json")
                && path
                    .file_stem()
                    .and_then(|id| id.to_str())
                    .is_some_and(|id| config::valid_bot_id(id).is_ok())
            {
                let marker = path.with_extension("stop.json");
                if marker.is_file() {
                    let revision: String = serde_json::from_slice(
                        &std::fs::read(marker).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                    let spec: WindowsBotSpec = serde_json::from_slice(
                        &std::fs::read(&path).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                    if spec.intent_revision.as_deref() != Some(&revision) {
                        continue;
                    }
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_actions_require_a_verified_live_process_identity() {
        let expected = Some((400, 10, 2));
        assert!(validate_stop_identity(expected, Ok(Some((10, 2)))).unwrap());
        assert!(!validate_stop_identity(expected, Ok(None)).unwrap());
        assert!(validate_stop_identity(expected, Ok(Some((11, 2)))).is_err());
        assert!(validate_stop_identity(expected, Ok(Some((10, 3)))).is_err());
        assert!(validate_stop_identity(expected, Err("Access is denied".into())).is_err());
        assert!(validate_stop_identity(None, Ok(Some((10, 2)))).is_err());
    }

    #[test]
    fn reused_pid_cannot_publish_a_new_managed_generation() {
        let status = BotStatus::running("123", 400, 1);
        let result = validate_status_identity(status, Some((400, 10, 2)), Ok(Some((400, 11, 2))));
        let WindowsAgentResult::BotStatus(status) = result else {
            panic!("expected stopped identity");
        };
        assert_eq!(status.state, ncd_domain::BotActorState::Stopped);
        assert_eq!(status.pid, None);
        assert!(!status.extra.contains_key("agent_generation"));
    }

    #[test]
    fn denied_identity_query_remains_unknown_instead_of_offline() {
        let status = BotStatus::running("123", 400, 1);
        let result = validate_status_identity(
            status,
            Some((400, 10, 2)),
            Err("Access is denied (os error 5)".into()),
        );
        assert!(
            matches!(result, WindowsAgentResult::Error(error) if error.code == "ProcessIdentityUnknown" && error.retryable)
        );
    }

    #[test]
    fn verified_identity_publishes_only_its_recorded_generation() {
        let status = BotStatus::running("123", 400, 1);
        let result = validate_status_identity(status, Some((400, 10, 2)), Ok(Some((400, 10, 2))));
        let WindowsAgentResult::BotStatus(status) = result else {
            panic!("expected verified status");
        };
        assert_eq!(status.extra["agent_generation"], "2:400:10");
    }

    #[tokio::test]
    async fn event_cursor_recovers_after_worker_restart_without_losing_new_events() {
        let root = tempfile::tempdir().unwrap();
        let service = BotService::new(root.path().to_path_buf());
        {
            let mut history = service.history.lock().await;
            history.sequence = 2;
            history
                .entries
                .push_back((1, DomainEvent::bot_log("123", "first")));
            history
                .entries
                .push_back((2, DomainEvent::bot_log("456", "other bot")));
        }
        let WindowsAgentResult::BotEvents(batch) = service
            .dispatch(&WindowsAgentAction::BotEvents {
                bot_id: "123".into(),
                after: 800,
                generation: Some("previous-worker".into()),
            })
            .await
            .unwrap()
        else {
            panic!("expected events");
        };
        assert_eq!(batch.next, 2);
        assert_eq!(batch.events, vec![DomainEvent::bot_log("123", "first")]);
        assert_eq!(batch.event_sequences, vec![1]);
    }

    #[tokio::test]
    async fn event_baseline_omits_old_lifecycle_but_keeps_subsequent_start_events() {
        let root = tempfile::tempdir().unwrap();
        let service = BotService::new(root.path().to_path_buf());
        {
            let mut history = service.history.lock().await;
            history.sequence = 1;
            history
                .entries
                .push_back((1, DomainEvent::bot_process_exited("123", Some(1), None)));
        }
        let WindowsAgentResult::BotEvents(baseline) = service
            .dispatch(&WindowsAgentAction::BotEvents {
                bot_id: "123".into(),
                after: u64::MAX,
                generation: None,
            })
            .await
            .unwrap()
        else {
            panic!("expected event baseline");
        };
        assert_eq!(baseline.next, 1);
        assert!(baseline.events.is_empty());
        assert!(baseline.event_sequences.is_empty());
        let webui = DomainEvent::napcat_webui_available("123", 6099, "fixture-token");
        {
            let mut history = service.history.lock().await;
            history.sequence = 2;
            history.entries.push_back((2, webui.clone()));
        }
        let WindowsAgentResult::BotEvents(events) = service
            .dispatch(&WindowsAgentAction::BotEvents {
                bot_id: "123".into(),
                after: baseline.next,
                generation: Some(baseline.generation),
            })
            .await
            .unwrap()
        else {
            panic!("expected subsequent start event");
        };
        assert_eq!(events.events, vec![webui]);
        assert_eq!(events.event_sequences, vec![2]);
    }

    #[test]
    fn stop_markers_are_not_managed_bots() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("users/owner/session-bots");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("123.stop.json"), b"\"revision\"").unwrap();
        assert!(!has_managed_bots(root.path()).unwrap());
        std::fs::write(directory.join("124.json"), b"{}").unwrap();
        assert!(has_managed_bots(root.path()).unwrap());
    }
}
