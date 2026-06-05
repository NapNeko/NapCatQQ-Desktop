// 系统托盘与主窗口显隐/退出收口。
// 对齐旧版 SystemTrayIcon：左键显示主窗口，右键菜单「显示主窗口」「退出程序」。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    AppHandle, Manager, WebviewWindow,
};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::AppState;

static TRAY_ATTACHED: AtomicBool = AtomicBool::new(false);

const TRAY_ID: &str = "main-tray";
const MENU_SHOW: &str = "tray-show";
const MENU_QUIT: &str = "tray-quit";

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主窗口未找到".to_string())
}

/// 显示并前置主窗口（从托盘或隐藏状态恢复）。
#[tauri::command]
pub fn window_show(app: AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    window.show().map_err(|e| e.to_string())?;
    let _ = window.unminimize();
    let _ = window.set_focus();
    Ok(())
}

/// 隐藏主窗口（最小化到托盘，进程继续运行）。
#[tauri::command]
pub fn window_hide_to_tray(app: AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    window.hide().map_err(|e| e.to_string())
}

/// 本机 Bot 是否处于活跃态，用于退出前拦截（与旧版 has_running_local_bot 对齐）。
#[tauri::command]
pub async fn count_local_active_bots(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())
}

/// 退出应用：先 shutdown 所有 Bot，再结束进程。
#[tauri::command]
pub async fn request_exit_app(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let result = state.bot_manager.shutdown_all().await;
    if !result.failed.is_empty() {
        eprintln!(
            "[bot_manager] request_exit_app: {} bot(s) failed to stop cleanly",
            result.failed.len()
        );
    }
    state.runtime.shutdown().await;
    app.exit(0);
    Ok(())
}

/// 在 setup 中注册托盘（幂等）。
pub fn attach_tray(app: &AppHandle) -> Result<(), String> {
    if TRAY_ATTACHED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let show = MenuItem::with_id(app, MENU_SHOW, "显示主窗口", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "退出程序", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&show, &separator, &quit]).map_err(|e| e.to_string())?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "应用图标未配置（请检查 bundle.icon）".to_string())?;

    let app_handle = app.clone();
    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("NapCatQQ Desktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |_app, event| {
            if event.id.as_ref() == MENU_SHOW {
                let _ = window_show(app_handle.clone());
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
                let _ = window_show(app);
            }
        })
        .build(app)
        .map_err(|e| e.to_string())?;

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
        let _ = window_show(app.clone());
        return Err(format!(
            "有 {local_active} 个本机 Bot 正在运行，请先停止后再退出"
        ));
    }
    let result = state.bot_manager.shutdown_all().await;
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