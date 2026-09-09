//! Karin 适配器：manifest + Component + Integration + `.env` 写入。

mod component;
pub mod config;
mod integration;
pub mod manifest;
mod probe;
pub mod plugin;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigText, AppFrameworkManifest, AppInstance, AppProjectProbe,
    AppStoreResource, OneBotLinkPlan,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::KarinComponent;
pub use config::{KarinInstanceConfig, karin_config_documents};
pub use integration::KarinIntegration;
pub use manifest::{KARIN_FRAMEWORK_ID, karin_manifest};
pub use plugin::{
    KarinPluginAppFile, KarinPluginAuthor, KarinPluginInstalled, KarinPluginKind,
    KarinPluginMarketEntry, KarinPluginRepo, apply_plugin_enabled, app_file_basename,
    confirm_plugin_on_disk, git_clone_url, parse_karin_plugins_list, write_app_file_bytes,
};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, apply_with_backup_ex, restore_from_backup,
};
use crate::adopt::{self, write_project_sidecar};
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, MISSING_REVISION,
    combined_revision_of, read_document,
};
use crate::env_file::EnvFile;
use manifest::{ENV_WS_SERVER_AUTH_KEY, KARIN_ADAPTER_JSON, KARIN_ENV_FILE, KARIN_STDOUT_LOG};

fn envelope(config: KarinInstanceConfig, snaps: &[DocumentSnapshot]) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::Karin(config),
        revision: combined_revision_of(snaps),
        documents: snaps.iter().map(DocumentSnapshot::revision_entry).collect(),
    }
}

pub struct KarinAdapter {
    integration: KarinIntegration,
}

impl Default for KarinAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl KarinAdapter {
    pub fn new() -> Self {
        Self {
            integration: KarinIntegration::new(),
        }
    }

    fn env_path(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir).join(KARIN_ENV_FILE)
    }

    fn adapter_json_path(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir).join(KARIN_ADAPTER_JSON)
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

    /// `adapter.json` 的 `onebot.ws_server.enable` 上游默认 true；用户关过就再打开。
    /// 缺路径时补上，避免解绑后再对接被旧文件 / 手改结构整单失败。
    fn enable_ws_server(text: &str) -> Result<Option<String>, AppFrameworkError> {
        let mut value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| AppFrameworkError::Integration(format!("adapter.json 解析失败: {e}")))?;
        let onebot = match value.get_mut("onebot") {
            Some(v) if v.is_object() => v,
            Some(_) => {
                return Err(AppFrameworkError::Integration(
                    "adapter.json 的 onebot 不是对象".to_string(),
                ));
            }
            None => {
                value
                    .as_object_mut()
                    .ok_or_else(|| {
                        AppFrameworkError::Integration("adapter.json 根必须是对象".to_string())
                    })?
                    .insert("onebot".into(), serde_json::json!({}));
                value.get_mut("onebot").expect("just inserted")
            }
        };
        let ws_server = match onebot.get_mut("ws_server") {
            Some(v) if v.is_object() => v,
            Some(_) => {
                return Err(AppFrameworkError::Integration(
                    "adapter.json 的 onebot.ws_server 不是对象".to_string(),
                ));
            }
            None => {
                onebot
                    .as_object_mut()
                    .expect("onebot is object")
                    .insert("ws_server".into(), serde_json::json!({ "timeout": 120 }));
                onebot.get_mut("ws_server").expect("just inserted")
            }
        };
        if ws_server.get("enable").and_then(|v| v.as_bool()) == Some(true) {
            return Ok(None);
        }
        ws_server
            .as_object_mut()
            .expect("ws_server is object")
            .insert("enable".into(), serde_json::Value::Bool(true));
        serde_json::to_string_pretty(&value)
            .map(|s| Some(format!("{s}\n")))
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))
    }
}

#[async_trait]
impl AppFrameworkAdapter for KarinAdapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(
            KarinComponent::new(spec.install_dir.clone(), spec.port)
                .with_node_bin(spec.node_bin.clone())
                .with_npm_registry(spec.npm_registry.clone())
                .with_install_renderer(spec.install_renderer)
                .with_adopt_existing(spec.adopt_existing),
        )
    }

    async fn probe_project(
        &self,
        host: &dyn Host,
        path: &HostPath,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        probe::probe_karin(host, path).await
    }

    async fn adopt_watch_rels(
        &self,
        host: &dyn Host,
        root: &HostPath,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let dotenv = adopt::list_dotenv_rels(host, root).await?;
        let mut extra: Vec<&str> = vec!["package.json", KARIN_ENV_FILE, KARIN_ADAPTER_JSON];
        let docs = karin_config_documents();
        let owned: Vec<String> = docs.iter().map(|d| d.rel_path.clone()).collect();
        extra.extend(owned.iter().map(String::as_str));
        Ok(adopt::merge_rels(dotenv, &extra))
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        KarinComponent::new(spec.install_dir.clone(), spec.port)
            .with_node_bin(spec.node_bin.clone())
            .resolve_launch_command(host, args)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        let Some(text) = Self::read_text(host, &Self::env_path(instance)).await? else {
            return Ok(None);
        };
        Ok(EnvFile::parse(&text)
            .get(ENV_WS_SERVER_AUTH_KEY)
            .filter(|v| !v.trim().is_empty()))
    }

    async fn read_outbound_ws_urls(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let Some(text) = Self::read_text(host, &Self::adapter_json_path(instance)).await? else {
            return Ok(Vec::new());
        };
        Ok(config::outbound_onebot_ws_urls(&text))
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let env_path = Self::env_path(instance);
        let adapter_path = Self::adapter_json_path(instance);
        let Some(env_text) = Self::read_text(host, &env_path).await? else {
            return Err(AppFrameworkError::Integration(format!(
                "Karin 实例缺少 .env（{}），请先完成安装",
                env_path.as_posix()
            )));
        };
        let adapter_text = Self::read_text(host, &adapter_path).await?;

        let mut env = EnvFile::parse(&env_text);
        env.apply(&KarinIntegration::env_writes(instance, &plan.access_token));
        let env_out = env.render();
        let adapter_out = match adapter_text.as_deref() {
            Some(text) => Self::enable_ws_server(text)?,
            None => None,
        };

        let mut touched = vec![env_path.clone()];
        if adapter_out.is_some() {
            touched.push(adapter_path.clone());
        }
        apply_with_backup_ex(host, &touched, write_project_sidecar(instance), || async {
            host.write_file(&env_path, env_out.as_bytes())
                .await
                .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
            if let Some(out) = &adapter_out {
                host.write_file(&adapter_path, out.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
            }
            Ok(())
        })
        .await
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        restore_from_backup(
            host,
            &[Self::env_path(instance), Self::adapter_json_path(instance)],
        )
        .await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(KARIN_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        karin_config_documents()
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let install_dir = HostPath::from_posix(&instance.install_dir);
        let (config, snaps) = config::read_karin_config(host, &install_dir).await?;
        Ok(envelope(config, &snaps))
    }

    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::Karin(karin) = config else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 Karin 配置".to_string(),
            ));
        };
        let install_dir = HostPath::from_posix(&instance.install_dir);
        let (_, current) = config::read_karin_config(host, &install_dir).await?;
        let (config, snaps) = config::write_karin_config(
            host,
            &install_dir,
            karin,
            &current,
            write_project_sidecar(instance),
        )
        .await?;
        Ok(envelope(config, &snaps))
    }

    async fn list_installed(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        resource: AppStoreResource,
    ) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
        if resource != AppStoreResource::Plugin {
            return Ok(Vec::new());
        }
        Ok(plugin::list_installed(host, instance)
            .await?
            .into_iter()
            .map(AppStoreInstalled::from_karin)
            .collect())
    }

    async fn install_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&plugin::PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        let karin = entry.to_karin().ok_or_else(|| {
            AppFrameworkError::Validation("Karin 不能安装 PyPI 条目".to_string())
        })?;
        plugin::install_plugin(host, instance, &karin, log).await
    }

    async fn update_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&plugin::PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        let karin = entry.to_karin().ok_or_else(|| {
            AppFrameworkError::Validation("Karin 不能更新 PyPI 条目".to_string())
        })?;
        plugin::update_plugin(host, instance, &karin, log).await
    }

    async fn uninstall_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        flavor: AppStoreFlavor,
        _resource: AppStoreResource,
        log: Option<&plugin::PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        let kind = flavor.to_karin().ok_or_else(|| {
            AppFrameworkError::Validation("Karin 不能卸载 PyPI 条目".to_string())
        })?;
        plugin::uninstall_plugin(host, instance, id, kind, log).await
    }

    async fn list_plugin_config_docs(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plugin_name: &str,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        plugin::list_plugin_config_docs(host, instance, plugin_name).await
    }

    fn find_document(
        &self,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigDocument, AppFrameworkError> {
        if let Some(doc) = plugin::resolve_plugin_config_doc(doc_id) {
            return Ok(doc);
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
        let doc = self.find_document(instance, doc_id)?;
        let root = HostPath::from_posix(&instance.install_dir);
        let snap = read_document(host, &root, &doc).await?;
        if let Some(text) = snap.text {
            return Ok(AppConfigText {
                doc_id: doc.id,
                text,
                revision: snap.revision,
            });
        }
        if let Some((name, rel)) = plugin::parse_plugin_doc_id(doc_id) {
            if let Some(text) = plugin::read_plugin_package_default(host, instance, name, rel).await?
            {
                return Ok(AppConfigText {
                    doc_id: doc.id,
                    text,
                    revision: MISSING_REVISION.to_string(),
                });
            }
        }
        Ok(AppConfigText {
            doc_id: doc.id,
            text: String::new(),
            revision: MISSING_REVISION.to_string(),
        })
    }

    fn store_enable_via_config(&self) -> bool {
        true
    }

    fn apply_store_enabled(
        &self,
        config: &AppInstanceConfig,
        name: &str,
        _resource: AppStoreResource,
        enabled: bool,
    ) -> Result<Option<AppInstanceConfig>, AppFrameworkError> {
        let AppInstanceConfig::Karin(mut cfg) = config.clone() else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 Karin 配置".into(),
            ));
        };
        apply_plugin_enabled(&mut cfg, name, enabled)?;
        Ok(Some(AppInstanceConfig::Karin(cfg)))
    }

    async fn confirm_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &crate::store::AppStoreMarketEntry,
    ) -> Result<(), AppFrameworkError> {
        let Some(karin) = entry.to_karin() else {
            return Ok(());
        };
        confirm_plugin_on_disk(host, instance, &karin).await
    }

    fn store_market_urls(&self, resource: AppStoreResource) -> Vec<String> {
        match resource {
            AppStoreResource::Plugin => vec![plugin::KARIN_PLUGINS_LIST_URL.to_string()],
            AppStoreResource::Adapter => Vec::new(),
        }
    }

    fn parse_store_market(
        &self,
        resource: AppStoreResource,
        text: &str,
    ) -> Result<Vec<crate::store::AppStoreMarketEntry>, AppFrameworkError> {
        match resource {
            AppStoreResource::Plugin => Ok(parse_karin_plugins_list(text)?
                .into_iter()
                .map(crate::store::AppStoreMarketEntry::from_karin)
                .collect()),
            AppStoreResource::Adapter => Ok(Vec::new()),
        }
    }

    fn store_app_file_dest(&self, instance: &AppInstance, basename: &str) -> Option<HostPath> {
        Some(
            HostPath::from_posix(&instance.install_dir)
                .join("plugins/karin-plugin-example")
                .join(basename),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_ws_server_only_rewrites_when_disabled() {
        let already = r#"{"console":{"isLocal":true},"onebot":{"ws_server":{"enable":true,"timeout":120}}}"#;
        assert_eq!(KarinAdapter::enable_ws_server(already).unwrap(), None);

        let disabled = r#"{"onebot":{"ws_server":{"enable":false,"timeout":120},"ws_client":[]}}"#;
        let out = KarinAdapter::enable_ws_server(disabled).unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["onebot"]["ws_server"]["enable"], true);
        assert_eq!(v["onebot"]["ws_server"]["timeout"], 120);

        let created = KarinAdapter::enable_ws_server(r#"{"onebot":{}}"#)
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&created).unwrap();
        assert_eq!(v["onebot"]["ws_server"]["enable"], true);

        let from_root = KarinAdapter::enable_ws_server(r#"{}"#)
            .unwrap()
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&from_root).unwrap();
        assert_eq!(v["onebot"]["ws_server"]["enable"], true);
    }
}
