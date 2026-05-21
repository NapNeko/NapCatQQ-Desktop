use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ids::{BackendId, BotId};
use crate::kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
