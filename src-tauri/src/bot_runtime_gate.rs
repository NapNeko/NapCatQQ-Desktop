//! BotManager 启动预检的 Tauri 实装:把 BotConfig 变成 (host, 组件构建输入) 再交给 resolver
//!
//! 与 TauriHostResolver 同款模式:trait 在 ncd-runtime,这里只是拿 ServerManager /
//! 库存缓存 / 设置把输入凑齐。Docker 部署不走组件图,返回 None。

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::RuntimeReadiness;
use ncd_domain::bot_config::{BotConfig, DeploymentType};
use ncd_domain::kinds::RuntimeTarget;
use ncd_host::{Host, Locality};
use ncd_runtime::{
    ComponentExecutor, HostResolver, RemoteHostProbe, RuntimeReadinessGate,
    framework_component_for, infer_snowluma_linux_package, probe_from_inventory,
};

pub struct TauriRuntimeGate {
    host_resolver: Arc<dyn HostResolver>,
    components: Arc<ComponentExecutor>,
}

impl TauriRuntimeGate {
    pub fn new(host_resolver: Arc<dyn HostResolver>, components: Arc<ComponentExecutor>) -> Self {
        Self {
            host_resolver,
            components,
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
                let inv = self
                    .components
                    .inventory()
                    .ensure(id, host.as_ref(), false)
                    .await?;
                let selected = inv.selected.clone();
                (probe_from_inventory(&inv), Some(selected))
            }
        };
        let package = (host.locality() != Locality::Local)
            .then(|| infer_snowluma_linux_package(selected.as_ref()));
        let inputs = self
            .components
            .build_inputs(host.as_ref(), &probe, selected, package)
            .await;
        inputs
            .readiness(framework_component_for(config.bot.backend_type), host.as_ref())
            .await
            .map(Some)
    }
}
