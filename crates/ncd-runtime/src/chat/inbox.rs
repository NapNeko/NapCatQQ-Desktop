//! 后台消息投影和单一档案写入；界面只是事件消费者。
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use ncd_domain::chat_archive::*;
use ncd_domain::chat_desktop::{ChatAccountPreference, ChatGroupNotification};
use ncd_domain::onebot_debug::{DebugEventBatch, DebugEventBody, DebugReceiverState};
use serde_json::Value;
use tokio::sync::{Mutex as AsyncMutex, Notify};
use crate::chat_archive::ChatArchiveStore;
use crate::DebugEventSink;
use super::notifications::NotificationPolicy;

pub(super) type Identity = (String, String);
pub(super) struct AccountInbox {
    pub archive: ChatArchive,
    pub seq: u64,
    pub dirty: bool,
    pub connection: DebugReceiverState,
    bytes: usize,
}
#[derive(Default)]
struct InboxState {
    accounts: HashMap<Identity, AccountInbox>,
    reading: HashMap<String, (Identity, String)>,
    summaries: HashMap<Identity, UnreadSummary>,
    policies: HashMap<Identity, NotificationPolicy>,
}
struct UnreadSummary { total: u32, private: u32, groups: Vec<(String, u32)> }
impl UnreadSummary {
    fn from_archive(archive: &ChatArchive) -> Self {
        Self::from_conversations(&archive.conversations)
    }
    fn from_conversations(conversations: &[ChatArchiveConversation]) -> Self {
        let total = conversations.iter().fold(0u32, |sum, c| sum.saturating_add(c.unread));
        let private = conversations.iter().filter(|c| c.kind == ChatArchiveConversationKind::Private).fold(0u32, |sum, c| sum.saturating_add(c.unread));
        let groups = conversations.iter().filter(|c| c.kind == ChatArchiveConversationKind::Group && c.unread > 0).map(|c| (c.id.clone(), c.unread)).collect();
        Self { total, private, groups }
    }
}
pub(super) struct Inbox {
    state: Mutex<InboxState>,
    io: AsyncMutex<()>,
    store: ChatArchiveStore,
    pub changed: Notify,
}

impl Inbox {
    pub fn new(store: ChatArchiveStore) -> Self {
        Self { state: Mutex::new(InboxState::default()), io: AsyncMutex::new(()), store, changed: Notify::new() }
    }
    fn state(&self) -> std::sync::MutexGuard<'_, InboxState> {
        self.state.lock().unwrap_or_else(|poison| poison.into_inner())
    }
    pub async fn load(&self, key: &Identity) -> Result<ChatArchive, String> {
        let _gate = self.io.lock().await;
        if let Some(account) = self.state().accounts.get(key) { return Ok(account.archive.clone()); }
        let store = self.store.clone();
        let identity = key.clone();
        let mut archive = tokio::task::spawn_blocking(move || store.load(&identity.0, &identity.1)).await
            .map_err(|e| e.to_string())??
            .unwrap_or_else(|| ChatArchive { v: 1, self_id: key.1.clone(), conversations: vec![], messages: vec![] });
        let bytes = trim(&mut archive);
        let mut state = self.state();
        if state.accounts.len() >= 9 { return Err("聊天缓存已满，请先关闭其他账号的后台接收".into()); }
        state.accounts.insert(key.clone(), AccountInbox { archive: archive.clone(), seq: 0, dirty: false, bytes, connection: DebugReceiverState::Stopped { reason: "尚未连接".into() } });
        Ok(archive)
    }
    pub async fn ensure(&self, key: &Identity) -> Result<(), String> {
        if self.state().accounts.contains_key(key) { return Ok(()); }
        self.load(key).await.map(|_| ())
    }
    pub async fn load_summary(&self, key: &Identity) -> Result<(), String> {
        let _gate = self.io.lock().await;
        {
            let state = self.state();
            if state.accounts.contains_key(key) || state.summaries.contains_key(key) { return Ok(()); }
        }
        let store = self.store.clone(); let identity = key.clone();
        let conversations = tokio::task::spawn_blocking(move || store.load_summary(&identity.0, &identity.1)).await.map_err(|e| e.to_string())??;
        let summary = UnreadSummary::from_conversations(&conversations);
        let mut state = self.state();
        if state.summaries.len() >= 128 { if let Some(old) = state.summaries.keys().next().cloned() { state.summaries.remove(&old); } }
        state.summaries.insert(key.clone(), summary);
        Ok(())
    }
    pub async fn merge(&self, key: &Identity, mut incoming: ChatArchive) -> Result<(), String> {
        crate::chat_archive::sanitize_archive(&mut incoming);
        incoming.validate_for(&key.1).map_err(|e| e.to_string())?;
        self.ensure(key).await?;
        {
            let mut state = self.state();
            if let Some(account) = state.accounts.get_mut(key) {
                merge_archive(&mut account.archive, incoming);
                account.bytes = trim(&mut account.archive);
                account.dirty = true;
            }
        }
        self.changed.notify_one();
        Ok(())
    }
    pub fn reading(&self, page: &str, key: Identity, session: Option<String>) {
        let mut state = self.state();
        state.reading.remove(page);
        if let Some(session) = session {
            if let Some(account) = state.accounts.get_mut(&key)
                && let Some(conversation) = account.archive.conversations.iter_mut().find(|c| c.key == session) {
                if conversation.unread > 0 { conversation.unread = 0; account.dirty = true; }
            }
            state.reading.insert(page.into(), (key, session));
        }
        drop(state);
        self.changed.notify_one();
    }
    pub fn clear_page(&self, page: &str) { self.state().reading.remove(page); }
    pub fn mark_read(&self, key: &Identity, session: &str) {
        let mut state = self.state();
        if let Some(account) = state.accounts.get_mut(key)
            && let Some(c) = account.archive.conversations.iter_mut().find(|c| c.key == session) {
            c.unread = 0; account.dirty = true;
        }
        drop(state); self.changed.notify_one();
    }
    pub fn summary(&self, key: &Identity) -> (u32, DebugReceiverState) {
        let state = self.state();
        state.accounts.get(key).map(|a| (
            a.archive.conversations.iter().fold(0u32, |total, c| total.saturating_add(c.unread)), a.connection.clone(),
        )).unwrap_or((state.summaries.get(key).map_or(0, |s| s.total), DebugReceiverState::Stopped { reason: "未接收".into() }))
    }
    pub async fn tray_conversations(&self, key: &Identity) -> Result<Vec<ChatArchiveConversation>, String> {
        let conversations = self.state().accounts.get(key).map(|a| a.archive.conversations.clone());
        let conversations = match conversations {
            Some(rows) => rows,
            None => {
                let store = self.store.clone();
                let identity = key.clone();
                tokio::task::spawn_blocking(move || store.load_summary(&identity.0, &identity.1))
                    .await.map_err(|e| e.to_string())??
            }
        };
        let state = self.state();
        let fallback = NotificationPolicy::default();
        let policy = state.policies.get(key).unwrap_or(&fallback);
        Ok(tray_conversations(conversations, policy))
    }
    pub fn configure_notifications(&self, key: Identity, preference: &ChatAccountPreference, can_query: bool) {
        self.state().policies.entry(key).or_default().configure(preference, can_query);
    }
    pub fn notification_summary(&self, key: &Identity) -> (u32, Vec<ChatGroupNotification>) {
        let state = self.state();
        let fallback = NotificationPolicy::default();
        let policy = state.policies.get(key).unwrap_or(&fallback);
        let count = if let Some(account) = state.accounts.get(key) { policy.unread(&account.archive) }
            else { state.summaries.get(key).map_or(0, |summary| summary.groups.iter().filter(|(id, _)| policy.allows(id)).fold(summary.private, |sum, (_, count)| sum.saturating_add(*count))) };
        (count, policy.states())
    }
    pub fn notification_queries(&self) -> Vec<(Identity, String)> {
        let mut state = self.state();
        let mut groups: Vec<_> = state.accounts.iter().filter(|(_, a)| matches!(a.connection, DebugReceiverState::Connected)).flat_map(|(key, a)| a.archive.conversations.iter().filter(|c| c.kind == ChatArchiveConversationKind::Group && c.unread > 0).map(|c| (key.clone(), c.id.clone()))).collect();
        groups.extend(state.reading.values().filter_map(|(key, session)| session.strip_prefix("group:").map(|id| (key.clone(), id.into()))));
        // 冷账号只剩未读汇总：未读群同样要解析 QQ 免打扰，否则角标一直把持久化未读压掉
        groups.extend(state.summaries.iter().filter(|entry| !state.accounts.contains_key(entry.0)).flat_map(|(key, summary)| summary.groups.iter().map(|(group, _)| (key.clone(), group.clone()))));
        groups.sort(); groups.dedup();
        // 先查尚未同步的群，再刷新最旧的设置，避免固定群号顺序饿死后面的群。
        groups.sort_by_key(|(key, group)| state.policies.get(key).and_then(|policy| policy.checked_at(group)));
        let mut queries = vec![];
        for (key, group) in groups {
            if queries.len() >= 4 { break; }
            if state.policies.get_mut(&key).is_some_and(|policy| policy.request(&group)) { queries.push((key, group)); }
        }
        queries
    }
    pub fn notification_result(&self, key: &Identity, group: String, muted: Option<bool>) {
        if let Some(policy) = self.state().policies.get_mut(key) { policy.finish(group, muted); }
        self.changed.notify_one();
    }
    pub fn accept(&self, key: &Identity, batch: &DebugEventBatch) {
        let mut state = self.state();
        let reading = state.reading.values().find(|(identity, _)| identity == key).map(|(_, session)| session.clone());
        let Some(account) = state.accounts.get_mut(key) else { return; };
        let mut changed = false;
        for event in &batch.events {
            if event.seq <= account.seq { continue; }
            account.seq = event.seq;
            match &event.body {
                DebugEventBody::Ob11 { payload } => {
                    if string_id(&payload["self_id"]).is_some_and(|id| id != key.1) { continue; }
                    let before = account.archive.messages.len();
                    changed |= ingest(&mut account.archive, payload, reading.as_deref());
                    if account.archive.messages.len() > before {
                        if let Some(message) = account.archive.messages.last() { account.bytes = account.bytes.saturating_add(message_bytes(message)); }
                        while (account.bytes > 2 * 1024 * 1024 || account.archive.messages.len() > CHAT_ARCHIVE_MAX_MESSAGES) && !account.archive.messages.is_empty() {
                            account.bytes = account.bytes.saturating_sub(message_bytes(&account.archive.messages[0]));
                            account.archive.messages.remove(0);
                        }
                    }
                }
                DebugEventBody::Receiver { state, .. } => { account.connection = state.clone(); changed = true; }
                _ => {},
            }
        }
        account.dirty |= changed;
        drop(state);
        if changed { self.changed.notify_one(); }
    }
    pub async fn flush(&self) -> Result<(), String> {
        let _gate = self.io.lock().await;
        let pending: Vec<_> = {
            let mut state = self.state();
            state.accounts.iter_mut().filter_map(|(key, a)| {
                if !a.dirty { return None; }
                a.dirty = false;
                Some((key.clone(), a.archive.clone()))
            }).collect()
        };
        let mut failure = None;
        for (key, archive) in pending {
            let store = self.store.clone(); let identity = key.clone();
            let result = tokio::task::spawn_blocking(move || store.save(&identity.0, &identity.1, archive)).await.map_err(|e| e.to_string()).and_then(|r| r);
            if let Err(error) = result {
                if let Some(account) = self.state().accounts.get_mut(&key) { account.dirty = true; }
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
    pub async fn release(&self, key: &Identity) -> Result<(), String> {
        self.flush().await?;
        let mut state = self.state();
        if !state.reading.values().any(|(identity, _)| identity == key)
            && let Some(account) = state.accounts.remove(key) {
            if state.summaries.len() >= 128 { if let Some(old) = state.summaries.keys().next().cloned() { state.summaries.remove(&old); } }
            state.summaries.insert(key.clone(), UnreadSummary::from_archive(&account.archive));
        }
        Ok(())
    }
    pub fn keys(&self) -> Vec<Identity> { self.state().accounts.keys().cloned().collect() }
}

fn tray_conversations(mut rows: Vec<ChatArchiveConversation>, policy: &NotificationPolicy) -> Vec<ChatArchiveConversation> {
    rows.retain(|c| c.unread > 0 && (c.kind == ChatArchiveConversationKind::Private || policy.allows(&c.id)));
    rows.sort_by(|a, b| b.last_at.cmp(&a.last_at).then_with(|| a.key.cmp(&b.key)));
    rows
}

pub(super) struct InboxSink {
    pub inbox: Arc<Inbox>,
    pub key: Identity,
    pub viewer: Option<Arc<dyn DebugEventSink>>,
}
impl DebugEventSink for InboxSink {
    fn send(&self, batch: &DebugEventBatch) -> bool {
        self.inbox.accept(&self.key, batch);
        self.viewer.as_ref().is_none_or(|viewer| viewer.send(batch))
    }
    fn opened_by(&self) -> Option<(&str, std::time::Instant)> { self.viewer.as_ref().and_then(|viewer| viewer.opened_by()) }
}

fn string_id(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned).or_else(|| value.as_i64().map(|n| n.to_string())).filter(|s| !s.is_empty())
}
fn segments(value: &Value) -> Vec<ChatArchiveSegment> {
    if let Some(text) = value.as_str() {
        let mut parts = vec![]; let mut remaining = text;
        while let Some(start) = remaining.find("[CQ:") {
            if start > 0 { parts.push(text_segment(&remaining[..start])); }
            let Some(end) = remaining[start..].find(']') else { parts.push(text_segment(&remaining[start..])); remaining = ""; break; };
            let code = &remaining[start + 4..start + end];
            let mut values = code.split(','); let kind = values.next().unwrap_or("");
            let data = values.filter_map(|entry| entry.split_once('=')).map(|(key, value)| (key.to_owned(), Value::String(unescape(value)))).collect();
            parts.push(ChatArchiveSegment { kind: kind.into(), data });
            remaining = &remaining[start + end + 1..];
        }
        if !remaining.is_empty() { parts.push(text_segment(remaining)); }
        return parts;
    }
    value.as_array().map(|rows| rows.iter().filter_map(|row| {
        Some(ChatArchiveSegment { kind: row["type"].as_str()?.into(), data: row["data"].as_object()?.iter().map(|(k, v)| (k.clone(), v.clone())).collect() })
    }).collect()).unwrap_or_default()
}
fn unescape(text: &str) -> String { text.replace("&#91;", "[").replace("&#93;", "]").replace("&#44;", ",").replace("&amp;", "&") }
fn text_segment(text: &str) -> ChatArchiveSegment { ChatArchiveSegment { kind: "text".into(), data: BTreeMap::from([("text".into(), Value::String(unescape(text)))]) } }
fn preview(parts: &[ChatArchiveSegment]) -> String {
    parts.iter().map(|p| if p.kind == "text" { p.data.get("text").and_then(Value::as_str).unwrap_or("") } else { match p.kind.as_str() { "image" => "[图片]", "record" => "[语音]", "file" => "[文件]", "video" => "[视频]", "face" => "[表情]", "reply" => "", _ => "[消息]" } }).collect::<String>().chars().take(256).collect()
}
pub(super) fn ingest(archive: &mut ChatArchive, row: &Value, reading: Option<&str>) -> bool {
    let notice = row["notice_type"].as_str().unwrap_or("");
    if matches!(notice, "group_recall" | "friend_recall") {
        let peer = string_id(&row[if notice == "group_recall" { "group_id" } else { "user_id" }]).unwrap_or_default();
        let session = format!("{}:{peer}", if notice == "group_recall" { "group" } else { "private" });
        let id = string_id(&row["message_id"]);
        for m in &mut archive.messages { if m.session == session && m.id == id { m.recalled = Some(true); } }
        return true;
    }
    let kind = match row["message_type"].as_str() { Some("group") => ChatArchiveConversationKind::Group, Some("private") => ChatArchiveConversationKind::Private, _ => return false };
    let sender_id = string_id(&row["sender"]["user_id"]).or_else(|| string_id(&row["user_id"])).unwrap_or_default();
    let mine = row["post_type"] == "message_sent" || sender_id == archive.self_id;
    let peer = if kind == ChatArchiveConversationKind::Group { string_id(&row["group_id"]) }
        else if mine { string_id(&row["target_id"]).or_else(|| string_id(&row["peer_id"])).or_else(|| string_id(&row["user_id"])) }
        else { string_id(&row["user_id"]).or_else(|| Some(sender_id.clone())) };
    let Some(peer) = peer.filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())) else { return false; };
    let session = format!("{}:{peer}", if kind == ChatArchiveConversationKind::Group { "group" } else { "private" });
    let id = string_id(&row["message_id"]);
    let at = row["time"].as_i64().unwrap_or_else(|| i64::try_from(crate::metrics::now_ms() / 1000).unwrap_or(i64::MAX)).saturating_mul(1000);
    let parts = segments(row.get("message").unwrap_or(&row["raw_message"]));
    let name = row["sender"]["card"].as_str().filter(|s| !s.is_empty()).or_else(|| row["sender"]["nickname"].as_str()).unwrap_or(&sender_id).chars().take(256).collect::<String>();
    let preview = preview(&parts);
    let key = id.as_ref().map(|id| format!("{session}/{id}")).unwrap_or_else(|| format!("{session}/event/{at}/{sender_id}/{preview}"));
    let file_id = if parts.len() == 1 && parts[0].kind == "file" { parts[0].data.get("file_id").and_then(string_id) } else { None };
    if let Some(existing) = archive.messages.iter_mut().find(|m| m.session == session && (id.is_some() && m.id == id || m.key == key || mine && file_id.is_some() && m.request_id.is_some() && m.file_id == file_id)) {
        if existing.request_id.is_some() {
            existing.id = id.or_else(|| existing.id.clone()); existing.status = ChatArchiveSendStatus::Sent; existing.error = None; existing.segments = parts;
            return true;
        }
        return false;
    }
    if !archive.conversations.iter().any(|c| c.key == session) {
        if archive.conversations.len() >= CHAT_ARCHIVE_MAX_CONVERSATIONS { return false; }
        archive.conversations.push(ChatArchiveConversation { key: session.clone(), kind, id: peer.clone(), name: row["group_name"].as_str().map(|s| s.chars().take(256).collect()).unwrap_or_else(|| if kind == ChatArchiveConversationKind::Private && !mine { name.clone() } else { peer }), members: None, unread: 0, pinned: false, last_at: 0, preview: String::new(), boxed: false });
    }
    if let Some(c) = archive.conversations.iter_mut().find(|c| c.key == session) {
        if !mine && reading != Some(session.as_str()) { c.unread = c.unread.saturating_add(1); }
        if at >= c.last_at { c.last_at = at; c.preview = preview; }
    }
    archive.messages.push(ChatArchiveMessage { key, session, id, file_id, sequence: string_id(&row["message_seq"]), request_id: None, sender_id, sender_name: name, at, mine, segments: parts, status: ChatArchiveSendStatus::Sent, error: None, recalled: None, notice: None });
    true
}
fn message_bytes(message: &ChatArchiveMessage) -> usize { serde_json::to_vec(message).map(|b| b.len()).unwrap_or(2 * 1024 * 1024) }
fn trim(archive: &mut ChatArchive) -> usize {
    for c in &mut archive.conversations { c.name = c.name.chars().take(256).collect(); c.preview = c.preview.chars().take(256).collect(); }
    archive.messages.sort_by_key(|m| m.at);
    if archive.messages.len() > CHAT_ARCHIVE_MAX_MESSAGES { archive.messages.drain(..archive.messages.len() - CHAT_ARCHIVE_MAX_MESSAGES); }
    let mut bytes: usize = archive.messages.iter().map(message_bytes).sum();
    while bytes > 2 * 1024 * 1024 && !archive.messages.is_empty() {
        bytes = bytes.saturating_sub(message_bytes(&archive.messages[0])); archive.messages.remove(0);
    }
    bytes
}
pub(super) fn merge_archive(current: &mut ChatArchive, incoming: ChatArchive) {
    for mut conversation in incoming.conversations {
        if let Some(existing) = current.conversations.iter_mut().find(|c| c.key == conversation.key) {
            conversation.unread = existing.unread;
            if existing.last_at > conversation.last_at { conversation.last_at = existing.last_at; conversation.preview.clone_from(&existing.preview); }
            *existing = conversation;
        } else if current.conversations.len() < CHAT_ARCHIVE_MAX_CONVERSATIONS { current.conversations.push(conversation); }
    }
    for mut message in incoming.messages {
        let matches = |m: &ChatArchiveMessage| m.session == message.session && (m.key == message.key || message.id.is_some() && m.id == message.id || message.request_id.is_some() && m.request_id == message.request_id || message.mine && m.mine && message.file_id.is_some() && m.file_id == message.file_id && message.request_id.is_some() != m.request_id.is_some());
        let previous = current.messages.iter().find(|m| matches(m));
        if let Some(previous) = previous {
            message.recalled = previous.recalled.or(message.recalled);
            if previous.status == ChatArchiveSendStatus::Sent && message.status == ChatArchiveSendStatus::Unknown { message.status = previous.status; message.id.clone_from(&previous.id); }
        }
        let matches = |m: &ChatArchiveMessage| m.session == message.session && (m.key == message.key || message.id.is_some() && m.id == message.id || message.request_id.is_some() && m.request_id == message.request_id || message.mine && m.mine && message.file_id.is_some() && m.file_id == message.file_id && message.request_id.is_some() != m.request_id.is_some());
        current.messages.retain(|m| !matches(m));
        current.messages.push(message);
    }
    trim(current);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn empty() -> ChatArchive { ChatArchive { v: 1, self_id: "99".into(), conversations: vec![], messages: vec![] } }
    fn message(id: u64) -> Value { json!({"post_type":"message","self_id":99,"message_type":"private","user_id":22,"sender":{"user_id":22,"nickname":"朋友"},"message_id":id,"time":100+id,"message":[{"type":"text","data":{"text":"后台消息"}}]}) }

    #[test]
    fn stale_view_cannot_erase_background_messages_or_unread() {
        let mut inbox = empty(); ingest(&mut inbox, &message(1), None);
        let mut stale = inbox.clone(); stale.conversations[0].unread = 0;
        ingest(&mut inbox, &message(2), None);
        merge_archive(&mut inbox, stale);
        assert_eq!(inbox.messages.len(), 2);
        assert_eq!(inbox.conversations[0].unread, 2);
    }
    #[test]
    fn background_and_viewer_replay_do_not_count_a_message_twice() {
        let mut inbox = empty();
        assert!(ingest(&mut inbox, &message(1), None));
        assert!(!ingest(&mut inbox, &message(1), None));
        assert_eq!(inbox.conversations[0].unread, 1);
    }
    #[test]
    fn reading_applies_only_to_the_selected_conversation() {
        let mut inbox = empty(); ingest(&mut inbox, &message(1), Some("private:22"));
        assert_eq!(inbox.conversations[0].unread, 0);
        ingest(&mut inbox, &message(2), Some("private:23"));
        assert_eq!(inbox.conversations[0].unread, 1);
    }
    #[test]
    fn recall_survives_an_older_view_snapshot() {
        let mut inbox = empty(); ingest(&mut inbox, &message(1), None);
        let stale = inbox.clone();
        ingest(&mut inbox, &json!({"notice_type":"friend_recall","user_id":22,"message_id":1}), None);
        merge_archive(&mut inbox, stale);
        assert_eq!(inbox.messages[0].recalled, Some(true));
    }
    #[test]
    fn cq_media_is_restored_as_segments_instead_of_literal_codes() {
        let parsed = segments(&json!("hi &#91;x&#93;[CQ:image,url=https://a.test/img?a=1&#44;2]"));
        assert_eq!(parsed[0].data["text"], "hi [x]");
        assert_eq!(parsed[1].kind, "image");
        assert_eq!(parsed[1].data["url"], "https://a.test/img?a=1,2");
    }
    #[test]
    fn upload_result_and_idless_file_echo_merge_once() {
        let mut inbox = empty();
        let mut row = message(1); row["post_type"] = json!("message_sent"); row["user_id"] = json!(99); row["sender"]["user_id"] = json!(99); row["target_id"] = json!(22); row.as_object_mut().unwrap().remove("message_id");
        row["message"] = json!([{"type":"file","data":{"file_id":"unique-upload","name":"test.txt"}}]);
        ingest(&mut inbox, &row, None);
        let mut view = inbox.clone(); view.messages[0].request_id = Some("request".into()); view.messages[0].key = "pending/request".into();
        merge_archive(&mut inbox, view);
        assert_eq!(inbox.messages.len(), 1);
        assert_eq!(inbox.messages[0].request_id.as_deref(), Some("request"));
    }
    #[tokio::test]
    async fn explicit_mark_read_survives_stale_archive_save_and_reopen() {
        let root = tempfile::tempdir().unwrap(); let store = ChatArchiveStore::new(root.path()); let inbox = Inbox::new(store);
        let key = ("bot".into(), "99".into());
        let mut archive = empty(); ingest(&mut archive, &message(1), None);
        inbox.merge(&key, archive.clone()).await.unwrap();
        inbox.mark_read(&key, "private:22");
        inbox.merge(&key, archive).await.unwrap();
        inbox.release(&key).await.unwrap();
        assert_eq!(inbox.load(&key).await.unwrap().conversations[0].unread, 0);
    }
    #[tokio::test]
    async fn unread_and_messages_survive_without_any_webview() {
        let root = tempfile::tempdir().unwrap(); let store = ChatArchiveStore::new(root.path());
        let inbox = Inbox::new(store.clone()); let key = ("bot".into(), "99".into());
        inbox.load(&key).await.unwrap();
        { let mut state = inbox.state(); let account = state.accounts.get_mut(&key).unwrap(); ingest(&mut account.archive, &message(1), None); account.dirty = true; }
        inbox.flush().await.unwrap(); inbox.release(&key).await.unwrap();
        assert!(inbox.keys().is_empty());
        assert_eq!(inbox.summary(&key).0, 1);
        let cold = Inbox::new(store.clone()); cold.load_summary(&key).await.unwrap();
        assert_eq!(cold.summary(&key).0, 1);
        assert!(cold.keys().is_empty());
        let restored = Inbox::new(store).load(&key).await.unwrap();
        assert_eq!(restored.messages.len(), 1);
        assert_eq!(restored.conversations[0].unread, 1);
    }
    #[tokio::test]
    async fn ignored_and_qq_muted_groups_keep_unread_without_triggering_tray_attention_after_release() {
        let root = tempfile::tempdir().unwrap();
        let inbox = Inbox::new(ChatArchiveStore::new(root.path()));
        let key = ("bot".into(), "99".into());
        let mut archive = empty(); ingest(&mut archive, &message(1), None);
        for group in [2, 3, 4] {
            ingest(&mut archive, &json!({"self_id":99,"post_type":"message","message_type":"group","group_id":group,"user_id":22,"message_id":group,"time":1,"message":"群消息","sender":{"nickname":"群成员"}}), None);
        }
        let mut preference = ChatAccountPreference { ignored_groups: vec!["3".into()], hidden_groups: vec!["4".into()], notify_unknown_groups: true, ..Default::default() };
        inbox.configure_notifications(key.clone(), &preference, true);
        inbox.merge(&key, archive).await.unwrap();
        inbox.notification_result(&key, "2".into(), Some(true));
        inbox.notification_result(&key, "3".into(), Some(false));
        assert_eq!(inbox.summary(&key).0, 4);
        assert_eq!(inbox.notification_summary(&key).0, 1);
        inbox.release(&key).await.unwrap();
        assert!(inbox.keys().is_empty());
        assert_eq!(inbox.notification_summary(&key).0, 1);
        preference.ignored_groups.clear(); preference.hidden_groups.clear();
        inbox.configure_notifications(key.clone(), &preference, true);
        assert_eq!(inbox.notification_summary(&key).0, 3);
        assert_eq!(inbox.summary(&key).0, 4);
    }
    #[tokio::test(start_paused = true)]
    async fn new_groups_are_queried_before_expired_settings_without_exceeding_concurrency() {
        let root = tempfile::tempdir().unwrap(); let inbox = Inbox::new(ChatArchiveStore::new(root.path()));
        let key = ("bot".into(), "99".into()); let mut archive = empty();
        for group in 1..=5 { ingest(&mut archive, &json!({"post_type":"message","message_type":"group","group_id":group,"user_id":22,"message_id":group,"time":1,"message":"群消息"}), None); }
        inbox.configure_notifications(key.clone(), &ChatAccountPreference::default(), true);
        inbox.merge(&key, archive).await.unwrap();
        inbox.state().accounts.get_mut(&key).unwrap().connection = DebugReceiverState::Connected;
        let first = inbox.notification_queries(); assert_eq!(first.len(), 4);
        for (identity, group) in first { inbox.notification_result(&identity, group, Some(false)); }
        tokio::time::advance(std::time::Duration::from_secs(301)).await;
        let next = inbox.notification_queries(); assert_eq!(next.len(), 4);
        assert_eq!(next[0].1, "5");
    }
    #[tokio::test(start_paused = true)]
    async fn cold_unread_groups_join_mute_resolution_queries() {
        let root = tempfile::tempdir().unwrap();
        let inbox = Inbox::new(ChatArchiveStore::new(root.path()));
        let key = ("bot".into(), "99".into());
        let mut archive = empty();
        ingest(&mut archive, &json!({"post_type":"message","message_type":"group","group_id":7,"user_id":22,"message_id":1,"time":1,"message":"群消息"}), None);
        inbox.merge(&key, archive).await.unwrap();
        inbox.release(&key).await.unwrap();
        assert!(inbox.keys().is_empty());
        inbox.configure_notifications(key.clone(), &ChatAccountPreference::default(), true);
        assert_eq!(inbox.notification_summary(&key).0, 0);
        let queries = inbox.notification_queries();
        assert_eq!(queries, vec![(key.clone(), "7".into())]);
        inbox.notification_result(&key, "7".into(), Some(false));
        assert_eq!(inbox.notification_summary(&key).0, 1);
        inbox.notification_result(&key, "7".into(), Some(true));
        assert_eq!(inbox.notification_summary(&key).0, 0);
    }
    #[tokio::test]
    async fn tray_preview_filters_muted_groups_orders_recent_first_and_never_marks_read() {
        let root = tempfile::tempdir().unwrap();
        let inbox = Inbox::new(ChatArchiveStore::new(root.path()));
        let key = ("bot".into(), "99".into());
        let mut archive = empty();
        ingest(&mut archive, &message(1), None);
        for group in [2, 3, 4] {
            ingest(&mut archive, &json!({"post_type":"message","message_type":"group","group_id":group,"user_id":22,"message_id":group,"time":group + 200,"message":"群消息"}), None);
        }
        let preference = ChatAccountPreference { ignored_groups: vec!["3".into()], notify_unknown_groups: true, ..Default::default() };
        inbox.configure_notifications(key.clone(), &preference, true);
        inbox.notification_result(&key, "4".into(), Some(true));
        inbox.merge(&key, archive).await.unwrap();
        let rows = inbox.tray_conversations(&key).await.unwrap();
        assert_eq!(rows.iter().map(|c| c.key.as_str()).collect::<Vec<_>>(), ["group:2", "private:22"]);
        assert_eq!(inbox.summary(&key).0, 4);
        inbox.release(&key).await.unwrap();
        let paused = inbox.tray_conversations(&key).await.unwrap();
        assert_eq!(paused, rows);
        assert!(inbox.keys().is_empty());
        assert_eq!(inbox.summary(&key).0, 4);
    }
}
