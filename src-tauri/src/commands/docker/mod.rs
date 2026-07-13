//! Docker 页 Tauri 命令薄壳层
//!
//! 业务编排尽量薄;真正的 docker 操作在 ncd_deploy::docker::DockerCli
//!
//! Tauri `#[tauri::command]` 不能 `pub use` 透传,lib.rs 须注册子模块路径:
//! `commands::docker::ops::*` / `install::*` / `deploy::*`

pub mod deploy;
pub mod install;
pub mod ops;
mod progress;
