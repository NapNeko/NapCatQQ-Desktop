//! 应用端框架契约：Integration（每框架一个）+ Runtime（骨架共用）。
//!
//! Deploy 安装/更新走既有 Component × Host × Action（R12），不在此重复定义。

use async_trait::async_trait;
use ncd_domain::{
    AppFrameworkId, AppInstance, AppInstanceId, AppInstanceState, OneBotEndpointExport, StopMode,
};

/// 把协议 Bot 的 OneBot 出口写入该应用端配置（差异最大的扩展点）。
///
/// 产品语义: 全自动对接；失败必须可解释，且不得静默写坏已有配置（实现侧保证回滚）。
#[async_trait]
pub trait AppIntegration: Send + Sync {
    fn framework_id(&self) -> &AppFrameworkId;

    /// 将 `export` 写入 `instance` 对应应用端配置。
    async fn link_onebot(
        &self,
        instance: &AppInstance,
        export: &OneBotEndpointExport,
    ) -> Result<AppInstance, AppFrameworkError>;

    /// 解除对接（可选清理应用端内协议配置）。
    async fn unlink_onebot(
        &self,
        instance: &AppInstance,
    ) -> Result<AppInstance, AppFrameworkError>;
}

/// 应用端运行时最小面：启停 / 状态 / 打开 WebUI。
/// 不负责 QQ 登录；不替代上游应用端业务 UI。
#[async_trait]
pub trait AppRuntime: Send + Sync {
    fn framework_id(&self) -> &AppFrameworkId;

    async fn start(&self, instance_id: &AppInstanceId) -> Result<AppInstance, AppFrameworkError>;

    async fn stop(
        &self,
        instance_id: &AppInstanceId,
        mode: StopMode,
    ) -> Result<AppInstance, AppFrameworkError>;

    async fn status(&self, instance_id: &AppInstanceId)
        -> Result<AppInstanceState, AppFrameworkError>;

    /// 返回可打开的 WebUI URL；无则 Ok(None)。
    async fn webui_url(
        &self,
        instance_id: &AppInstanceId,
    ) -> Result<Option<String>, AppFrameworkError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AppFrameworkError {
    #[error("app framework not registered: {0}")]
    NotRegistered(String),

    #[error("app instance not found: {0}")]
    InstanceNotFound(String),

    #[error("placement not supported for this phase: {0}")]
    PlacementUnsupported(String),

    #[error("onebot export unavailable: {0}")]
    OneBotExport(String),

    #[error("integration failed: {0}")]
    Integration(String),

    #[error("runtime failed: {0}")]
    Runtime(String),

    /// 框架尚未产品对接；桩实现返回此错误。
    #[error("app framework stub: {0}")]
    Stub(String),
}
