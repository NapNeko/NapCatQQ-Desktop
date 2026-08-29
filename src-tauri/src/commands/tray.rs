// 系统托盘与主窗口显隐/退出收口
// 左键显示主窗口,右键弹自绘托盘面板(tray_panel.rs),取代旧原生菜单

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::AppState;

static TRAY_ATTACHED: AtomicBool = AtomicBool::new(false);

pub const TRAY_ID: &str = "main-tray";

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
    crate::lightweight::enter_lightweight_mode(&app)
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
    crate::commands::ncd_watch::clear_present_on_all_remote_servers(state.inner()).await;
    state.runtime.shutdown().await;
    app.exit(0);
    Ok(())
}
