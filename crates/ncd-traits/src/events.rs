use tokio::sync::broadcast;

use ncd_domain::domain_event::{DomainEvent, DomainEventKind};
use ncd_domain::ids::BotId;

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
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        target: "ncd::event_bus",
                        skipped,
                        "broadcast receiver lagged; events were dropped"
                    );
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

pub trait EventBus: Send + Sync {
    fn publish(&self, event: DomainEvent);
    fn subscribe(&self, filter: EventFilter) -> EventSubscription;
}

pub const DEFAULT_BROADCAST_CAPACITY: usize = 1024;

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
        Self::new(DEFAULT_BROADCAST_CAPACITY)
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
