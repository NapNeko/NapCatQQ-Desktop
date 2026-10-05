//! 账号聊天偏好和后台接收租约，不依赖窗口或 Tauri。
use std::{collections::HashMap, path::PathBuf, sync::{Arc, Mutex}};
use ncd_domain::chat_desktop::*;
use ncd_domain::onebot_debug::{DebugChannelId, DebugReceiverState};
use ncd_traits::ConfigStore;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use tokio_util::sync::CancellationToken;
use crate::{DebugEventSink, EventBus, events::EventFilter};
use super::{ChatManager, ChatSink, inbox::{Identity, Inbox, InboxSink}};

#[derive(Default, Serialize, Deserialize)]
struct Preferences { #[serde(default)] accounts: Vec<ChatAccountPreference> }
#[derive(Default)]
struct ViewState { snapshot: ChatViewState, owner: String }
impl ViewState {
    fn claim(&mut self, owner: &str) -> ChatViewState {
        self.owner = owner.into();
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
        self.snapshot.clone()
    }
    fn accepts(&self, owner: &str, revision: u32) -> bool { self.owner == owner && self.snapshot.revision == revision }
}
pub(super) struct DesktopState {
    root: PathBuf,
    preferences: RwLock<Preferences>,
    pub inbox: Arc<Inbox>,
    pub lease_gate: AsyncMutex<()>,
    background: AsyncMutex<HashMap<Identity, String>>,
    pub viewers: AsyncMutex<HashMap<String, (Identity, String, std::time::Instant)>>,
    view: Mutex<ViewState>,
    pub(super) stop: CancellationToken,
    preference_error: Mutex<Option<String>>,
    preference_gate: AsyncMutex<()>,
}
impl DesktopState {
    pub fn new(root: PathBuf, inbox: Arc<Inbox>) -> Self {
        Self { root, preferences: RwLock::new(Preferences::default()), inbox, lease_gate: AsyncMutex::new(()), background: AsyncMutex::new(HashMap::new()), viewers: AsyncMutex::new(HashMap::new()), view: Mutex::new(ViewState { snapshot: ChatViewState { v: 1, ..Default::default() }, ..Default::default() }), stop: CancellationToken::new(), preference_error: Mutex::new(None), preference_gate: AsyncMutex::new(()) }
    }
}

impl ChatManager {
    /// Commit imported preferences before updating the cache and background subscriptions.
    pub async fn replace_preferences_with<R>(
        &self,
        preferences: Option<Vec<ChatAccountPreference>>,
        write: impl std::future::Future<Output = Result<R, String>>,
    ) -> Result<R, String> {
        let Some(preferences) = preferences else {
            return write.await;
        };
        let result = {
            let _gate = self.desktop.preference_gate.lock().await;
            let mut cache = self.desktop.preferences.write().await;
            let result = write.await?;
            cache.accounts = preferences;
            *self
                .desktop
                .preference_error
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = None;
            result
        };
        self.reconcile_background().await;
        self.desktop.inbox.changed.notify_one();
        Ok(result)
    }

    pub async fn desktop_status(&self) -> ChatDesktopStatus {
        let targets = self.targets().await;
        let preferences = self.desktop.preferences.read().await;
        ChatDesktopStatus { v: 1, accounts: targets.into_iter().map(|target| {
            let self_id = target.qq_id.to_string();
            let preference = preferences.accounts.iter().find(|p| p.bot_id == target.bot_id && p.self_id == self_id).cloned()
                .unwrap_or_else(|| ChatAccountPreference { bot_id: target.bot_id.clone(), self_id: self_id.clone(), ..Default::default() });
            let (unread, connection) = self.desktop.inbox.summary(&(target.bot_id.clone(), self_id));
            let (notification_unread, groups) = self.desktop.inbox.notification_summary(&(target.bot_id.clone(), preference.self_id.clone()));
            let error = self.desktop.preference_error.lock().unwrap_or_else(|p| p.into_inner()).clone();
            ChatAccountStatus { target, preference, unread, notification_unread, groups, connection, error }
        }).collect() }
    }
    pub async fn set_preference(&self, preference: ChatAccountPreference) -> Result<(), String> {
        let _gate = self.desktop.preference_gate.lock().await;
        self.save_preference(preference).await
    }
    pub async fn tray_snapshot(&self, bot_id: &str, self_id: &str) -> Result<ChatTraySnapshot, String> {
        let account = self.desktop_status().await.accounts.into_iter()
            .find(|row| row.target.bot_id == bot_id && row.preference.self_id == self_id
                && row.preference.enabled && row.preference.tray)
            .ok_or("此账号托盘已关闭")?;
        let mut conversations = self.desktop.inbox.tray_conversations(&(bot_id.into(), self_id.into())).await?;
        let conversation_count = conversations.len();
        conversations.truncate(8);
        Ok(ChatTraySnapshot { v: 1, account, conversations, conversation_count })
    }
    pub async fn update_tray_preference(&self, bot_id: &str, self_id: &str, background: Option<bool>, tray: Option<bool>) -> Result<(), String> {
        let _gate = self.desktop.preference_gate.lock().await;
        let mut preference = self.desktop.preferences.read().await.accounts.iter()
            .find(|p| p.bot_id == bot_id && p.self_id == self_id && p.enabled && p.tray)
            .cloned().ok_or("此账号托盘已关闭")?;
        // 只修改本次操作的字段，保留其他窗口刚保存的免打扰设置。
        if let Some(background) = background { preference.background = background; }
        if let Some(tray) = tray { preference.tray = tray; }
        self.save_preference(preference).await
    }
    pub async fn set_group_ignored(&self, bot_id: String, self_id: String, group_id: String, ignored: bool, hidden: bool) -> Result<(), String> {
        let _gate = self.desktop.preference_gate.lock().await;
        let mut preference = self.desktop.preferences.read().await.accounts.iter().find(|p| p.bot_id == bot_id && p.self_id == self_id).cloned()
            .unwrap_or_else(|| ChatAccountPreference { bot_id, self_id, ..Default::default() });
        let groups = if hidden { &mut preference.hidden_groups } else { &mut preference.ignored_groups };
        groups.retain(|id| id != &group_id);
        if ignored { groups.push(group_id); }
        self.save_preference(preference).await
    }
    pub async fn merge_hidden_groups(&self, bot_id: String, self_id: String, groups: Vec<String>) -> Result<(), String> {
        let _gate = self.desktop.preference_gate.lock().await;
        let mut preference = self.desktop.preferences.read().await.accounts.iter().find(|p| p.bot_id == bot_id && p.self_id == self_id).cloned()
            .unwrap_or_else(|| ChatAccountPreference { bot_id, self_id, ..Default::default() });
        preference.hidden_groups.extend(groups);
        preference.hidden_groups.sort(); preference.hidden_groups.dedup();
        self.save_preference(preference).await
    }
    async fn save_preference(&self, preference: ChatAccountPreference) -> Result<(), String> {
        if let Some(error) = self.desktop.preference_error.lock().unwrap_or_else(|p| p.into_inner()).clone() { return Err(error); }
        preference.validate().map_err(str::to_owned)?;
        let targets = self.targets().await;
        super::archive_identity(&targets, &preference.bot_id, &preference.self_id)?;
        let mut prefs = self.desktop.preferences.write().await;
        let mut next = prefs.accounts.clone();
        next.retain(|p| targets.iter().any(|t| t.bot_id == p.bot_id && t.qq_id.to_string() == p.self_id));
        next.retain(|p| p.bot_id != preference.bot_id || p.self_id != preference.self_id);
        next.push(preference);
        if next.len() > 64 || next.iter().filter(|p| p.enabled && p.background).count() > 8 {
            return Err("最多可同时后台接收 8 个聊天账号".into());
        }
        let root = self.desktop.root.clone(); let accounts = next.clone();
        tokio::task::spawn_blocking(move || {
            let value = serde_json::to_value(Preferences { accounts }).map_err(|e| e.to_string())?;
            ncd_config::store::LocalConfigStore::new(&root).write_json_atomic(&root.join("config/chat-desktop.json"), &value).map_err(|e| e.to_string())
        }).await.map_err(|e| e.to_string())??;
        prefs.accounts = next;
        drop(prefs);
        self.reconcile_background().await;
        self.desktop.inbox.changed.notify_one();
        Ok(())
    }
    pub fn view(&self) -> ChatViewState { self.desktop.view.lock().unwrap_or_else(|p| p.into_inner()).snapshot.clone() }
    pub fn claim_view(&self, owner: &str) -> ChatViewState { self.desktop.view.lock().unwrap_or_else(|p| p.into_inner()).claim(owner) }
    pub async fn set_view(&self, owner: &str, view: ChatViewState) -> Result<(), String> {
        if !self.desktop.view.lock().unwrap_or_else(|p| p.into_inner()).accepts(owner, view.revision) { return Ok(()); }
        view.validate().map_err(str::to_owned)?;
        let targets = self.targets().await;
        for account in &view.accounts { super::archive_identity(&targets, &account.bot_id, &account.self_id)?; }
        let mut state = self.desktop.view.lock().unwrap_or_else(|p| p.into_inner());
        if state.accepts(owner, view.revision) { state.snapshot = view; }
        Ok(())
    }
    pub fn select_view_bot(&self, bot_id: String) {
        self.desktop.view.lock().unwrap_or_else(|p| p.into_inner()).snapshot.selected_bot = Some(bot_id);
        self.notify_changed();
    }
    pub async fn set_reading(&self, page: &str, bot_id: String, self_id: String, session: Option<String>) -> Result<(), String> {
        self.archive_identity(&bot_id, &self_id).await?;
        if session.as_ref().is_some_and(|s| !(s.starts_with("group:") || s.starts_with("private:")) || !s.split(':').nth(1).is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))) {
            return Err("聊天会话无效".into());
        }
        self.desktop.inbox.reading(page, (bot_id, self_id), session);
        Ok(())
    }
    pub fn clear_reading(&self, page: &str) { self.desktop.inbox.clear_page(page); }
    pub async fn mark_read(&self, bot_id: String, self_id: String, session: String) -> Result<(), String> {
        let _lease = self.desktop.lease_gate.lock().await;
        self.archive_identity(&bot_id, &self_id).await?;
        let key = (bot_id, self_id); self.desktop.inbox.ensure(&key).await?;
        self.desktop.inbox.mark_read(&key, &session); Ok(())
    }
    pub async fn release_page(&self, page: &str) {
        self.clear_reading(page);
        self.release_page_before(page, std::time::Instant::now()).await;
    }
    pub async fn release_page_before(&self, page: &str, before: std::time::Instant) {
        // 等正在建立的订阅注册完，再按来源和开始时间回收，避免销毁后留下迟到租约。
        let _lease = self.desktop.lease_gate.lock().await;
        let ids: Vec<_> = self.desktop.viewers.lock().await.iter().filter(|(_, (_, owner, opened))| owner == page && *opened < before).map(|(id, _)| id.clone()).collect();
        for id in ids { self.transport.unsubscribe(&id).await; self.remove_viewer(&id).await; }
        for key in self.desktop.inbox.keys() { self.release_unused(&key).await; }
    }
    pub async fn release_account(&self, bot_id: &str, self_id: &str) {
        let _lease = self.desktop.lease_gate.lock().await;
        let key = (bot_id.to_owned(), self_id.to_owned());
        self.release_unused(&key).await;
    }
    async fn release_unused(&self, key: &Identity) {
        if !self.desktop.background.lock().await.contains_key(key) && !self.desktop.viewers.lock().await.values().any(|(identity, _, _)| identity == key) {
            self.transport.release_session(&key.0).await;
            if let Err(e) = self.desktop.inbox.release(key).await { tracing::warn!("chat cache release: {e}"); }
        }
    }
    pub async fn flush(&self) -> Result<(), String> { self.desktop.inbox.flush().await }
    pub async fn changed(&self) { self.desktop.inbox.changed.notified().await; }
    pub fn notify_changed(&self) { self.desktop.inbox.changed.notify_one(); }

    async fn load_preferences(&self) -> Result<(), String> {
        let _gate = self.desktop.preference_gate.lock().await;
        let result = self.load_preferences_locked().await;
        *self
            .desktop
            .preference_error
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = result.as_ref().err().cloned();
        result
    }
    async fn load_preferences_locked(&self) -> Result<(), String> {
        let path = self.desktop.root.join("config/chat-desktop.json");
        let preferences = tokio::task::spawn_blocking(move || -> Result<Preferences, String> {
            match std::fs::read(&path) {
                Ok(bytes) if bytes.len() <= 128 * 1024 => serde_json::from_slice(&bytes).map_err(|e| format!("聊天偏好损坏: {e}")),
                Ok(_) => Err("聊天偏好文件过大".into()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
                Err(e) => Err(e.to_string()),
            }
        }).await.map_err(|e| e.to_string())??;
        if preferences.accounts.len() > 64 || preferences.accounts.iter().filter(|p| p.enabled && p.background).count() > 8 { return Err("聊天账号数量超出限制".into()); }
        for p in &preferences.accounts { p.validate().map_err(str::to_owned)?; }
        *self.desktop.preferences.write().await = preferences;
        Ok(())
    }
    pub(super) async fn reconcile_background(&self) {
        let _lease = self.desktop.lease_gate.lock().await;
        let targets = self.targets().await;
        let preferences = self.desktop.preferences.read().await.accounts.clone();
        for target in &targets {
            let key = (target.bot_id.clone(), target.qq_id.to_string());
            let preference = preferences.iter().find(|p| p.bot_id == key.0 && p.self_id == key.1).cloned()
                .unwrap_or_else(|| ChatAccountPreference { bot_id: key.0.clone(), self_id: key.1.clone(), ..Default::default() });
            self.desktop.inbox.configure_notifications(key, &preference, target.backend == ncd_domain::BackendType::NapCat);
        }
        let wanted: Vec<_> = targets.iter().filter(|t| t.running && t.online != Some(false) && preferences.iter().any(|p| p.bot_id == t.bot_id && p.self_id == t.qq_id.to_string() && p.enabled && p.background)).map(|t| (t.bot_id.clone(), t.qq_id.to_string())).collect();
        let mut background = self.desktop.background.lock().await;
        let remove: Vec<_> = background.keys().filter(|key| !wanted.contains(key)).cloned().collect();
        for key in remove {
            if let Some(id) = background.remove(&key) { self.transport.unsubscribe(&id).await; }
            if !self.desktop.viewers.lock().await.values().any(|(identity, _, _)| identity == &key) {
                self.transport.release_session(&key.0).await;
                if let Err(e) = self.desktop.inbox.release(&key).await { tracing::warn!("chat inbox release: {e}"); }
            }
        }
        for key in wanted {
            // 停止后旧订阅不再活着，恢复时依据接收器状态重新取得租约。
            let (_, state) = self.desktop.inbox.summary(&key);
            if background.contains_key(&key) && !matches!(state, DebugReceiverState::Stopped { .. }) { continue; }
            if let Some(id) = background.remove(&key) { self.transport.unsubscribe(&id).await; }
            if let Err(e) = self.desktop.inbox.load(&key).await { tracing::warn!("chat inbox load: {e}"); continue; }
            let sink = Arc::new(ChatSink(Arc::new(InboxSink { inbox: Arc::clone(&self.desktop.inbox), key: key.clone(), viewer: None }))) as Arc<dyn DebugEventSink>;
            match self.transport.subscribe(&key.0, DebugChannelId::Auto, sink).await {
                Ok(response) => { background.insert(key, response.subscription_id); }
                Err(e) => tracing::warn!("chat background: {}", crate::onebot_debug::error_text(&e)),
            }
        }
        for preference in preferences.iter().filter(|p| p.enabled && p.tray) {
            if targets.iter().any(|t| t.bot_id == preference.bot_id && t.qq_id.to_string() == preference.self_id)
                && let Err(e) = self.desktop.inbox.load_summary(&(preference.bot_id.clone(), preference.self_id.clone())).await { tracing::warn!("chat unread summary: {e}"); }
        }
        self.desktop.inbox.changed.notify_one();
    }
    pub(super) async fn run_desktop(self: Arc<Self>, bus: Arc<dyn EventBus>) {
        let mut events = bus.subscribe(EventFilter::all());
        if let Err(e) = self.load_preferences().await {
            tracing::warn!("{e}");
        }
        self.reconcile_background().await;
        let mut flush_tick = tokio::time::interval(std::time::Duration::from_secs(2));
        let mut recovery_tick = tokio::time::interval(std::time::Duration::from_secs(15));
        recovery_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = self.desktop.stop.cancelled() => return,
                event = events.next() => {
                    let Some(event) = event else { return; };
                    if matches!(event, crate::events::DomainEvent::BotStateChanged { .. } | crate::events::DomainEvent::SnowLumaLoginStateChanged { .. } | crate::events::DomainEvent::SnowLumaUinDetected { .. }) { self.reconcile_background().await; }
                }
                _ = flush_tick.tick() => {
                    if let Err(e) = self.flush().await { tracing::warn!("chat inbox flush: {e}"); }
                }
                _ = recovery_tick.tick() => self.reconcile_background().await,
            }
        }
    }
    pub(super) async fn remove_viewer(&self, id: &str) {
        let removed = self.desktop.viewers.lock().await.remove(id);
        if let Some((key, _, _)) = removed {
            self.release_unused(&key).await;
        }
    }
    pub(super) async fn stop_desktop(&self) { self.desktop.stop.cancel(); if let Err(e) = self.flush().await { tracing::warn!("chat shutdown flush: {e}"); } }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBots(crate::onebot_debug::DebugBotView);

    #[async_trait::async_trait]
    impl crate::DebugBotPort for TestBots {
        async fn list_bots(&self) -> Vec<crate::onebot_debug::DebugBotView> {
            vec![self.0.clone()]
        }
        async fn bot(&self, id: &ncd_domain::BotId) -> Option<crate::onebot_debug::DebugBotView> {
            (id == &self.0.bot_id()).then(|| self.0.clone())
        }
        async fn napcat_webui(&self, _: &ncd_domain::BotId) -> Option<(u16, String)> {
            None
        }
        async fn snowluma_webui(&self, _: &ncd_domain::BotId) -> Result<(u16, String), String> {
            Err("not running".into())
        }
    }

    struct NoHosts;

    #[async_trait::async_trait]
    impl crate::host_resolver::HostResolver for NoHosts {
        async fn resolve(
            &self,
            _: &ncd_domain::RuntimeTarget,
        ) -> Result<Arc<dyn ncd_host::Host>, crate::host_resolver::HostResolveError> {
            Err("test has no running hosts".into())
        }
    }

    fn manager(root: &std::path::Path) -> ChatManager {
        let bot = crate::onebot_debug::DebugBotView {
            config: ncd_test_support::BotConfigBuilder::new()
                .qq_id(10001)
                .build(),
            snapshot: ncd_domain::bot_actor::BotActorSnapshot::new(ncd_domain::BotId::new("10001")),
            online: None,
        };
        ChatManager::new(
            Arc::new(TestBots(bot)),
            Arc::new(NoHosts),
            root.to_path_buf(),
        )
    }

    fn preference(group: &str) -> ChatAccountPreference {
        ChatAccountPreference {
            bot_id: "10001".into(),
            self_id: "10001".into(),
            ignored_groups: vec![group.into()],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn imported_preferences_survive_the_next_change_and_disk_reload() {
        let root = tempfile::tempdir().unwrap();
        let chat = manager(root.path());
        chat.set_preference(preference("111")).await.unwrap();
        let imported = vec![preference("222")];
        let transaction = ncd_traits::JsonTransaction::new().write(
            root.path().join("config/chat-desktop.json"),
            serde_json::json!({"accounts": imported}),
        );
        *chat.desktop.preference_error.lock().unwrap() = Some("old corrupt preferences".into());
        chat.replace_preferences_with(Some(imported), async {
            ncd_config::store::LocalConfigStore::new(root.path())
                .apply_transaction(transaction)
                .map_err(|e| e.to_string())
        })
        .await
        .unwrap();
        chat.set_group_ignored("10001".into(), "10001".into(), "333".into(), true, true)
            .await
            .unwrap();
        let reloaded = manager(root.path());
        reloaded.load_preferences().await.unwrap();
        let status = reloaded.desktop_status().await;
        assert_eq!(status.accounts[0].preference.ignored_groups, vec!["222"]);
        assert_eq!(status.accounts[0].preference.hidden_groups, vec!["333"]);
        assert_eq!(chat.desktop_status().await.accounts[0].error, None);
    }

    #[tokio::test]
    async fn failed_preferences_import_keeps_cached_preferences_and_error() {
        let root = tempfile::tempdir().unwrap();
        let chat = manager(root.path());
        chat.set_preference(preference("111")).await.unwrap();
        *chat.desktop.preference_error.lock().unwrap() = Some("existing error".into());
        let result = chat
            .replace_preferences_with(Some(vec![preference("222")]), async {
                Err::<(), _>("transaction failed".to_owned())
            })
            .await;
        assert_eq!(result.unwrap_err(), "transaction failed");
        let status = chat.desktop_status().await;
        assert_eq!(status.accounts[0].preference.ignored_groups, vec!["111"]);
        assert_eq!(status.accounts[0].error.as_deref(), Some("existing error"));
        let reloaded = manager(root.path());
        reloaded.load_preferences().await.unwrap();
        assert_eq!(
            reloaded.desktop_status().await.accounts[0]
                .preference
                .ignored_groups,
            vec!["111"]
        );
    }

    #[tokio::test]
    async fn absent_preferences_import_does_not_wait_for_preference_locks() {
        let root = tempfile::tempdir().unwrap();
        let chat = manager(root.path());
        let _gate = chat.desktop.preference_gate.lock().await;
        let _cache = chat.desktop.preferences.write().await;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            chat.replace_preferences_with(None, async { Ok::<_, String>(42) }),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn initial_preferences_load_waits_for_an_import_before_reading_files() {
        let root = tempfile::tempdir().unwrap();
        let chat = manager(root.path());
        let config = ncd_config::store::LocalConfigStore::new(root.path());
        let path = root.path().join("config/chat-desktop.json");
        config
            .write_json_atomic(&path, &serde_json::json!({"accounts": [preference("111")]}))
            .unwrap();
        let gate = chat.desktop.preference_gate.lock().await;
        let load = chat.load_preferences();
        tokio::pin!(load);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut load)
                .await
                .is_err()
        );
        config
            .write_json_atomic(&path, &serde_json::json!({"accounts": [preference("222")]}))
            .unwrap();
        drop(gate);
        load.await.unwrap();
        assert_eq!(
            chat.desktop_status().await.accounts[0]
                .preference
                .ignored_groups,
            vec!["222"]
        );
    }
    #[test]
    fn a_new_workspace_rejects_saves_from_the_previous_owner_and_mount() {
        let mut state = ViewState { snapshot: ChatViewState { v: 1, ..Default::default() }, ..Default::default() };
        let embedded = state.claim("main");
        assert!(state.accepts("main", embedded.revision));
        let detached = state.claim("chat-panel");
        assert!(!state.accepts("main", embedded.revision));
        assert!(!state.accepts("chat-panel", embedded.revision));
        assert!(state.accepts("chat-panel", detached.revision));
        let remounted = state.claim("chat-panel");
        assert!(!state.accepts("chat-panel", detached.revision));
        assert!(state.accepts("chat-panel", remounted.revision));
    }
}
