// BotStatus / ProcessHandle: Bot 运行时状态快照
//
// 纯 serde 数据结构,零运行时依赖。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::bot_actor::BotActorState;
use crate::ids::BotId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessHandle {
    pub bot_id: BotId,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotStatus {
    pub bot_id: BotId,
    pub state: BotActorState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_rss_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_total_memory_bytes: Option<u64>,
    #[serde(default)]
    pub extra: Map<String, Value>,
}

impl BotStatus {
    pub fn stopped(bot_id: impl Into<BotId>) -> Self {
        Self {
            bot_id: bot_id.into(),
            state: BotActorState::Stopped,
            transport_error: None,
            pid: None,
            started_at: None,
            memory_rss_bytes: None,
            server_total_memory_bytes: None,
            extra: Map::new(),
        }
    }

    pub fn running(bot_id: impl Into<BotId>, pid: u32, started_at: u64) -> Self {
        Self {
            bot_id: bot_id.into(),
            state: BotActorState::Running,
            transport_error: None,
            pid: Some(pid),
            started_at: Some(started_at),
            memory_rss_bytes: None,
            server_total_memory_bytes: None,
            extra: Map::new(),
        }
    }
}
