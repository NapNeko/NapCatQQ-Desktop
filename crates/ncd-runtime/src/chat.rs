//! 聊天独占协议会话，调试开关和停止操作不影响这里的接收器。
use crate::chat_archive::ChatArchiveStore;
use crate::host_resolver::HostResolver;
use crate::{DebugBotPort, DebugEventSink, DebugManager, DebugStreamSink, EventBus};
use ncd_domain::chat_archive::ChatArchive;
use ncd_domain::onebot_debug::{
    DebugCallRequest, DebugCallResponse, DebugCallResult, DebugChannelId, DebugError,
    DebugEventBatch, DebugStreamCallRequest, DebugSubscribeResponse, DebugTarget,
};
use std::{path::PathBuf, sync::Arc, time::Instant};
mod desktop;
pub mod group_files;
mod inbox;
mod moderation;
mod notifications;

pub struct ChatManager {
    transport: Arc<DebugManager>,
    desktop: desktop::DesktopState,
    files: group_files::FileState,
    screenshots: Arc<crate::chat_screenshots::ChatScreenshotCache>,
}

impl ChatManager {
    pub fn new(
        bots: Arc<dyn DebugBotPort>,
        hosts: Arc<dyn HostResolver>,
        data_root: PathBuf,
    ) -> Self {
        Self {
            screenshots: Arc::new(crate::chat_screenshots::ChatScreenshotCache::new(
                &data_root,
            )),
            desktop: desktop::DesktopState::new(
                data_root.clone(),
                Arc::new(inbox::Inbox::new(ChatArchiveStore::new(&data_root))),
            ),
            transport: Arc::new(DebugManager::new_ephemeral(bots, hosts, data_root)),
            files: group_files::FileState::default(),
        }
    }

    pub async fn targets(&self) -> Vec<DebugTarget> {
        self.transport.list_targets().await
    }

    pub fn is_enabled(&self) -> bool {
        self.transport.is_enabled()
    }

    pub fn screenshot_cache(&self) -> Arc<crate::chat_screenshots::ChatScreenshotCache> {
        Arc::clone(&self.screenshots)
    }

    async fn archive_identity(&self, bot_id: &str, self_id: &str) -> Result<(), String> {
        let targets = self.targets().await;
        archive_identity(&targets, bot_id, self_id)
    }

    pub async fn load_archive(
        &self,
        bot_id: String,
        self_id: String,
    ) -> Result<Option<ChatArchive>, String> {
        let _lease = self.desktop.lease_gate.lock().await;
        self.archive_identity(&bot_id, &self_id).await?;
        let key = (bot_id, self_id);
        let archive = self.desktop.inbox.load(&key).await?;
        if !self.is_enabled() {
            self.desktop.inbox.release(&key).await?;
        }
        Ok(Some(archive))
    }

    pub async fn save_archive(
        &self,
        bot_id: String,
        self_id: String,
        value: ChatArchive,
    ) -> Result<(), String> {
        let _lease = self.desktop.lease_gate.lock().await;
        self.archive_identity(&bot_id, &self_id).await?;
        let key = (bot_id, self_id);
        self.desktop.inbox.merge(&key, value).await?;
        if !self.is_enabled() {
            self.desktop.inbox.release(&key).await?;
        }
        Ok(())
    }

    pub async fn call(&self, mut request: DebugCallRequest) -> DebugCallResponse {
        if !self.is_enabled() {
            return DebugCallResponse {
                request_id: request.request_id,
                result: DebugCallResult::Err {
                    error: DebugError::FeatureDisabled,
                },
            };
        }
        if moderation::is_member_action(&request.action) {
            if let Err(error) = self.check_member_action(&mut request).await {
                return DebugCallResponse {
                    request_id: request.request_id,
                    result: DebugCallResult::Err { error },
                };
            }
            return self.transport.call(request).await;
        }
        if !chat_action(&request.action) {
            return rejected(request.request_id);
        }
        self.transport.call(request).await
    }

    pub async fn call_stream(
        &self,
        request: DebugStreamCallRequest,
        sink: Arc<dyn DebugStreamSink>,
    ) -> DebugCallResponse {
        if !chat_action(&request.action) {
            return rejected(request.request_id);
        }
        self.transport.call_stream(request, sink).await
    }

    pub async fn subscribe(
        &self,
        bot_id: &str,
        sink: Arc<dyn DebugEventSink>,
    ) -> Result<DebugSubscribeResponse, DebugError> {
        let _lease = self.desktop.lease_gate.lock().await;
        if !self.is_enabled() {
            return Err(DebugError::FeatureDisabled);
        }
        let target = self
            .targets()
            .await
            .into_iter()
            .find(|t| t.bot_id == bot_id)
            .ok_or(DebugError::BotNotRunning)?;
        let key = (bot_id.to_owned(), target.qq_id.to_string());
        self.desktop
            .inbox
            .load(&key)
            .await
            .map_err(|message| DebugError::Internal { message })?;
        let (page, opened_at) = sink
            .opened_by()
            .map(|(page, at)| (page.to_owned(), at))
            .unwrap_or_else(|| (String::new(), Instant::now()));
        let response = self
            .transport
            .subscribe(
                bot_id,
                DebugChannelId::Auto,
                Arc::new(ChatSink(Arc::new(inbox::InboxSink {
                    inbox: Arc::clone(&self.desktop.inbox),
                    key: key.clone(),
                    viewer: Some(sink),
                }))),
            )
            .await?;
        self.desktop
            .viewers
            .lock()
            .await
            .insert(response.subscription_id.clone(), (key, page, opened_at));
        Ok(response)
    }

    pub async fn unsubscribe(&self, id: &str) {
        let _lease = self.desktop.lease_gate.lock().await;
        self.transport.unsubscribe(id).await;
        self.remove_viewer(id).await;
    }
    pub async fn shutdown(&self) {
        self.stop_desktop().await;
        self.transport.close_all().await;
    }
    pub fn page_loading(&self, page: &str, at: Instant) {
        self.clear_reading(page);
        self.transport.page_loading(page, at);
    }
    pub async fn listen(self: &Arc<Self>, bus: Arc<dyn EventBus>) {
        tokio::join!(
            Arc::clone(&self.transport).run_bot_event_listener(Arc::clone(&bus)),
            Arc::clone(self).run_desktop(bus),
            Arc::clone(self).run_notification_sync()
        );
    }
    pub async fn sweep(&self) {
        Arc::clone(&self.transport).run_idle_sweeper().await;
    }
}

// 不向普通聊天页推调试调用回包、心跳或账号凭据。
struct ChatSink(Arc<dyn DebugEventSink>);
impl DebugEventSink for ChatSink {
    fn send(&self, batch: &DebugEventBatch) -> bool {
        use ncd_domain::onebot_debug::DebugEventBody;
        let events = batch
            .events
            .iter()
            .filter(|event| match &event.body {
                DebugEventBody::Call { .. } => false,
                DebugEventBody::Ob11 { payload } => matches!(
                    payload.get("post_type").and_then(|v| v.as_str()),
                    Some("message" | "message_sent" | "notice")
                ),
                _ => true,
            })
            .cloned()
            .collect();
        self.0.send(&DebugEventBatch {
            v: batch.v,
            bot_id: batch.bot_id.clone(),
            events,
        })
    }
    fn opened_by(&self) -> Option<(&str, Instant)> {
        self.0.opened_by()
    }
}

fn chat_action(action: &str) -> bool {
    matches!(
        action,
        "get_login_info"
            | "get_friend_list"
            | "get_friends_with_category"
            | "get_recent_contact"
            | "get_stranger_info"
            | "get_group_list"
            | "get_group_member_list"
            | "get_group_info"
            | "get_group_detail_info"
            | "get_group_member_info"
            | "get_msg"
            | "get_group_msg_history"
            | "get_friend_msg_history"
            | "get_image"
            | "get_forward_msg"
            | "get_record"
            | "get_file"
            | "fetch_ptt_text"
            | "fetch_custom_face"
            | "fetch_custom_face_detail"
            | "fetch_sys_faces"
            | "add_custom_face"
            | "download_file"
            | "send_group_msg"
            | "send_private_msg"
            | "delete_msg"
            | "send_group_forward_msg"
            | "send_private_forward_msg"
            | "upload_group_file"
            | "upload_private_file"
            | "group_poke"
            | "friend_poke"
    )
}

fn archive_identity(targets: &[DebugTarget], bot_id: &str, self_id: &str) -> Result<(), String> {
    if targets
        .iter()
        .any(|target| target.bot_id == bot_id && target.qq_id.to_string() == self_id)
    {
        Ok(())
    } else {
        Err("聊天档案账号与 Bot 不匹配".into())
    }
}
fn rejected(request_id: String) -> DebugCallResponse {
    DebugCallResponse {
        request_id,
        result: DebugCallResult::Err {
            error: DebugError::InvalidParams {
                message: "聊天不支持此操作".into(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_does_not_expose_arbitrary_debug_actions() {
        assert!(chat_action("send_group_msg"));
        assert!(!chat_action("set_restart"));
        assert!(!chat_action("get_credentials"));
    }

    #[test]
    fn chat_allows_member_poke() {
        assert!(chat_action("group_poke"));
        assert!(chat_action("friend_poke"));
    }

    #[test]
    fn chat_allows_recall_and_forward_without_unguarded_group_management() {
        for action in [
            "delete_msg",
            "send_group_forward_msg",
            "send_private_forward_msg",
        ] {
            assert!(chat_action(action), "{action}");
        }
        assert!(!chat_action("set_group_kick"));
        assert!(!chat_action("set_friend_add_request"));
    }

    #[test]
    fn chat_supports_recent_contacts_and_private_profiles() {
        assert!(chat_action("get_recent_contact"));
        assert!(chat_action("get_stranger_info"));
        assert!(chat_action("get_friends_with_category"));
    }

    #[test]
    fn chat_allows_media_and_sticker_collection_without_group_file_deletion() {
        for action in [
            "get_image",
            "get_forward_msg",
            "get_record",
            "get_file",
            "fetch_ptt_text",
            "fetch_custom_face",
            "fetch_custom_face_detail",
            "fetch_sys_faces",
            "download_file",
            "add_custom_face",
            "get_group_info",
            "get_group_member_list",
            "get_friend_list",
        ] {
            assert!(chat_action(action), "{action}");
        }
        // 群文件的改动只经 group_files 拼好的调用，不让页面直接点名
        for action in [
            "delete_custom_face",
            "delete_group_file",
            "delete_group_folder",
            "move_group_file",
        ] {
            assert!(!chat_action(action), "{action}");
        }
    }

    #[test]
    fn archive_access_requires_the_matching_configured_identity() {
        use ncd_domain::{bot_config::BackendType, onebot_debug::DebugHost};
        let target = DebugTarget {
            bot_id: "bot-1".into(),
            name: "Bot".into(),
            qq_id: 10001,
            backend: BackendType::NapCat,
            host: DebugHost::Local,
            running: false,
            online: None,
        };
        assert!(archive_identity(std::slice::from_ref(&target), "bot-1", "10001").is_ok());
        assert!(archive_identity(std::slice::from_ref(&target), "bot-1", "10002").is_err());
        assert!(archive_identity(std::slice::from_ref(&target), "missing", "10001").is_err());
    }
}
