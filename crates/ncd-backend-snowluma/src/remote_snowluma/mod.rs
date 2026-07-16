//! 远端 SnowLuma「直接运行」:内联 shell 编排 + SSH 隧道 WebUI 注入(无上传 launcher 文件)
//!
//! 子模块:
//! - layout / stack / orchestrator / tunnel / log: 路径、图形栈、启停、隧道、日志
//! - backend / daemon / inject / config / helpers: BotBackend 与注入编排

mod backend;
mod config;
mod daemon;
mod helpers;
mod inject;

pub mod layout;
pub mod log;
pub mod orchestrator;
pub mod stack;
pub mod tunnel;

pub use backend::{RemoteSlMetricsInjector, RemoteSnowLumaBackend};
pub use config::{is_remote_native_snowluma_config, render_native_snowluma_config_on_host};
pub use daemon::RemoteSnowLumaDaemon;
pub use inject::remote_qq_running_pid;
