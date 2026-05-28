//! Docker 部署：写 compose.yml + docker compose up，跑容器化 bot。
//!
//! P1.a 占位。`supports` 当前一律返回 false（让 UI 在 P4 实装前不暴露这个
//! 选项），所有方法返回 `Unsupported`。
//!
//! P4 阶段计划：
//! - install: 渲染 docker-compose.yml + nginx 反代配置 → 上传到 host →
//!   `docker compose pull`
//! - launch: `docker compose up -d <bot-service>`
//! - observe: `docker inspect <container>` 解析 State.Status
//! - stop: `docker compose stop <bot-service>`
//! - uninstall: `docker compose down -v` + 删 compose 目录

use async_trait::async_trait;
use ncd_domain::{BotConfig, BotFlavor, BotId, StopMode};
use ncd_host::Host;

use crate::deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
};

/// Docker 部署占位实装。
pub struct DockerDeployment {
    id: &'static str,
    flavors: &'static [BotFlavor],
}

impl DockerDeployment {
    /// 构造占位实例。P4 阶段会扩展为携带镜像 tag / compose template 等参数。
    pub fn new() -> Self {
        Self {
            id: "docker",
            flavors: &[BotFlavor::NapCat],
        }
    }
}

impl Default for DockerDeployment {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Deployment for DockerDeployment {
    fn id(&self) -> &str {
        self.id
    }

    fn supported_flavors(&self) -> &[BotFlavor] {
        self.flavors
    }

    fn supports(&self, _host: &dyn Host) -> bool {
        // P4 实装前一律不支持，避免 UI 把这个选项暴露给用户。
        false
    }

    async fn install(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
        _progress: &dyn DeploymentProgressSink,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "DockerDeployment pending P4 implementation",
        ))
    }

    async fn launch(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
    ) -> Result<DeploymentHandle, DeploymentError> {
        Err(DeploymentError::Unsupported(
            "DockerDeployment pending P4 implementation",
        ))
    }

    async fn observe(
        &self,
        _host: &dyn Host,
        _bot_id: &BotId,
    ) -> Result<DeploymentState, DeploymentError> {
        Err(DeploymentError::Unsupported(
            "DockerDeployment pending P4 implementation",
        ))
    }

    async fn stop(
        &self,
        _host: &dyn Host,
        _bot_id: &BotId,
        _mode: StopMode,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "DockerDeployment pending P4 implementation",
        ))
    }

    async fn uninstall(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "DockerDeployment pending P4 implementation",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_deployment_id_is_stable() {
        assert_eq!(DockerDeployment::new().id(), "docker");
    }

    #[test]
    fn docker_deployment_unsupported_error_messages_mention_p4() {
        // 占位阶段所有方法都返回 Unsupported variant，给后续 P4 实装留位。
        // 仅断言枚举字面量稳定，避免后续测试在真实装到来时不更新。
        let err = DeploymentError::Unsupported("DockerDeployment pending P4 implementation");
        assert!(err.to_string().contains("P4"));
    }
}
