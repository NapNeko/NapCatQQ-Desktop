// 轻量模式:销毁主 WebView 释放 WebView2,Bot 与托盘进程继续运行

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, WebviewWindowBuilder};

use crate::commands::window::apply_main_window_startup_geometry;

pub static LIGHTWEIGHT_MODE: AtomicBool = AtomicBool::new(false);

pub fn is_lightweight_mode() -> bool {
    LIGHTWEIGHT_MODE.load(Ordering::SeqCst)
}

/// 主窗口 label,与 capabilities/main.json 一致
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

/// 释放控制台 WebView2，保留正在使用的独立聊天窗。
pub fn enter_lightweight_mode(app: &AppHandle) -> Result<(), String> {
    if is_lightweight_mode() && app.get_webview_window(MAIN_WINDOW_LABEL).is_none() {
        return Ok(());
    }
    // 终端输出通道挂在要销毁的网页上：先摘掉，输出照常收进回放，网页重建后重新接上
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.terminals.detach_all();
    }
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.destroy().map_err(|e| e.to_string())?;
    }
    // 控制台回收不能连带销毁仍在使用的聊天窗口。
    if let Some(debug_window) = app.get_webview_window(crate::commands::window::DEBUG_WINDOW_LABEL)
    {
        let _ = debug_window.destroy();
    }
    LIGHTWEIGHT_MODE.store(true, Ordering::SeqCst);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::tray_summary::refresh_tray_tooltip(&app2).await;
    });
    crate::desktop_log::write_session_line("INFO", "ncd::lightweight", "控制台界面已回收");
    Ok(())
}

/// 轻量模式下重建主 WebView;已有窗口则仅 show/focus
pub async fn exit_lightweight_mode(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        LIGHTWEIGHT_MODE.store(false, Ordering::SeqCst);
        crate::webview_scheduler::show_window(&window).await?;
        let _ = window.unminimize();
        let _ = window.set_focus();
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::tray_summary::refresh_tray_tooltip(&app2).await;
        });
        return Ok(());
    }

    let conf = main_window_config(app)?;
    let window = WebviewWindowBuilder::from_config(app, &conf)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    crate::ui_scale::apply_to_window(&window)?;
    crate::window_frame::apply(&window)?;
    apply_main_window_startup_geometry(app)?;
    let _ = crate::window_icon::apply_window_icon(app, &window);
    crate::webview_scheduler::show_window(&window).await?;
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

pub fn should_prevent_exit(_app: &AppHandle, code: Option<i32>) -> bool {
    prevent_implicit_exit(is_lightweight_mode(), code)
}

fn prevent_implicit_exit(lightweight: bool, code: Option<i32>) -> bool {
    // 释放最后一个 WebView 要保住托盘；完成收尾后的 app.exit(0) 必须放行。
    lightweight && code.is_none()
}

#[cfg(test)]
mod tests {
    use super::prevent_implicit_exit;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_webview_config_uses_an_opaque_surface_after_platform_merge() {
        // 检查 Tauri 实际合并后的配置；启动、轻量模式重建和调试窗都从这里取配置。
        let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let main = context.config().app.windows.first().unwrap();
        assert!(!main.transparent);
        assert_eq!(main.background_color.unwrap().3, 255);
    }

    #[test]
    fn lightweight_keeps_the_tray_alive_but_allows_explicit_exit() {
        assert!(prevent_implicit_exit(true, None));
        assert!(!prevent_implicit_exit(true, Some(0)));
        assert!(!prevent_implicit_exit(true, Some(1)));
        assert!(!prevent_implicit_exit(false, None));
    }
}
