use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::bot_actor::BotActorSnapshot;
use crate::ids::BotId;
use crate::runtime_backend::BotStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainEventKind {
    BotStateChanged,
    BotStatusChanged,
    BotLogAppended,
    BotError,
    TaskProgress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DomainEvent {
    BotStateChanged {
        snapshot: BotActorSnapshot,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    BotStatusChanged {
        status: BotStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    BotLogAppended {
        bot_id: BotId,
        line: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    BotError {
        bot_id: BotId,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
    },
    TaskProgress {
        task_id: String,
        progress: u8,
        message: String,
    },
}

impl DomainEvent {
    pub fn kind(&self) -> DomainEventKind {
        match self {
            Self::BotStateChanged { .. } => DomainEventKind::BotStateChanged,
            Self::BotStatusChanged { .. } => DomainEventKind::BotStatusChanged,
            Self::BotLogAppended { .. } => DomainEventKind::BotLogAppended,
            Self::BotError { .. } => DomainEventKind::BotError,
            Self::TaskProgress { .. } => DomainEventKind::TaskProgress,
        }
    }

    pub fn tauri_event_name(&self) -> &'static str {
        match self {
            Self::BotStateChanged { .. } => "bot_state_changed",
            Self::BotStatusChanged { .. } => "bot_status_changed",
            Self::BotLogAppended { .. } => "log_appended",
            Self::BotError { .. } => "bot_error",
            Self::TaskProgress { .. } => "task_progress",
        }
    }

    pub fn bot_id(&self) -> Option<&BotId> {
        match self {
            Self::BotStateChanged { snapshot, .. } => Some(&snapshot.bot_id),
            Self::BotStatusChanged { status, .. } => Some(&status.bot_id),
            Self::BotLogAppended { bot_id, .. } => Some(bot_id),
            Self::BotError { bot_id, .. } => Some(bot_id),
            Self::TaskProgress { .. } => None,
        }
    }

    pub fn bot_state_changed(snapshot: BotActorSnapshot, reason: impl Into<String>) -> Self {
        Self::BotStateChanged {
            snapshot,
            reason: Some(reason.into()),
        }
    }

    pub fn bot_log(bot_id: impl Into<BotId>, line: impl Into<String>) -> Self {
        Self::BotLogAppended {
            bot_id: bot_id.into(),
            line: line.into(),
            channel: None,
        }
    }

    pub fn bot_status_changed(status: BotStatus, source: impl Into<String>) -> Self {
        Self::BotStatusChanged {
            status,
            source: Some(source.into()),
        }
    }

    pub fn bot_error(
        bot_id: impl Into<BotId>,
        message: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self::BotError {
            bot_id: bot_id.into(),
            message: message.into(),
            hint,
        }
    }

    pub fn task_progress(
        task_id: impl Into<String>,
        progress: u8,
        message: impl Into<String>,
    ) -> Self {
        Self::TaskProgress {
            task_id: task_id.into(),
            progress,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventFilter {
    pub bot_id: Option<BotId>,
    pub kind: Option<DomainEventKind>,
}

impl EventFilter {
    pub fn all() -> Self {
        Self::default()
    }

    pub fn bot(bot_id: impl Into<BotId>) -> Self {
        Self {
            bot_id: Some(bot_id.into()),
            kind: None,
        }
    }

    pub fn kind(kind: DomainEventKind) -> Self {
        Self {
            bot_id: None,
            kind: Some(kind),
        }
    }

    pub fn matches(&self, event: &DomainEvent) -> bool {
        if let Some(kind) = self.kind
            && event.kind() != kind
        {
            return false;
        }
        if let Some(bot_id) = &self.bot_id
            && event.bot_id() != Some(bot_id)
        {
            return false;
        }
        true
    }
}

pub struct EventSubscription {
    receiver: broadcast::Receiver<DomainEvent>,
    filter: EventFilter,
}

impl EventSubscription {
    pub async fn next(&mut self) -> Option<DomainEvent> {
        loop {
            match self.receiver.recv().await {
                Ok(event) if self.filter.matches(&event) => return Some(event),
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

pub trait EventBus: Send + Sync {
    fn publish(&self, event: DomainEvent);
    fn subscribe(&self, filter: EventFilter) -> EventSubscription;
}

#[derive(Debug, Clone)]
pub struct BroadcastEventBus {
    sender: broadcast::Sender<DomainEvent>,
}

impl BroadcastEventBus {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
}

impl Default for BroadcastEventBus {
    fn default() -> Self {
        Self::new(128)
    }
}

impl EventBus for BroadcastEventBus {
    fn publish(&self, event: DomainEvent) {
        let _ = self.sender.send(event);
    }

    fn subscribe(&self, filter: EventFilter) -> EventSubscription {
        EventSubscription {
            receiver: self.sender.subscribe(),
            filter,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_actor::BotActorSnapshot;

    #[test]
    fn event_name_mapping_matches_frontend_contract() {
        let event = DomainEvent::bot_log("10001", "hello");
        assert_eq!(event.tauri_event_name(), "log_appended");
        assert_eq!(event.kind(), DomainEventKind::BotLogAppended);
    }

    #[tokio::test]
    async fn broadcast_event_bus_filters_by_bot_id() {
        let bus = BroadcastEventBus::default();
        let mut subscription = bus.subscribe(EventFilter::bot("10001"));

        bus.publish(DomainEvent::bot_log("10002", "skip"));
        bus.publish(DomainEvent::bot_log("10001", "hit"));

        let event = subscription.next().await.expect("expected matching event");
        match event {
            DomainEvent::BotLogAppended { bot_id, line, .. } => {
                assert_eq!(bot_id.as_str(), "10001");
                assert_eq!(line, "hit");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn bot_status_changed_event_serializes() {
        let status = BotStatus::running("10004", 1234, 5678);
        let event = DomainEvent::bot_status_changed(status, "runtime_poll");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("bot_status_changed"));
        assert!(json.contains("runtime_poll"));
    }

    #[test]
    fn bot_state_changed_event_serializes() {
        let snapshot = BotActorSnapshot::new("10003");
        let event = DomainEvent::bot_state_changed(snapshot, "start_requested");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("bot_state_changed"));
        assert!(json.contains("start_requested"));
    }
}
