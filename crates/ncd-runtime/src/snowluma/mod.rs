//! SnowLuma backend runtime 子模块根。
//!
//! 模块声明 + 已落地类型 / trait / 函数的 re-export 清单。

pub mod daemon;
pub mod error;
pub mod log_sanitize;
pub mod proc_tree;
pub mod qq_login_probe;
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
pub use daemon::{DaemonState, SnowLumaDaemon, SnowLumaWebUiClientFactory};

// ---- status_poller.rs ----
pub use status_poller::{ProcessTreeProbe, SnowLumaLoginState};

// ---- launch_plan.rs(已迁移到 ncd-domain::snowluma_start_mode) ----
pub use ncd_domain::snowluma_start_mode::SnowLumaStartMode;

// ---- session.rs ----
pub use session::{SnowLumaSession, load_or_create_session, render_daemon_globals};

// ---- log_sanitize.rs ----
pub use log_sanitize::sanitize_log_line;

// ---- proc_tree.rs ----
//
// `MockProcessTreeProbe` 为单元测试 helper，但被声明为 `pub` 以便 ncd-runtime
// 之外的下游 crate（譬如 ncd-tauri 集成测试）复用。
pub use proc_tree::{MockProcessTreeProbe, SysinfoProcessTreeProbe};

// ---- runtime_backend.rs ----
pub use runtime_backend::SnowLumaRuntimeBackend;

// ---- status_poller.rs（追加导出） ----
pub use status_poller::{PollerDeps, SnowLumaStatusPoller};
