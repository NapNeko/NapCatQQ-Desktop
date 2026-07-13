//! 远端会话：Docker 隧道、日志 follow、runtime 会话表。

pub mod bot_log_follow;
pub mod docker_session;
pub(crate) mod runtime_sessions;

pub use bot_log_follow::RemoteBotLogFollowRegistry;
pub use docker_session::{
    DockerBotSessionRegistry, SnowLumaDockerEndpoints, is_remote_docker_config,
    is_remote_native_napcat_config,
};
