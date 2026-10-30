// 系统托盘与主窗口显隐/退出收口
// 左键显示主窗口,右键弹原生托盘面板(tray_panel_native.rs)

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::AppState;
use crate::commands::exit;
use crate::window_events::{DESKTOP_EXIT_BLOCKED, DesktopExitBlocked};

static TRAY_ATTACHED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct PendingTrayExit(AtomicBool);

impl PendingTrayExit {
    fn request(&self) -> bool {
        !self.0.swap(true, Ordering::SeqCst)
    }

    fn take(&self) -> bool {
        self.0.swap(false, Ordering::SeqCst)
    }
}

static PENDING_TRAY_EXIT: PendingTrayExit = PendingTrayExit(AtomicBool::new(false));

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
        return crate::lightweight::exit_lightweight_mode(&app).await;
    }
    let window = main_window(&app)?;
    crate::webview_scheduler::show_window(&window).await?;
    let _ = window.unminimize();
    let _ = window.set_focus();
    Ok(())
}

/// 隐藏主窗口到托盘,并在 close_action=tray 时按设置启动延迟/立即轻量计时
pub async fn hide_main_window_to_tray(app: AppHandle) -> Result<(), String> {
    let window = main_window(&app)?;
    // 连 WebView 一起藏、降到休眠：只藏原生窗口时页面仍当自己可见，动画照跑、缓存不放
    crate::webview_scheduler::hide_window(&window)?;
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

/// 注册托盘；Linux 能嵌入 XEmbed 时右键直出面板，其余环境保留系统菜单。
pub fn attach_tray(app: &AppHandle) -> Result<(), String> {
    if TRAY_ATTACHED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let icon = crate::tray_icon::idle_tray_icon(app)?;

    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon.clone())
        .tooltip("NapCatQQ Desktop")
        .on_tray_icon_event(crate::tray_panel::handle_tray_icon_event);
    // AppIndicator 把 title 作为面板文字标签；Linux 只显示图标。
    #[cfg(not(target_os = "linux"))]
    let builder = builder.title("NapCatQQ Desktop");
    // Linux 的 AppIndicator 不投递鼠标事件，恢复窗口和退出必须有系统菜单入口。
    #[cfg(target_os = "linux")]
    let builder = builder
        .menu(
            &tauri::menu::MenuBuilder::new(app)
                .text("panel", "打开托盘面板")
                .separator()
                .text("show", "显示主窗口")
                .text("lightweight", "释放界面内存")
                .separator()
                .text("quit", "退出")
                .build()
                .map_err(|e| e.to_string())?,
        )
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "panel" {
                let panel_app = app.clone();
                if let Err(error) = app.run_on_main_thread(move || {
                    let anchor = crate::native_panel::window::menu_anchor();
                    crate::tray_panel_native::open(&panel_app, anchor);
                }) {
                    tracing::warn!(%error, "打开托盘面板失败");
                }
                return;
            }
            let action = event.id().as_ref().to_owned();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let result = match action.as_str() {
                    "show" => window_show(app).await,
                    "lightweight" => tray_panel_enter_lightweight(app).await,
                    "quit" => tray_panel_quit(app).await,
                    _ => Ok(()),
                };
                if let Err(error) = result {
                    tracing::warn!(%error, "托盘菜单操作失败");
                }
            });
        });
    let tray = builder.build(app).map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    {
        let panel_app = app.clone();
        if let Err(error) = crate::linux_tray::attach(
            app,
            &tray,
            icon,
            "NapCatQQ Desktop",
            move |event| match event.button {
                tauri::tray::MouseButton::Left => {
                    let app = panel_app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(error) = window_show(app).await {
                            tracing::warn!(%error, "从托盘恢复主窗口失败");
                        }
                    });
                }
                tauri::tray::MouseButton::Right => {
                    crate::native_panel::window::set_tray_display(Some(&event.display));
                    crate::tray_panel_native::open(&panel_app, event.anchor);
                }
                _ => {}
            },
        ) {
            tracing::warn!(%error, "XEmbed 托盘初始化失败，保留系统菜单");
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = tray;

    let app_refresh = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = crate::tray_summary::refresh_tray_tooltip(&app_refresh).await;
    });
    crate::tray_summary::spawn_tray_tooltip_refresh_loop(app.clone());

    Ok(())
}

/// 托盘面板「退出」:有本机 Bot 在跑则拦截并拉起主窗;否则停 Bot、关 runtime、退出进程。
pub async fn tray_panel_quit(app: AppHandle) -> Result<(), String> {
    quit_from_tray(app).await
}

/// 重建主窗时通知可能早于 React 监听；监听就绪后再原子领取一次退出请求。
#[tauri::command]
pub fn consume_pending_tray_exit(window: WebviewWindow) -> bool {
    window.label() == crate::lightweight::MAIN_WINDOW_LABEL
        && window.is_visible().unwrap_or(false)
        && PENDING_TRAY_EXIT.take()
}

/// 托盘面板「释放界面内存」:进轻量模式销毁主 WebView。
pub async fn tray_panel_enter_lightweight(app: AppHandle) -> Result<(), String> {
    crate::chat_window::release_control_panel(&app).await
}

async fn quit_from_tray(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let local_active = exit::local_active_bots(&state).await?;
    if local_active > 0 {
        if PENDING_TRAY_EXIT.request() {
            if let Err(error) = window_show(app.clone()).await {
                PENDING_TRAY_EXIT.take();
                tracing::warn!(%error, "tray exit: main window could not be restored");
                return Err(error);
            }
            if let Err(error) = app.emit_to(
                crate::lightweight::MAIN_WINDOW_LABEL,
                DESKTOP_EXIT_BLOCKED,
                DesktopExitBlocked::new(local_active),
            ) {
                tracing::warn!(%error, "tray exit: confirmation notification failed");
            }
        }
        return Err(exit::exit_blocked_message(local_active));
    }
    PENDING_TRAY_EXIT.take();
    exit::shutdown_and_exit(&app, &state, "tray_panel_quit").await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::PendingTrayExit;

    #[test]
    fn pending_tray_exit_is_delivered_once_after_rebuilding_the_window() {
        let pending = PendingTrayExit::default();
        assert!(!pending.take());
        assert!(pending.request());
        assert!(
            !pending.request(),
            "repeated clicks keep the pending request"
        );
        assert!(
            pending.take(),
            "listener-ready or live notification claims it"
        );
        assert!(
            !pending.take(),
            "the other notification must not reopen the gate"
        );
        assert!(pending.request(), "a later exit request can be delivered");
    }
}
