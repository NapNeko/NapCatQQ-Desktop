//! `ncd-component`:NapCatQQ-Desktop 的"组件"抽象。
//!
//! 蓝图 §5 / M4:把"装什么"(NapCat / SnowLuma / LinuxQQ / Node / noVNC / Desktop 自身)
//! 抽成统一的 [`Component`] trait,与 [`ncd_host::Host`] trait 正交。
//!
//! ## 三维抽象的中间层
//!
//! ```text
//! Component(WHAT)  ×  Host(WHERE)  ×  Action(VERB)
//!     ↓                  ↓                ↓
//! 这个 crate         ncd-host         本 crate 的 Action trait
//! ```
//!
//! ## 核心设计原则
//!
//! - **Component 不知道在哪台机器**:只描述"我是什么、URL、SHA256、装在哪个目录、启动命令长啥样"
//! - **Host 不知道在装什么**:只提供 `read_file / spawn / extract_archive` 等能力
//! - **`ncd-deploy`(M5)负责编排**:把 Component 列表 + Host = `DeployPlan`
//!
//! ## 当前(M4.1)
//!
//! - ✅ `Component` trait + `ActionCtx` + `ProgressEvent`
//! - ✅ `DownloadHelper`(共享 HTTP 下载 + SHA256 校验逻辑)
//! - ✅ `NodeJsComponent`(M4 第一个 component,跑通 detect + install + verify + launch_command)
//! - ⏳ `NapCatComponent` / `SnowLumaComponent` / `LinuxQQComponent` / `NoVncComponent` /
//!    `DesktopSelfComponent`(M4 后续 + M5)

pub mod context;
pub mod download;
pub mod error;
pub mod nodejs;
pub mod traits;
pub mod types;

pub use context::{ActionCtx, ProgressEvent, ProgressKind};
pub use download::DownloadHelper;
pub use error::ActionError;
pub use nodejs::NodeJsComponent;
pub use traits::{Action, Component};
pub use types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};
