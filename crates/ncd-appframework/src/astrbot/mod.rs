//! AstrBot 安装、WebUI 入口与 OneBot 对接。

mod component;
pub mod config;
mod config_json;
pub mod dashboard_auth;
mod integration;
pub mod manifest;
mod platform;
mod probe;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppFrameworkManifest, AppInstance, AppProjectProbe, OneBotLinkPlan,
    TerminalSnippet,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::AstrBotComponent;
pub use integration::{AstrBotIntegration, join_webui_url};
pub use manifest::{ASTRBOT_FRAMEWORK_ID, astrbot_manifest};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, WebUiAccountProbe, apply_with_backup_ex,
    restore_from_backup,
};
use crate::adopt::write_project_sidecar;
use config::astrbot_config_documents;
use config_json::{cmd_config_path, load_cmd_config, save_cmd_config};
use manifest::ASTRBOT_STDOUT_LOG;
use platform::upsert_claimed_row;

pub struct AstrBotAdapter {
    integration: AstrBotIntegration,
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
        }
    }

    fn component_for(spec: &AppComponentSpec) -> AstrBotComponent {
        AstrBotComponent::new(
            spec.install_dir.clone(),
            spec.port,
            spec.instance_id.clone(),
        )
        .with_uv_bin(spec.uv_bin.clone())
        .with_pypi_index(spec.pypi_index.clone())
        .with_adopt_existing(spec.adopt_existing)
        .with_webui_account(spec.webui_username.clone(), spec.webui_password.clone())
    }
}

#[async_trait]
impl AppFrameworkAdapter for AstrBotAdapter {
    async fn read_webui_endpoint(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(u16, String), AppFrameworkError> {
        let root = load_cmd_config(host, &HostPath::from_posix(&instance.install_dir)).await?;
        let port = platform::dashboard_port(&root);
        Ok((port, String::new()))
    }

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
        // 检查落盘是否为哈希格式（无明文时用于判断是否能代登录）
        let stored_hash = account.stored_hash.as_deref().unwrap_or("");
        let is_hash = dashboard_auth::is_hash_format(stored_hash);
        Ok(Some(WebUiAccountProbe {
            username: account.username,
            password_matches,
            stored_is_hash: is_hash,
        }))
    }

    fn default_webui_username(&self) -> &str {
        dashboard_auth::ASTRBOT_DEFAULT_USERNAME
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
            &[manifest::ASTRBOT_CMD_CONFIG, manifest::ASTRBOT_SHARED_PREFS],
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

    async fn terminal_profile(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
    ) -> crate::terminal::AppTerminalProfile {
        crate::terminal::uv_venv_profile(
            host,
            spec,
            vec![
                TerminalSnippet::new("装了哪些包", "uv pip list"),
                TerminalSnippet::new("AstrBot 版本", "astrbot --version"),
                TerminalSnippet::new("命令帮助", "astrbot help"),
            ],
        )
        .await
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
            &[cmd_config_path(&HostPath::from_posix(
                &instance.install_dir,
            ))],
        )
        .await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(ASTRBOT_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        astrbot_config_documents()
    }

    fn desktop_session_env_keys(&self) -> Option<(&'static str, &'static str)> {
        Some((
            manifest::ENV_DESKTOP_MANAGED,
            manifest::ENV_DESKTOP_SESSION_SECRET,
        ))
    }
}
