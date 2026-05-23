//! SnowLuma backend runtime 子模块根。
//! 模块声明+ 当下已落地类型 / trait / 函数的 re-export 清单
//! 。
//! ## Re-export 范围
//! 仅 re-export 当下（task 2 / 3 / 4 系列）已落地的类型；下列条目尚在 TODO
//! 落地后由后续 task 各自的 mod.rs 维护任务回填：
//! - `SnowLumaDaemon` 主体 —— 。
//! - `SnowLumaStatusPoller` / `PollerDeps` —— 。
//! - `SnowLumaRuntimeBackend` —— / 7.2（`runtime_backend.rs` 当前仍是占位）。

pub mod daemon;
pub mod error;
pub mod launch_plan;
pub mod log_sanitize;
pub mod proc_tree;
pub mod runtime_backend;
pub mod session;
pub mod status_poller;
pub mod webui_client;

// ---- error.rs ----
pub use error::{SnowLumaDaemonError, SnowLumaWebUiError};

// ---- webui_client.rs ----
//
// 跨边界类型（ts-rs 已导出）：HookProcessInfo / HookProcessStatus /
// OneBotInstanceInfo / AuthState。
// trait + 默认实装：SnowLumaWebUiClient / ReqwestSnowLumaWebUiClient。
pub use webui_client::{
    AuthState, HookProcessInfo, HookProcessStatus, OneBotInstanceInfo, ReqwestSnowLumaWebUiClient,
    ReqwestSnowLumaWebUiClientFactory, SnowLumaWebUiClient,
};

// ---- daemon.rs ----
//
// `SnowLumaDaemon` 主体由 在 daemon.rs 内追加；当前仅 re-export
// 已落地的 5 档状态机 + factory trait 占位。
pub use daemon::{DaemonState, SnowLumaDaemon, SnowLumaWebUiClientFactory};

// ---- status_poller.rs ----
//
// `SnowLumaStatusPoller` / `PollerDeps` 由 在 status_poller.rs 内追加
// 当前仅 re-export 4 档登录态 + 进程树枚举 trait 占位。
pub use status_poller::{ProcessTreeProbe, SnowLumaLoginState};

// ---- launch_plan.rs ----
pub use launch_plan::SnowLumaStartMode;

// ---- session.rs ----
pub use session::{SnowLumaSession, load_or_create_session, render_daemon_globals};

// ---- log_sanitize.rs ----
pub use log_sanitize::sanitize_log_line;

// ---- proc_tree.rs ----
//
// `MockProcessTreeProbe` 为单元测试 helper，但被声明为 `pub` 以便 ncd-core
// 之外的下游 crate（譬如 ncd-tauri 集成测试）复用，按 spec 一并 re-export。
pub use proc_tree::{MockProcessTreeProbe, SysinfoProcessTreeProbe};

// ---- runtime_backend.rs ----
pub use runtime_backend::SnowLumaRuntimeBackend;

// ---- status_poller.rs (full, + 之后扩展) ----
pub use status_poller::{PollerDeps, SnowLumaStatusPoller};
