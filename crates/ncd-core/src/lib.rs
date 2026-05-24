pub mod app_config;
pub mod app_config_migration;
pub mod backend_config_renderer;
pub mod bot_actor;
pub mod bot_config;
pub mod bot_config_migration;
pub mod bot_config_repo_impl;
pub mod bot_manager;
pub mod config_store_impl;
pub mod events;
pub mod legacy_discovery;
pub mod migration;
pub mod napcat;
pub mod path_probe_impl;
pub mod remote_host;
pub mod runtime_backend;
pub mod runtime_launch_plan;
pub mod secret_store_impl;
pub mod snowluma;
pub mod traits;

// ===== Layer 1 数据(已迁移到 ncd-domain,此处 re-export 保持向后兼容) =====
//
// M2.1:这些类型从 ncd-core 移动到 ncd-domain crate(蓝图 §2.1 / §3.1)。
// 下游代码可继续 `use ncd_core::{BotId, ...}`,但**新代码应直接** `use ncd_domain::...`。
// M6 阶段 ncd-core 改名 ncd-runtime 时,这些 re-export 会被移除。
pub use ncd_domain::{
    AppError, BackendId, BackendKind, BackupInfo, BootstrapSnapshot, BootstrapStatus, BotFlavor,
    BotId, BotRuntimeSummary, ConfigError, MigrationError, MigrationOutcome, MigrationReport,
    MigrationSource, MigrationStage, MigrationWarning, PathError, RepairAction, RuntimeTarget,
    SchemaVersion, SecretError,
};

// 兼容老路径:`ncd_core::ids::BotId` 这种调用继续可用
pub use ncd_domain::{bootstrap, errors, ids, kinds, models, report};

pub use app_config::{
    SnowLumaAppConfig, WebUiPollerSettings, default_login_interval, default_snowluma_port,
};
pub use backend_config_renderer::{
    DispatchRenderer, NapCatConfigRenderer, SnowLumaConfigRenderer, create_renderer,
};
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
pub use events::{
    BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter, EventSubscription,
};
pub use legacy_discovery::{LegacyDiscovery, LegacySelection};
pub use migration::MigrationOrchestrator;
pub use napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
pub use napcat::offline_notifier::{NoopOfflineNotifier, OfflineNoticeKind, OfflineNotifier};
pub use napcat::webui_client::{NapCatWebUiClient, NapCatWebUiError, ReqwestNapCatWebUiClient};
pub use path_probe_impl::LocalPathProbe;
pub use remote_host::{
    ExecResult, MockRemoteHost, PosixPath, ProcessNode, ProcessTree, RemoteFileEntry, RemoteHost,
    RemoteHostError, RemoteInstallInfo, ShellCmd, TunnelHandle, TunnelSpec,
};
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
