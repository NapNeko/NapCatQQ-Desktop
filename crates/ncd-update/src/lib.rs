//! Desktop 自更新业务层
//!
//! 在 tauri-plugin-updater 之上加业务能力: schema 兼容预检, graceful
//! shutdown, resume snapshot, 失败 telemetry, 多通道
//!
//! 纯 Rust 业务, 不直接依赖 tauri-plugin-updater
//! (避免被强制拖到 Tauri runtime 上), tauri-plugin-updater 集成由
//! src-tauri 在 Layer 4 完成, 通过 [UpdateProvider] trait 注入,
//! ncd-update 自己也能用 mock provider 测试

pub mod channel;
pub mod error;
pub mod orchestrator;
pub mod provider;
pub mod resume;
pub mod types;

pub use channel::UpdateChannel;
pub use error::UpdateError;
pub use orchestrator::UpdateOrchestrator;
pub use provider::{MockUpdateProvider, UpdateProvider};
pub use resume::{ResumeStore, UpdateResumePoint};
pub use types::{
    AvailableUpdate, DesktopUpdateNoticeKind, DesktopUpdateStartupNotice, PrecheckReport,
    RecordedFailure, UpdatePhase,
};
