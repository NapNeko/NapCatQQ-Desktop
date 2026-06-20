pub mod app_config_migration;
pub mod backend_config_renderer;
pub mod bot_actor;
pub mod bot_config_migration;
pub mod bot_config_repo_impl;
pub mod bot_manager;
pub mod config_drift;
pub mod config_store_impl;
pub mod crash_bundle;
pub mod credential_sync;
pub mod docker_bot_session;
pub use docker_bot_session::{
    DockerBotSessionRegistry, SnowLumaDockerEndpoints, is_remote_docker_config,
    is_remote_native_napcat_config,
};
pub mod desktop_log;
pub mod events;
pub mod host_resolver;
pub mod legacy_discovery;
pub mod migration;
pub mod napcat;
pub mod package_lock;
pub mod path_probe_impl;
pub mod release;
pub mod native_deployment_adapter;
pub mod remote_bot_log_follow;
pub mod remote_native_launch;
pub mod remote_native_napcat_session;
pub mod remote_snowluma;
pub mod remote_snowluma_layout;
pub mod remote_snowluma_log;
pub mod remote_snowluma_orchestrator;
pub mod remote_snowluma_stack;
pub mod remote_snowluma_tunnel;
pub mod runtime_backend;
pub mod runtime_launch_plan;
pub mod secret_store_impl;
pub mod server_manager;
pub mod ssh_keygen;
pub mod snowluma;

// ===== Layer 1 数据(已迁移到 ncd-domain,此处 re-export 保持向后兼容) =====
//
// 这些类型实际定义在 ncd-domain crate。下游代码可继续 use ncd_runtime::{BotId, ...},
// 但新代码应直接 use ncd_domain::...。这些 re-export 只作过渡。
pub use ncd_domain::{
    AdvancedConfig, AppError, AppSettings, AppSettingsDto, AutoRestartSchedule, BackendId,
    BackendKind, BackendType, BackupInfo, BootstrapSnapshot, BootstrapStatus, BotBasicConfig,
    BotConfig, BotConfigError, BotFlavor, BotId, BotRuntimeSummary, BypassConfig, ConfigError,
    ConnectConfig, DeploymentType, HttpClientConfig, HttpServerConfig, HttpSseServerConfig, LocalVersionSnapshot,
    LogLevel, MessagePostFormat, MigrationError, MigrationOutcome, MigrationReport, MigrationSource,
    MigrationStage, MigrationWarning, NetworkBaseFields, O3HookMode, PathError, ReleaseInfo,
    ReleaseSnapshot, RepairAction, RuntimeTarget, SchemaVersion, SecretError, SnowLumaAppConfig,
    SnowLumaStartMode, TimeUnit, WebUiPollerSettings, WebsocketClientConfig, WebsocketServerConfig,
    WsRole, default_login_interval, default_perf_monitor_interval, default_snowluma_port,
    DesktopNotifySettings,
};
// StopMode 也来自 ncd-domain 但在 runtime_backend pub use 链已 re-export，
// 这里就不再重复导出避免 ambiguity。

// 兼容老路径:ncd_runtime::ids::BotId / ncd_runtime::bot_config::... 这种调用继续可用
pub use ncd_domain::{
    app_config, bootstrap, bot_config, errors, ids, kinds, models, release_snapshot, report,
    snowluma_start_mode, version_snapshot,
};

pub use backend_config_renderer::{
    DispatchRenderer, NapCatConfigRenderer, SnowLumaConfigRenderer, create_renderer,
};
pub use bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
pub use bot_config_repo_impl::LocalBotConfigRepo;
pub use bot_manager::{BatchResult, BootstrapResult, BotManager, BotManagerError};
pub use config_store_impl::LocalConfigStore;
pub use crash_bundle::{desktop_output_dir, write_crash_bundle, CrashBundleInput};
pub use credential_sync::{CredentialSyncLayer, PasswordSlot};
pub use events::{
    BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter, EventSubscription,
};
pub use host_resolver::{HostResolver, LocalOnlyHostResolver};
pub use legacy_discovery::{LegacyDiscovery, LegacySelection};
pub use migration::MigrationOrchestrator;
pub use napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
pub use napcat::offline_notifier::{NoopOfflineNotifier, OfflineNoticeKind, OfflineNotifier};
pub use napcat::webui_client::{NapCatWebUiClient, NapCatWebUiError, ReqwestNapCatWebUiClient};
pub use path_probe_impl::LocalPathProbe;
pub use runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, BotStatus, LogSnapshot,
    ProcessHandle, StopMode, TailOpts,
};
pub use runtime_launch_plan::{
    FileSystemRuntimeLaunchPlanner, NapCatLaunchPlan, RuntimeLaunchPlan, RuntimeLaunchPlanError,
    RuntimeLaunchPlanner, SnowLumaLaunchPlan, build_napcat_launch_plan_with_qq_install_path,
};
pub use native_deployment_adapter::{
    DockerDeploymentBackend, EventBusSink, NativeDeploymentBackend, RemoteNativeDeploymentBackend,
    RuntimeLaunchPlannerAdapter,
};
pub use remote_bot_log_follow::RemoteBotLogFollowRegistry;
pub use remote_native_napcat_session::RemoteNativeNapcatSessionRegistry;
pub use remote_native_launch::RemoteNativeLaunchTranslator;
pub use remote_snowluma::{RemoteSnowLumaBackend, RemoteSnowLumaDaemon};
pub use remote_snowluma_tunnel::{
    RemoteSnowLumaTunnelEndpoints, RemoteSnowLumaTunnelRegistry,
};
pub use secret_store_impl::SecretStoreImpl;
pub use server_manager::{
    AuthMethod, ConnectionHealth, InMemoryCredentialStore, KeyringCredentialStore, ProbeReport, ServerCredentialStore,
    ServerManager, ServerProfile, ServerState,
};
pub use snowluma::{
    AuthState, DaemonState, HookProcessInfo, HookProcessStatus, MockProcessTreeProbe,
    OneBotInstanceInfo, ProcessTreeProbe, ReqwestSnowLumaWebUiClient,
    ReqwestSnowLumaWebUiClientFactory, SnowLumaDaemon, SnowLumaDaemonError, SnowLumaLoginState,
    SnowLumaRuntimeBackend, SnowLumaSession, SnowLumaStatusPoller, SnowLumaWebUiClient,
    SnowLumaWebUiClientFactory, SnowLumaWebUiError, SysinfoProcessTreeProbe,
    load_or_create_session, load_snowluma_app_config, render_daemon_globals, sanitize_log_line,
};
pub use traits::{
    BackendConfigRenderer, BotConfigRepo, ConfigStore, JsonTransaction, JsonWrite, MigrationStep,
    PathProbe, RenderError, SecretStore, TransactionReport,
};

// 兼容老路径:ncd_runtime::traits::xxx::* 老调用继续可用
pub use ncd_traits as traits;

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
