pub mod app_framework;
pub mod bot_actor;
pub mod bot_manager;
pub mod events;
pub mod napcat;
pub mod native_deployment_adapter;
pub mod notify;
pub mod release;

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
pub mod watch;

// 配置横切已抽出 ncd-config；旧路径 re-export 保持 API。
pub mod config {
    pub use ncd_config::*;
}
pub mod app_config_migration {
    pub use ncd_config::app_migration::*;
}
pub mod backend_config_renderer {
    pub use ncd_config::renderer::*;
}
pub mod bot_config_migration {
    pub use ncd_config::bot_migration::*;
}
pub mod bot_config_repo_impl {
    pub use ncd_config::bot_repo::*;
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
pub mod secret_store_impl {
    pub use ncd_config::secret_store::*;
}
pub mod data_paths {
    pub use ncd_config::data_paths::*;
}
pub mod legacy_discovery {
    pub use ncd_config::legacy_discovery::*;
}
pub mod path_probe_impl {
    pub use ncd_config::path_probe::*;
}

// server 轴已抽出 ncd-server；旧路径 re-export 保持 API。
pub mod credential_sync {
    pub use ncd_server::credential_sync::*;
}
pub mod host_resolver {
    pub use ncd_server::host_resolver::*;
}
pub mod server_manager {
    pub use ncd_server::server_manager::*;
}
pub mod server_profile_migration {
    pub use ncd_server::server_profile_migration::*;
}
pub mod ssh_keygen {
    pub use ncd_server::ssh_keygen::*;
}

// 旧根文件路径兼容（目录化前的名字；新代码优先域目录 bootstrap/launch/remote/…）
// 外部 crate 仍可用这些路径；后续可标 deprecated 再删。
pub(crate) mod bootstrap_reconcile {
    pub(crate) use crate::bootstrap::reconcile::*;
}
/// 导入迁移：远端 NC/SL Native/Docker 网络配置（tauri 命令经此门面调用）
pub use remote::import_network::fetch_imported_network;
pub mod component_action_policy {
    pub use crate::components::action_policy::*;
}
pub mod component_factory {
    pub use crate::components::factory::*;
}
pub mod package_lock {
    pub use crate::components::package_lock::*;
}
pub mod crash_bundle {
    pub use crate::desktop::crash_bundle::*;
}
pub mod desktop_log {
    pub use crate::desktop::log::*;
}
pub mod data_consolidate {
    pub use crate::data::consolidate::*;
}
pub mod data_relocate {
    pub use crate::data::relocate::*;
}
pub mod deployment_tasks {
    pub use crate::deploy::tasks::*;
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
pub mod bot_runtime_metrics {
    pub use crate::metrics::*;
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
    AdvancedConfig, AutoRestartMode, AutoRestartSchedule, BackendKind, BackendType,
    BotBasicConfig, BotConfig, BotConfigError, BotFlavor, BotId, BotStatus, ConnectConfig, DeploymentType,
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

pub use app_framework::{
    AppInstanceStore, AppManager, BotConfigPort, NativeAppRuntime, OneBotExportError,
    app_link_connections, export_onebot_endpoint,
};
pub use ncd_appframework::{
    AppConfigWriteResult, AppFrameworkRegistry, AppInstanceConfig, AppInstanceConfigEnvelope,
};
pub use backend_config_renderer::{
    DispatchRenderer, NapCatConfigRenderer, SnowLumaConfigRenderer, create_renderer,
};
pub use bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
pub use bot_config_repo_impl::LocalBotConfigRepo;
pub use bot_manager::{
    BatchResult, BootstrapResult, BotManager, BotManagerError, RuntimeReadinessGate,
    describe_not_ready, framework_component_for,
};
pub use bot_manager::auto_restart::{
    PREVIEW_COUNT as AUTO_RESTART_PREVIEW_COUNT, preview_cron as preview_auto_restart_cron,
    validate_schedule as validate_auto_restart_schedule,
};
pub use component_action_policy::{
    RemoteHostProbe, RemoteLayout, asset_sha256, component_action_cancellable,
    component_action_needs_runtime_closure, component_catalog, component_dedupe_key,
    component_needs_download_slot, component_needs_package_manager, component_task_resources,
    data_root_to_host_path, infer_snowluma_linux_package, normalize_github_release_tag,
    parse_remote_host_probe_stdout, require_remote_home, snowluma_github_release_tag,
    snowluma_linux_release_asset, snowluma_windows_release_asset,
};
pub use component_factory::{BuildComponentCtx, build_component_for_host};
pub use components::{
    ClosureNode, ComponentActionRequest, ComponentBuildInputs, ComponentBuilder,
    ComponentExecutor, GRAPH_COMPONENT_IDS, ResolveCtx, catalog_version_reqs_for,
    graph_component, infer_local_snowluma_package, render_dependency_graph, requirement_closure,
    resolve_dependencies, resolve_runtime_readiness,
};
pub use config_store_impl::LocalConfigStore;
pub use crash_bundle::{CrashBundleInput, desktop_output_dir, write_crash_bundle};
pub use credential_sync::{CredentialSyncLayer, PasswordSlot};
pub use data_consolidate::{ConsolidateReport, consolidate_data_root};
pub use data_paths::{DataPaths, LAYOUT_VERSION};
pub use data_relocate::{
    RelocateError, STAGING_DIR_NAME, delete_retired_data_root, estimate_copy_bytes,
    execute_relocate, normalize_root, preflight_relocate, read_retired_marker,
    target_inside_source,
};
pub use deployment_tasks::{
    DeploymentTaskContext, DeploymentTaskManager, DeploymentTaskRequest, DeploymentTaskRunResult,
};
pub use docker_bot_session::{
    DockerBotSessionRegistry, SnowLumaDockerEndpoints, is_remote_docker_config,
    is_remote_native_napcat_config,
};
pub use events::{
    BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter, EventSubscription,
};
pub use host_resolver::{HostResolveError, HostResolver, LocalOnlyHostResolver};
pub use legacy_discovery::{LegacyDiscovery, LegacySelection};
pub use migration::MigrationOrchestrator;
pub use napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
pub use napcat::offline_notifier::{NoopOfflineNotifier, OfflineNoticeKind, OfflineNotifier};
pub use napcat::webui_client::{NapCatWebUiClient, NapCatWebUiError, ReqwestNapCatWebUiClient};
pub use native_deployment_adapter::{
    DockerDeploymentBackend, EventBusSink, NativeDeploymentBackend, RemoteNativeDeploymentBackend,
    RuntimeLaunchPlannerAdapter,
};
pub use ncd_server::DiscoveredSshHost;
pub use notify::{
    CompositeOfflineNotifier, DesktopToastSink, LocalHttpServerCandidate, MessengerResolveSkip,
    NoopOneBotEndpointResolver, OneBotEndpointResolver, OneBotMessenger,
    SwappableOneBotEndpointResolver, resolve_local_onebot_messenger, send_offline_email,
    send_offline_webhook, send_test_email, send_test_webhook,
};
pub use path_probe_impl::LocalPathProbe;
pub use remote::importable_bots::collect_importable_remote_bots;
pub use remote::inventory::{
    INVENTORY_PROBE_TIMEOUT, INVENTORY_STALE_AFTER, build_probe_script, inventory_from_stdout,
    inventory_is_stale, layout_from_selected, probe_from_inventory, probe_remote_inventory,
    select_paths,
};
pub use remote_bot_log_follow::RemoteBotLogFollowRegistry;
pub use runtime_launch_plan::{
    FileSystemRuntimeLaunchPlanner, NapCatLaunchPlan, RuntimeLaunchPlan, RuntimeLaunchPlanError,
    RuntimeLaunchPlanner, SnowLumaLaunchPlan, build_napcat_launch_plan_with_qq_install_path,
};
pub use secret_store_impl::SecretStoreImpl;
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
    ReqwestSnowLumaWebUiClient, ReqwestSnowLumaWebUiClientFactory, SnowLumaDaemon,
    SnowLumaDaemonError, SnowLumaRuntimeBackend, SnowLumaSession, SnowLumaStatusPoller,
    SnowLumaWebUiClient, SnowLumaWebUiClientFactory, SnowLumaWebUiError, SysinfoProcessTreeProbe,
    load_or_create_session, load_snowluma_app_config, render_daemon_globals, sanitize_log_line,
};
pub use ssh_keygen::{GeneratedKeyPair, SshKeygenError, generate_ed25519};

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
