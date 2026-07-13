//! 适配层:把 ncd-deploy 的 NativeDeployment / DockerDeployment 接入 BotManager
//!
//! - launch: RuntimeLaunchPlannerAdapter
//! - native / remote / docker: 三类 BotBackend 过渡壳
//! - config / docker_helpers / log_helpers: 共享辅助

mod config;
mod docker;
mod docker_helpers;
mod launch;
mod log_helpers;
mod native;
mod remote;

pub use docker::DockerDeploymentBackend;
pub use launch::RuntimeLaunchPlannerAdapter;
pub use native::NativeDeploymentBackend;
pub use remote::RemoteNativeDeploymentBackend;

// re-export EventBusSink from ncd-deploy for backward compatibility
pub use ncd_deploy::EventBusSink;

// 测试 `use super::*` 对齐旧单体可见性
#[cfg(test)]
pub(crate) use config::{
    data_root_from_config_path, real_bot_config_from_ctx, status_for_deployment_state,
};
#[cfg(test)]
pub(crate) use docker_helpers::{docker_project_dir, render_docker_config_on_host};
#[cfg(test)]
pub(crate) use ncd_deploy::DeploymentState;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
