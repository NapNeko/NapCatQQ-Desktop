// Migration 数据模型 + 报告
//
// 原 models.rs 和 report.rs 合并到此处: 两者都是迁移流程的数据契约,
// report 直接消费 models 的类型, 合并后消除跨模块 import 噪音

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::bootstrap::RepairAction;
use crate::ids::{BackendId, BotId};
use crate::kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion};

// ===== models (原 models.rs) =====

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStage {
    #[default]
    Pending,
    Running,
    Completed,
    RepairRequired,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MigrationOutcome {
    #[default]
    NoChange,
    Updated,
    NeedsRepair,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationWarning {
    pub code: String,
    pub message: String,
}

impl MigrationWarning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MigrationSource {
    pub root: PathBuf,
    #[serde(default)]
    pub app_config: Option<PathBuf>,
    #[serde(default)]
    pub bot_config: Option<PathBuf>,
    #[serde(default)]
    pub auxiliary_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BackupInfo {
    pub root: PathBuf,
    #[serde(default)]
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotRuntimeSummary {
    pub bot_id: BotId,
    pub backend_id: BackendId,
    pub backend_kind: BackendKind,
    pub flavor: BotFlavor,
    pub runtime_target: RuntimeTarget,
    pub schema_version: SchemaVersion,
}

impl BotRuntimeSummary {
    pub fn new(
        bot_id: impl Into<BotId>,
        backend_id: impl Into<BackendId>,
        backend_kind: BackendKind,
        flavor: BotFlavor,
        runtime_target: impl Into<RuntimeTarget>,
        schema_version: SchemaVersion,
    ) -> Self {
        Self {
            bot_id: bot_id.into(),
            backend_id: backend_id.into(),
            backend_kind,
            flavor,
            runtime_target: runtime_target.into(),
            schema_version,
        }
    }

    pub fn is_local(&self) -> bool {
        self.runtime_target.is_local()
    }
}

// ===== report (原 report.rs) =====

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrationReport {
    pub stage: MigrationStage,
    pub outcome: MigrationOutcome,
    #[serde(default)]
    pub warnings: Vec<MigrationWarning>,
    #[serde(default)]
    pub source: Option<MigrationSource>,
    #[serde(default)]
    pub backup: Option<BackupInfo>,
    #[serde(default)]
    pub rules_applied: Vec<String>,
    #[serde(default)]
    pub repair_actions: Vec<RepairAction>,
}

impl MigrationReport {
    pub fn clean() -> Self {
        Self {
            stage: MigrationStage::Completed,
            outcome: MigrationOutcome::NoChange,
            warnings: Vec::new(),
            source: None,
            backup: None,
            rules_applied: Vec::new(),
            repair_actions: Vec::new(),
        }
    }

    pub fn migrated(rules_applied: Vec<String>) -> Self {
        Self {
            stage: MigrationStage::Completed,
            outcome: MigrationOutcome::Updated,
            warnings: Vec::new(),
            source: None,
            backup: None,
            rules_applied,
            repair_actions: Vec::new(),
        }
    }

    pub fn migrating() -> Self {
        Self {
            stage: MigrationStage::Running,
            outcome: MigrationOutcome::NoChange,
            warnings: Vec::new(),
            source: None,
            backup: None,
            rules_applied: Vec::new(),
            repair_actions: Vec::new(),
        }
    }

    pub fn repair_required(warnings: Vec<MigrationWarning>) -> Self {
        Self {
            stage: MigrationStage::RepairRequired,
            outcome: MigrationOutcome::NeedsRepair,
            warnings,
            source: None,
            backup: None,
            rules_applied: Vec::new(),
            repair_actions: vec![
                RepairAction::OpenDataDir,
                RepairAction::ExportMigrationReport,
            ],
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            stage: MigrationStage::Failed,
            outcome: MigrationOutcome::NeedsRepair,
            warnings: vec![MigrationWarning::new("migration_failed", message)],
            source: None,
            backup: None,
            rules_applied: Vec::new(),
            repair_actions: vec![
                RepairAction::OpenDataDir,
                RepairAction::ExportMigrationReport,
            ],
        }
    }

    pub fn with_source(mut self, source: MigrationSource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_backup(mut self, backup: BackupInfo) -> Self {
        self.backup = Some(backup);
        self
    }

    pub fn with_warnings(mut self, warnings: Vec<MigrationWarning>) -> Self {
        self.warnings = warnings;
        self
    }

    pub fn with_repair_action(mut self, action: RepairAction) -> Self {
        if !self.repair_actions.contains(&action) {
            self.repair_actions.push(action);
        }
        self
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

    // --- models tests ---

    #[test]
    fn builds_local_runtime_summary() {
        let summary = BotRuntimeSummary::new(
            "10001",
            "backend-1",
            BackendKind::Local,
            BotFlavor::NapCat,
            RuntimeTarget::Local,
            SchemaVersion::V3,
        );

        assert!(summary.is_local());
        assert_eq!(summary.bot_id.as_str(), "10001");
    }

    // --- report tests ---

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
        assert!(report.repair_actions.contains(&RepairAction::OpenDataDir));
    }
}
