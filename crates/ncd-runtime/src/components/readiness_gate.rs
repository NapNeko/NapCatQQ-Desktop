//! BotManager 启动预检的实装:BotConfig → 主机 → ComponentExecutor::runtime_readiness
//!
//! 组件页的 resolve_runtime_readiness 命令走同一个 runtime_readiness,两边只在
//! 「怎么拿到 host」上不同。Docker 部署不走组件图,返回 None。

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::RuntimeReadiness;
use ncd_domain::bot_config::{BotConfig, DeploymentType};
use ncd_domain::host_id_of_runtime_target;
use ncd_server::HostResolver;

use crate::bot_manager::{RuntimeReadinessGate, framework_component_for};
use crate::components::executor::ComponentExecutor;

pub struct ComponentRuntimeGate {
    host_resolver: Arc<dyn HostResolver>,
    components: Arc<ComponentExecutor>,
}

impl ComponentRuntimeGate {
    pub fn new(host_resolver: Arc<dyn HostResolver>, components: Arc<ComponentExecutor>) -> Self {
        Self {
            host_resolver,
            components,
        }
    }
}

#[async_trait]
impl RuntimeReadinessGate for ComponentRuntimeGate {
    async fn check(&self, config: &BotConfig) -> Result<Option<RuntimeReadiness>, String> {
        if config.bot.deployment_type == DeploymentType::Docker {
            return Ok(None);
        }
        let target = &config.bot.runtime_target;
        let host = self
            .host_resolver
            .resolve(target)
            .await
            .map_err(|e| e.to_string())?;
        self.components
            .runtime_readiness(
                framework_component_for(config.bot.backend_type),
                &host_id_of_runtime_target(target),
                host.as_ref(),
            )
            .await
            .map(Some)
    }
}
