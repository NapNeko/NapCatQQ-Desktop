//! 外接部署：对接用户已经在跑的 OneBot 服务（Lagrange / NapCat-CLI / 其它实装）。
//!
//! P1.a 占位。`supports` 当前一律返回 false（让 UI 在 P5 实装前不暴露这个
//! 选项），所有方法返回 `Unsupported`。
//!
//! P5 阶段计划：
//! - install: HTTP probe 用户给的 endpoint 是否可达
//! - launch: 注册到内部 endpoint 监控（不真启动，user 自己管）
//! - observe: 周期 health check endpoint
//! - stop: no-op（user 自己管启停）
//! - uninstall: 仅清 desktop 这边的注册记录

use async_trait::async_trait;
use ncd_domain::{BotConfig, BotFlavor, BotId, StopMode};
use ncd_host::Host;

use crate::deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
};

/// 外接部署占位实装。
pub struct ExternalDeployment {
    id: &'static str,
    flavors: &'static [BotFlavor],
}

impl ExternalDeployment {
    /// 构造占位实例。P5 阶段会扩展为携带 endpoint URL / auth token / 协议类型。
    pub fn new() -> Self {
        Self {
            id: "external",
            // External 不限制 flavor 类型——任何 OneBot 11 兼容的服务都能接入。
            // 这里给个空切片让 UI 不预设支持，调用方应通过 endpoint 协议探测决定。
            flavors: &[],
        }
    }
}

impl Default for ExternalDeployment {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Deployment for ExternalDeployment {
    fn id(&self) -> &str {
        self.id
    }

    fn supported_flavors(&self) -> &[BotFlavor] {
        self.flavors
    }

    fn supports(&self, _host: &dyn Host) -> bool {
        // P5 实装前一律不支持。
        false
    }

    async fn install(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
        _progress: &dyn DeploymentProgressSink,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "ExternalDeployment pending P5 implementation",
        ))
    }

    async fn launch(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
    ) -> Result<DeploymentHandle, DeploymentError> {
        Err(DeploymentError::Unsupported(
            "ExternalDeployment pending P5 implementation",
        ))
    }

    async fn observe(
        &self,
        _host: &dyn Host,
        _bot_id: &BotId,
    ) -> Result<DeploymentState, DeploymentError> {
        Err(DeploymentError::Unsupported(
            "ExternalDeployment pending P5 implementation",
        ))
    }

    async fn stop(
        &self,
        _host: &dyn Host,
        _bot_id: &BotId,
        _mode: StopMode,
    ) -> Result<(), DeploymentError> {
        // P5 实装时这里应该是 no-op：external 服务由用户自己管启停。
        // 占位阶段维持 Unsupported 让调用方知道功能尚未上线。
        Err(DeploymentError::Unsupported(
            "ExternalDeployment pending P5 implementation",
        ))
    }

    async fn uninstall(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "ExternalDeployment pending P5 implementation",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_deployment_id_is_stable() {
        assert_eq!(ExternalDeployment::new().id(), "external");
    }

    #[test]
    fn external_deployment_has_no_preset_flavors() {
        // External 不预设支持的 flavor，保留给 P5 通过 endpoint 协议探测决定。
        let d = ExternalDeployment::new();
        assert!(d.supported_flavors().is_empty());
    }
}
