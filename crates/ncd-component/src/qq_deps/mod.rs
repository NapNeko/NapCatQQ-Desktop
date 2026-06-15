//! QQ 系统依赖管理模块。
//!
//! 提供 Linux QQ 运行时所需系统依赖的检测、安装和管理功能。

pub mod detector;
pub mod installer;
pub mod manifest;

pub use detector::QqDependencyDetector;
pub use installer::QqDependencyInstaller;
pub use manifest::{QQDependencyManifest, qq_qqnt_dependencies_v3_2_25};
