//! Deployment trait 的具体实装。
//!
//! 当前三种形态：
//! - [`native`]：原生部署，宿主机上装 NapCat / SL 二进制并 spawn 进程。
//! - [`docker`]：Docker 部署，写 compose.yml + docker compose up。尚未实装。
//! - [`external`]：外接部署，对接用户已有的 OneBot 服务。尚未实装。

pub mod docker;
pub mod external;
pub mod native;

pub use docker::{DockerDeployment, bot_docker_container_name};
pub use external::ExternalDeployment;
pub use native::{NativeDeployment, NativeLogSnapshot, NativeRuntimeEventSink, NullRuntimeEventSink};
