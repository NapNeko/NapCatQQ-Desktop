use std::sync::Arc;
use std::time::Duration;

use crate::bot_manager::BotManagerError;
use crate::host_resolver::HostResolver;
use crate::runtime_router::RuntimeBackendRouter;
use crate::snowluma::{
    AgreementsPayload, ReqwestSnowLumaWebUiClient, SnowLumaDaemon, SnowLumaWebUiClient,
};
use crate::snowluma_consent_files::{
    SnowLumaConsentFileError, consent_record_json, load_payload_from_runtime_root,
    parse_consent_version_json, payload_from_agreement_texts, record_consent_to_runtime_root,
};
use ncd_backend_snowluma::remote_snowluma::RemoteSnowLumaDaemon;
use ncd_backend_snowluma::remote_snowluma_layout::SnowLumaRemotePaths;
use ncd_domain::{BackendType, BotConfig, RuntimeScenario, RuntimeTarget};
use ncd_host::{Host, HostPath};
use ncd_traits::runtime_backend::BotBackendError;

#[derive(Clone)]
pub(crate) struct SnowLumaAgreementService {
    local_daemon: Option<Arc<SnowLumaDaemon>>,
    host_resolver: Option<Arc<dyn HostResolver>>,
    runtime_router: RuntimeBackendRouter,
}

impl SnowLumaAgreementService {
    pub fn new(
        local_daemon: Option<Arc<SnowLumaDaemon>>,
        host_resolver: Option<Arc<dyn HostResolver>>,
        runtime_router: RuntimeBackendRouter,
    ) -> Self {
        Self {
            local_daemon,
            host_resolver,
            runtime_router,
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
                // 本机 prepare 只读 runtime 下协议文件，不 ensure_running，
                // 没有占用 daemon 引用。这里不能 release：否则会误减正在跑的
                // Bot 的 ref_count，最后一个引用归零时还会杀掉 node.exe。
            }
            RuntimeScenario::RemoteNative {
                backend: BackendType::SnowLuma,
                ..
            } => {
                // 远端 prepare 只读安装目录协议文件，不 ensure_running（与本机一致）。
                // 不能 release：否则会误减正在跑的 Bot 的 ref_count。
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
        let host = daemon.host().await;
        let agreements = load_payload_from_remote(host.as_ref(), daemon.paths()).await?;
        Ok(agreements.and_then(|payload| payload.consent_required.then_some(payload)))
    }

    async fn record_remote(&self, server_id: &str, version: &str) -> Result<bool, BotManagerError> {
        let daemon = self.ensure_remote_daemon(server_id).await?;
        let host = daemon.host().await;
        record_consent_on_remote(host.as_ref(), daemon.paths(), version).await?;
        // node 已在跑时再通知进程内 gate；失败不回滚文件（下次启动会读 consent.json）
        if daemon.tunnel_endpoints().await.is_some() {
            if let Err(err) = self.remote_record_consent(&daemon, version).await {
                tracing::warn!(
                    target: "ncd_runtime::snowluma",
                    server_id,
                    %err,
                    "远端 consent.json 已写入，但未能经 WebUI 通知已运行的 node"
                );
            }
        }
        Ok(true)
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

async fn load_payload_from_remote(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
) -> Result<Option<AgreementsPayload>, BotManagerError> {
    let eula = read_remote_agreement(host, &paths.snowluma_dir, "EULA.md").await;
    let privacy = read_remote_agreement(host, &paths.snowluma_dir, "PRIVACY.md").await;
    let consent_path = format!("{}/consent.json", paths.config_dir);
    let consent_text = read_remote_optional(host, &consent_path).await;
    let consent_version = parse_consent_version_json(&consent_text);
    Ok(payload_from_agreement_texts(
        &eula,
        &privacy,
        consent_version.as_deref(),
    ))
}

async fn record_consent_on_remote(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    version: &str,
) -> Result<(), BotManagerError> {
    let current = load_payload_from_remote(host, paths).await?;
    if let Some(payload) = current.as_ref()
        && payload.version != version
    {
        return Err(map_consent_file_io(
            SnowLumaConsentFileError::VersionMismatch {
                expected: payload.version.clone(),
                actual: version.to_string(),
            },
        ));
    }
    host.create_dir_all(&HostPath::from_posix(&paths.config_dir))
        .await
        .map_err(|e| BotManagerError::Runtime(BotBackendError::Io(e.to_string())))?;
    let bytes = consent_record_json(version).map_err(map_consent_file_io)?;
    let path = format!("{}/consent.json", paths.config_dir);
    host.write_file(&HostPath::from_posix(&path), &bytes)
        .await
        .map_err(|e| BotManagerError::Runtime(BotBackendError::Io(e.to_string())))?;
    Ok(())
}

async fn read_remote_agreement(host: &dyn Host, snowluma_dir: &str, file_name: &str) -> String {
    let primary = format!("{snowluma_dir}/{file_name}");
    let text = read_remote_optional(host, &primary).await;
    if !text.trim().is_empty() {
        return text;
    }
    let dist = format!("{snowluma_dir}/dist/{file_name}");
    read_remote_optional(host, &dist).await
}

async fn read_remote_optional(host: &dyn Host, path: &str) -> String {
    match host.read_file(&HostPath::from_posix(path)).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}
