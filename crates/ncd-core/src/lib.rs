pub mod bootstrap;
pub mod errors;
pub mod ids;
pub mod kinds;
pub mod models;
pub mod report;
pub mod traits;

pub use bootstrap::{BootstrapSnapshot, BootstrapStatus, RepairAction};
pub use errors::{AppError, ConfigError, MigrationError, PathError, SecretError};
pub use ids::{BackendId, BotId};
pub use kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion};
pub use models::{BotRuntimeSummary, MigrationOutcome, MigrationStage, MigrationWarning};
pub use report::MigrationReport;
pub use traits::{ConfigStore, MigrationStep, PathProbe, SecretStore};

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
