//! Windows 远端 Bot 的会话托管与事件桥接；SSH 断开不改变远端运行意图。
mod endpoint;
mod events;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ncd_domain::windows_bot::WindowsBotSpec;
use ncd_domain::windows_remote::{
    WINDOWS_AGENT_PROTOCOL, WindowsAgentAction, WindowsAgentRequest, WindowsAgentResult,
};
use ncd_domain::{BackendKind, BackendType, BotFlavor, BotId, BotStatus, RuntimeTarget, StopMode};
use ncd_host::{Host, HostPath, Os};
use ncd_traits::events::BroadcastEventBus;
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LogSnapshot, TailOpts,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::HostResolver;

#[derive(Default)]
pub struct WindowsBotRegistry {
    backends: Mutex<HashMap<String, Arc<RemoteWindowsBotBackend>>>,
    daemon_events: Arc<Mutex<events::DaemonEvents>>,
}

impl WindowsBotRegistry {
    pub async fn backend(
        &self,
        server: &str,
        qq: u64,
        flavor: BackendType,
        resolver: Arc<dyn HostResolver>,
        bus: Arc<BroadcastEventBus>,
        selected: ncd_domain::RemoteSelectedPaths,
        local_data_root: Option<std::path::PathBuf>,
    ) -> Arc<RemoteWindowsBotBackend> {
        let key = format!("{server}/{qq}/{flavor:?}");
        let mut entries = self.backends.lock().await;
        let prefix = format!("{server}/{qq}/");
        let previous: Vec<_> = entries
            .keys()
            .filter(|candidate| candidate.starts_with(&prefix) && **candidate != key)
            .cloned()
            .collect();
        let removed: Vec<_> = previous
            .into_iter()
            .filter_map(|previous| entries.remove(&previous))
            .collect();
        let backend = entries
            .entry(key)
            .or_insert_with(|| {
                Arc::new(RemoteWindowsBotBackend {
                    id: BotId::new(qq.to_string()),
                    server: server.into(),
                    flavor,
                    resolver,
                    bus,
                    local_data_root,
                    selected: Mutex::new(selected.clone()),
                    runtime: Mutex::new(None),
                    endpoint: Mutex::new(None),
                    cancel: CancellationToken::new(),
                    following: Mutex::new(None),
                    cursor: Mutex::new(events::EventCursor::default()),
                    daemon_events: Arc::clone(&self.daemon_events),
                })
            })
            .clone();
        drop(entries);
        for previous in removed {
            previous.close().await;
        }
        *backend.selected.lock().await = selected;
        backend.ensure_follow().await;
        backend
    }

    pub async fn endpoint(
        &self,
        server: &str,
        qq: u64,
        flavor: BackendType,
    ) -> Option<(u16, String, u16)> {
        let backend = self
            .backends
            .lock()
            .await
            .get(&format!("{server}/{qq}/{flavor:?}"))
            .cloned()?;
        backend.ensure_endpoint().await.ok()
    }

    pub async fn snowluma_endpoint(&self, server: &str) -> Option<(u16, String, u16)> {
        let backend = self
            .backends
            .lock()
            .await
            .values()
            .find(|backend| backend.server == server && backend.flavor == BackendType::SnowLuma)
            .cloned()?;
        backend.ensure_endpoint().await.ok()
    }

    pub async fn detach_server(&self, server: &str) {
        let backends: Vec<_> = self
            .backends
            .lock()
            .await
            .values()
            .filter(|backend| backend.server == server)
            .cloned()
            .collect();
        for backend in backends {
            backend.endpoint.lock().await.take();
        }
    }

    pub async fn manages(&self, server: &str, qq: u64, flavor: BackendType) -> bool {
        self.backends
            .lock()
            .await
            .contains_key(&format!("{server}/{qq}/{flavor:?}"))
    }

    pub async fn close_all(&self) {
        let entries = std::mem::take(&mut *self.backends.lock().await);
        for (_, backend) in entries {
            backend.close().await;
        }
    }
}

pub struct RemoteWindowsBotBackend {
    id: BotId,
    server: String,
    flavor: BackendType,
    resolver: Arc<dyn HostResolver>,
    bus: Arc<BroadcastEventBus>,
    local_data_root: Option<std::path::PathBuf>,
    selected: Mutex<ncd_domain::RemoteSelectedPaths>,
    runtime: Mutex<Option<BotRuntimeConfig>>,
    endpoint: Mutex<Option<endpoint::Endpoint>>,
    cancel: CancellationToken,
    following: Mutex<Option<tokio::task::JoinHandle<()>>>,
    cursor: Mutex<events::EventCursor>,
    daemon_events: Arc<Mutex<events::DaemonEvents>>,
}

impl RemoteWindowsBotBackend {
    async fn host(&self) -> Result<Arc<dyn Host>, BotBackendError> {
        let target = RuntimeTarget::server(&self.server);
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(Self::closed_error()),
            result = self.resolver.resolve(&target) => {
                result.map_err(|error| BotBackendError::RemoteHostTransport(error.to_string()))
            }
        }
    }

    pub(super) async fn request(
        &self,
        action: WindowsAgentAction,
    ) -> Result<WindowsAgentResult, BotBackendError> {
        let host = self.host().await?;
        self.request_on(host.as_ref(), action).await
    }

    pub(super) async fn request_on(
        &self,
        host: &dyn Host,
        action: WindowsAgentAction,
    ) -> Result<WindowsAgentResult, BotBackendError> {
        let request = WindowsAgentRequest {
            v: WINDOWS_AGENT_PROTOCOL,
            request_id: uuid::Uuid::new_v4().to_string(),
            action,
        };
        let response = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(Self::closed_error()),
            response = host.windows_agent_request(request) => response
                .map_err(|error| BotBackendError::RemoteHostTransport(error.to_string()))?,
        };
        match response.result {
            WindowsAgentResult::Error(error) => Err(BotBackendError::Io(format!(
                "{}: {}",
                error.code, error.message
            ))),
            result => Ok(result),
        }
    }

    async fn ensure_follow(self: &Arc<Self>) {
        let mut following = self.following.lock().await;
        if following.is_some() || self.cancel.is_cancelled() {
            return;
        }
        let _ = self.ensure_event_baseline(false).await;
        let weak = Arc::downgrade(self);
        let cancel = self.cancel.clone();
        *following = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(1500));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { _ = cancel.cancelled() => break, _ = interval.tick() => {} }
                let Some(backend) = weak.upgrade() else {
                    break;
                };
                match backend.poll_events().await {
                    Ok(()) => {
                        let _ = backend.ensure_endpoint().await;
                    }
                    Err(_) => {
                        backend.endpoint.lock().await.take();
                    }
                }
            }
        }));
    }

    fn closed_error() -> BotBackendError {
        BotBackendError::Io("Windows 远端会话已关闭".into())
    }

    fn check_open(&self) -> Result<(), BotBackendError> {
        if self.cancel.is_cancelled() {
            Err(Self::closed_error())
        } else {
            Ok(())
        }
    }

    async fn close(&self) {
        self.cancel.cancel();
        if let Some(following) = self.following.lock().await.take() {
            following.abort();
            let _ = following.await;
        }
        self.endpoint.lock().await.take();
    }

    async fn spec(&self, ctx: &BotStartCtx) -> Result<WindowsBotSpec, BotBackendError> {
        let host = self.host().await?;
        let info = host
            .remote_windows_info()
            .await
            .map_err(|error| BotBackendError::RemoteHostTransport(error.to_string()))?;
        if info
            .agent
            .as_ref()
            .is_none_or(|agent| !agent.service_running)
        {
            return Err(BotBackendError::InvalidConfig(
                "请先在目标主机组件页安装 Windows 托管端".into(),
            ));
        }
        let config = ctx.bot_config.clone().ok_or_else(|| {
            BotBackendError::InvalidConfig("Windows 远端启动缺少 Bot 配置".into())
        })?;
        let native = |value: &str| {
            if value.starts_with('/') {
                HostPath::from_posix(value).render_for(Os::Windows)
            } else {
                value.to_string()
            }
        };
        let missing = |name: &str| {
            BotBackendError::InvalidConfig(format!(
                "目标主机未检测到 {name}，请刷新组件库存并完成安装"
            ))
        };
        let selected = self.selected.lock().await.clone();
        let napcat = selected
            .napcat_root
            .as_deref()
            .map(native)
            .unwrap_or_default();
        let snowluma = selected
            .snowluma_dir
            .as_deref()
            .map(native)
            .unwrap_or_default();
        let qq = selected
            .qq_bin
            .as_deref()
            .map(native)
            .ok_or_else(|| missing("QQ"))?;
        let node = selected
            .node_bin
            .as_deref()
            .map(native)
            .unwrap_or_else(|| format!("{snowluma}\\node.exe"));
        if self.flavor == BackendType::NapCat && napcat.is_empty() {
            return Err(missing("NapCat"));
        }
        if self.flavor == BackendType::SnowLuma && snowluma.is_empty() {
            return Err(missing("SnowLuma"));
        }
        let secret_path = HostPath::from_windows(&info.user_root).join("snowluma/webui.secret");
        let password = if self.flavor == BackendType::SnowLuma {
            if host
                .exists(&secret_path)
                .await
                .map_err(|error| BotBackendError::Io(error.to_string()))?
            {
                let bytes = host
                    .read_file(&secret_path)
                    .await
                    .map_err(|error| BotBackendError::Io(error.to_string()))?;
                let password = String::from_utf8(bytes.to_vec())
                    .map_err(|error| BotBackendError::Io(error.to_string()))?;
                if password.trim().is_empty() {
                    return Err(BotBackendError::InvalidConfig(
                        "目标 SnowLuma 凭据为空，未覆盖".into(),
                    ));
                }
                password
            } else {
                let password = uuid::Uuid::new_v4().simple().to_string();
                host.create_dir_all(&HostPath::from_windows(&info.user_root).join("snowluma"))
                    .await
                    .map_err(|error| BotBackendError::Io(error.to_string()))?;
                host.write_file(&secret_path, password.as_bytes())
                    .await
                    .map_err(|error| BotBackendError::Io(error.to_string()))?;
                password
            }
        } else {
            String::new()
        };
        let mut metrics_environment = std::collections::BTreeMap::new();
        if let Some(local_root) = &self.local_data_root {
            let prefs = crate::metrics::BotRuntimeMetricsPrefs::from_data_root(local_root);
            if prefs.enabled || self.flavor == BackendType::SnowLuma {
                let home = HostPath::from_windows(&info.user_root);
                let paths = crate::metrics::ensure_remote_metrics_assets(
                    host.as_ref(),
                    home.as_posix(),
                    self.id.as_str(),
                    &config,
                    &prefs,
                    local_root,
                )
                .await
                .map_err(BotBackendError::Io)?;
                metrics_environment = if self.flavor == BackendType::SnowLuma {
                    crate::metrics::write_remote_snowluma_registration(
                        host.as_ref(),
                        home.as_posix(),
                        self.id.as_str(),
                        &config,
                        &prefs,
                    )
                    .await
                    .map_err(BotBackendError::Io)?;
                    paths.env_map_for_snowluma()
                } else {
                    paths.env_map()
                };
            }
        }
        let settings = self
            .local_data_root
            .as_ref()
            .map(|root| {
                ncd_backend_snowluma::snowluma::session::load_snowluma_app_config(
                    &crate::DataPaths::new(root).snowluma_data_dir(),
                )
            })
            .unwrap_or_default();
        let remote_qr_login_enabled = settings.remote_qr_login_enabled;
        Ok(WindowsBotSpec {
            intent_revision: None,
            remote_qr_login_enabled,
            metrics_environment,
            bot_id: ctx.config.bot_id.to_string(),
            config,
            napcat_dir: napcat,
            qq_executable: qq,
            snowluma_dir: snowluma,
            node_executable: node,
            data_root: info.user_root,
            webui_password: password,
            webui_port: settings.webui_port,
        })
    }
}

#[async_trait]
impl BotBackend for RemoteWindowsBotBackend {
    fn id(&self) -> &BotId {
        &self.id
    }
    fn kind(&self) -> BackendKind {
        BackendKind::RemoteSsh
    }
    fn flavor(&self) -> BotFlavor {
        self.flavor.into()
    }
    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let spec = self.spec(ctx).await?;
        match self.start_with_event_baseline(spec).await? {
            WindowsAgentResult::BotStatus(status) => {
                *self.runtime.lock().await = Some(ctx.config.clone());
                let _ = self.ensure_endpoint().await;
                Ok(status)
            }
            _ => Err(BotBackendError::Io(
                "Windows 托管端返回了意外的启动响应".into(),
            )),
        }
    }
    async fn stop(&self, id: BotId, _: StopMode) -> Result<(), BotBackendError> {
        self.request(WindowsAgentAction::BotStop {
            bot_id: id.to_string(),
        })
        .await?;
        self.endpoint.lock().await.take();
        Ok(())
    }
    async fn status(&self, id: BotId) -> Result<BotStatus, BotBackendError> {
        match self
            .request(WindowsAgentAction::BotStatus {
                bot_id: id.to_string(),
            })
            .await?
        {
            WindowsAgentResult::BotStatus(status) => Ok(status),
            _ => Err(BotBackendError::Io(
                "Windows 托管端返回了意外的状态响应".into(),
            )),
        }
    }
    async fn qr_login_snapshot(
        &self,
        id: &BotId,
        refresh: bool,
    ) -> Result<ncd_domain::SnowLumaQrLoginSnapshot, BotBackendError> {
        match self
            .request(WindowsAgentAction::BotQr {
                bot_id: id.to_string(),
                refresh,
            })
            .await?
        {
            WindowsAgentResult::QrLogin(snapshot) => Ok(snapshot),
            _ => Err(BotBackendError::Io(
                "Windows 托管端返回了意外的扫码响应".into(),
            )),
        }
    }
    async fn close_qr_login(&self, id: &BotId) -> Result<(), BotBackendError> {
        self.request(WindowsAgentAction::BotCloseQr {
            bot_id: id.to_string(),
        })
        .await
        .map(|_| ())
    }
    async fn sync_bot_config(&self, config: &ncd_domain::BotConfig) -> Result<(), BotBackendError> {
        let runtime = self
            .runtime
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| BotRuntimeConfig::default_path("", self.id.clone()));
        let spec = self
            .spec(&BotStartCtx {
                config: runtime,
                bot_config: Some(config.clone()),
            })
            .await?;
        self.request(WindowsAgentAction::BotConfigure(spec))
            .await
            .map(|_| ())
    }
    async fn tail_log(&self, id: BotId, options: TailOpts) -> Result<LogSnapshot, BotBackendError> {
        match self
            .request(WindowsAgentAction::BotTail {
                bot_id: id.to_string(),
                lines: options.lines.min(2000) as u32,
            })
            .await?
        {
            WindowsAgentResult::BotLog(log) => Ok(log),
            _ => Err(BotBackendError::Io(
                "Windows 托管端返回了意外的日志响应".into(),
            )),
        }
    }
    async fn read_config(&self, id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        self.runtime
            .lock()
            .await
            .clone()
            .ok_or(BotBackendError::ConfigNotFound(id))
    }
    async fn write_config(
        &self,
        _: BotId,
        config: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        *self.runtime.lock().await = Some(config.clone());
        Ok(())
    }
}
