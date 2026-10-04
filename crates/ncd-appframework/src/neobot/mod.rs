//! NeoBot 适配器：manifest + Component + Integration + 类型化配置 + 导入探测。

pub mod component;
pub mod config;
pub mod integration;
pub mod manifest;
pub mod probe;
pub mod versions;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppFrameworkManifest, AppInstance, AppProjectProbe, OneBotLinkPlan,
    TerminalSnippet,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::NeoBotComponent;
pub use config::{
    DOC_ADAPTER, DOC_DASHBOARD, NeoBotAdapterConfig, NeoBotDashboardConfig, NeoBotInstanceConfig,
    neobot_config_documents,
};
pub use integration::{NeoBotIntegration, join_webui_url};
pub use manifest::{
    NEOBOT_DEFAULT_DASHBOARD_PORT, NEOBOT_DEFAULT_ONEBOT_PORT, NEOBOT_FRAMEWORK_ID,
    neobot_manifest,
};
pub use probe::probe_neobot;
pub use versions::{PackageVersions, fetch_versions, parse_versions};

use crate::adapter::{AppComponentSpec, AppFrameworkAdapter, restore_from_backup};
use crate::adopt::write_project_sidecar;
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, combined_revision_of,
};
use crate::terminal;

fn envelope(
    config: NeoBotInstanceConfig,
    snaps: &[DocumentSnapshot],
) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::NeoBot(config),
        revision: combined_revision_of(snaps),
        documents: snaps.iter().map(DocumentSnapshot::revision_entry).collect(),
    }
}

pub struct NeoBotAdapter {
    integration: NeoBotIntegration,
}

impl Default for NeoBotAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl NeoBotAdapter {
    pub fn new() -> Self {
        Self {
            integration: NeoBotIntegration::new(),
        }
    }

    fn component_for(spec: &AppComponentSpec) -> NeoBotComponent {
        let dashboard_port = dashboard_port_for(spec);
        NeoBotComponent::new(spec.install_dir.clone(), spec.port, dashboard_port)
            .with_uv_bin(spec.uv_bin.clone())
            .with_pypi_index(spec.pypi_index.clone())
            .with_adopt_existing(spec.adopt_existing)
            .with_install_version(spec.install_version.clone())
    }

    fn install_dir(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir)
    }
}

/// 面板口：目前没有从 spec 传进来的通道（`AppComponentSpec` 里没有这个字段），
/// 一律用出厂值；组件装完会把真实口写进面板配置，之后读类型化配置就能拿到。
fn dashboard_port_for(_spec: &AppComponentSpec) -> u16 {
    manifest::NEOBOT_DEFAULT_DASHBOARD_PORT
}

#[async_trait]
impl AppFrameworkAdapter for NeoBotAdapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(Self::component_for(spec))
    }

    /// NeoBot 发在 PyPI，可安装版本查 PyPI 的 JSON API
    async fn available_versions(
        &self,
    ) -> Result<Option<versions::PackageVersions>, AppFrameworkError> {
        versions::fetch_versions(manifest::PYPI_NEOBOT, None)
            .await
            .map(Some)
            .map_err(|e| AppFrameworkError::Integration(format!("查 PyPI 版本失败：{e}")))
    }

    async fn probe_project(
        &self,
        host: &dyn Host,
        path: &HostPath,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        probe::probe_neobot(host, path).await
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

    async fn terminal_profile(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
    ) -> terminal::AppTerminalProfile {
        // 插件与依赖平时由面板自己装；终端里照同样的办法来
        terminal::uv_venv_profile(
            host,
            spec,
            vec![
                TerminalSnippet::new("装了哪些包", "uv pip list"),
                TerminalSnippet::new("换一个版本", "uv pip install -U neobot-app"),
                TerminalSnippet::new("直接跑一次", "neobot"),
            ],
        )
        .await
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        match config::read_neobot_config(host, &Self::install_dir(instance)).await {
            Ok((cfg, _)) => {
                let token = cfg.adapter.reverse_ws_access_token.trim();
                Ok((!token.is_empty()).then(|| token.to_string()))
            }
            // 没装好 / 文件坏了都不算「已有 token」
            Err(_) => Ok(None),
        }
    }

    /// 把对接写进 `data/config.toml` 的 `[adapter]`。
    ///
    /// 反向 WS：应用端是监听方，**口以实例口为准**（桌面端分配的实例口就是受管口，
    /// 也是 `listen_port()` 报给编排层的值），不拿 plan 里 URL 的端口去改写——
    /// 那个 url 是给协议 Bot 连的地址，跨机时还可能是隧道口。
    /// token 两边必须一致，所以写 plan 给的。
    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let root = Self::install_dir(instance);
        let (mut cfg, _) = config::read_neobot_config(host, &root).await.map_err(|e| {
            AppFrameworkError::Integration(format!("读 NeoBot 配置失败，无法对接：{e}"))
        })?;
        cfg.adapter.reverse_ws_access_token = plan.access_token.clone();
        if instance.port > 0 {
            cfg.adapter.reverse_ws_port = instance.port;
        }
        let issues = cfg.validate();
        if !issues.is_empty() {
            return Err(AppFrameworkError::ConfigInvalid(issues));
        }
        // write_neobot_config 内部走 apply_with_backup_ex：备份 -> 差量写 -> 失败还原
        config::write_neobot_config(host, &root, &cfg, write_project_sidecar(instance))
            .await
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
        Ok(())
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        restore_from_backup(
            host,
            &[Self::install_dir(instance).join(manifest::NEOBOT_CONFIG_TOML)],
        )
        .await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(Self::install_dir(instance).join(manifest::NEOBOT_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        neobot_config_documents()
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let (cfg, snaps) = config::read_neobot_config(host, &Self::install_dir(instance)).await?;
        Ok(envelope(cfg, &snaps))
    }

    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::NeoBot(next) = config else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 NeoBot 配置".to_string(),
            ));
        };
        let root = Self::install_dir(instance);
        let (cfg, snaps) =
            config::write_neobot_config(host, &root, next, write_project_sidecar(instance)).await?;
        Ok(envelope(cfg, &snaps))
    }

    fn webui_fallback_port(&self, _instance: &AppInstance) -> u16 {
        manifest::NEOBOT_DEFAULT_DASHBOARD_PORT
    }

    fn validate_webui_password(&self, password: &str) -> Result<(), String> {
        if password.chars().count() < 8 {
            return Err("面板密码至少 8 位".to_string());
        }
        Ok(())
    }
}
