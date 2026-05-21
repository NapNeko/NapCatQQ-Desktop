use serde::{Deserialize, Serialize};

use crate::kinds::SchemaVersion;
use crate::models::{MigrationOutcome, MigrationStage};
use crate::report::MigrationReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStatus {
    #[default]
    Ready,
    Migrating,
    RepairRequired,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    OpenDataDir,
    ExportMigrationReport,
    RestoreBackup,
    Reauthenticate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapSnapshot {
    pub status: BootstrapStatus,
    pub schema_version: SchemaVersion,
    pub report: MigrationReport,
}

impl BootstrapSnapshot {
    pub fn new(
        status: BootstrapStatus,
        schema_version: SchemaVersion,
        report: MigrationReport,
    ) -> Self {
        Self {
            status,
            schema_version,
            report,
        }
    }

    pub fn from_report(report: MigrationReport) -> Self {
        let status = match (report.stage, report.outcome) {
            (MigrationStage::Failed, _) => BootstrapStatus::Failed,
            (MigrationStage::RepairRequired, _) | (_, MigrationOutcome::NeedsRepair) => {
                BootstrapStatus::RepairRequired
            }
            (MigrationStage::Pending | MigrationStage::Running, _) => BootstrapStatus::Migrating,
            (MigrationStage::Completed, _) => BootstrapStatus::Ready,
        };

        Self::new(status, SchemaVersion::V3, report)
    }

    pub fn ready() -> Self {
        Self::from_report(MigrationReport::clean())
    }
}

impl From<MigrationReport> for BootstrapSnapshot {
    fn from(report: MigrationReport) -> Self {
        Self::from_report(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_clean_report_to_ready_snapshot() {
        let snapshot = BootstrapSnapshot::ready();

        assert_eq!(snapshot.status, BootstrapStatus::Ready);
        assert_eq!(snapshot.schema_version, SchemaVersion::V3);
        assert_eq!(snapshot.report, MigrationReport::clean());
    }

    #[test]
    fn maps_failed_report_to_failed_snapshot() {
        let snapshot = BootstrapSnapshot::from_report(MigrationReport::failed("boom"));

        assert_eq!(snapshot.status, BootstrapStatus::Failed);
    }
}
