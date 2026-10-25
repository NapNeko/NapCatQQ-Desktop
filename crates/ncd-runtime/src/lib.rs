pub mod app_framework;
pub mod assistant;
pub mod bot_actor;
pub mod bot_manager;
pub mod chat;
mod chat_archive;
pub mod chat_screenshots;
pub mod events;
pub mod napcat;
pub mod native_deployment_adapter;
pub mod notify;
pub mod onebot_debug;
pub mod release;
mod snowluma_local_endpoint;

// 域目录（命名收束）
pub mod bootstrap;
pub mod components;
pub mod data;
pub mod deploy;
pub mod desktop;
pub mod launch;
pub mod metrics;
pub mod remote;
pub mod snowluma;
pub mod terminal;
pub mod watch;

// 配置横切已抽出 ncd-config；旧路径 re-export 保持 API。
pub mod app_config_migration {
    pub use ncd_config::app_migration::*;
}
pub mod backend_config_renderer {
    pub use ncd_config::renderer::*;
}
pub mod bot_config_migration {
    pub use ncd_config::bot_migration::*;
}
pub mod config_drift {
    pub use ncd_config::drift::*;
}
pub mod config_store_impl {
    pub use ncd_config::store::*;
}
pub mod migration {
    pub use ncd_config::migration::*;
}
pub mod data_paths {
    pub use ncd_config::data_paths::*;
}

// server 轴已抽出 ncd-server；旧路径 re-export 保持 API。
pub mod host_resolver {
    pub use ncd_server::host_resolver::*;
}
pub mod server_manager {
    pub use ncd_server::server_manager::*;
}
pub mod server_profile_migration {
    pub use ncd_server::server_profile_migration::*;
}

// 旧根文件路径兼容（目录化前的名字；新代码优先域目录 bootstrap/launch/remote/…）
// 外部 crate 仍可用这些路径；后续可标 deprecated 再删。
pub(crate) mod bootstrap_reconcile {
    pub(crate) use crate::bootstrap::reconcile::*;
}
/// 导入迁移：远端 NC/SL Native/Docker 网络配置（tauri 命令经此门面调用）
pub use remote::import_network::fetch_imported_network;
pub mod crash_bundle {
    pub use crate::desktop::crash_bundle::*;
}
pub mod desktop_log {
    pub use crate::desktop::log::*;
}
pub mod data_relocate {
    pub use crate::data::relocate::*;
}
pub mod docker_bot_session {
    pub use crate::remote::docker_session::*;
}
pub mod remote_bot_log_follow {
    pub use crate::remote::bot_log_follow::*;
}
pub(crate) mod remote_runtime_sessions {
    pub(crate) use crate::remote::runtime_sessions::*;
}
pub mod runtime_launch_plan {
    pub use crate::launch::plan::*;
}
pub mod runtime_router {
    pub use crate::launch::router::RuntimeRouterError;
    pub(crate) use crate::launch::router::{DockerSecretProvider, RuntimeBackendRouter};
}
pub mod ncd_watch_sync {
    pub use crate::watch::sync::*;
}
pub(crate) mod snowluma_agreements {
    pub(crate) use crate::snowluma::agreements::*;
}
pub(crate) mod snowluma_consent_files {
    pub(crate) use crate::snowluma::consent_files::*;
}
pub mod snowluma_ui_state {
    pub use crate::snowluma::ui_state::*;
}

pub mod bot_config {
    pub use ncd_domain::bot_config::*;
}

pub use ncd_domain::{
    AdvancedConfig, AutoRestartMode, AutoRestartSchedule, BackendKind, BackendType, BotBasicConfig,
    BotConfig, BotConfigError, BotFlavor, BotId, BotStatus, ConnectConfig, DeploymentType,
    DesktopNotifySettings, DiscoveredRemoteBot, DiscoveredRemoteBotSource, HttpServerConfig,
    ImportableRemoteBot, ImportedNetworkConfig, LogLevel, MessagePostFormat, MigrationOutcome,
    MigrationStage, O3HookMode, PathError, REMOTE_INVENTORY_VERSION, RemoteInventory,
    RemoteInventoryItem, RemoteInventoryKind, RemoteInventorySource, RemotePathOverrides,
    RemoteSelectedPaths, RuntimeScenario, RuntimeTarget, SchemaVersion, SnowLumaLinuxPackage,
    SnowLumaStartMode, StopMode, WebUiPollerSettings, WebsocketServerConfig, WsRole,
};
pub use ncd_traits::{
    BotConfigRepo, ConfigStore, JsonTransaction, PathProbe, SecretStore,
    runtime_backend::{
        BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LogSnapshot, TailOpts,
    },
};

pub use app_framework::{AppInstanceStore, AppManager, BotConfigPort, NativeAppRuntime};
pub use backend_config_renderer::{
    DispatchRenderer, NapCatConfigRenderer, SnowLumaConfigRenderer, create_renderer,
};
pub use bot_actor::{BotActorError, BotActorSnapshot, BotActorState};
pub use bot_manager::auto_restart::{
    PREVIEW_COUNT as AUTO_RESTART_PREVIEW_COUNT, preview_cron as preview_auto_restart_cron,
};
pub use bot_manager::{BatchResult, BotManager, BotManagerError, RuntimeReadinessGate};
pub use components::action_policy::{
    RemoteHostProbe, RemoteLayout, component_catalog, infer_snowluma_linux_package,
};
pub use components::{ComponentActionRequest, ComponentExecutor, resolve_runtime_readiness};
pub use config_store_impl::LocalConfigStore;
pub use crash_bundle::{CrashBundleInput, write_crash_bundle};
pub use data::consolidate::consolidate_data_root;
pub use data_paths::{DataPaths, LAYOUT_VERSION};
pub use data_relocate::{
    RelocateError, delete_retired_data_root, execute_relocate, normalize_root, preflight_relocate,
};
pub use deploy::tasks::{
    DeploymentTaskContext, DeploymentTaskManager, DeploymentTaskRequest, DeploymentTaskRunResult,
};
pub use docker_bot_session::{is_remote_docker_config, is_remote_native_napcat_config};
pub use events::{
    BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter, EventSubscription,
};
pub use host_resolver::{HostResolveError, HostResolver, LocalOnlyHostResolver};
pub use migration::MigrationOrchestrator;
pub use napcat::debug_client::{NapCatDebugClient, NapCatDebugError};
pub use napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
pub use napcat::offline_notifier::{NoopOfflineNotifier, OfflineNoticeKind, OfflineNotifier};
pub use napcat::webui_client::{NapCatWebUiClient, NapCatWebUiError, ReqwestNapCatWebUiClient};
pub use native_deployment_adapter::{
    EventBusSink, NativeDeploymentBackend, RuntimeLaunchPlannerAdapter,
};
pub use ncd_appframework::{
    AppConfigWriteResult, AppFrameworkRegistry, AppInstanceConfig, AppInstanceConfigEnvelope,
    AppPanelOutcomeKind, AppPanelResult, AppStoreInstalled, AppStoreMarketEntry, AstrBotAbconfInfo,
    AstrBotDashboardStatus, AstrBotKbCreate, AstrBotKnowledgeBase, AstrBotPersona,
    AstrBotSessionRule, KarinPluginInstalled, KarinPluginMarketEntry, KoishiBotState,
    KoishiBotStatus, KoishiCommandRow, KoishiDatabaseTable, KoishiFileContent, KoishiFileEntry,
    KoishiInstanceConfig, KoishiPackageInfo, KoishiPluginNode, KoishiPluginSchema,
    KoishiRuntimeGate, KoishiRuntimeStatus, KoishiSandboxMessage, MaiBotAPIProvider,
    MaiBotBehaviorDetail, MaiBotBehaviorOverview, MaiBotBehaviorPage, MaiBotBehaviorQuery,
    MaiBotChatSession, MaiBotChatTicket, MaiBotEmojiAction, MaiBotEmojiImage, MaiBotEmojiOverview,
    MaiBotEmojiPage, MaiBotEmojiQuery, MaiBotEmojiUpload, MaiBotEmojiUploadDone,
    MaiBotExpressionAction, MaiBotExpressionOverview, MaiBotExpressionPage, MaiBotExpressionQuery,
    MaiBotJargonAction, MaiBotJargonOverview, MaiBotJargonPage, MaiBotJargonQuery,
    MaiBotLocalImage, MaiBotLocalTextFile, MaiBotMCPServerItemConfig, MaiBotMcpStatus,
    MaiBotMcpTest, MaiBotMemoryDeleteAction, MaiBotMemoryDeleteOp, MaiBotMemoryDeleteResult,
    MaiBotMemoryGraph, MaiBotMemoryGraphHit, MaiBotMemoryImport, MaiBotMemoryImportSetup,
    MaiBotMemoryNodeDetail, MaiBotMemoryQuery, MaiBotMemoryRecordDetail, MaiBotMemoryRecordKind,
    MaiBotMemoryRecordPage, MaiBotMemorySource, MaiBotMemoryStatus, MaiBotMemoryTask,
    MaiBotMemoryTaskAction, MaiBotMemoryTaskDetail, MaiBotPersonAction, MaiBotPersonOverview,
    MaiBotPersonPage, MaiBotPersonQuery, MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile,
    MaiBotProviderCheck, MaiBotProviderModel, MaiBotResourceDone, MaiBotRuntimeStatus,
    MaiBotStatsSummary, PackageVersions,
};
pub use ncd_config::LocalBotConfigRepo;
pub use ncd_config::LocalPathProbe;
pub use ncd_config::{SecretStoreImpl, SystemSecretStore};
pub use ncd_server::DiscoveredSshHost;
pub use notify::{
    CompositeOfflineNotifier, DesktopToastSink, LocalHttpServerCandidate,
    NoopOneBotEndpointResolver, OneBotEndpointResolver, OneBotMessenger,
    SwappableOneBotEndpointResolver, resolve_local_onebot_messenger, send_test_email,
    send_test_webhook,
};
pub use onebot_debug::{DebugBotPort, DebugEventSink, DebugManager, DebugStreamSink};
pub use remote::importable_bots::collect_importable_remote_bots;
pub use remote::inventory::layout_from_selected;
pub use remote_bot_log_follow::RemoteBotLogFollowRegistry;
pub use runtime_launch_plan::{
    FileSystemRuntimeLaunchPlanner, NapCatLaunchPlan, RuntimeLaunchPlan, RuntimeLaunchPlanError,
    RuntimeLaunchPlanner, SnowLumaLaunchPlan, build_napcat_launch_plan_with_qq_install_path,
    local_qq_source,
};
pub use server_manager::{
    AuthMethod, ConnectionHealth, InMemoryCredentialStore, KeyringCredentialStore, ProbeReport,
    ServerCredentialStore, ServerManager, ServerProfile, ServerState,
};
pub use server_profile_migration::{
    SERVER_PROFILE_COMPAT_VERSION, ServerProfileMigrationResult,
    migrate_legacy_single_server_app_config, migrate_server_profiles_payload,
};
pub use snowluma::{
    AgreementDoc, AgreementsPayload, AuthState, HookProcessInfo, HookProcessStatus,
    LinuxSinglePidProbe, MockProcessTreeProbe, OneBotInstanceInfo, ProcessTreeProbe,
    ReqwestSnowLumaWebUiClient, ReqwestSnowLumaWebUiClientFactory, SlStreamFrame, SnowLumaDaemon,
    SnowLumaDaemonError, SnowLumaDebugClient, SnowLumaDebugError, SnowLumaRuntimeBackend,
    SnowLumaSession, SnowLumaStatusPoller, SnowLumaWebUiClient, SnowLumaWebUiClientFactory,
    SnowLumaWebUiError, SysinfoProcessTreeProbe, load_or_create_session, load_snowluma_app_config,
    parse_stream_frame, render_daemon_globals, sanitize_log_line,
};
pub use snowluma_local_endpoint::local_snowluma_webui_endpoint;

#[cfg(test)]
mod tests {
    #[test]
    fn bootstrap_snapshot_round_trips() {
        use ncd_domain::{BootstrapSnapshot, BootstrapStatus, MigrationReport, SchemaVersion};
        let snapshot = BootstrapSnapshot::ready();
        let json = serde_json::to_string(&snapshot).unwrap();
        let decoded: BootstrapSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.status, BootstrapStatus::Ready);
        assert_eq!(decoded.schema_version, SchemaVersion::V3);
        assert_eq!(decoded.report, MigrationReport::clean());
    }
}
