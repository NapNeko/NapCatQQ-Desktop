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
pub mod desktop_log;
pub mod docker_bot_session;
pub mod events;
pub mod host_resolver;
pub mod legacy_discovery;
pub mod migration;
pub mod napcat;
pub mod native_deployment_adapter;
pub mod package_lock;
pub mod path_probe_impl;
pub mod release;
pub mod remote_bot_log_follow;
pub mod runtime_launch_plan;
pub mod secret_store_impl;
pub mod server_manager;
pub mod snowluma;
pub mod ssh_keygen;

pub use backend_config_renderer::{
    DispatchRenderer, NapCatConfigRenderer, SnowLumaConfigRenderer, create_renderer,
};
pub use bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
pub use bot_config_repo_impl::LocalBotConfigRepo;
pub use bot_manager::{BatchResult, BootstrapResult, BotManager, BotManagerError};
pub use config_store_impl::LocalConfigStore;
pub use crash_bundle::{CrashBundleInput, desktop_output_dir, write_crash_bundle};
pub use credential_sync::{CredentialSyncLayer, PasswordSlot};
pub use docker_bot_session::{
    DockerBotSessionRegistry, SnowLumaDockerEndpoints, is_remote_docker_config,
    is_remote_native_napcat_config,
};
pub use events::{
    BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter, EventSubscription,
};
pub use host_resolver::{HostResolver, LocalOnlyHostResolver};
pub use legacy_discovery::{LegacyDiscovery, LegacySelection};
pub use migration::MigrationOrchestrator;
pub use napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
pub use napcat::offline_notifier::{NoopOfflineNotifier, OfflineNoticeKind, OfflineNotifier};
pub use napcat::webui_client::{NapCatWebUiClient, NapCatWebUiError, ReqwestNapCatWebUiClient};
pub use native_deployment_adapter::{
    DockerDeploymentBackend, EventBusSink, NativeDeploymentBackend, RemoteNativeDeploymentBackend,
    RuntimeLaunchPlannerAdapter,
};
pub use path_probe_impl::LocalPathProbe;
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
pub use snowluma::{
    AuthState, HookProcessInfo, HookProcessStatus, MockProcessTreeProbe,
    OneBotInstanceInfo, ProcessTreeProbe, ReqwestSnowLumaWebUiClient,
    ReqwestSnowLumaWebUiClientFactory, SnowLumaDaemon, SnowLumaDaemonError,
    SnowLumaRuntimeBackend, SnowLumaSession, SnowLumaStatusPoller, SnowLumaWebUiClient,
    SnowLumaWebUiClientFactory, SnowLumaWebUiError, SysinfoProcessTreeProbe,
    load_or_create_session, load_snowluma_app_config, render_daemon_globals, sanitize_log_line,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_snapshot_round_trips() {
        use ncd_domain::{BootstrapSnapshot, BootstrapStatus, SchemaVersion, MigrationReport};
        let snapshot = BootstrapSnapshot::ready();
        let json = serde_json::to_string(&snapshot).unwrap();
        let decoded: BootstrapSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.status, BootstrapStatus::Ready);
        assert_eq!(decoded.schema_version, SchemaVersion::V3);
        assert_eq!(decoded.report, MigrationReport::clean());
    }
}
