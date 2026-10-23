//! 服务器间整套 Bot 迁移的预检和进度契约。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BotMigrationRequest {
    pub bot_id: String,
    pub target_server_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BotMigrationApp {
    pub instance_id: String,
    pub display_name: String,
    pub framework_id: String,
    pub source_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BotMigrationPlan {
    pub bot_id: String,
    pub source_host_id: String,
    pub target_host_id: String,
    pub apps: Vec<BotMigrationApp>,
    pub blockers: Vec<String>,
    pub notes: Vec<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum BotMigrationPhase {
    Preparing,
    Stopping,
    Copying,
    Rebuilding,
    Switching,
    Starting,
    AwaitingLogin,
    Completed,
    Failed,
    RollingBack,
    RolledBack,
    RecoveryRequired,
}

impl BotMigrationPhase {
    pub fn is_working(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::Stopping
                | Self::Copying
                | Self::Rebuilding
                | Self::Switching
                | Self::Starting
                | Self::RollingBack
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BotMigrationJob {
    pub id: String,
    pub bot_id: String,
    pub source_host_id: String,
    pub target_host_id: String,
    pub phase: BotMigrationPhase,
    pub message: String,
    pub apps: Vec<BotMigrationApp>,
    pub error: Option<String>,
    pub rollback_available: bool,
    pub target_started: bool,
    pub updated_at_ms: u64,
}
