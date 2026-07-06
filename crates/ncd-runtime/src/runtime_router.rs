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
    remote_snowluma_tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    remote_qq_entry_coordinator: Arc<RemoteQqEntryCoordinator>,
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
            remote_snowluma_tunnels,
            remote_qq_entry_coordinator,
        }
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
                Ok(Arc::new(DockerDeploymentBackend::new(
                    deployment, host, backend_id, flavor,
                )))
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
                        let daemon = self
                            .remote_snowluma_daemon_for_server(server_id, Arc::clone(&host))
                            .await?;
                        let backend_id = BotId::new(format!("remote-sl-{qq_id}"));
                        Ok(Arc::new(RemoteSnowLumaBackend::new(
                            backend_id,
                            daemon,
                            Arc::clone(&self.event_bus),
                            Arc::clone(&self.remote_snowluma_tunnels),
                            Arc::clone(&self.remote_qq_entry_coordinator),
                        )))
                    }
                    BackendType::NapCat => {
                        let coordinator = Arc::clone(&self.remote_qq_entry_coordinator);
                        let backend_id = BotId::new(format!("remote-native-{qq_id}"));
                        let translator = Arc::new(RemoteNativeLaunchTranslator::new(
                            Arc::clone(&host),
                            BotFlavor::NapCat,
                            server_id.to_string(),
                            coordinator,
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
            .map_err(RuntimeRouterError::Render)
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
