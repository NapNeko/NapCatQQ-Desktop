pub mod app_config_migration;
pub mod bot_actor;
pub mod bootstrap;
pub mod bot_config_migration;
pub mod config_store_impl;
pub mod errors;
pub mod events;
pub mod ids;
pub mod kinds;
pub mod legacy_discovery;
pub mod migration;
pub mod models;
pub mod path_probe_impl;
pub mod report;
pub mod runtime_backend;
pub mod secret_store_impl;
pub mod traits;

pub use bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
pub use bootstrap::{BootstrapSnapshot, BootstrapStatus, RepairAction};
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
pub use path_probe_impl::LocalPathProbe;
pub use report::MigrationReport;
pub use runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LocalRuntimeBackend,
    LogSnapshot, ProcessHandle, StopMode, TailOpts, BotStatus,
};
pub use secret_store_impl::SecretStoreImpl;
pub use traits::{
    ConfigStore, JsonTransaction, JsonWrite, MigrationStep, PathProbe, SecretStore,
    TransactionReport,
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
