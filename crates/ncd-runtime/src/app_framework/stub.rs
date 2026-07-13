//! 未选定具体框架前的桩：契约可编译可测，调用返回 Stub 错误。

use async_trait::async_trait;
use ncd_domain::{
    AppFrameworkId, AppInstance, AppInstanceId, AppInstanceState, OneBotEndpointExport, StopMode,
};
use ncd_traits::{AppFrameworkError, AppIntegration, AppRuntime};

/// 占位 Integration：产品选定 NoneBot2 / AstrBot 等后再换真实现。
pub struct StubAppIntegration {
    framework_id: AppFrameworkId,
}

impl StubAppIntegration {
    pub fn new(framework_id: impl Into<AppFrameworkId>) -> Self {
        Self {
            framework_id: framework_id.into(),
        }
    }
}

#[async_trait]
impl AppIntegration for StubAppIntegration {
    fn framework_id(&self) -> &AppFrameworkId {
        &self.framework_id
    }

    async fn link_onebot(
        &self,
        _instance: &AppInstance,
        _export: &OneBotEndpointExport,
    ) -> Result<AppInstance, AppFrameworkError> {
        Err(AppFrameworkError::Stub(format!(
            "framework {} not productized yet; OneBot export is available, write-path deferred",
            self.framework_id
        )))
    }

    async fn unlink_onebot(
        &self,
        _instance: &AppInstance,
    ) -> Result<AppInstance, AppFrameworkError> {
        Err(AppFrameworkError::Stub(format!(
            "framework {} not productized yet",
            self.framework_id
        )))
    }
}

pub struct StubAppRuntime {
    framework_id: AppFrameworkId,
}

impl StubAppRuntime {
    pub fn new(framework_id: impl Into<AppFrameworkId>) -> Self {
        Self {
            framework_id: framework_id.into(),
        }
    }
}

#[async_trait]
impl AppRuntime for StubAppRuntime {
    fn framework_id(&self) -> &AppFrameworkId {
        &self.framework_id
    }

    async fn start(&self, _instance_id: &AppInstanceId) -> Result<AppInstance, AppFrameworkError> {
        Err(AppFrameworkError::Stub(format!(
            "framework {} runtime not productized yet",
            self.framework_id
        )))
    }

    async fn stop(
        &self,
        _instance_id: &AppInstanceId,
        _mode: StopMode,
    ) -> Result<AppInstance, AppFrameworkError> {
        Err(AppFrameworkError::Stub(format!(
            "framework {} runtime not productized yet",
            self.framework_id
        )))
    }

    async fn status(
        &self,
        _instance_id: &AppInstanceId,
    ) -> Result<AppInstanceState, AppFrameworkError> {
        Err(AppFrameworkError::Stub(format!(
            "framework {} runtime not productized yet",
            self.framework_id
        )))
    }

    async fn webui_url(
        &self,
        _instance_id: &AppInstanceId,
    ) -> Result<Option<String>, AppFrameworkError> {
        Ok(None)
    }
}
