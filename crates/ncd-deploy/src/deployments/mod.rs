//! Deployment trait 的具体实装。
//!
//! 当前三种形态：
//! - [`native`]：原生部署，宿主机上装 NapCat / SL 二进制并 spawn 进程。
//!   P1.b 阶段把 `LocalRuntimeBackend` / `RemoteRuntimeBackend` 的逻辑迁过来。
//! - [`docker`]：Docker 部署，写 compose.yml + docker compose up。P4 才做。
//! - [`external`]：外接部署，对接用户已有的 OneBot 服务。P5 才做。
//!
//! P1.a 当前阶段：三个实装都是占位，方法返回 `Unsupported`。让 trait 体系
//! 先就位、ts-rs 类型先派生，下一步再填真逻辑。

pub mod docker;
pub mod external;
pub mod native;

pub use docker::DockerDeployment;
pub use external::ExternalDeployment;
pub use native::NativeDeployment;
