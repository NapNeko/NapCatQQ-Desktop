//! `ncd-deploy`:NapCatQQ-Desktop 的部署编排层。
//!
//! 双轨架构：
//!
//! 1. 旧轨 DeployPlan：ncd-component Component x ncd-host Host 三维抽象
//!    的部署编排，把 Component 列表 + Host 编排成顺序执行计划，用于"装组件"
//!    类操作（NapCat zip / SnowLuma node 等）。
//!
//! 2. 新轨 Deployment trait（[`deployment::Deployment`]）：bot 部署形态的
//!    统一抽象，把"在哪跑（Host）"和"怎么跑（Native / Docker / External）"
//!    解耦。Component 沦为 NativeDeployment 的实现细节。

pub mod deployment;
pub mod deployments;
pub mod docker;
pub mod error;
pub mod plan;
pub mod result;
pub mod runner;

pub use deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
    NativeLaunchCommand, NativeLaunchTranslator, NullProgressSink,
};
pub use deployments::{
    DockerDeployment, ExternalDeployment, NativeDeployment, NativeLogSnapshot,
    NativeRuntimeEventSink, NullRuntimeEventSink, bot_docker_container_name,
    parse_napcat_webui_line, strip_ansi_escapes,
};
pub use docker::{
    install_docker, render_compose, DockerCli, DockerCliError, DockerInstallOutcome,
};
pub use error::DeployError;
pub use plan::{DeployBuilder, DeployPlan, DeployStep, StepKind};
pub use result::{DeployOutcome, StepOutcome, StepStatus};
