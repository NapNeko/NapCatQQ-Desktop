//! `ncd-update`:Desktop 自更新业务包装层。
//!
//! 蓝图 §7 / M5:在 `tauri-plugin-updater` 之上加业务能力 ——
//! schema 兼容预检、graceful shutdown、resume snapshot、失败 telemetry、多通道。
//!
//! ## 设计原则
//!
//! - 本 crate 是**纯 Rust 业务**,**不**直接依赖 `tauri-plugin-updater`(避免 ncd-update
//!   被强制拖到 Tauri runtime 上)
//! - `tauri-plugin-updater` 集成由 `src-tauri` 在 Layer 4 完成,通过 [`UpdateProvider`]
//!   trait 注入到 [`UpdateOrchestrator`]
//! - 这样 `ncd-update` 自己也能用 mock provider 单测,不依赖 Tauri
//!
//! ## 当前(M5.3)
//!
//! - ✅ `UpdateChannel` / `AvailableUpdate` / `PrecheckReport` / `UpdateResumePoint` 数据类型
//! - ✅ `UpdateProvider` trait(由 src-tauri 实装包 tauri-plugin-updater)
//! - ⏳ `UpdateOrchestrator`(M5.4)

pub mod channel;
pub mod error;
pub mod orchestrator;
pub mod provider;
pub mod resume;
pub mod types;

pub use channel::UpdateChannel;
pub use error::UpdateError;
pub use orchestrator::UpdateOrchestrator;
pub use provider::{UpdateProvider, MockUpdateProvider};
pub use resume::{UpdateResumePoint, ResumeStore};
pub use types::{AvailableUpdate, PrecheckReport, RecordedFailure};
