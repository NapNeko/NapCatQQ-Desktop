//! AstrBot 适配器：manifest + Component + Integration + 窄配置 + 官方插件店。

pub mod ai;
pub mod api;
mod component;
pub mod config;
mod config_json;
pub mod dashboard_auth;
pub mod dashboard_client;
mod integration;
pub mod live;
pub mod manifest;
mod platform;
pub mod plugin_config;
mod probe;
pub mod runtime;
pub mod store;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigText, AppFrameworkManifest, AppInstance, AppPluginConfigSchema,
    AppProjectProbe, AppStoreResource, OneBotLinkPlan,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use ai::{
    AstrBotAiSettings, AstrBotKbBind, AstrBotPlatformGates, AstrBotProviderModel,
    AstrBotProviderSource, AstrBotSttSettings, AstrBotSubagentConfig, AstrBotSubagentRow,
    AstrBotTtsSettings, AstrBotWebSearchSettings,
};
pub use api::{AstrBotRuntimeApi, AstrBotSession};
pub use component::AstrBotComponent;
pub use config::{AstrBotInstanceConfig, AstrBotOneBotRow};
pub use integration::{join_webui_url, AstrBotIntegration};
pub use manifest::{ASTRBOT_FRAMEWORK_ID, astrbot_manifest};
pub use runtime::{
    AstrBotAbconfInfo, AstrBotDashboardGate, AstrBotDashboardStatus, AstrBotKbCreate,
    AstrBotKnowledgeBase, AstrBotPersona, AstrBotSessionRule,
};
pub use store::{
    ASTRBOT_PLUGINS_URL, astrbot_plugin_market_urls, parse_astrbot_plugins_json,
};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, WebUiAccountProbe, apply_with_backup_ex,
    restore_from_backup,
};
use crate::adopt::write_project_sidecar;
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, combined_revision_of,
    read_document,
};
use crate::adapter::PluginLogSink;
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};
use config::astrbot_config_documents;
use config_json::{cmd_config_path, load_cmd_config, save_cmd_config};
use manifest::ASTRBOT_STDOUT_LOG;
use platform::upsert_claimed_row;

fn envelope(config: AstrBotInstanceConfig, snaps: &[DocumentSnapshot]) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::AstrBot(config),
        revision: combined_revision_of(snaps),
        documents: snaps.iter().map(DocumentSnapshot::revision_entry).collect(),
    }
}

pub struct AstrBotAdapter {
    integration: AstrBotIntegration,
    runtime: api::DashboardRuntime,
}

impl Default for AstrBotAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl AstrBotAdapter {
    pub fn new() -> Self {
        Self {
            integration: AstrBotIntegration::new(),
            runtime: api::DashboardRuntime,
        }
    }

    fn component_for(spec: &AppComponentSpec) -> AstrBotComponent {
        AstrBotComponent::new(spec.install_dir.clone(), spec.port, spec.instance_id.clone())
            .with_uv_bin(spec.uv_bin.clone())
            .with_adopt_existing(spec.adopt_existing)
            .with_webui_account(spec.webui_username.clone(), spec.webui_password.clone())
    }
}

#[async_trait]
impl AppFrameworkAdapter for AstrBotAdapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(Self::component_for(spec))
    }

    fn suggested_import_port(&self, probe: &AppProjectProbe) -> Option<u16> {
        probe.port.filter(|p| *p > 0)
    }

    fn webui_fallback_port(&self, _instance: &AppInstance) -> u16 {
        manifest::ASTRBOT_DEFAULT_DASHBOARD_PORT
    }

    async fn read_webui_account(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        remembered_password: Option<&str>,
    ) -> Result<Option<WebUiAccountProbe>, AppFrameworkError> {
        let root = load_cmd_config(host, &HostPath::from_posix(&instance.install_dir)).await?;
        let account = dashboard_auth::read_dashboard_account(&root);
        let password_matches = remembered_password
            .filter(|p| !p.is_empty())
            .and_then(|p| dashboard_auth::password_matches(&root, p));
        Ok(Some(WebUiAccountProbe {
            username: account.username,
            password_matches,
        }))
    }

    async fn write_webui_password(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        password: &str,
    ) -> Result<(), AppFrameworkError> {
        let install_dir = HostPath::from_posix(&instance.install_dir);
        let mut root = load_cmd_config(host, &install_dir).await?;
        dashboard_auth::set_dashboard_account(&mut root, None, password)?;
        save_cmd_config(host, &install_dir, &root, write_project_sidecar(instance)).await?;
        Ok(())
    }

    fn validate_webui_password(&self, password: &str) -> Result<(), String> {
        dashboard_auth::validate_password(password)
    }

    fn generate_webui_password(&self) -> String {
        dashboard_auth::generate_password()
    }

    async fn probe_project(
        &self,
        host: &dyn Host,
        path: &HostPath,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        probe::probe_astrbot(host, path).await
    }

    async fn adopt_watch_rels(
        &self,
        host: &dyn Host,
        root: &HostPath,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let dotenv = crate::adopt::list_dotenv_rels(host, root).await?;
        Ok(crate::adopt::merge_rels(
            dotenv,
            &[
                manifest::ASTRBOT_CMD_CONFIG,
                manifest::ASTRBOT_SHARED_PREFS,
            ],
        ))
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        Self::component_for(spec)
            .resolve_launch_command(host, args)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        let root = load_cmd_config(host, &HostPath::from_posix(&instance.install_dir)).await?;
        Ok(config::read_access_token_from_root(
            &root,
            instance.id.as_str(),
            instance.port,
        ))
    }

    async fn read_outbound_ws_urls(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<Vec<String>, AppFrameworkError> {
        Ok(Vec::new())
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let install_dir = HostPath::from_posix(&instance.install_dir);
        let path = cmd_config_path(&install_dir);
        let mut root = load_cmd_config(host, &install_dir).await?;
        upsert_claimed_row(
            &mut root,
            instance.id.as_str(),
            instance.port,
            &plan.access_token,
        )?;
        let out = crate::config_doc::render_json_pretty(&root)?;
        apply_with_backup_ex(
            host,
            std::slice::from_ref(&path),
            write_project_sidecar(instance),
            || async {
                host.write_file(&path, out.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))
            },
        )
        .await
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        restore_from_backup(
            host,
            &[cmd_config_path(&HostPath::from_posix(&instance.install_dir))],
        )
        .await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(ASTRBOT_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        astrbot_config_documents()
    }

    fn find_document(
        &self,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigDocument, AppFrameworkError> {
        if let Some(dir) = plugin_config::parse_plugin_doc_id(doc_id) {
            return Ok(plugin_config::plugin_config_document(dir));
        }
        self.config_documents(instance)
            .into_iter()
            .find(|d| d.id == doc_id)
            .ok_or_else(|| AppFrameworkError::Validation(format!("未知的配置文档: {doc_id}")))
    }

    async fn read_config_text(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigText, AppFrameworkError> {
        if let Some(dir) = plugin_config::parse_plugin_doc_id(doc_id) {
            return plugin_config::read_plugin_config_text(host, instance, dir).await;
        }
        let doc = self.find_document(instance, doc_id)?;
        let snap = read_document(host, &HostPath::from_posix(&instance.install_dir), &doc).await?;
        Ok(AppConfigText {
            doc_id: doc.id,
            text: snap.text.unwrap_or_default(),
            revision: snap.revision,
        })
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let install_dir = HostPath::from_posix(&instance.install_dir);
        let (config, snaps) = config::read_astrbot_config(
            host,
            &install_dir,
            instance.id.as_str(),
            instance.port,
        )
        .await?;
        Ok(envelope(config, &snaps))
    }

    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::AstrBot(ab) = config else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 AstrBot 配置".to_string(),
            ));
        };
        let (_, current) = config::read_astrbot_config(
            host,
            &HostPath::from_posix(&instance.install_dir),
            instance.id.as_str(),
            instance.port,
        )
        .await?;
        let (config, snaps) = config::write_astrbot_config(host, instance, ab, &current).await?;
        Ok(envelope(config, &snaps))
    }

    fn supports_live_config(&self) -> bool {
        true
    }

    async fn write_live_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        loopback_port: u16,
        username: &str,
        password: &str,
        config: &AppInstanceConfig,
        conf_id: &str,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::AstrBot(ab) = config else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 AstrBot 配置".to_string(),
            ));
        };
        let install_dir = HostPath::from_posix(&instance.install_dir);
        let (config, snaps) = live::write_live(
            host,
            &install_dir,
            instance.id.as_str(),
            instance.port,
            ab,
            &live::LiveTarget {
                instance_id: instance.id.as_str().to_string(),
                port: loopback_port,
                username: username.to_string(),
                password: password.to_string(),
                conf_id: if conf_id.trim().is_empty() {
                    "default".into()
                } else {
                    conf_id.trim().to_string()
                },
            },
        )
        .await?;
        Ok(envelope(config, &snaps))
    }

    fn astrbot_runtime(&self) -> Option<&dyn api::AstrBotRuntimeApi> {
        Some(&self.runtime)
    }

    async fn list_installed(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        resource: AppStoreResource,
    ) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
        store::list_installed(host, instance, resource).await
    }

    async fn install_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        store::install_item(host, instance, entry, log).await
    }

    async fn update_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        store::update_item(host, instance, entry, log).await
    }

    async fn uninstall_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        _flavor: AppStoreFlavor,
        resource: AppStoreResource,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        if resource != AppStoreResource::Plugin {
            return Err(AppFrameworkError::PluginUnsupported(
                "AstrBot 没有平台适配器商店".into(),
            ));
        }
        store::uninstall_item(host, instance, id, log).await
    }

    async fn set_store_enabled(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        resource: AppStoreResource,
        enabled: bool,
        _overwrite: bool,
    ) -> Result<(), AppFrameworkError> {
        if resource != AppStoreResource::Plugin {
            return Err(AppFrameworkError::PluginUnsupported(
                "AstrBot 没有平台适配器商店".into(),
            ));
        }
        store::set_enabled(host, instance, id, enabled).await
    }

    async fn list_plugin_config_docs(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plugin_name: &str,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        plugin_config::list_plugin_config_docs(host, instance, plugin_name).await
    }

    async fn plugin_config_schema(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plugin_name: &str,
    ) -> Result<Option<AppPluginConfigSchema>, AppFrameworkError> {
        plugin_config::plugin_config_schema(host, instance, plugin_name).await
    }

    fn store_market_urls(&self, resource: AppStoreResource) -> Vec<String> {
        match resource {
            AppStoreResource::Plugin => store::astrbot_plugin_market_urls(),
            AppStoreResource::Adapter => Vec::new(),
        }
    }

    fn store_market_cache_key(&self, resource: AppStoreResource) -> Option<&'static str> {
        match resource {
            AppStoreResource::Plugin => Some("astrbot-plugins"),
            AppStoreResource::Adapter => None,
        }
    }

    fn parse_store_market(
        &self,
        resource: AppStoreResource,
        text: &str,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        match resource {
            AppStoreResource::Plugin => store::parse_astrbot_plugins_json(text),
            AppStoreResource::Adapter => Err(AppFrameworkError::PluginUnsupported(
                "AstrBot 没有平台适配器商店".into(),
            )),
        }
    }
}
