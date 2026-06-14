// 轻量模式：销毁主 WebView 释放 WebView2，Bot 与托盘进程继续运行。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, WebviewWindowBuilder};

use crate::commands::window::apply_main_window_startup_geometry;

pub static LIGHTWEIGHT_MODE: AtomicBool = AtomicBool::new(false);

pub fn is_lightweight_mode() -> bool {
    LIGHTWEIGHT_MODE.load(Ordering::SeqCst)
}

/// 主窗口 label，与 capabilities/main.json 一致。
pub const MAIN_WINDOW_LABEL: &str = "main";

fn main_window_config(app: &AppHandle) -> Result<tauri::utils::config::WindowConfig, String> {
    let mut conf = app
        .config()
        .app
        .windows
        .first()
        .cloned()
        .ok_or_else(|| "tauri.conf.json 未配置 app.windows".to_string())?;
    conf.label = MAIN_WINDOW_LABEL.to_string();
    Ok(conf)
}

/// 释放主界面 WebView2；调用前若窗口可见可先 hide。
pub fn enter_lightweight_mode(app: &AppHandle) -> Result<(), String> {
    if is_lightweight_mode() {
        return Ok(());
    }
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.destroy().map_err(|e| e.to_string())?;
    }
    LIGHTWEIGHT_MODE.store(true, Ordering::SeqCst);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::tray_summary::refresh_tray_tooltip(&app2).await;
    });
    crate::desktop_log::write_session_line(
        "INFO",
        "ncd::lightweight",
        "已进入轻量模式（主 WebView 已销毁）",
    );
    Ok(())
}

/// 轻量模式下重建主 WebView；已有窗口则仅 show/focus。
pub fn exit_lightweight_mode(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        LIGHTWEIGHT_MODE.store(false, Ordering::SeqCst);
        window.show().map_err(|e| e.to_string())?;
        let _ = window.unminimize();
        let _ = window.set_focus();
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::tray_summary::refresh_tray_tooltip(&app2).await;
        });
        return Ok(());
    }

    if !is_lightweight_mode() {
        return Err("主窗口未找到且当前非轻量模式".to_string());
    }

    let conf = main_window_config(app)?;
    let window = WebviewWindowBuilder::from_config(app, &conf)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    apply_main_window_startup_geometry(app)?;
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();

    LIGHTWEIGHT_MODE.store(false, Ordering::SeqCst);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::tray_summary::refresh_tray_tooltip(&app2).await;
    });
    crate::desktop_log::write_session_line(
        "INFO",
        "ncd::lightweight",
        "已退出轻量模式（主 WebView 已重建）",
    );
    Ok(())
}

pub fn should_prevent_exit(_app: &AppHandle) -> bool {
    is_lightweight_mode()
}