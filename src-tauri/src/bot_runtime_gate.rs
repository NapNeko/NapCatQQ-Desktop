//! BotManager 启动预检的 Tauri 实装:把 BotConfig 变成 (host, 组件构建输入) 再交给 resolver
//!
//! 与 TauriHostResolver 同款模式:trait 在 ncd-runtime,这里只是拿 ServerManager /
//! 库存缓存 / 设置把输入凑齐。Docker 部署不走组件图,返回 None。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::RuntimeReadiness;
use ncd_domain::bot_config::{BotConfig, DeploymentType};
use ncd_domain::kinds::RuntimeTarget;
use ncd_domain::{AppSettings, RemoteInventory};
use ncd_host::{Host, Locality};
use ncd_runtime::{
    HostResolver, RemoteHostProbe, RuntimeReadinessGate, ServerManager, framework_component_for,
    infer_snowluma_linux_package, probe_from_inventory,
};
use tokio::sync::{Mutex, RwLock};

use crate::commands::components::build_inputs_with;
use crate::commands::servers::ensure_remote_inventory_with;

pub struct TauriRuntimeGate {
    host_resolver: Arc<dyn HostResolver>,
    server_manager: Arc<ServerManager>,
    host_probe_cache: Arc<Mutex<HashMap<String, RemoteInventory>>>,
    app_settings: Arc<RwLock<AppSettings>>,
    data_root: PathBuf,
    local_snowluma_version: Option<String>,
}

impl TauriRuntimeGate {
    pub fn new(
        host_resolver: Arc<dyn HostResolver>,
        server_manager: Arc<ServerManager>,
        host_probe_cache: Arc<Mutex<HashMap<String, RemoteInventory>>>,
        app_settings: Arc<RwLock<AppSettings>>,
        data_root: PathBuf,
        local_snowluma_version: Option<String>,
    ) -> Self {
        Self {
            host_resolver,
            server_manager,
            host_probe_cache,
            app_settings,
            data_root,
            local_snowluma_version,
        }
    }
}

#[async_trait]
impl RuntimeReadinessGate for TauriRuntimeGate {
    async fn check(&self, config: &BotConfig) -> Result<Option<RuntimeReadiness>, String> {
        if config.bot.deployment_type == DeploymentType::Docker {
            return Ok(None);
        }
        let host: Arc<dyn Host> = self
            .host_resolver
            .resolve(&config.bot.runtime_target)
            .await
            .map_err(|e| e.to_string())?;

        let (probe, selected) = match &config.bot.runtime_target {
            RuntimeTarget::Local => (RemoteHostProbe::local_default(), None),
            RuntimeTarget::Server(id) => {
                let inv = ensure_remote_inventory_with(
                    id,
                    host.as_ref(),
                    &self.server_manager,
                    &self.host_probe_cache,
                    false,
                )
                .await?;
                let selected = inv.selected.clone();
                (probe_from_inventory(&inv), Some(selected))
            }
        };
        let package = (host.locality() != Locality::Local)
            .then(|| infer_snowluma_linux_package(selected.as_ref()));
        let inputs = build_inputs_with(
            &self.data_root,
            &self.app_settings,
            self.local_snowluma_version.as_deref(),
            host.as_ref(),
            &probe,
            selected,
            package,
        )
        .await;
        inputs
            .readiness(framework_component_for(config.bot.backend_type), host.as_ref())
            .await
            .map(Some)
    }
}
