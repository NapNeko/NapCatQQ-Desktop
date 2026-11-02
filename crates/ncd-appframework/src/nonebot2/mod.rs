//! NoneBot2 安装、启动与 OneBot 对接。

mod component;
pub mod config;
mod driver;
pub mod env_layout;
mod integration;
pub mod manifest;
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

pub use component::NoneBot2Component;
pub use integration::NoneBot2Integration;
pub use manifest::{NONEBOT2_FRAMEWORK_ID, nonebot2_manifest};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, apply_with_backup_ex, restore_from_backup,
};
use crate::adopt::{self, write_project_sidecar};
use crate::env_file::EnvFile;
use config::nonebot2_config_documents;
use driver::ensure_reverse_driver;
use manifest::{ENV_DRIVER, NONEBOT2_ENV_FILE, NONEBOT2_STDOUT_LOG};

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

    fn component_for(spec: &AppComponentSpec) -> NoneBot2Component {
        NoneBot2Component::new(spec.install_dir.clone(), spec.port)
            .with_uv_bin(spec.uv_bin.clone())
            .with_pypi_index(spec.pypi_index.clone())
            .with_adopt_existing(spec.adopt_existing)
    }

    async fn write_target(
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<HostPath, AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let layout = config::load_env_layout(host, &root).await?;
        Ok(root.join(layout.write_rel))
    }

    async fn read_text(
        host: &dyn Host,
        path: &HostPath,
    ) -> Result<Option<String>, AppFrameworkError> {
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

    async fn probe_project(
        &self,
        host: &dyn Host,
        path: &HostPath,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        probe::probe_nonebot2(host, path).await
    }

    async fn adopt_watch_rels(
        &self,
        host: &dyn Host,
        root: &HostPath,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let dotenv = adopt::list_dotenv_rels(host, root).await?;
        Ok(adopt::merge_rels(
            dotenv,
            &[
                NONEBOT2_ENV_FILE,
                manifest::NONEBOT2_ENV_PROD_FILE,
                manifest::NONEBOT2_BOT_PY,
                manifest::NONEBOT2_PYPROJECT,
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

    async fn terminal_profile(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
    ) -> crate::terminal::AppTerminalProfile {
        // uv add 同时记录依赖并更新锁文件，供下次安装复用。
        crate::terminal::uv_venv_profile(
            host,
            spec,
            vec![
                TerminalSnippet::new("装了哪些包", "uv pip list"),
                TerminalSnippet::new("装一个包", "uv add "),
                TerminalSnippet::new("Python 版本", "python -V"),
            ],
        )
        .await
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let base = Self::read_text(host, &root.join(NONEBOT2_ENV_FILE)).await?;
        let layout = config::load_env_layout(host, &root).await?;
        let overlay = if layout.is_overlay() {
            Self::read_text(host, &root.join(&layout.write_rel)).await?
        } else {
            None
        };
        Ok(config::read_access_token_from_texts(
            base.as_deref(),
            overlay.as_deref(),
        ))
    }

    async fn read_outbound_ws_urls(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let base = Self::read_text(host, &root.join(NONEBOT2_ENV_FILE)).await?;
        let layout = config::load_env_layout(host, &root).await?;
        let overlay = if layout.is_overlay() {
            Self::read_text(host, &root.join(&layout.write_rel)).await?
        } else {
            None
        };
        Ok(config::read_outbound_ws_urls_from_texts(
            base.as_deref(),
            overlay.as_deref(),
        ))
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let path = Self::write_target(host, instance).await?;
        let text = Self::read_text(host, &path).await?.unwrap_or_default();
        let mut env = EnvFile::parse_python(&text);
        for write in NoneBot2Integration::env_writes(instance, &plan.access_token) {
            env.set_ci(&write.key, &write.value);
        }
        config::write_onebot_env(
            &mut env,
            manifest::ENV_ONEBOT_ACCESS_TOKEN,
            &plan.access_token,
        );
        let current = env.get_ci(ENV_DRIVER).unwrap_or_default();
        env.set_ci(ENV_DRIVER, &ensure_reverse_driver(&current));
        let out = env.render();
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
        restore_from_backup(host, &[Self::write_target(host, instance).await?]).await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(NONEBOT2_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        nonebot2_config_documents()
    }

    async fn list_config_documents(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let layout = config::load_env_layout(host, &root).await?;
        Ok(config::nonebot2_config_documents_for(&layout))
    }
}
