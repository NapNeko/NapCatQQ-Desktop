// 系统托盘与主窗口显隐/退出收口
// 对齐旧版 SystemTrayIcon:左键显示主窗口,右键菜单「显示主窗口」「退出程序」

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    AppHandle, Emitter, Manager, WebviewWindow,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::AppState;

static TRAY_ATTACHED: AtomicBool = AtomicBool::new(false);

pub const TRAY_ID: &str = "main-tray";
pub const MENU_SHOW: &str = "tray-show";
pub const MENU_LIGHTWEIGHT: &str = "tray-lightweight";
pub const MENU_QUIT: &str = "tray-quit";

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主窗口未找到".to_string())
}

/// 显示并前置主窗口(从托盘或隐藏状态恢复;轻量模式下重建 WebView)
#[tauri::command]
pub async fn window_show(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.lightweight_scheduler.cancel_pending().await;
    if crate::lightweight::is_lightweight_mode() || app.get_webview_window("main").is_none() {
        return crate::lightweight::exit_lightweight_mode(&app);
    }
    let window = main_window(&app)?;
    window.show().map_err(|e| e.to_string())?;
    let _ = window.unminimize();
    let _ = window.set_focus();
    Ok(())
}

/// 隐藏主窗口到托盘,并在 close_action=tray 时按设置启动延迟/立即轻量计时
pub async fn hide_main_window_to_tray(app: AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    window.hide().map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    state
        .lightweight_scheduler
        .on_main_window_hidden(app.clone())
        .await;
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::tray_summary::refresh_tray_tooltip(&app2).await;
    });
    Ok(())
}

/// 隐藏主窗口(最小化到托盘,进程继续运行)
#[tauri::command]
pub async fn window_hide_to_tray(app: AppHandle) -> Result<(), String> {
    hide_main_window_to_tray(app).await
}

/// 本机 Bot 是否处于活跃态,用于退出前拦截(与旧版 has_running_local_bot 对齐)
#[tauri::command]
pub async fn count_local_active_bots(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())
}

/// 在 setup 中注册托盘(幂等)菜单在阻塞线程构建,避免 async setup 里缺 runtime
pub fn attach_tray(app: &AppHandle) -> Result<(), String> {
    if TRAY_ATTACHED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let app_menu = app.clone();
    let menu = tauri::async_runtime::block_on(async move {
        crate::tray_menu::build_tray_menu(&app_menu).await
    })?;

    let icon = crate::tray_icon::idle_tray_icon(app)?;

    let app_handle = app.clone();
    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .title("NapCatQQ Desktop")
        .tooltip("NapCatQQ Desktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |_app, event| {
            if event.id.as_ref() == MENU_SHOW {
                let app = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = window_show(app).await;
                });
            } else if event.id.as_ref() == MENU_LIGHTWEIGHT {
                let app = app_handle.clone();
                let _ = crate::lightweight::enter_lightweight_mode(&app);
            } else if event.id.as_ref() == MENU_QUIT {
                let app = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = quit_from_tray(app).await {
                        eprintln!("[tray] quit failed: {err}");
                    }
                });
            }
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = window_show(app).await;
                });
            }
        })
        .build(app)
        .map_err(|e| e.to_string())?;

    let app_refresh = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::tray_summary::refresh_tray_tooltip(&app_refresh).await;
    });
    crate::tray_summary::spawn_tray_tooltip_refresh_loop(app.clone());

    Ok(())
}

async fn quit_from_tray(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let local_active = state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())?;
    if local_active > 0 {
        let _ = window_show(app.clone()).await;
        let _ = app.emit("desktop-exit-blocked", local_active);
        return Err(format!(
            "有 {local_active} 个本机 Bot 正在运行，请先停止后再退出"
        ));
    }
    let result = state.bot_manager.exit_desktop().await;
    if !result.failed.is_empty() {
        eprintln!(
            "[bot_manager] tray quit: {} bot(s) failed to stop",
            result.failed.len()
        );
    }
    state.runtime.shutdown().await;
    app.exit(0);
    Ok(())
}