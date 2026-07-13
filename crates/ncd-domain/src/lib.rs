//! Layer 1 数据契约: 跨边界类型定义
//!
//! 依赖白名单: serde / serde_json / thiserror / ts-rs.
//! 禁止引入运行时依赖(tokio / reqwest 等).

#[macro_use]
mod macros;

pub mod app_config;
pub mod app_framework;
pub mod bootstrap;
pub mod bot_actor;
pub mod bot_config;
pub mod bot_runtime_metrics;
pub mod bot_status;
pub mod daemon_state;
pub mod deployment_task;
pub mod docker;
pub mod domain_event;
pub mod errors;
pub mod ids;
pub mod kinds;
pub mod migration;
pub mod napcat_events;
pub mod offline_alert;
pub mod progress;
pub mod qq_dependency;
pub mod release_snapshot;
pub mod runtime_scenario;
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
pub use app_framework::{
    AppFrameworkId, AppInstance, AppInstanceId, AppInstanceState, AppPlacement,
    OneBotEndpointExport,
};
pub use bootstrap::{
    BootstrapSnapshot, BootstrapStatus, DataLayoutConsolidateSnapshot, LocalVersionSnapshot,
    RepairAction,
};
pub use bot_actor::{BotActorError, BotActorSnapshot, BotActorState};
pub use bot_config::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, BotConfigError,
    BypassConfig, ConnectConfig, DeploymentType, HttpClientConfig, HttpServerConfig,
    HttpSseServerConfig, LogLevel, MessagePostFormat, NetworkBaseFields, O3HookMode,
    StatusCommandConfig, TimeUnit, WebsocketClientConfig, WebsocketServerConfig, WsRole,
    is_remote_docker_config, is_remote_native_napcat_config,
};
pub use bot_runtime_metrics::{
    BOT_RUNTIME_METRICS_INTERVAL_MAX_MS, BOT_RUNTIME_METRICS_INTERVAL_MIN_MS,
    BOT_RUNTIME_METRICS_RETENTION_MAX_DAYS, BOT_RUNTIME_METRICS_RETENTION_MIN_DAYS,
    BotRuntimeMetrics, MemoryMetrics, MetricsHistoryPoint, MetricsNodeMapEntry, MetricsSource,
    NetworkNodeKind, NetworkNodeMetrics, NodesRollup, ProbeHealth, ProbeStatsFile,
    clamp_bot_runtime_metrics_interval_ms, clamp_bot_runtime_metrics_retention_days,
    default_bot_runtime_metrics_interval_ms, default_bot_runtime_metrics_retention_days,
    history_min_interval_ms,
};
pub use bot_status::{BotStatus, ProcessHandle};
pub use daemon_state::{DaemonState, SnowLumaLoginState};
pub use deployment_task::{
    DeploymentTaskKind, DeploymentTaskList, DeploymentTaskResource, DeploymentTaskSnapshot,
    DeploymentTaskStatus,
};
pub use docker::{
    ContainerAction, ContainerInfo, ContainerState, DeployedContainer, DockerDeploySpec,
    DockerFlavor, DockerImageReady, DockerInstallReport, DockerInstallStatus,
    DockerPullLayerSnapshot, DockerPullSpec, DockerSpecError, DockerStatus, ImageInfo,
    ImageRemoveOptions, PortMapping,
};
pub use domain_event::{DOMAIN_EVENT_ENVELOPE_VERSION, DomainEvent, DomainEventKind};
pub use errors::{AppError, ConfigError, MigrationError, PathError, SecretError};
pub use ids::{BackendId, BotId};
pub use kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion, StopMode};
pub use migration::{
    BackupInfo, BotRuntimeSummary, MigrationOutcome, MigrationReport, MigrationSource,
    MigrationStage, MigrationWarning,
};
pub use napcat_events::NapCatLoginInvalidationReason;
pub use offline_alert::{
    EnsureOneBotMessengerHttpResult, OfflineAlert, OfflineAlertKind, OfflineAlertSource,
    OfflineDeliveryChannelResult, OfflineDeliveryRecord, OfflineEmailSettings,
    OfflineNotifyBehavior, OfflineOneBotSettings, OfflineWebhookChannel, OfflineWebhookSettings,
    OneBotMessengerCandidate, OneBotMessengerScope, default_webhook_body_template, render_template,
};
pub use progress::{ProgressEvent, ProgressKind, ProgressLogLevel};
pub use qq_dependency::{
    DependencyCategory, DependencyInstallError, DetectionMethod, DistroFamily, DistroInfo,
    FailedPackage, InstallDependenciesResult, PackageStatus, QqDependencyReport, SystemDependency,
};
pub use release_snapshot::{ReleaseInfo, ReleaseSnapshot};
pub use runtime_scenario::RuntimeScenario;
pub use snowluma_start_mode::SnowLumaStartMode;
