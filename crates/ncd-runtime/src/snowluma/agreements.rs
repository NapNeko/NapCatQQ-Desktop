use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::bot_manager::BotManagerError;
use crate::host_resolver::HostResolver;
use crate::runtime_router::RuntimeBackendRouter;
use crate::snowluma::{
    AgreementsPayload, ReqwestSnowLumaWebUiClient, SnowLumaDaemon, SnowLumaWebUiClient,
};
use crate::snowluma_consent_files::{
    SnowLumaConsentFileError, load_payload_from_runtime_root, record_consent_to_runtime_root,
};
use ncd_backend_snowluma::remote_snowluma::RemoteSnowLumaDaemon;
use ncd_domain::{BackendType, BotConfig, RuntimeScenario, RuntimeTarget};
use ncd_traits::runtime_backend::BotBackendError;

#[derive(Clone)]
pub(crate) struct SnowLumaAgreementService {
    local_daemon: Option<Arc<SnowLumaDaemon>>,
    host_resolver: Option<Arc<dyn HostResolver>>,
    runtime_router: RuntimeBackendRouter,
    remote_daemons: Arc<Mutex<HashMap<String, Arc<RemoteSnowLumaDaemon>>>>,
}

impl SnowLumaAgreementService {
    pub fn new(
        local_daemon: Option<Arc<SnowLumaDaemon>>,
        host_resolver: Option<Arc<dyn HostResolver>>,
        runtime_router: RuntimeBackendRouter,
        remote_daemons: Arc<Mutex<HashMap<String, Arc<RemoteSnowLumaDaemon>>>>,
    ) -> Self {
        Self {
            local_daemon,
            host_resolver,
            runtime_router,
            remote_daemons,
        }
    }

    pub async fn prepare(
        &self,
        config: &BotConfig,
    ) -> Result<Option<AgreementsPayload>, BotManagerError> {
        match RuntimeScenario::from_config(config)? {
            RuntimeScenario::LocalNative {
                backend: BackendType::SnowLuma,
            } => self.prepare_local().await,
            RuntimeScenario::RemoteNative {
                server_id,
                backend: BackendType::SnowLuma,
            } => self.prepare_remote(&server_id).await,
            _ => Ok(None),
        }
    }

    pub async fn record_consent(
        &self,
        config: &BotConfig,
        version: &str,
    ) -> Result<bool, BotManagerError> {
        match RuntimeScenario::from_config(config)? {
            RuntimeScenario::LocalNative {
                backend: BackendType::SnowLuma,
            } => self.record_local(version).await,
            RuntimeScenario::RemoteNative {
                server_id,
                backend: BackendType::SnowLuma,
            } => self.record_remote(&server_id, version).await,
            _ => Ok(false),
        }
    }

    pub async fn release(&self, config: &BotConfig) -> Result<(), BotManagerError> {
        match RuntimeScenario::from_config(config)? {
            RuntimeScenario::LocalNative {
                backend: BackendType::SnowLuma,
            } => {
                if let Some(daemon) = self.local_daemon.as_ref() {
                    daemon.release().await;
                }
            }
            RuntimeScenario::RemoteNative {
                server_id,
                backend: BackendType::SnowLuma,
            } => {
                if let Some(daemon) = self.remote_daemon_if_known(&server_id).await {
                    daemon.release().await;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn prepare_local(&self) -> Result<Option<AgreementsPayload>, BotManagerError> {
        let daemon = self.local_daemon()?;
        let agreements =
            load_payload_from_runtime_root(daemon.runtime_root()).map_err(map_consent_file_io)?;
        Ok(agreements.and_then(|payload| payload.consent_required.then_some(payload)))
    }

    async fn record_local(&self, version: &str) -> Result<bool, BotManagerError> {
        let daemon = self.local_daemon()?;
        record_consent_to_runtime_root(daemon.runtime_root(), version)
            .map_err(map_consent_file_io)?;
        if let Some(client) = daemon.current_webui_client().await {
            client
                .record_agreement_consent(version)
                .await
                .map_err(map_webui_io)?;
        }
        Ok(true)
    }

    async fn prepare_remote(
        &self,
        server_id: &str,
    ) -> Result<Option<AgreementsPayload>, BotManagerError> {
        let daemon = self.ensure_remote_daemon(server_id).await?;
        daemon.ensure_running().await?;
        let agreements_result = self.remote_agreements(&daemon).await;
        let agreements = match agreements_result {
            Ok(agreements) => agreements,
            Err(err) => {
                daemon.release().await;
                return Err(err);
            }
        };
        if agreements.consent_required {
            return Ok(Some(agreements));
        }
        daemon.release().await;
        Ok(None)
    }

    async fn record_remote(&self, server_id: &str, version: &str) -> Result<bool, BotManagerError> {
        let daemon = match self.remote_daemon_if_tunneled(server_id).await {
            Some(daemon) => daemon,
            None => {
                let daemon = self.ensure_remote_daemon(server_id).await?;
                daemon.ensure_running().await?;
                daemon
            }
        };
        self.remote_record_consent(&daemon, version).await?;
        Ok(true)
    }

    async fn remote_agreements(
        &self,
        daemon: &RemoteSnowLumaDaemon,
    ) -> Result<AgreementsPayload, BotManagerError> {
        let client = self.remote_client(daemon).await?;
        client.get_agreements().await.map_err(map_webui_io)
    }

    async fn remote_record_consent(
        &self,
        daemon: &RemoteSnowLumaDaemon,
        version: &str,
    ) -> Result<(), BotManagerError> {
        let client = self.remote_client(daemon).await?;
        client
            .record_agreement_consent(version)
            .await
            .map_err(map_webui_io)
    }

    async fn remote_client(
        &self,
        daemon: &RemoteSnowLumaDaemon,
    ) -> Result<ReqwestSnowLumaWebUiClient, BotManagerError> {
        let endpoints = daemon
            .tunnel_endpoints()
            .await
            .ok_or_else(|| BotManagerError::Render("SnowLuma 隧道未建立".into()))?;
        let client = ReqwestSnowLumaWebUiClient::new(
            endpoints.webui_local_port,
            endpoints.webui_password.clone(),
        )
        .map_err(map_webui_io)?;
        client
            .wait_ready(Duration::from_secs(30), Box::new(|| false))
            .await
            .map_err(map_webui_io)?;
        client.login().await.map_err(map_webui_io)?;
        Ok(client)
    }

    async fn ensure_remote_daemon(
        &self,
        server_id: &str,
    ) -> Result<Arc<RemoteSnowLumaDaemon>, BotManagerError> {
        let resolver = self
            .host_resolver
            .as_ref()
            .ok_or_else(|| BotManagerError::Render("HostResolver 未初始化".into()))?;
        let host = resolver
            .resolve(&RuntimeTarget::server(server_id.to_string()))
            .await
            .map_err(|e| BotManagerError::Render(e.to_string()))?;
        self.runtime_router
            .remote_snowluma_daemon_for_server(server_id, host)
            .await
            .map_err(BotManagerError::from)
    }

    async fn remote_daemon_if_tunneled(
        &self,
        server_id: &str,
    ) -> Option<Arc<RemoteSnowLumaDaemon>> {
        let daemon = self.remote_daemon_if_known(server_id).await?;
        if daemon.tunnel_endpoints().await.is_some() {
            Some(daemon)
        } else {
            None
        }
    }

    async fn remote_daemon_if_known(&self, server_id: &str) -> Option<Arc<RemoteSnowLumaDaemon>> {
        self.remote_daemons.lock().await.get(server_id).cloned()
    }

    fn local_daemon(&self) -> Result<Arc<SnowLumaDaemon>, BotManagerError> {
        self.local_daemon
            .clone()
            .ok_or_else(|| BotManagerError::Render("SnowLuma daemon 未初始化".into()))
    }
}

fn map_webui_io(error: impl std::fmt::Display) -> BotManagerError {
    BotManagerError::Runtime(BotBackendError::Io(error.to_string()))
}

fn map_consent_file_io(error: SnowLumaConsentFileError) -> BotManagerError {
    BotManagerError::Runtime(BotBackendError::Io(error.to_string()))
}
