// 系统托盘与主窗口显隐/退出收口
// 左键显示主窗口,右键弹自绘托盘面板(tray_panel.rs),取代旧原生菜单

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::AppState;
use crate::commands::exit;
use crate::window_events::{DESKTOP_EXIT_BLOCKED, DesktopExitBlocked};

static TRAY_ATTACHED: AtomicBool = AtomicBool::new(false);

pub const TRAY_ID: &str = "main-tray";

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主窗口未找到".to_string())
}

/// 显示并前置主窗口(从托盘或隐藏状态恢复;轻量模式下重建 WebView)
#[tauri::command]
pub async fn window_show(app: AppHandle) -> Result<(), String> {
    // 托盘面板在这边收起:面板的 capability 不给窗口 hide,主窗没抢到焦点时面板就一直浮着
    crate::tray_panel::hide_tray_panel(&app);
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

/// 在 setup 中注册托盘(幂等)。不再附原生菜单,右键走自绘面板;面板窗口在退出轻量模式后补建。
pub fn attach_tray(app: &AppHandle) -> Result<(), String> {
    if TRAY_ATTACHED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let icon = crate::tray_icon::idle_tray_icon(app)?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .title("NapCatQQ Desktop")
        .tooltip("NapCatQQ Desktop")
        .on_tray_icon_event(crate::tray_panel::handle_tray_icon_event)
        .build(app)
        .map_err(|e| e.to_string())?;

    let app_refresh = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::tray_summary::refresh_tray_tooltip(&app_refresh).await;
    });
    crate::tray_summary::spawn_tray_tooltip_refresh_loop(app.clone());

    Ok(())
}

/// 退出前校验:有本机 Bot 在跑则拦截并拉起主窗;否则停 Bot、关 runtime、退出进程。
/// 供自绘托盘面板的「退出」按钮调用(面板先隐藏,再走与旧托盘退出一致的流程)。
#[tauri::command]
pub async fn tray_panel_quit(app: AppHandle) -> Result<(), String> {
    crate::tray_panel::hide_tray_panel(&app);
    quit_from_tray(app).await
}

/// 自绘托盘面板的「释放界面内存」:先收起面板再进轻量模式销毁主 WebView。
#[tauri::command]
pub async fn tray_panel_enter_lightweight(app: AppHandle) -> Result<(), String> {
    crate::tray_panel::hide_tray_panel(&app);
    crate::chat_window::release_control_panel(&app).await
}

async fn quit_from_tray(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let local_active = exit::local_active_bots(&state).await?;
    if local_active > 0 {
        let _ = window_show(app.clone()).await;
        let _ = app.emit(DESKTOP_EXIT_BLOCKED, DesktopExitBlocked::new(local_active));
        return Err(exit::exit_blocked_message(local_active));
    }
    exit::shutdown_and_exit(&app, &state, "tray_panel_quit").await;
    Ok(())
}
