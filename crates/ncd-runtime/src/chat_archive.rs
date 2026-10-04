//! 按 Bot 与登录身份隔离聊天档案。

use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use ncd_domain::chat_archive::{ChatArchive, ChatArchiveConversation, CHAT_ARCHIVE_MAX_BYTES};
use ncd_config::store::LocalConfigStore;
use ncd_traits::ConfigStore;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub(crate) struct ChatArchiveStore { root: PathBuf, writer: LocalConfigStore }

impl ChatArchiveStore {
    pub(crate) fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), writer: LocalConfigStore::new(root) }
    }

    fn path(&self, bot_id: &str, self_id: &str) -> Result<PathBuf, String> {
        if bot_id.is_empty() || bot_id.len() > 128
            || !bot_id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            || self_id.len() > 20 || !self_id.bytes().all(|b| b.is_ascii_digit())
            || !self_id.parse::<u64>().is_ok_and(|id| id > 0)
        {
            return Err("聊天档案身份无效".into());
        }
        // 哈希 Bot 键避免 Windows 大小写与保留文件名产生档案碰撞。
        let bot = hex::encode(Sha256::digest(bot_id.as_bytes()));
        Ok(self.root.join("state/chat/archives").join(bot).join(format!("{self_id}.json")))
    }

    pub(crate) fn load(&self, bot_id: &str, self_id: &str) -> Result<Option<ChatArchive>, String> {
        let Some(bytes) = self.read_bytes(bot_id, self_id)? else { return Ok(None); };
        let archive: ChatArchive = serde_json::from_slice(&bytes)
            .map_err(|e| format!("聊天档案损坏，原文件已保留: {e}"))?;
        archive.validate_for(self_id).map_err(|e| e.to_string())?;
        Ok(Some(archive))
    }

    pub(crate) fn load_summary(&self, bot_id: &str, self_id: &str) -> Result<Vec<ChatArchiveConversation>, String> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Summary { v: u32, self_id: String, conversations: Vec<ChatArchiveConversation> }
        let Some(bytes) = self.read_bytes(bot_id, self_id)? else { return Ok(vec![]); };
        // serde 跳过消息体，托盘无需恢复整个消息缓存。
        let summary: Summary = serde_json::from_slice(&bytes).map_err(|e| format!("聊天档案损坏，原文件已保留: {e}"))?;
        let archive = ChatArchive { v: summary.v, self_id: summary.self_id, conversations: summary.conversations, messages: vec![] };
        archive.validate_for(self_id).map_err(|e| e.to_string())?;
        Ok(archive.conversations)
    }

    fn read_bytes(&self, bot_id: &str, self_id: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self.path(bot_id, self_id)?;
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("读取聊天档案失败: {error}")),
        };
        let mut bytes = Vec::new();
        file.take(CHAT_ARCHIVE_MAX_BYTES as u64 + 1).read_to_end(&mut bytes)
            .map_err(|e| format!("读取聊天档案失败: {e}"))?;
        if bytes.len() > CHAT_ARCHIVE_MAX_BYTES { return Err("聊天档案过大".into()); }
        Ok(Some(bytes))
    }

    pub(crate) fn save(&self, bot_id: &str, self_id: &str, mut archive: ChatArchive) -> Result<(), String> {
        let path = self.path(bot_id, self_id)?;
        sanitize_archive(&mut archive);
        archive.validate_for(self_id).map_err(|e| e.to_string())?;
        let payload = serde_json::to_value(&archive).map_err(|e| format!("序列化聊天档案失败: {e}"))?;
        if serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?.len() > CHAT_ARCHIVE_MAX_BYTES {
            return Err("聊天档案过大".into());
        }
        self.load(bot_id, self_id)?;
        self.writer.write_json_atomic(&path, &payload).map_err(|e| format!("保存聊天档案失败: {e}"))
    }
}

pub(crate) fn sanitize_archive(archive: &mut ChatArchive) {
        for message in &mut archive.messages {
            for segment in &mut message.segments {
                segment.data.retain(|key, value| {
                    if segment.kind == "text" && key == "text" { return true; }
                    if sensitive_field(key) || transient_value(value) { return false; }
                    sanitize_value(value);
                    true
                });
            }
        }
}

fn sensitive_field(key: &str) -> bool {
    matches!(key.to_ascii_lowercase().as_str(), "token" | "access_token" | "password" | "authorization" | "cookie" | "base64" | "path" | "local_path")
}

fn transient_value(value: &Value) -> bool {
    let Some(value) = value.as_str() else { return false; };
    let lower = value.to_ascii_lowercase();
    ["base64://", "data:", "file://", "ncd-local-file://", "blob:"].iter().any(|prefix| lower.starts_with(prefix))
        || value.starts_with(['/', '\\'])
        || (value.as_bytes().get(1) == Some(&b':') && value.as_bytes().first().is_some_and(|b| b.is_ascii_alphabetic()))
}

fn sanitize_value(value: &mut Value) {
    match value {
        Value::Object(values) => values.retain(|key, value| {
            if sensitive_field(key) || transient_value(value) { return false; }
            sanitize_value(value);
            true
        }),
        Value::Array(values) => {
            values.retain(|value| !transient_value(value));
            for value in values { sanitize_value(value); }
        },
        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> ChatArchive {
        serde_json::from_value(json!({
            "v":1, "selfId":"10001",
            "conversations":[{"key":"group:20001","type":"group","id":"20001","name":"群","unread":2,"pinned":true,"lastAt":1000,"preview":"消息","boxed":true}],
            "messages":[{"key":"group:20001/123","session":"group:20001","id":"123","senderId":"10002","senderName":"朋友","at":1000,"mine":false,"segments":[{"type":"text","data":{"text":"消息"}}],"status":"sent"}]
        })).unwrap()
    }

    #[test]
    fn restores_history_after_store_recreation() {
        let tmp = tempfile::tempdir().unwrap();
        ChatArchiveStore::new(tmp.path()).save("bot-1", "10001", snapshot()).unwrap();
        assert_eq!(ChatArchiveStore::new(tmp.path()).load("bot-1", "10001").unwrap(), Some(snapshot()));
    }

    #[test]
    fn keeps_bot_and_account_archives_separate() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ChatArchiveStore::new(tmp.path());
        store.save("bot-1", "10001", snapshot()).unwrap();
        assert!(store.load("bot-2", "10001").unwrap().is_none());
        assert!(store.load("bot-1", "10002").unwrap().is_none());
        assert_eq!(store.load("bot-1", "10001").unwrap(), Some(snapshot()));
    }

    #[test]
    fn invalid_save_preserves_last_valid_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ChatArchiveStore::new(tmp.path());
        store.save("bot-1", "10001", snapshot()).unwrap();
        let mut invalid = snapshot();
        invalid.v = 2;
        assert!(store.save("bot-1", "10001", invalid).is_err());
        assert_eq!(store.load("bot-1", "10001").unwrap(), Some(snapshot()));
    }

    #[test]
    fn strips_attachment_payloads_and_secrets_before_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ChatArchiveStore::new(tmp.path());
        let mut value = snapshot();
        value.messages[0].segments.push(serde_json::from_value(json!({
            "type":"image", "data":{"file":"base64://c2VjcmV0", "url":"data:image/png;base64,aGVsbG8=", "token":"secret", "metadata":{"authorization":"secret","path":"ncd-local-file://C:/secret.png"}, "summary":"图片"}
        })).unwrap());
        store.save("bot-1", "10001", value).unwrap();
        let loaded = store.load("bot-1", "10001").unwrap().unwrap();
        assert_eq!(loaded.messages[0].segments[0].data["text"], "消息");
        let data = &loaded.messages[0].segments[1].data;
        assert!(!data.contains_key("file"));
        assert!(!data.contains_key("url"));
        assert!(!data.contains_key("token"));
        assert_eq!(data["metadata"], json!({}));
        assert_eq!(data["summary"], "图片");
    }

    #[test]
    fn rejects_path_like_account_identifiers() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ChatArchiveStore::new(tmp.path());
        assert!(store.load("bot-1", "../10001").is_err());
        assert!(store.save("../bot-1", "10001", snapshot()).is_err());
    }

    #[test]
    fn reports_write_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("not-a-directory");
        std::fs::write(&root, b"file").unwrap();
        assert!(ChatArchiveStore::new(&root).save("bot-1", "10001", snapshot()).is_err());
    }

    #[test]
    fn corrupted_files_remain_visible_and_cannot_be_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ChatArchiveStore::new(tmp.path());
        store.save("bot-1", "10001", snapshot()).unwrap();
        let path = store.path("bot-1", "10001").unwrap();
        std::fs::write(&path, b"{broken").unwrap();
        assert!(store.load("bot-1", "10001").is_err());
        assert!(store.save("bot-1", "10001", snapshot()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
    }

    #[test]
    fn oversized_files_are_rejected_before_deserialization() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ChatArchiveStore::new(tmp.path());
        store.save("bot-1", "10001", snapshot()).unwrap();
        let path = store.path("bot-1", "10001").unwrap();
        std::fs::File::create(&path).unwrap().set_len(CHAT_ARCHIVE_MAX_BYTES as u64 + 1).unwrap();
        assert_eq!(store.load("bot-1", "10001").unwrap_err(), "聊天档案过大");
    }
}
