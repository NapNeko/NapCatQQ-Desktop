use serde::{Deserialize, Serialize};

use crate::models::{MigrationOutcome, MigrationStage, MigrationWarning};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub stage: MigrationStage,
    pub outcome: MigrationOutcome,
    #[serde(default)]
    pub warnings: Vec<MigrationWarning>,
}

impl MigrationReport {
    pub fn clean() -> Self {
        Self {
            stage: MigrationStage::Completed,
            outcome: MigrationOutcome::NoChange,
            warnings: Vec::new(),
        }
    }

    pub fn migrating() -> Self {
        Self {
            stage: MigrationStage::Running,
            outcome: MigrationOutcome::NoChange,
            warnings: Vec::new(),
        }
    }

    pub fn repair_required(warnings: Vec<MigrationWarning>) -> Self {
        Self {
            stage: MigrationStage::RepairRequired,
            outcome: MigrationOutcome::NeedsRepair,
            warnings,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            stage: MigrationStage::Failed,
            outcome: MigrationOutcome::NeedsRepair,
            warnings: vec![MigrationWarning::new("migration_failed", message)],
        }
    }
}

impl Default for MigrationReport {
    fn default() -> Self {
        Self::clean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_report_is_completed_and_warning_free() {
        let report = MigrationReport::clean();

        assert_eq!(report.stage, MigrationStage::Completed);
        assert_eq!(report.outcome, MigrationOutcome::NoChange);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn failed_report_carries_warning() {
        let report = MigrationReport::failed("boom");

        assert_eq!(report.stage, MigrationStage::Failed);
        assert_eq!(report.outcome, MigrationOutcome::NeedsRepair);
        assert_eq!(report.warnings[0].code, "migration_failed");
    }
}
