//! Docker 管理面:在 &dyn Host 之上封装 docker CLI 调用
//!
//! 这是一条独立于 bot 部署(Deployment trait)的链路bot 那套围绕 BotConfig /
//! BotActor 转,而 Docker 管理面要做的是"探测 docker,列已有容器,起停删,看
//! 日志,一键部署 NapCat/SnowLuma",对象是容器本身,不进 bot 列表
//!
//! 三个子模块:
//! - cli:DockerCli 原语,所有 docker 命令收敛在这里
//! - install:缺 docker 时帮装(仅 Linux,走阿里云 docker-ce 源)
//! - compose:把 DockerDeploySpec 渲染成 docker-compose.yml

pub mod cli;
pub mod compose;
pub mod install;
pub mod install_progress;
pub mod pkg_install_emit;
pub mod pull_failure;

pub use cli::{DockerCli, DockerCliError, LayerPhase, PullProgress};
pub use compose::{
    DOCKER_METRICS_CONTAINER_ROOT, DOCKER_NAPCAT_LOAD_CONTAINER_PATH, DOCKER_NAPCAT_MJS_URI,
    DockerMetricsOverlay, render_compose, render_compose_with_env, render_compose_with_metrics,
    render_snowluma_compose_with_env, render_snowluma_compose_with_metrics,
};
pub use install::{DockerInstallOutcome, install_docker};
pub use install_progress::{
    INSTALL_TOTAL_STEPS, InstallProgressEmit, install_docker_with_progress, progress_event,
};
pub use pull_failure::{PullFailureKind, classify_pull_failure};
