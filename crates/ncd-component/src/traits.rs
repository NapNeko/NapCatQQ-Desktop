//! `Component` / `Action` trait 定义。
//!
//! 蓝图 §5.3:Component 描述"是什么 + 怎么装 / 启",但**不直接 spawn 进程或调 SSH**,
//! 只通过 [`ncd_host::Host`] 提供的能力实现操作。

use async_trait::async_trait;

use ncd_host::{Host, HostCommand, Locality, Os};

use crate::context::ActionCtx;
use crate::error::ActionError;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// `Component` trait:可装可启的"组件"统一接口。
#[async_trait]
pub trait Component: Send + Sync {
    /// 组件标识。
    fn id(&self) -> ComponentId;

    /// 该组件支持哪些 (Os, Locality) 组合。
    /// 例:NapCat 支持 (Windows, Local) + (Linux, Remote)
    ///     LinuxQQ 仅支持 (Linux, Remote)
    ///     DesktopSelf 仅支持 (*, Local)
    fn supported_targets(&self) -> &'static [(Os, Locality)];

    /// 探测目标 Host 上是否已装,装的是哪个版本。
    /// `Ok(None)` 表示未安装,`Ok(Some(_))` 表示已装,`Err(_)` 是探测出错。
    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError>;

    /// 装(可能涉及下载 + 校验 + 解压 + 启动初始化)。
    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError>;

    /// 更新到新版(可能复用 install 路径,但要求先 graceful stop)。
    /// M4 默认实现:直接走 install(各 component 按需 override)。
    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.install(host, ctx).await
    }

    /// 卸载。M4 默认实现:返回 `Other("uninstall not implemented")`,各 component 按需 override。
    async fn uninstall(
        &self,
        _host: &dyn Host,
        _ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        Err(ActionError::other(format!(
            "uninstall not implemented for {:?}",
            self.id()
        )))
    }

    /// 校验完整性(SHA256 / 数字签名 / 关键文件存在性)。
    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError>;

    /// 拼接启动命令(由 backend 调用,不实际 spawn)。
    fn launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError>;

    /// 检查 (host.os(), host.locality()) 是否在 supported_targets 中。
    /// 默认实现,各 component 通常不需要 override。
    fn check_target(&self, host: &dyn Host) -> Result<(), ActionError> {
        let target = (host.os(), host.locality());
        if self.supported_targets().contains(&target) {
            Ok(())
        } else {
            Err(ActionError::UnsupportedTarget {
                component: self.id().as_str().to_string(),
                os: host.os(),
                locality: host.locality(),
            })
        }
    }
}

/// `Action` trait:对单个 Component 的具体操作。
///
/// 蓝图 §5.2 Action 维度抽象。M4.1 阶段 Component trait 上的 detect/install/...
/// 已经直接对应 5 种 Action;本 trait 主要在 `ncd-deploy`(M5)中拼接 Plan 时用。
#[async_trait]
pub trait Action: Send + Sync {
    /// Action 名称(用于日志 / 进度上报)。
    fn name(&self) -> &'static str;

    /// 执行 Action。
    async fn execute(
        &self,
        component: &dyn Component,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError>;
}
