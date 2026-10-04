//! 聊天 IPC，连接状态及动作边界由 runtime 管理。
use std::{sync::Arc, time::Instant};
use ncd_domain::chat_archive::ChatArchive;
use ncd_domain::chat_desktop::{ChatAccountPreference, ChatDesktopStatus, ChatViewState};
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

#[tauri::command]
pub async fn chat_archive_load(state: State<'_, AppState>, bot_id: String, self_id: String) -> Result<Option<ChatArchive>, String> {
    state.chat.load_archive(bot_id, self_id).await
}

#[tauri::command]
pub async fn chat_archive_save(state: State<'_, AppState>, bot_id: String, self_id: String, archive: ChatArchive) -> Result<(), String> {
    state.migrate_gate.ensure_idle()?;
    state.chat.save_archive(bot_id, self_id, archive).await
}

#[tauri::command]
pub async fn chat_desktop_status(state: State<'_, AppState>) -> Result<ChatDesktopStatus, String> { Ok(state.chat.desktop_status().await) }
#[tauri::command]
pub async fn chat_set_preference(state: State<'_, AppState>, preference: ChatAccountPreference) -> Result<(), String> {
    state.migrate_gate.ensure_idle()?;
    state.chat.set_preference(preference).await
}
#[tauri::command]
pub async fn chat_view_load(webview: Webview, state: State<'_, AppState>, claim: bool) -> Result<ChatViewState, String> {
    Ok(if claim { state.chat.claim_view(webview.label()) } else { state.chat.view() })
}
#[tauri::command]
pub async fn chat_set_group_ignored(state: State<'_, AppState>, bot_id: String, self_id: String, group_id: String, ignored: bool, hidden: bool) -> Result<(), String> {
    state.migrate_gate.ensure_idle()?;
    state.chat.set_group_ignored(bot_id, self_id, group_id, ignored, hidden).await
}
#[tauri::command]
pub async fn chat_merge_hidden_groups(state: State<'_, AppState>, bot_id: String, self_id: String, groups: Vec<String>) -> Result<(), String> {
    state.migrate_gate.ensure_idle()?;
    state.chat.merge_hidden_groups(bot_id, self_id, groups).await
}
#[tauri::command]
pub async fn chat_select_account(webview: Webview, state: State<'_, AppState>, bot_id: String) -> Result<(), String> {
    if webview.label() != crate::chat_window::CHAT_WINDOW_LABEL { return Ok(()); }
    if !state.chat.targets().await.iter().any(|target| target.bot_id == bot_id) { return Err("聊天账号不存在".into()); }
    state.chat.select_view_bot(bot_id);
    Ok(())
}
#[tauri::command]
pub async fn chat_view_save(webview: Webview, state: State<'_, AppState>, view: ChatViewState) -> Result<(), String> { state.chat.set_view(webview.label(), view).await }
#[tauri::command]
pub async fn chat_set_reading(webview: Webview, state: State<'_, AppState>, bot_id: String, self_id: String, session: Option<String>) -> Result<(), String> {
    // 可见但失焦的窗口不能替用户把消息读掉。
    let focused = webview.window().is_focused().unwrap_or(false);
    state.chat.set_reading(webview.label(), bot_id, self_id, if focused { session } else { None }).await
}
#[tauri::command]
pub async fn chat_flush(state: State<'_, AppState>) -> Result<(), String> { state.chat.flush().await }
#[tauri::command]
pub async fn chat_mark_read(state: State<'_, AppState>, bot_id: String, self_id: String, session: String) -> Result<(), String> { state.chat.mark_read(bot_id, self_id, session).await }
#[tauri::command]
pub async fn chat_release_account(state: State<'_, AppState>, bot_id: String, self_id: String) -> Result<(), String> { state.chat.release_account(&bot_id, &self_id).await; Ok(()) }

// 自己刚发的图直接读本机字节做预览，不绕协议回环；上限与媒体 inline 一致。
#[tauri::command]
pub async fn chat_read_local_image(path: String) -> Result<String, String> {
    use base64::Engine as _;
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(path)).await.map_err(|e| e.to_string())?.map_err(|e: std::io::Error| format!("读取本地图片失败：{e}"))?;
    if bytes.len() > 16 * 1024 * 1024 { return Err("图片超过 16 MiB，无法预览".into()); }
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[cfg(test)]
pub(crate) fn test_chat(root: &std::path::Path, bots: Arc<dyn ncd_runtime::DebugBotPort>) -> Arc<ncd_runtime::chat::ChatManager> {
    Arc::new(ncd_runtime::chat::ChatManager::new(
        bots,
        Arc::new(ncd_runtime::LocalOnlyHostResolver::new(Arc::new(ncd_host::local::LocalWindowsHost::new()))),
        root.to_path_buf(),
    ))
}
