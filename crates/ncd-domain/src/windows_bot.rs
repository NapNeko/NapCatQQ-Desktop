//! 用户会话里的 Windows Bot 启动参数，所有路径属于目标主机。
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::BotConfig;

#[derive(Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsBotSpec {
    #[serde(default)]
    pub intent_revision: Option<String>,
    #[serde(default)]
    pub remote_qr_login_enabled: bool,
    #[serde(default)]
    pub metrics_environment: std::collections::BTreeMap<String, String>,
    pub bot_id: String,
    pub config: BotConfig,
    pub napcat_dir: String,
    pub qq_executable: String,
    pub snowluma_dir: String,
    pub node_executable: String,
    pub data_root: String,
    pub webui_password: String,
    pub webui_port: u16,
}

impl std::fmt::Debug for WindowsBotSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WindowsBotSpec")
            .field("bot_id", &self.bot_id)
            .field("backend", &self.config.bot.backend_type)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsBotEventBatch {
    pub generation: String,
    #[ts(type = "number")]
    pub next: u64,
    pub events: Vec<crate::DomainEvent>,
    #[serde(default)]
    #[ts(type = "number[]")]
    pub event_sequences: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsBotEndpoint {
    pub backend: crate::BackendType,
    pub host_port: u16,
    pub secret: String,
}
