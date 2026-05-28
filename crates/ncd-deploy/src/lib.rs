//! `ncd-deploy`:NapCatQQ-Desktop 的部署编排层。
//!
//! 双轨架构（2026-05-29 远端架构重构 P1.a 起）：
//!
//! 1. **旧轨 DeployPlan**：`ncd-component` Component × `ncd-host` Host 三维抽象
//!    的部署编排，把 Component 列表 + Host 编排成顺序执行计划，用于"装组件"
//!    类操作（NapCat zip / SnowLuma node 等）。
//!
//! 2. **新轨 Deployment trait**（[`deployment::Deployment`]）：bot 部署形态的
//!    统一抽象，把"在哪跑（Host）"和"怎么跑（Native / Docker / External）"
//!    解耦。Component 沦为 NativeDeployment 的实现细节。这一轨用于把
//!    `LocalRuntimeBackend` / `RemoteRuntimeBackend` 收敛成统一接口。
//!
//! 详见 .kiro/REMOTE_REFACTOR_PLAN.md。

pub mod deployment;
pub mod deployments;
pub mod error;
pub mod plan;
pub mod result;
pub mod runner;

pub use deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
    NullProgressSink,
};
pub use deployments::{DockerDeployment, ExternalDeployment, NativeDeployment};
pub use error::DeployError;
pub use plan::{DeployBuilder, DeployPlan, DeployStep, StepKind};
pub use result::{DeployOutcome, StepOutcome, StepStatus};
