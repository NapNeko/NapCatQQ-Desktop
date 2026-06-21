//! ncd-domain:NapCatQQ-Desktop 的 Layer 1 数据契约
//!
//! 依赖白名单:serde / serde_json / thiserror / ts-rs,禁止引入 tokio /
//! reqwest 等运行时库所有跨边界数据结构(BotConfig / AppConfig / errors /
//! ids / kinds 等)都在本 crate,前后端通过 ts-rs 派生保证类型一致

pub mod app_config;
pub mod bootstrap;
pub mod bot_config;
pub mod docker;
pub mod errors;
pub mod ids;
pub mod kinds;
pub mod models;
pub mod qq_dependency;
pub mod release_snapshot;
pub mod report;
pub mod snowluma_start_mode;
pub mod system_resource;
pub mod version_snapshot;

// ===== 顶层 re-export(对齐旧 ncd-core API,方便下游 crate 引用) =====

pub use app_config::{
    AppSettings, AppSettingsDto, DesktopNotifySettings, SnowLumaAppConfig, WebUiPollerSettings,
    clamp_lightweight_delay_secs, default_login_interval, default_perf_monitor_interval,
    default_snowluma_port,
};
pub use bootstrap::{BootstrapSnapshot, BootstrapStatus, RepairAction};
pub use bot_config::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, BotConfigError,
    BypassConfig, ConnectConfig, DeploymentType, HttpClientConfig, HttpServerConfig,
    HttpSseServerConfig, LogLevel, MessagePostFormat, NetworkBaseFields, O3HookMode,
    StatusCommandConfig, TimeUnit,
    WebsocketClientConfig, WebsocketServerConfig, WsRole,
};
pub use docker::{
    ContainerAction, ContainerInfo, ContainerState, DeployedContainer, DockerDeploySpec,
    DockerFlavor, DockerImageReady, DockerInstallReport, DockerInstallStatus,
    DockerPullLayerSnapshot, DockerPullSpec, ImageInfo, ImageRemoveOptions,
    DockerSpecError, DockerStatus, PortMapping,
};
pub use errors::{AppError, ConfigError, MigrationError, PathError, SecretError};
pub use ids::{BackendId, BotId};
pub use kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion, StopMode};
pub use models::{
    BackupInfo, BotRuntimeSummary, MigrationOutcome, MigrationSource, MigrationStage,
    MigrationWarning,
};
pub use qq_dependency::{
    DependencyCategory, DependencyInstallError, DistroFamily, DistroInfo, FailedPackage,
    InstallDependenciesResult, PackageStatus, QqDependencyReport, SystemDependency,
    DetectionMethod,
};
pub use release_snapshot::{ReleaseInfo, ReleaseSnapshot};
pub use report::MigrationReport;
pub use snowluma_start_mode::SnowLumaStartMode;
pub use system_resource::SystemResourceSnapshot;
pub use version_snapshot::LocalVersionSnapshot;
