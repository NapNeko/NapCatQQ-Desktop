//! 聊天窗口的单实例创建、宿主交接与关闭。
use ncd_domain::chat_desktop::ChatTrayNavigation;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, Webview, WebviewWindowBuilder};
use tokio::sync::{Mutex, oneshot};
use ts_rs::TS;

pub const CHAT_WINDOW_LABEL: &str = "chat-panel";
#[derive(Default)]
pub struct ChatWindowCoordinator {
    gate: Mutex<()>,
    pending: Mutex<HashMap<String, (String, oneshot::Sender<Result<(), String>>)>>,
    embed_requested: AtomicBool,
    release_main: AtomicBool,
    ready: AtomicBool,
    navigation: Mutex<Option<ChatTrayNavigation>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/chat/")]
pub struct ChatWindowRequest {
    pub v: u32,
    pub request_id: String,
    pub action: String,
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/chat/")]
pub struct ChatWindowState {
    pub v: u32,
    pub detached: bool,
    pub embed_requested: bool,
}

#[tauri::command]
pub async fn chat_take_tray_navigation(
    app: AppHandle,
    webview: Webview,
) -> Result<Option<ChatTrayNavigation>, String> {
    if webview.label() != CHAT_WINDOW_LABEL {
        return Err("聊天窗口来源无效".into());
    }
    Ok(app
        .state::<ChatWindowCoordinator>()
        .navigation
        .lock()
        .await
        .take())
}

pub async fn open_from_tray(
    app: AppHandle,
    bot_id: String,
    self_id: String,
    session: Option<String>,
) -> Result<(), String> {
    let snapshot = app
        .state::<crate::AppState>()
        .chat
        .tray_snapshot(&bot_id, &self_id)
        .await?;
    let conversation = match session {
        Some(key) => Some(
            snapshot
                .conversations
                .into_iter()
                .find(|c| c.key == key)
                .ok_or("会话已更新，请重新打开消息列表")?,
        ),
        None => snapshot.conversations.into_iter().next(),
    };
    let navigation = conversation.map(|conversation| ChatTrayNavigation {
        v: 1,
        bot_id: bot_id.clone(),
        self_id,
        conversation,
    });
    open_chat_window_with_navigation(app, Some(bot_id), false, navigation).await
}

async fn prepare(app: &AppHandle, owner: &str, action: &str) -> Result<(), String> {
    if app.get_webview_window(owner).is_none() {
        return Ok(());
    }
    let coordinator = app.state::<ChatWindowCoordinator>();
    let request_id = uuid::Uuid::new_v4().to_string();
    let (send, receive) = oneshot::channel();
    coordinator
        .pending
        .lock()
        .await
        .insert(request_id.clone(), (owner.into(), send));
    let request = ChatWindowRequest {
        v: 1,
        request_id: request_id.clone(),
        action: action.into(),
    };
    let result = match app.emit_to(owner, crate::window_events::CHAT_WINDOW_REQUEST, request) {
        Ok(()) => match tokio::time::timeout(Duration::from_secs(10), receive).await {
            Ok(Ok(result)) => result,
            _ => Err("聊天界面尚未就绪，请稍后再试".into()),
        },
        Err(e) => Err(e.to_string()),
    };
    coordinator.pending.lock().await.remove(&request_id);
    if result.is_err() {
        let _ = app.emit_to(
            owner,
            crate::window_events::CHAT_WINDOW_REQUEST,
            ChatWindowRequest {
                v: 1,
                request_id: String::new(),
                action: "resume".into(),
            },
        );
    }
    result
}

#[tauri::command]
pub async fn chat_window_handoff_ready(
    app: AppHandle,
    webview: Webview,
    request_id: String,
    error: Option<String>,
) -> Result<(), String> {
    if !matches!(webview.label(), "main" | CHAT_WINDOW_LABEL) {
        return Err("聊天窗口来源无效".into());
    }
    let coordinator = app.state::<ChatWindowCoordinator>();
    let mut pending = coordinator.pending.lock().await;
    if pending
        .get(&request_id)
        .is_some_and(|(owner, _)| owner != webview.label())
    {
        return Err("聊天交接来源不匹配".into());
    }
    let Some((_, reply)) = pending.remove(&request_id) else {
        return Err("聊天交接已取消".into());
    };
    let _ = reply.send(error.map_or(Ok(()), Err));
    Ok(())
}

#[tauri::command]
pub async fn open_chat_window(
    app: AppHandle,
    bot_id: Option<String>,
    release_main: bool,
) -> Result<(), String> {
    open_chat_window_with_navigation(app, bot_id, release_main, None).await
}

async fn open_chat_window_with_navigation(
    app: AppHandle,
    bot_id: Option<String>,
    release_main: bool,
    navigation: Option<ChatTrayNavigation>,
) -> Result<(), String> {
    let coordinator = app.state::<ChatWindowCoordinator>();
    let _gate = coordinator.gate.lock().await;
    let state = app.state::<crate::AppState>();
    if let Some(bot_id) = &bot_id {
        if !state
            .chat
            .targets()
            .await
            .iter()
            .any(|t| &t.bot_id == bot_id)
        {
            return Err("聊天账号不存在".into());
        }
    }
    if let Some(window) = app.get_webview_window(CHAT_WINDOW_LABEL) {
        if release_main {
            coordinator.release_main.store(true, Ordering::SeqCst);
        }
        *coordinator.navigation.lock().await = navigation;
        if let Some(bot_id) = bot_id {
            state.chat.select_view_bot(bot_id);
        }
        let _ = app.emit_to(
            CHAT_WINDOW_LABEL,
            crate::window_events::CHAT_ACCOUNT_SELECTED,
            crate::window_events::WindowSignal::V1,
        );
        if !coordinator.ready.load(Ordering::SeqCst) {
            return Ok(());
        }
        crate::webview_scheduler::show_window(&window).await?;
        let _ = window.unminimize();
        let _ = window.set_focus();
        release_requested_main(&app).await?;
        return Ok(());
    }
    prepare(&app, "main", "popout").await?;
    *coordinator.navigation.lock().await = navigation;
    if let Some(bot_id) = bot_id {
        state.chat.select_view_bot(bot_id);
    }
    let result = create_chat_window(&app);
    match result {
        Ok(window) => {
            if let Ok(icon) = crate::window_icon::main_window_icon(&app) {
                let _ = window.set_icon(icon);
            }
            coordinator.embed_requested.store(false, Ordering::SeqCst);
            coordinator
                .release_main
                .store(release_main, Ordering::SeqCst);
            coordinator.ready.store(false, Ordering::SeqCst);
            Ok(())
        }
        Err(error) => {
            coordinator.embed_requested.store(true, Ordering::SeqCst);
            let _ = app.emit_to(
                "main",
                crate::window_events::CHAT_EMBED_REQUESTED,
                crate::window_events::WindowSignal::V1,
            );
            Err(error.to_string())
        }
    }
}

pub fn chat_window_config(app: &AppHandle) -> Result<tauri::utils::config::WindowConfig, String> {
    let mut conf = app
        .config()
        .app
        .windows
        .first()
        .cloned()
        .ok_or("主窗口配置缺失")?;
    conf.label = CHAT_WINDOW_LABEL.into();
    conf.title = "聊天 - NapCatQQ Desktop".into();
    conf.width = 880.0;
    conf.height = 700.0;
    conf.min_width = Some(480.0);
    conf.min_height = Some(480.0);
    conf.visible = false;
    conf.url = tauri::WebviewUrl::App("chat.html".into());
    // 聊天画布本身不透明，不继承控制台的透明窗口与 Mica 合成资源。
    conf.transparent = false;
    conf.window_effects = None;
    Ok(conf)
}

pub fn create_chat_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    let conf = chat_window_config(app)?;
    WebviewWindowBuilder::from_config(app, &conf)
        .and_then(|builder| builder.build())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reveal_chat_window(app: AppHandle) -> Result<(), String> {
    let coordinator = app.state::<ChatWindowCoordinator>();
    let _gate = coordinator.gate.lock().await;
    let window = app
        .get_webview_window(CHAT_WINDOW_LABEL)
        .ok_or("聊天窗口不存在")?;
    // 此 IPC 已确认前端完成 bootstrap；显示失败也应允许后续 open 再试。
    coordinator.ready.store(true, Ordering::SeqCst);
    crate::webview_scheduler::show_window(&window).await?;
    let _ = window.set_focus();
    release_requested_main(&app).await
}

async fn release_requested_main(app: &AppHandle) -> Result<(), String> {
    if !app
        .state::<ChatWindowCoordinator>()
        .release_main
        .swap(false, Ordering::SeqCst)
        || !app
            .state::<crate::AppState>()
            .components
            .active_tasks()
            .is_empty()
    {
        return Ok(());
    }
    let Some(main) = app.get_webview_window("main") else {
        return Ok(());
    };
    let was_visible = main.is_visible().unwrap_or(false);
    crate::webview_scheduler::hide_window(&main)?;
    // 调用方已经持有 gate，不能再经公开入口重复加锁。
    if let Err(error) = release_control_panel_inner(app).await {
        if was_visible {
            let _ = crate::webview_scheduler::show_window(&main).await;
        }
        return Err(error);
    }
    Ok(())
}
#[tauri::command]
pub async fn focus_chat_window(app: AppHandle) -> bool {
    let Some(window) = app.get_webview_window(CHAT_WINDOW_LABEL) else {
        return false;
    };
    let _ = crate::webview_scheduler::show_window(&window).await;
    let _ = window.unminimize();
    let _ = window.set_focus();
    true
}
#[tauri::command]
pub async fn chat_window_state(app: AppHandle) -> ChatWindowState {
    ChatWindowState {
        v: 1,
        detached: app.get_webview_window(CHAT_WINDOW_LABEL).is_some(),
        embed_requested: app
            .state::<ChatWindowCoordinator>()
            .embed_requested
            .swap(false, Ordering::SeqCst),
    }
}
#[tauri::command]
pub async fn close_chat_window(app: AppHandle, embed: bool) -> Result<(), String> {
    let coordinator = app.state::<ChatWindowCoordinator>();
    let _gate = coordinator.gate.lock().await;
    prepare(
        &app,
        CHAT_WINDOW_LABEL,
        if embed { "embed" } else { "close" },
    )
    .await?;
    app.state::<crate::AppState>()
        .chat
        .release_page(CHAT_WINDOW_LABEL)
        .await;
    if let Some(window) = app.get_webview_window(CHAT_WINDOW_LABEL)
        && let Err(error) = window.destroy()
    {
        let _ = app.emit_to(
            CHAT_WINDOW_LABEL,
            crate::window_events::CHAT_WINDOW_REQUEST,
            ChatWindowRequest {
                v: 1,
                request_id: String::new(),
                action: "resume".into(),
            },
        );
        return Err(error.to_string());
    }
    coordinator.ready.store(false, Ordering::SeqCst);
    if embed {
        coordinator.embed_requested.store(true, Ordering::SeqCst);
        crate::commands::tray::window_show(app.clone()).await?;
        let _ = app.emit_to(
            "main",
            crate::window_events::CHAT_EMBED_REQUESTED,
            crate::window_events::WindowSignal::V1,
        );
    }
    Ok(())
}

pub async fn release_control_panel(app: &AppHandle) -> Result<(), String> {
    let coordinator = app.state::<ChatWindowCoordinator>();
    let _gate = coordinator.gate.lock().await;
    release_control_panel_inner(app).await
}

async fn release_control_panel_inner(app: &AppHandle) -> Result<(), String> {
    prepare(app, "main", "release").await?;
    app.state::<crate::AppState>()
        .chat
        .release_page("main")
        .await;
    crate::lightweight::enter_lightweight_mode(app)
}
