pub mod app_config;
pub mod app_config_migration;
pub mod backend_config_renderer;
pub mod bootstrap;
pub mod bot_actor;
pub mod bot_config;
pub mod bot_config_migration;
pub mod bot_config_repo_impl;
pub mod bot_manager;
pub mod config_store_impl;
pub mod errors;
pub mod events;
pub mod ids;
pub mod kinds;
pub mod legacy_discovery;
pub mod migration;
pub mod models;
pub mod napcat;
pub mod path_probe_impl;
pub mod remote_host;
pub mod report;
pub mod runtime_backend;
pub mod runtime_launch_plan;
pub mod secret_store_impl;
pub mod snowluma;
pub mod traits;

pub use app_config::{
    SnowLumaAppConfig, WebUiPollerSettings, default_login_interval, default_snowluma_port,
};
pub use backend_config_renderer::{
    DispatchRenderer, NapCatConfigRenderer, SnowLumaConfigRenderer, create_renderer,
};
pub use bootstrap::{BootstrapSnapshot, BootstrapStatus, RepairAction};
pub use bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
pub use bot_config::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, BotConfigError,
    BypassConfig, ConnectConfig, HttpClientConfig, HttpServerConfig, HttpSseServerConfig, LogLevel,
    MessagePostFormat, NetworkBaseFields, O3HookMode, TimeUnit, WebsocketClientConfig,
    WebsocketServerConfig, WsRole,
};
pub use bot_config_repo_impl::LocalBotConfigRepo;
pub use bot_manager::{BatchResult, BootstrapResult, BotManager, BotManagerError};
pub use config_store_impl::LocalConfigStore;
pub use errors::{AppError, ConfigError, MigrationError, PathError, SecretError};
pub use events::{
    BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter, EventSubscription,
};
pub use ids::{BackendId, BotId};
pub use kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion};
pub use legacy_discovery::{LegacyDiscovery, LegacySelection};
pub use migration::MigrationOrchestrator;
pub use models::{
    BackupInfo, BotRuntimeSummary, MigrationOutcome, MigrationSource, MigrationStage,
    MigrationWarning,
};
pub use napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
pub use napcat::offline_notifier::{NoopOfflineNotifier, OfflineNoticeKind, OfflineNotifier};
pub use napcat::webui_client::{NapCatWebUiClient, NapCatWebUiError, ReqwestNapCatWebUiClient};
pub use path_probe_impl::LocalPathProbe;
pub use remote_host::{
    ExecResult, MockRemoteHost, PosixPath, ProcessNode, ProcessTree, RemoteFileEntry, RemoteHost,
    RemoteHostError, RemoteInstallInfo, ShellCmd, TunnelHandle, TunnelSpec,
};
pub use report::MigrationReport;
pub use runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, BotStatus, LocalRuntimeBackend,
    LogSnapshot, ProcessHandle, RemoteRuntimeBackend, StopMode, TailOpts,
};
pub use runtime_launch_plan::{
    FileSystemRuntimeLaunchPlanner, NapCatLaunchPlan, RuntimeLaunchPlan, RuntimeLaunchPlanError,
    RuntimeLaunchPlanner, SnowLumaLaunchPlan, build_napcat_launch_plan_with_qq_install_path,
};
pub use secret_store_impl::SecretStoreImpl;
pub use snowluma::{
    AuthState, DaemonState, HookProcessInfo, HookProcessStatus, MockProcessTreeProbe,
    OneBotInstanceInfo, ProcessTreeProbe, ReqwestSnowLumaWebUiClient,
    ReqwestSnowLumaWebUiClientFactory, SnowLumaDaemon, SnowLumaDaemonError, SnowLumaLoginState,
    SnowLumaRuntimeBackend, SnowLumaSession, SnowLumaStartMode, SnowLumaStatusPoller,
    SnowLumaWebUiClient, SnowLumaWebUiClientFactory, SnowLumaWebUiError, SysinfoProcessTreeProbe,
    load_or_create_session, render_daemon_globals, sanitize_log_line,
};
pub use traits::{
    BackendConfigRenderer, BotConfigRepo, ConfigStore, JsonTransaction, JsonWrite, MigrationStep,
    PathProbe, RenderError, SecretStore, TransactionReport,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_snapshot_round_trips() {
        let snapshot = BootstrapSnapshot::ready();
        let json = serde_json::to_string(&snapshot).unwrap();
        let decoded: BootstrapSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.status, BootstrapStatus::Ready);
        assert_eq!(decoded.schema_version, SchemaVersion::V3);
        assert_eq!(decoded.report, MigrationReport::clean());
    }
}
