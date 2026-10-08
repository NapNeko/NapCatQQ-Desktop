// 截图 IPC 只把调用方窗口身份交给桌面适配器。
use crate::AppState;
use ncd_domain::chat_screenshot::{
    ChatScreenshotAttachment, ChatScreenshotRequest, ChatScreenshotShortcut,
};
use tauri::{Manager, State, WebviewWindow};

fn ensure_chat_window(window: &WebviewWindow) -> Result<(), String> {
    if matches!(window.label(), "main" | "chat-panel") {
        Ok(())
    } else {
        Err("请从聊天窗口发起截图".into())
    }
}
#[tauri::command]
pub async fn chat_screenshot_capture(
    window: WebviewWindow,
    state: State<'_, AppState>,
    request: ChatScreenshotRequest,
) -> Result<Option<ChatScreenshotAttachment>, String> {
    ensure_chat_window(&window)?;
    state.migrate_gate.ensure_idle()?;
    if !state.chat.is_enabled() {
        return Err("聊天功能已关闭".into());
    }
    crate::screenshot::capture_chat(
        window.app_handle().clone(),
        window,
        state.chat.screenshot_cache(),
        request,
    )
    .await
}
#[tauri::command]
pub fn chat_screenshot_cancel(window: WebviewWindow) -> Result<(), String> {
    ensure_chat_window(&window)?;
    crate::screenshot::cancel(window.app_handle(), window.label());
    Ok(())
}
#[tauri::command]
pub async fn chat_screenshot_shortcut(
    window: WebviewWindow,
    request: ChatScreenshotShortcut,
) -> Result<(), String> {
    ensure_chat_window(&window)?;
    crate::screenshot::configure_shortcut(
        window.app_handle().clone(),
        window.label().to_string(),
        request,
    )
    .await
}
