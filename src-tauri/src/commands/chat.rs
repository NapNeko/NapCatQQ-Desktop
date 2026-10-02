//! 聊天 IPC，连接状态及动作边界由 runtime 管理。
use std::{sync::Arc, time::Instant};
use ncd_domain::onebot_debug::{DebugCallRequest, DebugCallResponse, DebugEventBatch, DebugStreamCallRequest, DebugStreamProgress, DebugSubscribeResponse, DebugTarget};
use ncd_runtime::{DebugEventSink, DebugStreamSink};
use tauri::{ipc::Channel, State, Webview};
use crate::AppState;

struct Events { channel: Channel<DebugEventBatch>, page: String, at: Instant }
impl DebugEventSink for Events {
    fn send(&self, batch: &DebugEventBatch) -> bool { self.channel.send(batch.clone()).is_ok() }
    fn opened_by(&self) -> Option<(&str, Instant)> { Some((&self.page, self.at)) }
}
struct Progress(Channel<DebugStreamProgress>);
impl DebugStreamSink for Progress {
    fn send(&self, progress: &DebugStreamProgress) -> bool { self.0.send(progress.clone()).is_ok() }
}

#[tauri::command]
pub async fn chat_targets(state: State<'_, AppState>) -> Result<Vec<DebugTarget>, String> { Ok(state.chat.targets().await) }
#[tauri::command]
pub async fn chat_call(state: State<'_, AppState>, request: DebugCallRequest) -> Result<DebugCallResponse, String> { Ok(state.chat.call(request).await) }
#[tauri::command]
pub async fn chat_call_stream(state: State<'_, AppState>, request: DebugStreamCallRequest, progress: Channel<DebugStreamProgress>) -> Result<DebugCallResponse, String> {
    Ok(state.chat.call_stream(request, Arc::new(Progress(progress))).await)
}
#[tauri::command]
pub async fn chat_subscribe(webview: Webview, state: State<'_, AppState>, bot_id: String, events: Channel<DebugEventBatch>) -> Result<DebugSubscribeResponse, String> {
    let sink = Events { channel: events, page: webview.label().into(), at: Instant::now() };
    state.chat.subscribe(&bot_id, Arc::new(sink)).await.map_err(|e| ncd_runtime::onebot_debug::error_text(&e))
}
#[tauri::command]
pub async fn chat_unsubscribe(state: State<'_, AppState>, subscription_id: String) -> Result<(), String> {
    state.chat.unsubscribe(&subscription_id).await;
    Ok(())
}

#[cfg(test)]
pub(crate) fn test_chat(root: &std::path::Path, bots: Arc<dyn ncd_runtime::DebugBotPort>) -> Arc<ncd_runtime::chat::ChatManager> {
    Arc::new(ncd_runtime::chat::ChatManager::new(
        bots,
        Arc::new(ncd_runtime::LocalOnlyHostResolver::new(Arc::new(ncd_host::local::LocalWindowsHost::new()))),
        root.to_path_buf(),
    ))
}
