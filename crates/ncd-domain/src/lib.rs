//! `ncd-domain`:NapCatQQ-Desktop 的 Layer 1 数据契约。
//!
//! 蓝图 §3.2 / §10.4 R10 铁律:
//! - 本 crate 只允许 serde / serde_json / thiserror / ts-rs 这 4 个依赖
//! - 禁止引入 tokio / reqwest / 任何运行时库
//! - 所有跨边界数据结构(BotConfig / AppConfig / errors / ids / kinds 等)
//!   都在本 crate,前后端通过 ts-rs 派生保证类型一致
//!
//! 当前(M2.1)只搬了 6 个零依赖文件,后续步骤:
//! - M2.2:搬 `app_config` / `bot_config` / `SnowLumaStartMode`
//! - 未来:把 events.rs 的"数据"部分(DomainEvent / DomainEventKind / EventFilter)
//!   也迁过来,实装(BroadcastEventBus)留在 ncd-runtime

pub mod app_config;
pub mod bootstrap;
pub mod bot_config;
pub mod errors;
pub mod ids;
pub mod kinds;
pub mod models;
pub mod report;
pub mod snowluma_start_mode;

// ===== 顶层 re-export(对齐旧 ncd-core API,方便下游 crate 引用) =====

pub use app_config::{
    SnowLumaAppConfig, WebUiPollerSettings, default_login_interval, default_snowluma_port,
};
pub use bootstrap::{BootstrapSnapshot, BootstrapStatus, RepairAction};
pub use bot_config::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, BotConfigError,
    BypassConfig, ConnectConfig, HttpClientConfig, HttpServerConfig, HttpSseServerConfig, LogLevel,
    MessagePostFormat, NetworkBaseFields, O3HookMode, TimeUnit, WebsocketClientConfig,
    WebsocketServerConfig, WsRole,
};
pub use errors::{AppError, ConfigError, MigrationError, PathError, SecretError};
pub use ids::{BackendId, BotId};
pub use kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion};
pub use models::{
    BackupInfo, BotRuntimeSummary, MigrationOutcome, MigrationSource, MigrationStage,
    MigrationWarning,
};
pub use report::MigrationReport;
pub use snowluma_start_mode::SnowLumaStartMode;
