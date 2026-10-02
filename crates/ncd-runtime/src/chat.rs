//! 聊天独占协议会话，调试开关和停止操作不影响这里的接收器。
use std::{path::PathBuf, sync::Arc, time::Instant};
use ncd_domain::onebot_debug::{DebugCallRequest, DebugCallResponse, DebugCallResult, DebugChannelId, DebugError, DebugEventBatch, DebugStreamCallRequest, DebugSubscribeResponse, DebugTarget};
use crate::{DebugBotPort, DebugEventSink, DebugManager, DebugStreamSink, EventBus};
use crate::host_resolver::HostResolver;

pub struct ChatManager { transport: Arc<DebugManager> }

impl ChatManager {
    pub fn new(bots: Arc<dyn DebugBotPort>, hosts: Arc<dyn HostResolver>, data_root: PathBuf) -> Self {
        Self { transport: Arc::new(DebugManager::new_ephemeral(bots, hosts, data_root)) }
    }

    pub async fn targets(&self) -> Vec<DebugTarget> { self.transport.list_targets().await }

    pub async fn call(&self, request: DebugCallRequest) -> DebugCallResponse {
        if !chat_action(&request.action) { return rejected(request.request_id); }
        self.transport.call(request).await
    }

    pub async fn call_stream(&self, request: DebugStreamCallRequest, sink: Arc<dyn DebugStreamSink>) -> DebugCallResponse {
        if !chat_action(&request.action) { return rejected(request.request_id); }
        self.transport.call_stream(request, sink).await
    }

    pub async fn subscribe(&self, bot_id: &str, sink: Arc<dyn DebugEventSink>) -> Result<DebugSubscribeResponse, DebugError> {
        self.transport.subscribe(bot_id, DebugChannelId::Auto, Arc::new(ChatSink(sink))).await
    }

    pub async fn unsubscribe(&self, id: &str) { self.transport.unsubscribe(id).await; }
    pub async fn shutdown(&self) { self.transport.close_all().await; }
    pub fn page_loading(&self, page: &str, at: Instant) { self.transport.page_loading(page, at); }
    pub async fn listen(&self, bus: Arc<dyn EventBus>) { Arc::clone(&self.transport).run_bot_event_listener(bus).await; }
    pub async fn sweep(&self) { Arc::clone(&self.transport).run_idle_sweeper().await; }
}

// 不向普通聊天页推调试调用回包、心跳或账号凭据。
struct ChatSink(Arc<dyn DebugEventSink>);
impl DebugEventSink for ChatSink {
    fn send(&self, batch: &DebugEventBatch) -> bool {
        use ncd_domain::onebot_debug::DebugEventBody;
        let events = batch.events.iter().filter(|event| match &event.body {
            DebugEventBody::Call { .. } => false,
            DebugEventBody::Ob11 { payload } => matches!(payload.get("post_type").and_then(|v| v.as_str()), Some("message" | "message_sent" | "notice")),
            _ => true,
        }).cloned().collect();
        self.0.send(&DebugEventBatch { v: batch.v, bot_id: batch.bot_id.clone(), events })
    }
    fn opened_by(&self) -> Option<(&str, Instant)> { self.0.opened_by() }
}

fn chat_action(action: &str) -> bool {
    matches!(action, "get_login_info" | "get_friend_list" | "get_group_list" | "get_group_member_list" | "get_group_info" | "get_msg" | "get_group_msg_history" | "get_friend_msg_history" | "send_group_msg" | "send_private_msg" | "upload_group_file" | "upload_private_file")
}
fn rejected(request_id: String) -> DebugCallResponse {
    DebugCallResponse { request_id, result: DebugCallResult::Err { error: DebugError::InvalidParams { message: "聊天不支持此操作".into() } } }
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
}
