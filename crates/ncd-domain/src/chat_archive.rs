//! 聊天历史档案的持久化契约，不包含草稿、凭据或本地附件路径。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use ts_rs::TS;

pub const CHAT_ARCHIVE_VERSION: u32 = 1;
pub const CHAT_ARCHIVE_MAX_MESSAGES: usize = 5_000;
pub const CHAT_ARCHIVE_MAX_CONVERSATIONS: usize = 1_000;
pub const CHAT_ARCHIVE_MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ChatArchiveError {
    #[error("不支持的聊天档案版本: {0}")]
    UnsupportedVersion(u32),
    #[error("聊天档案账号不匹配")]
    IdentityMismatch,
    #[error("聊天档案会话数量过多")]
    TooManyConversations,
    #[error("聊天档案消息数量过多")]
    TooManyMessages,
    #[error("聊天档案包含无效会话")]
    InvalidConversation,
    #[error("聊天档案包含无效消息")]
    InvalidMessage,
    #[error("聊天档案过大")]
    TooLarge,
    #[error("序列化聊天档案失败: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatArchive {
    pub v: u32,
    pub self_id: String,
    pub conversations: Vec<ChatArchiveConversation>,
    pub messages: Vec<ChatArchiveMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatArchiveConversation {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: ChatArchiveConversationKind,
    pub id: String,
    pub name: String,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<u32>,
    pub unread: u32,
    pub pinned: bool,
    #[ts(type = "number")]
    pub last_at: i64,
    pub preview: String,
    pub boxed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub enum ChatArchiveConversationKind {
    Group,
    Private,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatArchiveMessage {
    pub key: String,
    pub session: String,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub sender_id: String,
    pub sender_name: String,
    #[ts(type = "number")]
    pub at: i64,
    pub mine: bool,
    pub segments: Vec<ChatArchiveSegment>,
    pub status: ChatArchiveSendStatus,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recalled: Option<bool>,
    // 拍一拍等系统提示行：无消息段，展示文案直接存这里。
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatArchiveSegment {
    #[serde(rename = "type")]
    pub kind: String,
    // OneBot 消息段字段由协议扩展，结构边界保留 type + data。
    #[ts(type = "Record<string, unknown>")]
    pub data: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub enum ChatArchiveSendStatus {
    Sending,
    Sent,
    Failed,
    Unknown,
}

impl ChatArchive {
    pub fn validate_for(&self, self_id: &str) -> Result<(), ChatArchiveError> {
        if self.v != CHAT_ARCHIVE_VERSION {
            return Err(ChatArchiveError::UnsupportedVersion(self.v));
        }
        if !valid_identity(self_id) || self.self_id != self_id {
            return Err(ChatArchiveError::IdentityMismatch);
        }
        if self.conversations.len() > CHAT_ARCHIVE_MAX_CONVERSATIONS {
            return Err(ChatArchiveError::TooManyConversations);
        }
        if self.messages.len() > CHAT_ARCHIVE_MAX_MESSAGES {
            return Err(ChatArchiveError::TooManyMessages);
        }
        let mut sessions = HashSet::with_capacity(self.conversations.len());
        for conversation in &self.conversations {
            let prefix = match conversation.kind {
                ChatArchiveConversationKind::Group => "group",
                ChatArchiveConversationKind::Private => "private",
            };
            if !valid_identity(&conversation.id)
                || conversation.key != format!("{prefix}:{}", conversation.id)
                || !sessions.insert(conversation.key.as_str())
                || conversation.name.len() > 4096
                || conversation.preview.len() > 8192
                || !valid_timestamp(conversation.last_at)
                || (conversation.boxed && conversation.kind != ChatArchiveConversationKind::Group)
            {
                return Err(ChatArchiveError::InvalidConversation);
            }
        }
        let mut messages = HashSet::with_capacity(self.messages.len());
        for message in &self.messages {
            if message.key.is_empty()
                || message.key.len() > 8192
                || !messages.insert(message.key.as_str())
                || !sessions.contains(message.session.as_str())
                || message.sender_id.len() > 128
                || message.sender_name.len() > 4096
                || !valid_timestamp(message.at)
                || [
                    &message.id,
                    &message.file_id,
                    &message.request_id,
                    &message.sequence,
                ]
                .iter()
                .any(|v| v.as_ref().is_some_and(|s| s.len() > 1024))
                || message.error.as_ref().is_some_and(|s| s.len() > 8192)
                || message.segments.len() > 256
                || message.segments.iter().any(|s| {
                    s.kind.is_empty()
                        || s.kind.len() > 64
                        || s.data.values().any(|v| !valid_json_depth(v, 0))
                })
            {
                return Err(ChatArchiveError::InvalidMessage);
            }
        }
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > CHAT_ARCHIVE_MAX_BYTES {
            return Err(ChatArchiveError::TooLarge);
        }
        Ok(())
    }
}

fn valid_identity(value: &str) -> bool {
    value.len() <= 20
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|id| id > 0)
}

fn valid_timestamp(value: i64) -> bool {
    (0..=9_007_199_254_740_991).contains(&value)
}

fn valid_json_depth(value: &serde_json::Value, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match value {
        serde_json::Value::Array(values) => values.iter().all(|v| valid_json_depth(v, depth + 1)),
        serde_json::Value::Object(values) => {
            values.values().all(|v| valid_json_depth(v, depth + 1))
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn archive() -> ChatArchive {
        ChatArchive {
            v: 1,
            self_id: "10001".into(),
            conversations: vec![],
            messages: vec![],
        }
    }

    #[test]
    fn validates_identity_and_version() {
        assert!(archive().validate_for("10001").is_ok());
        assert!(archive().validate_for("10002").is_err());
        let mut value = archive();
        value.v = 2;
        assert!(value.validate_for("10001").is_err());
    }

    fn populated() -> ChatArchive {
        serde_json::from_value(json!({
            "v":1, "selfId":"10001",
            "conversations":[{"key":"group:20001","type":"group","id":"20001","name":"群","unread":2,"pinned":true,"lastAt":1000,"preview":"消息","boxed":true}],
            "messages":[{"key":"group:20001/123","session":"group:20001","id":"123","senderId":"10002","senderName":"朋友","at":1000,"mine":false,"segments":[{"type":"text","data":{"text":"消息"}}],"status":"sent"}]
        })).unwrap()
    }

    #[test]
    fn accepts_real_message_keys_and_camel_case_fields() {
        let mut value = populated();
        value.messages[0].sequence = Some("9007199254740993".into());
        assert!(value.validate_for("10001").is_ok());
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json["conversations"][0]["type"], "group");
        assert_eq!(json["messages"][0]["senderId"], "10002");
        assert_eq!(json["messages"][0]["sequence"], "9007199254740993");
    }

    #[test]
    fn rejects_mismatched_conversation_and_orphan_messages() {
        let mut value = populated();
        value.conversations[0].id = "20002".into();
        assert!(value.validate_for("10001").is_err());
        value.conversations.clear();
        assert!(value.validate_for("10001").is_err());
    }

    #[test]
    fn rejects_duplicate_conversations_and_messages() {
        let mut value = populated();
        value.messages[0].key = "123".into();
        value.messages.push(value.messages[0].clone());
        assert!(value.validate_for("10001").is_err());
        value.messages.clear();
        value.conversations.push(value.conversations[0].clone());
        assert!(value.validate_for("10001").is_err());
    }

    #[test]
    fn rejects_negative_timestamps_and_private_box() {
        let mut value = populated();
        value.messages.clear();
        value.conversations[0].last_at = -1;
        assert!(value.validate_for("10001").is_err());
        value.conversations[0].last_at = 0;
        value.conversations[0].kind = ChatArchiveConversationKind::Private;
        value.conversations[0].key = "private:20001".into();
        assert!(value.validate_for("10001").is_err());
    }
}
