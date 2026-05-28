//! 原生部署：在宿主机上装 NapCat / SnowLuma 二进制，spawn 进程跑 bot。
//!
//! P1.a 占位骨架。`supports` / `id` / `supported_flavors` 已经返回正确值，
//! 但 `install` / `launch` / `observe` / `stop` / `uninstall` 都返回
//! `Unsupported`。P1.b 阶段把 `LocalRuntimeBackend` 的逻辑迁过来填实。

use async_trait::async_trait;
use ncd_domain::{BotConfig, BotFlavor, BotId, StopMode};
use ncd_host::Host;

use crate::deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
};

/// 原生部署：装二进制 + spawn 进程。
///
/// 支持的 flavor：NapCat、SnowLuma。
/// 支持的 host：所有，因为本机 / SSH / 未来 Docker host 都能跑原生进程。
pub struct NativeDeployment {
    /// 部署形态稳定 id。
    id: &'static str,
    /// 支持的 flavor 列表。
    flavors: &'static [BotFlavor],
}

impl NativeDeployment {
    /// 构造支持 NapCat + SnowLuma 双 flavor 的原生部署。
    pub fn new() -> Self {
        Self {
            id: "native",
            flavors: &[BotFlavor::NapCat, BotFlavor::SnowLuma],
        }
    }
}

impl Default for NativeDeployment {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Deployment for NativeDeployment {
    fn id(&self) -> &str {
        self.id
    }

    fn supported_flavors(&self) -> &[BotFlavor] {
        self.flavors
    }

    fn supports(&self, _host: &dyn Host) -> bool {
        // 原生部署对 host 没有限制，任何 host 都能跑。
        true
    }

    async fn install(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
        _progress: &dyn DeploymentProgressSink,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "NativeDeployment::install pending P1.b implementation",
        ))
    }

    async fn launch(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
    ) -> Result<DeploymentHandle, DeploymentError> {
        Err(DeploymentError::Unsupported(
            "NativeDeployment::launch pending P1.b implementation",
        ))
    }

    async fn observe(
        &self,
        _host: &dyn Host,
        _bot_id: &BotId,
    ) -> Result<DeploymentState, DeploymentError> {
        Err(DeploymentError::Unsupported(
            "NativeDeployment::observe pending P1.b implementation",
        ))
    }

    async fn stop(
        &self,
        _host: &dyn Host,
        _bot_id: &BotId,
        _mode: StopMode,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "NativeDeployment::stop pending P1.b implementation",
        ))
    }

    async fn uninstall(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "NativeDeployment::uninstall pending P1.b implementation",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_deployment_id_is_stable() {
        let d = NativeDeployment::new();
        assert_eq!(d.id(), "native");
    }

    #[test]
    fn native_deployment_supports_napcat_and_snowluma() {
        let d = NativeDeployment::new();
        let flavors = d.supported_flavors();
        assert!(flavors.contains(&BotFlavor::NapCat));
        assert!(flavors.contains(&BotFlavor::SnowLuma));
    }
}
