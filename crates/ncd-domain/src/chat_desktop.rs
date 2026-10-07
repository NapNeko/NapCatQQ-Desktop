//! 桌面聊天用途、后台状态与仅存内存的视图交接。
use crate::onebot_debug::{DebugReceiverState, DebugTarget};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatAccountPreference {
    pub bot_id: String,
    pub self_id: String,
    pub enabled: bool,
    pub background: bool,
    pub tray: bool,
    #[serde(default)]
    pub tray_notification: ChatTrayNotification,
    #[serde(default)]
    pub ignored_groups: Vec<String>,
    #[serde(default)]
    pub hidden_groups: Vec<String>,
    #[serde(default)]
    pub notify_unknown_groups: bool,
    #[serde(default)]
    pub prevent_recall: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub enum ChatTrayNotification {
    Flash,
    #[default]
    Badge,
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatGroupNotification {
    pub group_id: String,
    pub qq_muted: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatAccountStatus {
    pub target: DebugTarget,
    pub preference: ChatAccountPreference,
    pub unread: u32,
    pub notification_unread: u32,
    pub groups: Vec<ChatGroupNotification>,
    pub connection: DebugReceiverState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatDesktopStatus {
    pub v: u32,
    pub accounts: Vec<ChatAccountStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatTraySnapshot {
    pub v: u32,
    pub account: ChatAccountStatus,
    pub conversations: Vec<crate::chat_archive::ChatArchiveConversation>,
    pub conversation_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatTrayNavigation {
    pub v: u32,
    pub bot_id: String,
    pub self_id: String,
    pub conversation: crate::chat_archive::ChatArchiveConversation,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatViewState {
    pub v: u32,
    #[serde(default)]
    pub revision: u32,
    pub selected_bot: Option<String>,
    pub accounts: Vec<ChatAccountView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatAccountView {
    pub bot_id: String,
    pub self_id: String,
    pub active: Option<String>,
    pub drafts: BTreeMap<String, ChatDraft>,
    pub scroll: BTreeMap<String, f64>,
    #[serde(default)]
    pub reading: BTreeMap<String, ChatReadingPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatReadingPosition {
    pub message_key: String,
    pub message_id: Option<String>,
    pub offset: f64,
    pub at_bottom: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatDraft {
    pub text: String,
    pub attachments: Vec<ChatDraftAttachment>,
    pub reply: Option<ChatDraftReply>,
    #[serde(default)]
    pub mentions: Vec<ChatDraftMention>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub enum ChatDraftAttachment {
    Image {
        key: String,
        name: String,
        path: String,
        #[serde(rename = "subType", default)]
        sub_type: Option<u8>,
    },
    File {
        key: String,
        name: String,
        path: String,
    },
    Face {
        key: String,
        name: String,
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatDraftReply {
    pub id: String,
    pub name: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatDraftMention {
    pub qq: String,
    pub label: String,
}

impl ChatAccountPreference {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.bot_id.is_empty()
            || self.bot_id.len() > 128
            || self.self_id.len() > 20
            || !self.self_id.bytes().all(|b| b.is_ascii_digit())
            || !self.self_id.parse::<u64>().is_ok_and(|id| id > 0)
        {
            return Err("聊天账号身份无效");
        }
        if !self.enabled && (self.background || self.tray) {
            return Err("请先启用聊天用途");
        }
        if self.ignored_groups.len() > 1000
            || self.hidden_groups.len() > 1000
            || self
                .ignored_groups
                .iter()
                .chain(&self.hidden_groups)
                .any(|id| {
                    id.len() > 20
                        || !id.bytes().all(|b| b.is_ascii_digit())
                        || !id.parse::<u64>().is_ok_and(|id| id > 0)
                })
        {
            return Err("忽略的群号无效或过多");
        }
        Ok(())
    }
}

impl ChatViewState {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.v != 1 || self.accounts.len() > 8 {
            return Err("聊天视图交接格式无效");
        }
        if serde_json::to_vec(self)
            .map_err(|_| "聊天视图无法编码")?
            .len()
            > 16 * 1024 * 1024
        {
            return Err("草稿附件过大，暂时无法切换窗口");
        }
        for account in &self.accounts {
            if account.drafts.len() > 1000
                || account.scroll.len() > 1000
                || account.reading.len() > 1000
                || account
                    .scroll
                    .values()
                    .any(|value| !value.is_finite() || *value < 0.0)
                || account.reading.values().any(|value| {
                    !value.offset.is_finite()
                        || value.offset.abs() > 1_000_000.0
                        || value.message_key.len() > 2048
                        || value.message_id.as_ref().is_some_and(|id| id.len() > 128)
                })
            {
                return Err("聊天视图超出缓存限制");
            }
            for draft in account.drafts.values() {
                if draft.text.len() > 512 * 1024
                    || draft.attachments.len() > 32
                    || draft.mentions.len() > 1000
                {
                    return Err("聊天草稿过大");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_preferences_default_to_badge_and_keep_group_settings_distinct() {
        let old = r#"{"botId":"bot","selfId":"99","enabled":true,"background":true,"tray":true}"#;
        let mut preference: ChatAccountPreference = serde_json::from_str(old).unwrap();
        assert_eq!(preference.tray_notification, ChatTrayNotification::Badge);
        assert!(!preference.notify_unknown_groups);
        assert!(!preference.prevent_recall);
        preference.ignored_groups = vec!["123".into()];
        preference.hidden_groups = vec!["456".into()];
        preference.prevent_recall = true;
        assert!(preference.validate().is_ok());
        let restored: ChatAccountPreference =
            serde_json::from_str(&serde_json::to_string(&preference).unwrap()).unwrap();
        assert_eq!(restored, preference);
        preference.hidden_groups.push("../99".into());
        assert!(preference.validate().is_err());
    }
    #[test]
    fn background_and_tray_require_explicit_chat_enablement() {
        let mut preference = ChatAccountPreference {
            bot_id: "bot".into(),
            self_id: "99".into(),
            ..Default::default()
        };
        assert!(preference.validate().is_ok());
        preference.background = true;
        assert!(preference.validate().is_err());
        preference.enabled = true;
        assert!(preference.validate().is_ok());
    }
    #[test]
    fn view_rejects_nonfinite_reading_positions() {
        let view = ChatViewState {
            v: 1,
            revision: 0,
            selected_bot: None,
            accounts: vec![ChatAccountView {
                bot_id: "bot".into(),
                self_id: "99".into(),
                active: None,
                drafts: BTreeMap::new(),
                reading: BTreeMap::new(),
                scroll: BTreeMap::from([("private:1".into(), f64::INFINITY)]),
            }],
        };
        assert!(view.validate().is_err());
    }
    #[test]
    fn reading_positions_round_trip_and_accept_older_handoffs() {
        let old = r#"{"botId":"bot","selfId":"99","active":"private:1","drafts":{},"scroll":{}}"#;
        let mut account: ChatAccountView = serde_json::from_str(old).unwrap();
        assert!(account.reading.is_empty());
        account.reading.insert(
            "private:1".into(),
            ChatReadingPosition {
                message_key: "private:1/42".into(),
                message_id: Some("42".into()),
                offset: 36.0,
                at_bottom: false,
            },
        );
        let encoded = serde_json::to_string(&account).unwrap();
        let restored: ChatAccountView = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.reading["private:1"].offset, 36.0);
        let mut view = ChatViewState {
            v: 1,
            accounts: vec![restored],
            ..Default::default()
        };
        assert!(view.validate().is_ok());
        view.accounts[0]
            .reading
            .get_mut("private:1")
            .unwrap()
            .offset = f64::NAN;
        assert!(view.validate().is_err());
    }
}
