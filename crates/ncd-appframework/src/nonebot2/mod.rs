//! NoneBot2 适配器：manifest + Component + Integration + 窄配置 + 官方商店。

mod component;
pub mod config;
mod driver;
mod integration;
pub mod manifest;
pub mod store;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppFrameworkManifest, AppInstance, AppStoreResource, OneBotLinkPlan,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::NoneBot2Component;
pub use config::{NoneBot2EnvEntry, NoneBot2EnvProd, NoneBot2InstanceConfig};
pub use integration::NoneBot2Integration;
pub use manifest::{NONEBOT2_FRAMEWORK_ID, nonebot2_manifest};
pub use store::{
    NONEBOT_ADAPTERS_URL, NONEBOT_PLUGINS_URL, ONEBOT_V11_MODULE, nonebot_registry_urls,
    parse_nonebot_adapters_json, parse_nonebot_plugins_json,
};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, apply_with_backup, restore_from_backup,
};
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, combined_revision_of,
};
use crate::env_file::EnvFile;
use crate::adapter::PluginLogSink;
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};
use config::nonebot2_config_documents;
use manifest::{ENV_ONEBOT_ACCESS_TOKEN, NONEBOT2_ENV_PROD_FILE, NONEBOT2_STDOUT_LOG};

fn envelope(config: NoneBot2InstanceConfig, snaps: &[DocumentSnapshot]) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::NoneBot2(config),
        revision: combined_revision_of(snaps),
        documents: snaps.iter().map(DocumentSnapshot::revision_entry).collect(),
    }
}

pub struct NoneBot2Adapter {
    integration: NoneBot2Integration,
}

impl Default for NoneBot2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl NoneBot2Adapter {
    pub fn new() -> Self {
        Self {
            integration: NoneBot2Integration::new(),
        }
    }

    fn env_prod_path(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir).join(NONEBOT2_ENV_PROD_FILE)
    }

    fn component_for(spec: &AppComponentSpec) -> NoneBot2Component {
        NoneBot2Component::new(spec.install_dir.clone(), spec.port)
            .with_uv_bin(spec.uv_bin.clone())
    }

    async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
        if !host
            .exists(path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?
        {
            return Ok(None);
        }
        let bytes = host
            .read_file(path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

#[async_trait]
impl AppFrameworkAdapter for NoneBot2Adapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(Self::component_for(spec))
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        store::ensure_dynamic_bot_py_at(host, &spec.install_dir, None).await?;
        store::ensure_forward_driver_at(host, &spec.install_dir, None).await?;
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
        let Some(text) = Self::read_text(host, &Self::env_prod_path(instance)).await? else {
            return Ok(None);
        };
        Ok(EnvFile::parse(&text)
            .get(ENV_ONEBOT_ACCESS_TOKEN)
            .filter(|v| !v.trim().is_empty()))
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let path = Self::env_prod_path(instance);
        let Some(text) = Self::read_text(host, &path).await? else {
            return Err(AppFrameworkError::Integration(format!(
                "NoneBot2 实例缺少 .env.prod（{}），请先完成安装",
                path.as_posix()
            )));
        };
        let mut env = EnvFile::parse(&text);
        env.apply(&NoneBot2Integration::env_writes(instance, &plan.access_token));
        let out = env.render();
        apply_with_backup(host, std::slice::from_ref(&path), || async {
            host.write_file(&path, out.as_bytes())
                .await
                .map_err(|e| AppFrameworkError::Integration(e.to_string()))
        })
        .await
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        restore_from_backup(host, &[Self::env_prod_path(instance)]).await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(NONEBOT2_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        nonebot2_config_documents()
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let install_dir = HostPath::from_posix(&instance.install_dir);
        let (config, snaps) = config::read_nonebot2_config(host, &install_dir).await?;
        Ok(envelope(config, &snaps))
    }

    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::NoneBot2(nb) = config else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 NoneBot2 配置".to_string(),
            ));
        };
        let install_dir = HostPath::from_posix(&instance.install_dir);
        let (_, current) = config::read_nonebot2_config(host, &install_dir).await?;
        let (config, snaps) =
            config::write_nonebot2_config(host, &install_dir, nb, &current).await?;
        Ok(envelope(config, &snaps))
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
        if entry.flavor != AppStoreFlavor::Pypi {
            return Err(AppFrameworkError::Validation(
                "NoneBot2 只能安装 PyPI 条目".into(),
            ));
        }
        store::install_item(host, instance, entry, log).await
    }

    async fn update_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        if entry.flavor != AppStoreFlavor::Pypi {
            return Err(AppFrameworkError::Validation(
                "NoneBot2 只能更新 PyPI 条目".into(),
            ));
        }
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
        store::uninstall_item(host, instance, id, resource, log).await
    }

    async fn set_store_enabled(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        resource: AppStoreResource,
        enabled: bool,
        overwrite: bool,
    ) -> Result<(), AppFrameworkError> {
        store::set_enabled(host, instance, id, resource, enabled, overwrite).await
    }

    async fn list_plugin_config_docs(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
        _plugin_name: &str,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        Ok(store::plugin_config_docs())
    }

    fn store_market_urls(&self, resource: AppStoreResource) -> Vec<String> {
        match resource {
            AppStoreResource::Adapter => store::nonebot_registry_urls("adapters.json"),
            AppStoreResource::Plugin => store::nonebot_registry_urls("plugins.json"),
        }
    }

    fn store_market_cache_key(&self, resource: AppStoreResource) -> Option<&'static str> {
        match resource {
            AppStoreResource::Adapter => Some("adapters"),
            AppStoreResource::Plugin => Some("plugins"),
        }
    }

    fn parse_store_market(
        &self,
        resource: AppStoreResource,
        text: &str,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        match resource {
            AppStoreResource::Adapter => store::parse_nonebot_adapters_json(text),
            AppStoreResource::Plugin => store::parse_nonebot_plugins_json(text),
        }
    }
}
