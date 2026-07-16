use std::collections::HashMap;
use std::sync::Arc;

use rand::RngCore;
use tokio::sync::Mutex;

use crate::events::BroadcastEventBus;
use crate::host_resolver::HostResolver;
use crate::native_deployment_adapter::{
    DockerDeploymentBackend, EventBusSink, RemoteNativeDeploymentBackend,
};
use ncd_backend_napcat::remote_native_launch::RemoteNativeLaunchTranslator;
use ncd_backend_snowluma::remote_snowluma::{RemoteSnowLumaBackend, RemoteSnowLumaDaemon};
use ncd_backend_snowluma::remote_snowluma_tunnel::RemoteSnowLumaTunnelRegistry;
use ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator;
use ncd_deploy::{DockerDeployment, NativeDeployment};
use ncd_domain::{
    BackendType, BotConfig, BotConfigError, BotFlavor, BotId, RuntimeScenario, RuntimeTarget,
};
use ncd_host::{Host, Os};
use ncd_traits::SecretStore;
use ncd_traits::runtime_backend::BotBackend;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeRouterError {
    #[error(transparent)]
    Config(#[from] BotConfigError),
    #[error("{0}")]
    Render(String),
}

#[derive(Clone)]
pub(crate) struct DockerSecretProvider {
    store: Option<Arc<dyn SecretStore + Send + Sync>>,
}

impl DockerSecretProvider {
    pub fn new(store: Option<Arc<dyn SecretStore + Send + Sync>>) -> Self {
        Self { store }
    }

    pub fn napcat_webui_token(&self, qq_id: u64) -> Result<String, RuntimeRouterError> {
        self.get_or_create(qq_id, Self::napcat_webui_key, generate_docker_webui_token)
    }

    pub(crate) fn napcat_webui_key(qq_id: u64) -> String {
        format!("bot:{qq_id}:napcat_docker_webui_token")
    }

    pub fn snowluma_vnc_passwd(&self, qq_id: u64) -> Result<String, RuntimeRouterError> {
        self.get_or_create(
            qq_id,
            |id| format!("bot:{id}:snowluma_docker_vnc_passwd"),
            generate_docker_password,
        )
    }

    pub fn snowluma_webui_bootstrap(&self, qq_id: u64) -> Result<String, RuntimeRouterError> {
        self.get_or_create(
            qq_id,
            |id| format!("bot:{id}:snowluma_docker_webui_bootstrap"),
            generate_docker_password,
        )
    }

    fn get_or_create(
        &self,
        qq_id: u64,
        key_fn: fn(u64) -> String,
        generate: fn() -> String,
    ) -> Result<String, RuntimeRouterError> {
        let store = self.store.as_ref().ok_or_else(|| {
            RuntimeRouterError::Render("Docker 部署需要凭据 secret store".to_string())
        })?;
        let key = key_fn(qq_id);
        if let Some(existing) = store
            .get(&key)
            .map_err(|e| RuntimeRouterError::Render(e.to_string()))?
        {
            let trimmed = existing.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        let value = generate();
        store
            .put(&key, &value)
            .map_err(|e| RuntimeRouterError::Render(e.to_string()))?;
        Ok(value)
    }
}

fn generate_docker_webui_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn generate_docker_password() -> String {
    crate::snowluma::session::generate_strong_password(16)
}

#[derive(Clone)]
pub(crate) struct RuntimeBackendRouter {
    local_backend: Arc<dyn BotBackend>,
    local_snowluma_backend: Option<Arc<dyn BotBackend>>,
    host_resolver: Option<Arc<dyn HostResolver>>,
    docker_secrets: DockerSecretProvider,
    event_bus: Arc<BroadcastEventBus>,
    remote_snowluma_daemons: Arc<Mutex<HashMap<String, Arc<RemoteSnowLumaDaemon>>>>,
    // 必须缓存:RemoteSnowLumaBackend.pollers 持有 status poller 句柄。
    // 以前每次 start 新建 backend,函数返回后 Arc drop → poller Drop 补发
    // Disconnected,UI 永远显示「QQ 已掉线」即使 WebUI 在线。
    remote_snowluma_backends: Arc<Mutex<HashMap<String, Arc<RemoteSnowLumaBackend>>>>,
    remote_snowluma_tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    remote_qq_entry_coordinator: Arc<RemoteQqEntryCoordinator>,
    /// 远端 NC 启动时注入探针（可选；未设则远端永不写 net-stats）
    remote_metrics_injector: Option<Arc<crate::metrics::RuntimeRemoteMetricsInjector>>,
    /// Docker bot 指标：本机 data_root + prefs（可选）
    docker_metrics: Option<(
        std::path::PathBuf,
        crate::metrics::BotRuntimeMetricsPrefs,
    )>,
}

impl RuntimeBackendRouter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        local_backend: Arc<dyn BotBackend>,
        local_snowluma_backend: Option<Arc<dyn BotBackend>>,
        host_resolver: Option<Arc<dyn HostResolver>>,
        docker_secrets: DockerSecretProvider,
        event_bus: Arc<BroadcastEventBus>,
        remote_snowluma_daemons: Arc<Mutex<HashMap<String, Arc<RemoteSnowLumaDaemon>>>>,
        remote_snowluma_backends: Arc<Mutex<HashMap<String, Arc<RemoteSnowLumaBackend>>>>,
        remote_snowluma_tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        remote_qq_entry_coordinator: Arc<RemoteQqEntryCoordinator>,
    ) -> Self {
        Self {
            local_backend,
            local_snowluma_backend,
            host_resolver,
            docker_secrets,
            event_bus,
            remote_snowluma_daemons,
            remote_snowluma_backends,
            remote_snowluma_tunnels,
            remote_qq_entry_coordinator,
            remote_metrics_injector: None,
            docker_metrics: None,
        }
    }

    pub fn with_remote_metrics_injector(
        mut self,
        injector: Arc<crate::metrics::RuntimeRemoteMetricsInjector>,
    ) -> Self {
        self.remote_metrics_injector = Some(injector);
        self
    }

    pub fn with_docker_metrics(
        mut self,
        local_data_root: impl Into<std::path::PathBuf>,
        prefs: crate::metrics::BotRuntimeMetricsPrefs,
    ) -> Self {
        self.docker_metrics = Some((local_data_root.into(), prefs));
        self
    }

    pub async fn backend_for_config(
        &self,
        config: &BotConfig,
    ) -> Result<Arc<dyn BotBackend>, RuntimeRouterError> {
        let scenario = RuntimeScenario::from_config(config)?;
        self.backend_for_scenario(&scenario, config.bot.qq_id).await
    }

    async fn backend_for_scenario(
        &self,
        scenario: &RuntimeScenario,
        qq_id: u64,
    ) -> Result<Arc<dyn BotBackend>, RuntimeRouterError> {
        match scenario {
            RuntimeScenario::LocalNative { backend } => Ok(self.local_backend_for(*backend)),
            RuntimeScenario::RemoteDocker { server_id, backend } => {
                let host = self.resolve_remote_host(server_id).await?;
                let backend_id = BotId::new(format!("docker-{qq_id}"));
                let flavor = BotFlavor::from(*backend);
                let deployment: Arc<DockerDeployment> = match backend {
                    BackendType::SnowLuma => {
                        let vnc = self.docker_secrets.snowluma_vnc_passwd(qq_id)?;
                        let webui = self.docker_secrets.snowluma_webui_bootstrap(qq_id)?;
                        Arc::new(DockerDeployment::with_sl_secrets(vnc, webui))
                    }
                    BackendType::NapCat => {
                        let token = self.docker_secrets.napcat_webui_token(qq_id)?;
                        Arc::new(DockerDeployment::with_webui_token(token))
                    }
                };
                let mut backend =
                    DockerDeploymentBackend::new(deployment, host, backend_id, flavor);
                if let Some((data_root, prefs)) = self.docker_metrics.clone() {
                    backend = backend.with_metrics(data_root, prefs);
                }
                Ok(Arc::new(backend))
            }
            RuntimeScenario::RemoteNative { server_id, backend } => {
                let host = self.resolve_remote_host(server_id).await?;
                if host.os() != Os::Linux {
                    return Err(RuntimeRouterError::Render(
                        "远端「直接运行」目前仅支持 Linux SSH 主机。".to_string(),
                    ));
                }
                match backend {
                    BackendType::SnowLuma => {
                        let backend = self
                            .remote_snowluma_backend_for_server(server_id, Arc::clone(&host))
                            .await?;
                        Ok(backend as Arc<dyn BotBackend>)
                    }
                    BackendType::NapCat => {
                        let coordinator = Arc::clone(&self.remote_qq_entry_coordinator);
                        let backend_id = BotId::new(format!("remote-native-{qq_id}"));
                        // 远端指标：上传探针 + 改 loadNapCat + 启动 env（失败不阻断）
                        let metrics_injector = self.remote_metrics_injector.clone().map(
                            |inj| inj as Arc<dyn ncd_backend_napcat::remote_native_launch::RemoteMetricsInjector>,
                        );
                        let translator = Arc::new(RemoteNativeLaunchTranslator::new_with_metrics(
                            Arc::clone(&host),
                            BotFlavor::NapCat,
                            server_id.to_string(),
                            coordinator,
                            metrics_injector,
                        ));
                        let event_sink: Arc<dyn ncd_deploy::NativeRuntimeEventSink> =
                            Arc::new(EventBusSink::new(Arc::clone(&self.event_bus)));
                        let deployment =
                            Arc::new(NativeDeployment::new(translator, event_sink, None));
                        let target = RuntimeTarget::server(server_id.to_string());
                        let resolver = self.host_resolver.as_ref().ok_or_else(|| {
                            RuntimeRouterError::Render("HostResolver 未初始化".to_string())
                        })?;
                        Ok(Arc::new(RemoteNativeDeploymentBackend::new(
                            deployment,
                            Arc::clone(resolver),
                            target,
                            backend_id,
                            BotFlavor::NapCat,
                        )))
                    }
                }
            }
        }
    }

    pub async fn remote_snowluma_daemon_for_server(
        &self,
        server_id: &str,
        host: Arc<dyn Host>,
    ) -> Result<Arc<RemoteSnowLumaDaemon>, RuntimeRouterError> {
        let sid = server_id.to_string();
        let mut guard = self.remote_snowluma_daemons.lock().await;
        if let Some(daemon) = guard.get(&sid) {
            return Ok(Arc::clone(daemon));
        }
        let daemon = Arc::new(
            RemoteSnowLumaDaemon::new(
                sid.clone(),
                Arc::clone(&host),
                Arc::clone(&self.remote_snowluma_tunnels),
                Arc::clone(&self.event_bus),
            )
            .await
            .map_err(|e| RuntimeRouterError::Render(e.to_string()))?,
        );
        guard.insert(sid, Arc::clone(&daemon));
        Ok(daemon)
    }

    /// 同 server 的远端 SL backend 单例:持有 per-bot status poller。
    /// start / stop / bootstrap reconcile 必须走同一实例,否则 poller 会随
    /// 临时 backend drop 被 dispose 并误发 Disconnected。
    pub async fn remote_snowluma_backend_for_server(
        &self,
        server_id: &str,
        host: Arc<dyn Host>,
    ) -> Result<Arc<RemoteSnowLumaBackend>, RuntimeRouterError> {
        let sid = server_id.to_string();
        {
            let guard = self.remote_snowluma_backends.lock().await;
            if let Some(backend) = guard.get(&sid) {
                return Ok(Arc::clone(backend));
            }
        }
        let daemon = self
            .remote_snowluma_daemon_for_server(server_id, host)
            .await?;
        // backend 身份按 server 聚合(多 Bot 共享同一 daemon / 隧道 / poller 表)
        let backend_id = BotId::new(format!("remote-sl-{sid}"));
        let sl_metrics = self.remote_metrics_injector.clone().map(
            |inj| inj as Arc<dyn ncd_backend_snowluma::remote_snowluma::RemoteSlMetricsInjector>,
        );
        let backend = Arc::new(RemoteSnowLumaBackend::new_with_metrics(
            backend_id,
            daemon,
            Arc::clone(&self.event_bus),
            Arc::clone(&self.remote_snowluma_tunnels),
            Arc::clone(&self.remote_qq_entry_coordinator),
            sl_metrics,
        ));
        let mut guard = self.remote_snowluma_backends.lock().await;
        // 双检:并发 start 时先到者胜,后来者复用
        if let Some(existing) = guard.get(&sid) {
            return Ok(Arc::clone(existing));
        }
        guard.insert(sid, Arc::clone(&backend));
        Ok(backend)
    }

    async fn resolve_remote_host(
        &self,
        server_id: &str,
    ) -> Result<Arc<dyn Host>, RuntimeRouterError> {
        let resolver = self
            .host_resolver
            .as_ref()
            .ok_or_else(|| RuntimeRouterError::Render("HostResolver 未初始化".to_string()))?;
        resolver
            .resolve(&RuntimeTarget::server(server_id.to_string()))
            .await
            .map_err(|e| RuntimeRouterError::Render(e.to_string()))
    }

    fn local_backend_for(&self, backend: BackendType) -> Arc<dyn BotBackend> {
        match backend {
            BackendType::SnowLuma => self
                .local_snowluma_backend
                .clone()
                .unwrap_or_else(|| Arc::clone(&self.local_backend)),
            BackendType::NapCat => Arc::clone(&self.local_backend),
        }
    }
}
