//! 账号托盘共用一个按需创建的面板，悬停预览不夺取聊天焦点。
use std::{sync::{Mutex, atomic::{AtomicU64, Ordering}}, time::{Duration, Instant}};
use ncd_domain::chat_desktop::{ChatTrayPanelAction, ChatTrayPanelData};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Webview, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "chat-tray-panel";
const WIDTH: f64 = crate::tray_panel::PANEL_WIDTH;
const CHANGED: &str = crate::window_events::CHAT_TRAY_PANEL_CHANGED;

#[derive(Clone)]
struct Request {
    generation: u32,
    bot: String,
    qq: String,
    anchor: PhysicalPosition<f64>,
    menu: bool,
}

#[derive(Default)]
pub struct ChatTrayPanelState {
    gate: tokio::sync::Mutex<()>,
    request: Mutex<Option<Request>>,
    serial: std::sync::atomic::AtomicU32,
    hover: AtomicU64,
    watching: std::sync::atomic::AtomicU32,
}

fn request(app: &AppHandle) -> Option<Request> {
    app.state::<ChatTrayPanelState>().request.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

fn check_source(webview: &Webview) -> Result<(), String> {
    if webview.label() == LABEL { Ok(()) } else { Err("托盘面板来源无效".into()) }
}

#[cfg_attr(windows, expect(unsafe_code, reason = "Win32 显隐在所属 UI 线程执行，悬停不能激活窗口"))]
async fn visibility(window: &tauri::WebviewWindow, visible: bool, focus: bool) -> Result<(), String> {
    #[cfg(windows)] {
        use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE}};
        let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
        let (send, receive) = tokio::sync::oneshot::channel();
        window.run_on_main_thread(move || {
            // Tao 再次 show 会激活窗口，此面板的显隐统一走 HWND。
            // SAFETY: HWND 来自尚存的 WebviewWindow，操作在其 UI 线程执行。
            unsafe {
                let _ = ShowWindow(HWND(hwnd as _), if !visible { SW_HIDE } else if focus { SW_SHOW } else { SW_SHOWNOACTIVATE });
                if visible && focus { let _ = SetForegroundWindow(HWND(hwnd as _)); }
            }
            let _ = send.send(());
        }).map_err(|e| e.to_string())?;
        receive.await.map_err(|e| e.to_string())?;
    }
    #[cfg(not(windows))] {
        if visible {
            window.set_focusable(focus).map_err(|e| e.to_string())?;
            window.show().map_err(|e| e.to_string())?;
            if focus { window.set_focus().map_err(|e| e.to_string())?; }
        } else { window.hide().map_err(|e| e.to_string())?; }
    }
    Ok(())
}

pub fn cancel_hover(app: &AppHandle) {
    app.state::<ChatTrayPanelState>().hover.fetch_add(1, Ordering::Relaxed);
}

pub fn enter(app: AppHandle, bot: String, qq: String, anchor: PhysicalPosition<f64>) {
    if request(&app).is_some_and(|r| r.menu) { return; }
    let ticket = app.state::<ChatTrayPanelState>().hover.fetch_add(1, Ordering::Relaxed) + 1;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(240)).await;
        if app.state::<ChatTrayPanelState>().hover.load(Ordering::Relaxed) != ticket { return; }
        if let Err(e) = show(&app, bot, qq, anchor, false).await { tracing::warn!("chat tray hover: {e}"); }
    });
}

pub async fn show(app: &AppHandle, bot: String, qq: String, anchor: PhysicalPosition<f64>, menu: bool) -> Result<(), String> {
    let state = app.state::<ChatTrayPanelState>();
    let _gate = state.gate.lock().await;
    if !menu && request(app).is_some_and(|r| r.menu) { return Ok(()); }
    cancel_hover(app);
    app.state::<crate::AppState>().chat.tray_snapshot(&bot, &qq).await?;
    let generation = state.serial.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let next = Request { generation, bot, qq, anchor, menu };
    let window = match app.get_webview_window(LABEL) {
        Some(window) => { visibility(&window, false, false).await?; window }
        None => {
            let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("/".into()))
                .title("聊天托盘").inner_size(WIDTH, 280.0).resizable(false)
                .maximizable(false).minimizable(false).decorations(false)
                .always_on_top(true).skip_taskbar(true).visible(false).focused(false).focusable(menu)
                .build().map_err(|e| e.to_string())?;
            let handle = app.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let app = handle.clone();
                    if let Some(current) = request(&app) {
                        tauri::async_runtime::spawn(async move { hide(&app, current.generation).await; });
                    }
                } else if matches!(event, tauri::WindowEvent::Focused(false)) {
                    let app = handle.clone();
                    let current = request(&app).filter(|r| r.menu).map(|r| r.generation);
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                        if let Some(id) = current
                            && app.get_webview_window(LABEL).is_some_and(|window|
                                window.is_visible().unwrap_or(false) && !window.is_focused().unwrap_or(false)) {
                            hide(&app, id).await;
                        }
                    });
                }
            });
            window
        }
    };
    *state.request.lock().unwrap_or_else(|p| p.into_inner()) = Some(next);
    window.emit(CHANGED, crate::window_events::WindowSignal::V1).map_err(|e| e.to_string())?;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        if request(&handle).is_some_and(|r| r.generation == generation)
            && handle.get_webview_window(LABEL).is_some_and(|w| !w.is_visible().unwrap_or(false)) {
            hide(&handle, generation).await;
        }
    });
    Ok(())
}

pub async fn hide(app: &AppHandle, generation: u32) {
    let state = app.state::<ChatTrayPanelState>();
    let _gate = state.gate.lock().await;
    if !request(app).is_some_and(|r| r.generation == generation) { return; }
    *state.request.lock().unwrap_or_else(|p| p.into_inner()) = None;
    if let Some(window) = app.get_webview_window(LABEL) {
        if let Err(error) = visibility(&window, false, false).await { tracing::warn!("hide chat tray: {error}"); }
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // 短时间再次点开复用 WebView；闲置后销毁，后台只留下 Rust 收件箱。
        tokio::time::sleep(Duration::from_secs(15)).await;
        let state = app.state::<ChatTrayPanelState>();
        let _gate = state.gate.lock().await;
        if request(&app).is_none() && state.serial.load(Ordering::Relaxed) == generation {
            if let Some(window) = app.get_webview_window(LABEL) { let _ = window.destroy(); }
        }
    });
}

pub async fn refresh(app: &AppHandle) {
    let Some(current) = request(app) else { return; };
    if app.state::<crate::AppState>().chat.tray_snapshot(&current.bot, &current.qq).await.is_err() {
        hide(app, current.generation).await;
    } else if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.emit(CHANGED, crate::window_events::WindowSignal::V1);
    }
}

#[tauri::command]
pub async fn chat_tray_panel_data(app: AppHandle, webview: Webview) -> Result<Option<ChatTrayPanelData>, String> {
    check_source(&webview)?;
    let Some(current) = request(&app) else { return Ok(None); };
    let snapshot = app.state::<crate::AppState>().chat.tray_snapshot(&current.bot, &current.qq).await?;
    Ok(Some(ChatTrayPanelData { v: 1, generation: current.generation, menu: current.menu, snapshot }))
}

#[tauri::command]
pub async fn chat_tray_panel_hide(app: AppHandle, webview: Webview, generation: u32) -> Result<(), String> {
    check_source(&webview)?;
    hide(&app, generation).await;
    Ok(())
}

#[tauri::command]
pub async fn chat_tray_panel_action(app: AppHandle, webview: Webview, generation: u32, action: ChatTrayPanelAction, session: Option<String>) -> Result<(), String> {
    check_source(&webview)?;
    let current = request(&app).filter(|r| r.generation == generation).ok_or("托盘面板已切换")?;
    let state = app.state::<crate::AppState>();
    let snapshot = state.chat.tray_snapshot(&current.bot, &current.qq).await?;
    match action {
        ChatTrayPanelAction::Background => {
            state.migrate_gate.ensure_idle()?;
            state.chat.update_tray_preference(&current.bot, &current.qq, Some(!snapshot.account.preference.background), None).await?;
            refresh(&app).await;
            return Ok(());
        }
        ChatTrayPanelAction::Hide => {
            state.migrate_gate.ensure_idle()?;
            state.chat.update_tray_preference(&current.bot, &current.qq, None, Some(false)).await?;
        }
        ChatTrayPanelAction::Open => crate::chat_window::open_from_tray(app.clone(), current.bot, current.qq, session).await?,
        ChatTrayPanelAction::Console => crate::commands::tray::window_show(app.clone()).await?,
    }
    hide(&app, generation).await;
    Ok(())
}

fn panel_position(anchor: (f64, f64), size: (f64, f64), work: (f64, f64, f64, f64), gap: f64) -> (f64, f64) {
    let (left, top, width, height) = work;
    let x = (anchor.0 - size.0 + 20.0).clamp(left, (left + width - size.0).max(left));
    let above = anchor.1 - size.1 - gap;
    let y = if above >= top { above } else { anchor.1 + gap };
    (x, y.clamp(top, (top + height - size.1).max(top)))
}

#[tauri::command]
pub async fn chat_tray_panel_ready(app: AppHandle, webview: Webview, generation: u32, height: f64) -> Result<(), String> {
    check_source(&webview)?;
    if !height.is_finite() || !(80.0..=720.0).contains(&height) { return Err("托盘面板高度无效".into()); }
    let state = app.state::<ChatTrayPanelState>();
    let _gate = state.gate.lock().await;
    let Some(current) = request(&app).filter(|r| r.generation == generation) else { return Ok(()); };
    let window = app.get_webview_window(LABEL).ok_or("托盘面板已关闭")?;
    let monitor = app.monitor_from_point(current.anchor.x, current.anchor.y).map_err(|e| e.to_string())?
        .or(app.primary_monitor().map_err(|e| e.to_string())?).ok_or("无法读取显示器")?;
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let size = (WIDTH * scale, (height * scale).min(f64::from(work.size.height)));
    let position = panel_position((current.anchor.x, current.anchor.y), size,
        (f64::from(work.position.x), f64::from(work.position.y), f64::from(work.size.width), f64::from(work.size.height)), 18.0 * scale);
    window.set_position(PhysicalPosition::new(position.0.round() as i32, position.1.round() as i32)).map_err(|e| e.to_string())?;
    window.set_size(PhysicalSize::new(size.0.round() as u32, size.1.round() as u32)).map_err(|e| e.to_string())?;
    let was_visible = window.is_visible().unwrap_or(false);
    if !current.menu && !was_visible && app.cursor_position().ok().is_some_and(|cursor|
        (cursor.x - current.anchor.x).abs() > 24.0 * scale || (cursor.y - current.anchor.y).abs() > 24.0 * scale) {
        drop(_gate);
        hide(&app, generation).await;
        return Ok(());
    }
    window.set_focusable(current.menu).map_err(|e| e.to_string())?;
    if !was_visible { visibility(&window, true, current.menu).await?; }
    if !current.menu && state.watching.swap(generation, Ordering::Relaxed) != generation {
        watch_pointer(app.clone(), current, scale);
    }
    Ok(())
}

fn watch_pointer(app: AppHandle, current: Request, scale: f64) {
    tauri::async_runtime::spawn(async move {
        let mut outside = None;
        loop {
            tokio::time::sleep(Duration::from_millis(80)).await;
            if !request(&app).is_some_and(|r| r.generation == current.generation) { return; }
            let Some(window) = app.get_webview_window(LABEL) else { return; };
            let inside = app.cursor_position().ok().is_some_and(|cursor| {
                let at_icon = (cursor.x - current.anchor.x).abs() <= 24.0 * scale && (cursor.y - current.anchor.y).abs() <= 24.0 * scale;
                let at_panel = window.outer_position().ok().zip(window.outer_size().ok()).is_some_and(|(origin, size)| {
                    cursor.x >= f64::from(origin.x) && cursor.x <= f64::from(origin.x) + f64::from(size.width)
                        && cursor.y >= f64::from(origin.y) && cursor.y <= f64::from(origin.y) + f64::from(size.height)
                });
                at_icon || at_panel
            });
            if inside { outside = None; }
            else if outside.get_or_insert_with(Instant::now).elapsed() >= Duration::from_millis(320) {
                hide(&app, current.generation).await;
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::panel_position;
    #[test]
    fn stays_in_work_area_on_negative_and_scaled_monitors() {
        assert_eq!(panel_position((-10.0, 1060.0), (304.0, 360.0), (-1920.0, 0.0, 1920.0, 1040.0), 18.0), (-304.0, 680.0));
        assert_eq!(panel_position((3800.0, 2120.0), (608.0, 720.0), (0.0, 0.0, 3840.0, 2080.0), 36.0), (3212.0, 1360.0));
    }
    #[test]
    fn opens_below_top_taskbar_and_clamps_tall_panels() {
        assert_eq!(panel_position((20.0, 20.0), (304.0, 360.0), (0.0, 40.0, 1920.0, 1040.0), 18.0), (0.0, 40.0));
        assert_eq!(panel_position((100.0, 200.0), (304.0, 720.0), (0.0, 0.0, 800.0, 600.0), 18.0), (0.0, 0.0));
    }
}
