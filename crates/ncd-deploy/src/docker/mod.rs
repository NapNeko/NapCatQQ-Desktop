//! Docker 管理面:在 &dyn Host 之上封装 docker CLI 调用。
//!
//! 这是一条独立于 bot 部署(Deployment trait)的链路。bot 那套围绕 BotConfig /
//! BotActor 转,而 Docker 管理面要做的是"探测 docker、列已有容器、起停删、看
//! 日志、一键部署 NapCat/SnowLuma",对象是容器本身,不进 bot 列表。
//!
//! 三个子模块:
//! - cli:DockerCli 原语,所有 docker 命令收敛在这里
//! - install:缺 docker 时帮装(Linux 跑官方脚本,Windows 只引导)
//! - compose:把 DockerDeploySpec 渲染成 docker-compose.yml

pub mod cli;
pub mod compose;
pub mod install;
pub mod install_progress;
pub mod pkg_install_emit;
pub mod pull_failure;

pub use cli::{DockerCli, DockerCliError, LayerPhase, PullProgress};
pub use pull_failure::{classify_pull_failure, PullFailureKind};
pub use compose::render_compose;
pub use install::{install_docker, DockerInstallOutcome};
pub use install_progress::{install_docker_with_progress, progress_event, InstallProgressEmit, INSTALL_TOTAL_STEPS};
