// BotActor 状态机的数据类型定义
//
// 纯 serde + ts-rs 数据结构,零运行时依赖。
// Actor 的 tokio 通道/调度逻辑留在 ncd-runtime/src/bot_actor.rs。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::BotId;

// BotActorState

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum BotActorState {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
    Repairing,
}

impl BotActorState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running | Self::Stopping)
    }
}

// BotActorSnapshot

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct BotActorSnapshot {
    #[ts(type = "string")]
    pub bot_id: BotId,
    pub state: BotActorState,
    #[ts(type = "number")]
    pub revision: u64,
    #[ts(type = "number")]
    pub token_generation: u64,
    pub pending_restart: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_transition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_error: Option<String>,
}

impl BotActorSnapshot {
    pub fn new(bot_id: impl Into<BotId>) -> Self {
        Self {
            bot_id: bot_id.into(),
            state: BotActorState::Stopped,
            revision: 0,
            token_generation: 0,
            pending_restart: false,
            last_transition: None,
            last_error: None,
        }
    }

    pub fn advance(&mut self, state: BotActorState, transition: impl Into<String>) {
        self.state = state;
        self.revision = self.revision.saturating_add(1);
        self.last_transition = Some(transition.into());
    }

    pub fn note_error(&mut self, error: impl Into<String>) {
        self.last_error = Some(error.into());
        self.revision = self.revision.saturating_add(1);
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }
}

// BotActorError

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum BotActorError {
    #[error("invalid transition from {from:?} via {command}")]
    InvalidTransition {
        from: BotActorState,
        command: &'static str,
    },
    #[error("actor mailbox closed")]
    MailboxClosed,
}
