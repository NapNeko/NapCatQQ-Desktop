//! MaiBot 适配器：manifest + Component + 正向对接 + 窄配置 + 上游条款核对。

mod component;
pub mod config;
mod integration;
pub mod manifest;
pub mod release;
pub mod terms;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppFrameworkManifest, AppInstance, AppPendingTerms, AppProjectProbe,
    OneBotLinkPlan,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::{
    MaiBotComponent, generate_webui_token, legacy_port_candidates, read_config_version,
    render_webui_json,
};
pub use config::{
    MaiBotAdapterConfig, MaiBotChatFilter, MaiBotInstanceConfig, MaiBotListMode,
    maibot_config_documents,
};
pub use integration::MaiBotIntegration;
pub use manifest::{MAIBOT_FRAMEWORK_ID, maibot_manifest};

use crate::adapter::{AppComponentSpec, AppFrameworkAdapter, apply_with_backup_ex, restore_from_backup};
use crate::adopt::write_project_sidecar;
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, combined_revision_of,
    read_documents,
};
use manifest::{ADAPTER_CONFIG, ADAPTER_DIR, BOT_CONFIG, CONFIG_PY, MAIBOT_STDOUT_LOG, WEBUI_JSON};

fn host_err(e: ncd_host::HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await.map_err(host_err)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn envelope(config: MaiBotInstanceConfig, snaps: &[DocumentSnapshot]) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::MaiBot(config),
        revision: combined_revision_of(snaps),
        documents: snaps.iter().map(DocumentSnapshot::revision_entry).collect(),
    }
}

fn snapshot_text<'a>(snaps: &'a [DocumentSnapshot], id: &str) -> Option<&'a str> {
    snaps
        .iter()
        .find(|s| s.doc.id == id)
        .and_then(|s| s.text.as_deref())
}

pub struct MaiBotAdapter {
    integration: MaiBotIntegration,
}

impl Default for MaiBotAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MaiBotAdapter {
    pub fn new() -> Self {
        Self {
            integration: MaiBotIntegration::new(),
        }
    }

    fn component_for(spec: &AppComponentSpec) -> MaiBotComponent {
        MaiBotComponent::new(spec.install_dir.clone(), spec.port).with_uv_bin(spec.uv_bin.clone())
    }

    fn adapter_config_path(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir).join(ADAPTER_CONFIG)
    }

    /// 适配器目录不在（装坏了 / 被删了）就别往里写配置，写了也没人读
    async fn require_adapter_dir(
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        let dir = HostPath::from_posix(&instance.install_dir).join(ADAPTER_DIR);
        if host.exists(&dir).await.map_err(host_err)? {
            return Ok(());
        }
        Err(AppFrameworkError::Integration(
            "实例里没有 NapCat 适配器插件，重新安装实例可修复".to_string(),
        ))
    }

    async fn write_adapter_config(
        host: &dyn Host,
        instance: &AppInstance,
        render: impl FnOnce(Option<&str>) -> Result<String, String> + Send,
    ) -> Result<(), AppFrameworkError> {
        Self::require_adapter_dir(host, instance).await?;
        let path = Self::adapter_config_path(instance);
        let current = read_text(host, &path).await?;
        let next = render(current.as_deref()).map_err(AppFrameworkError::Integration)?;
        apply_with_backup_ex(
            host,
            std::slice::from_ref(&path),
            write_project_sidecar(instance),
            || async {
                host.write_file(&path, next.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))
            },
        )
        .await
    }
}

#[async_trait]
impl AppFrameworkAdapter for MaiBotAdapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(Self::component_for(spec))
    }

    /// 首版不导入已有 MaiBot（0.x 升级确认、旧的独立适配器形态都还没处理），默认 probe 已经拒了
    fn suggested_import_port(&self, _probe: &AppProjectProbe) -> Option<u16> {
        None
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        // 不拦的话上游会卡在 input() 等「同意」，没有 stdin 直接退出，用户只看到一句异常退出
        let pending = terms::pending_terms(host, &spec.install_dir).await?;
        if !pending.is_empty() {
            let names: Vec<&str> = pending.iter().map(|t| t.title.as_str()).collect();
            return Err(AppFrameworkError::Validation(format!(
                "{}有更新或还没同意，到应用端页同意后再启动",
                names.join("、")
            )));
        }
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
        let text = read_text(host, &Self::adapter_config_path(instance)).await?;
        Ok(config::read_adapter_token(text.as_deref()))
    }

    async fn read_outbound_ws_urls(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let text = read_text(host, &Self::adapter_config_path(instance)).await?;
        let a = config::read_adapter_config(text.as_deref());
        if !a.enabled {
            return Ok(Vec::new());
        }
        Ok(vec![format!("ws://{}:{}", a.napcat_host, a.napcat_port)])
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let Some(server) = plan.connection.as_ws_server() else {
            return Err(AppFrameworkError::Integration(
                "MaiBot 只能由适配器主动连协议 Bot（正向对接）".to_string(),
            ));
        };
        if server.port == 0 {
            return Err(AppFrameworkError::Integration(
                "协议 Bot 的 WS 服务还没分配端口".to_string(),
            ));
        }
        let listen_host = server.host.clone();
        let port = server.port;
        let token = plan.access_token.clone();
        Self::write_adapter_config(host, instance, move |text| {
            config::write_adapter_link(text, &listen_host, port, &token)
        })
        .await
    }

    async fn unlink(&self, host: &dyn Host, instance: &AppInstance) -> Result<(), AppFrameworkError> {
        Self::write_adapter_config(host, instance, config::write_adapter_disabled).await
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        restore_from_backup(host, &[Self::adapter_config_path(instance)]).await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(MAIBOT_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        maibot_config_documents()
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let snaps = read_documents(host, &root, &maibot_config_documents()).await?;
        let (webui_port, legacy_ws_port) =
            config::read_bot_config_ports(snapshot_text(&snaps, config::DOC_BOT_CONFIG));
        let token_json = read_text(host, &root.join(WEBUI_JSON)).await?;
        let adapter = if host.exists(&root.join(ADAPTER_DIR)).await.map_err(host_err)? {
            Some(config::read_adapter_config(snapshot_text(
                &snaps,
                config::DOC_ADAPTER_CONFIG,
            )))
        } else {
            None
        };
        Ok(envelope(
            MaiBotInstanceConfig {
                webui_port,
                legacy_ws_port,
                webui_token: config::read_webui_token(token_json.as_deref()),
                adapter,
            },
            &snaps,
        ))
    }

    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::MaiBot(cfg) = config else {
            return Err(AppFrameworkError::Validation("写入的不是 MaiBot 配置".to_string()));
        };
        let issues = config::validate(cfg);
        if !issues.is_empty() {
            return Err(AppFrameworkError::ConfigInvalid(issues));
        }
        let root = HostPath::from_posix(&instance.install_dir);
        let bot_path = root.join(BOT_CONFIG);
        let bot_text = read_text(host, &bot_path).await?;
        let version = if bot_text.is_none() {
            let py = read_text(host, &root.join(CONFIG_PY)).await?.unwrap_or_default();
            read_config_version(&py).ok_or_else(|| {
                AppFrameworkError::Integration(format!("{CONFIG_PY} 里找不到 CONFIG_VERSION"))
            })?
        } else {
            String::new()
        };
        let invalid = |doc: &str, e: String| {
            AppFrameworkError::ConfigInvalid(vec![ncd_domain::AppConfigIssue::new(doc, e)])
        };
        let next_bot = config::write_bot_config_ports(
            bot_text.as_deref(),
            &version,
            cfg.webui_port,
            cfg.legacy_ws_port,
        )
        .map_err(|e| invalid(config::DOC_BOT_CONFIG, e))?;
        // 没变的文件不写，免得版本号跳了、白白提示重启
        let mut writes: Vec<(HostPath, String)> = Vec::new();
        if bot_text.as_deref() != Some(next_bot.as_str()) {
            writes.push((bot_path, next_bot));
        }
        if let Some(adapter) = &cfg.adapter
            && host.exists(&root.join(ADAPTER_DIR)).await.map_err(host_err)?
        {
            let path = root.join(ADAPTER_CONFIG);
            let current = read_text(host, &path).await?;
            let next = config::write_adapter_chat(current.as_deref(), &adapter.chat)
                .map_err(|e| invalid(config::DOC_ADAPTER_CONFIG, e))?;
            if current.as_deref() != Some(next.as_str()) {
                writes.push((path, next));
            }
        }
        if !writes.is_empty() {
            let paths: Vec<HostPath> = writes.iter().map(|(p, _)| p.clone()).collect();
            apply_with_backup_ex(host, &paths, write_project_sidecar(instance), || async {
                for (path, text) in &writes {
                    crate::config_doc::ensure_parent_dir(host, path).await?;
                    host.write_file(path, text.as_bytes())
                        .await
                        .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
                }
                Ok(())
            })
            .await?;
        }
        self.read_config(host, instance).await
    }

    async fn pending_terms(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<AppPendingTerms>, AppFrameworkError> {
        terms::pending_terms(host, &HostPath::from_posix(&instance.install_dir)).await
    }

    async fn accept_terms(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        terms::write_confirmations(host, &HostPath::from_posix(&instance.install_dir), false).await
    }
}
