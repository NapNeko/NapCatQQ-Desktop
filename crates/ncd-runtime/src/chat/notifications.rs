//! 群提醒设置只过滤桌面提示，不丢消息或改动 QQ 的接收设置。
use super::ChatManager;
use ncd_domain::chat_archive::{ChatArchive, ChatArchiveConversationKind};
use ncd_domain::chat_desktop::{ChatAccountPreference, ChatGroupNotification};
use ncd_domain::onebot_debug::{
    DebugCallOrigin, DebugCallRequest, DebugCallResult, DebugChannelId,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Default)]
pub(super) struct NotificationPolicy {
    ignored: HashSet<String>,
    unknown: bool,
    pub can_query: bool,
    groups: HashMap<String, GroupState>,
}
struct GroupState {
    muted: Option<bool>,
    checked: Instant,
    pending: bool,
}
impl NotificationPolicy {
    pub fn configure(&mut self, preference: &ChatAccountPreference, can_query: bool) {
        self.ignored = preference
            .ignored_groups
            .iter()
            .chain(&preference.hidden_groups)
            .cloned()
            .collect();
        self.unknown = preference.notify_unknown_groups;
        self.can_query = can_query;
    }
    pub fn allows(&self, group: &str) -> bool {
        !self.ignored.contains(group)
            && self
                .groups
                .get(group)
                .and_then(|state| state.muted)
                .map_or(self.unknown, |muted| !muted)
    }
    pub fn unread(&self, archive: &ChatArchive) -> u32 {
        archive
            .conversations
            .iter()
            .filter(|conversation| {
                conversation.kind == ChatArchiveConversationKind::Private
                    || self.allows(&conversation.id)
            })
            .fold(0u32, |total, conversation| {
                total.saturating_add(conversation.unread)
            })
    }
    pub fn states(&self) -> Vec<ChatGroupNotification> {
        self.groups
            .iter()
            .map(|(group, state)| ChatGroupNotification {
                group_id: group.clone(),
                qq_muted: state.muted,
            })
            .collect()
    }
    pub fn checked_at(&self, group: &str) -> Option<Instant> {
        self.groups.get(group).map(|state| state.checked)
    }
    pub fn request(&mut self, group: &str) -> bool {
        if self.ignored.contains(group) || !self.can_query {
            return false;
        }
        let now = Instant::now();
        if let Some(state) = self.groups.get(group) {
            let ttl = if state.muted.is_some() {
                Duration::from_secs(300)
            } else {
                Duration::from_secs(60)
            };
            if now.duration_since(state.checked) < ttl || state.pending {
                return false;
            }
        }
        if self.groups.len() >= 1000 && !self.groups.contains_key(group) {
            return false;
        }
        self.groups
            .entry(group.into())
            .and_modify(|state| state.pending = true)
            .or_insert(GroupState {
                muted: None,
                checked: now,
                pending: true,
            });
        true
    }
    pub fn finish(&mut self, group: String, muted: Option<bool>) {
        let previous = self.groups.get(&group).and_then(|state| state.muted);
        self.groups.insert(
            group,
            GroupState {
                muted: muted.or(previous),
                checked: Instant::now(),
                pending: false,
            },
        );
    }
}

pub(super) fn qq_muted(data: &Value) -> Option<bool> {
    // QQ GroupMsgMask: 1 接收并提醒，2 群助手，3 屏蔽，4 接收但不提醒。
    match data.get("cmdUinMsgMask").and_then(Value::as_u64) {
        Some(1) => Some(false),
        Some(2..=4) => Some(true),
        _ => None,
    }
}

impl ChatManager {
    pub(super) async fn run_notification_sync(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = self.desktop.stop.cancelled() => return,
                _ = tick.tick() => {},
            }
            if !self.is_enabled() {
                continue;
            }
            let mut requests = tokio::task::JoinSet::new();
            for (key, group_id) in self.desktop.inbox.notification_queries() {
                let transport = Arc::clone(&self.transport);
                requests.spawn(async move {
                    let response = transport
                        .call(DebugCallRequest {
                            request_id: uuid::Uuid::new_v4().to_string(),
                            bot_id: key.0.clone(),
                            channel: DebugChannelId::Auto,
                            action: "get_group_detail_info".into(),
                            params: serde_json::json!({"group_id": group_id}),
                            timeout_ms: Some(6000),
                            origin: DebugCallOrigin::Other,
                        })
                        .await;
                    let muted = match response.result {
                        DebugCallResult::Ok { outcome } if outcome.ok && !outcome.truncated => {
                            qq_muted(&outcome.data)
                        }
                        _ => None,
                    };
                    (key, group_id, muted)
                });
            }
            while !requests.is_empty() {
                tokio::select! {
                    _ = self.desktop.stop.cancelled() => return,
                    result = requests.join_next() => {
                        if let Some(Ok((key, group, muted))) = result { self.desktop.inbox.notification_result(&key, group, muted); }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_recognized_qq_settings_are_treated_as_synced() {
        for mask in [2, 3, 4] {
            assert_eq!(
                qq_muted(&serde_json::json!({"cmdUinMsgMask": mask})),
                Some(true)
            );
        }
        assert_eq!(
            qq_muted(&serde_json::json!({"cmdUinMsgMask": 1})),
            Some(false)
        );
        assert_eq!(qq_muted(&serde_json::json!({"group_all_shut": -1})), None);
        assert_eq!(qq_muted(&serde_json::json!({"cmdUinMsgMask": 9})), None);
    }
    #[test]
    fn local_ignore_overrides_qq_and_unknown_groups_require_explicit_opt_in() {
        let mut policy = NotificationPolicy::default();
        assert!(!policy.allows("123"));
        policy.finish("123".into(), Some(false));
        assert!(policy.allows("123"));
        policy.configure(
            &ChatAccountPreference {
                ignored_groups: vec!["123".into()],
                notify_unknown_groups: true,
                ..Default::default()
            },
            true,
        );
        assert!(!policy.allows("123"));
        assert!(policy.allows("456"));
        policy.finish("456".into(), Some(true));
        assert!(!policy.allows("456"));
        policy.finish("456".into(), None);
        assert!(!policy.allows("456"));
    }
}
