//! NoneBot2 适配器：manifest + Component + Integration + `.env.prod` 写入。
//!
//! 第二个框架，验证「只加一个子目录 + 注册一行」：不碰 AppManager / BotManager / UI。

mod component;
mod integration;
pub mod manifest;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigFormat, AppFrameworkManifest, AppInstance, OneBotLinkPlan,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::NoneBot2Component;
pub use integration::NoneBot2Integration;
pub use manifest::{NONEBOT2_FRAMEWORK_ID, nonebot2_manifest};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, apply_with_backup, restore_from_backup,
};
use crate::env_file::EnvFile;
use manifest::{
    ENV_ONEBOT_ACCESS_TOKEN, NONEBOT2_ENV_FILE, NONEBOT2_ENV_PROD_FILE, NONEBOT2_PYPROJECT,
    NONEBOT2_STDOUT_LOG,
};

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

    /// 首版只开原始文件（类型化模型留后续）；NoneBot2 三个文件都不热加载，改完要重启
    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        let d = |id: &str, rel: &str, format: AppConfigFormat| AppConfigDocument {
            id: id.to_string(),
            label: rel.to_string(),
            rel_path: rel.to_string(),
            format,
            hot_reload: false,
        };
        vec![
            d("env", NONEBOT2_ENV_FILE, AppConfigFormat::DotEnv),
            d("env_prod", NONEBOT2_ENV_PROD_FILE, AppConfigFormat::DotEnv),
            d("pyproject", NONEBOT2_PYPROJECT, AppConfigFormat::Toml),
        ]
    }
}
