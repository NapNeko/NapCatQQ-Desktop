//! Layer 1 数据契约: 跨边界类型定义
//!
//! 依赖白名单: serde / serde_json / thiserror / ts-rs.
//! 禁止引入运行时依赖(tokio / reqwest 等).

#[macro_use]
mod macros;

pub mod app_config;
pub mod bootstrap;
pub mod bot_config;
pub mod docker;
pub mod errors;
pub mod ids;
pub mod kinds;
pub mod migration;
pub mod qq_dependency;
pub mod release_snapshot;
pub mod snowluma_start_mode;

// 向后兼容: 下游 crate 仍可 use ncd_domain::{models::*, report::*}
pub use migration as models;
pub use migration as report;

// 向后兼容: 已合并到 bootstrap / app_config, 模块路径保留
pub mod version_snapshot {
    pub use crate::bootstrap::LocalVersionSnapshot;
}
pub mod system_resource {
    pub use crate::app_config::SystemResourceSnapshot;
}

// 顶层 re-export(对齐旧 ncd-core API, 方便下游 crate 引用)

pub use app_config::{
    AfterCloseUiBehavior, AppSettings, AppSettingsDto, CloseAction, DesktopNotifySettings,
    SnowLumaAppConfig, SystemResourceSnapshot, UiModeOnStartup, WebUiPollerSettings,
    clamp_lightweight_delay_secs, default_login_interval, default_perf_monitor_interval,
    default_snowluma_port,
};
pub use bootstrap::{BootstrapSnapshot, BootstrapStatus, LocalVersionSnapshot, RepairAction};
pub use bot_config::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, BotConfigError,
    BypassConfig, ConnectConfig, DeploymentType, HttpClientConfig, HttpServerConfig,
    HttpSseServerConfig, LogLevel, MessagePostFormat, NetworkBaseFields, O3HookMode,
    StatusCommandConfig, TimeUnit, WebsocketClientConfig, WebsocketServerConfig, WsRole,
};
pub use docker::{
    ContainerAction, ContainerInfo, ContainerState, DeployedContainer, DockerDeploySpec,
    DockerFlavor, DockerImageReady, DockerInstallReport, DockerInstallStatus,
    DockerPullLayerSnapshot, DockerPullSpec, DockerSpecError, DockerStatus, ImageInfo,
    ImageRemoveOptions, PortMapping,
};
pub use errors::{AppError, ConfigError, MigrationError, PathError, SecretError};
pub use ids::{BackendId, BotId};
pub use kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion, StopMode};
pub use migration::{
    BackupInfo, BotRuntimeSummary, MigrationOutcome, MigrationReport, MigrationSource,
    MigrationStage, MigrationWarning,
};
pub use qq_dependency::{
    DependencyCategory, DependencyInstallError, DetectionMethod, DistroFamily, DistroInfo,
    FailedPackage, InstallDependenciesResult, PackageStatus, QqDependencyReport, SystemDependency,
};
pub use release_snapshot::{ReleaseInfo, ReleaseSnapshot};
pub use snowluma_start_mode::SnowLumaStartMode;
