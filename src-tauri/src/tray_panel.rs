// 自绘托盘面板:预创建隐藏的 tray-panel 窗口,右键托盘图标时定位显示,失焦自动收起。
// 对齐旧版托盘右键菜单的三个操作(显示主窗 / 释放内存 / 退出),但视觉走 UI 设计 token。

use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition,
    WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::commands::tray::window_show;

pub const TRAY_PANEL_LABEL: &str = "tray-panel";
const PANEL_WIDTH: f64 = 260.0;
// 高度随内容自适应:右键先按最近一次的测量高摆位,前端展开前再量一次真实高度精调。
// 初次还没量过时用这个兜底（180 包含 Bot 卡片列表与全局操作项）。
const PANEL_FALLBACK_HEIGHT: f64 = 180.0;
const EDGE_GAP: f64 = 10.0;

// 最近一次前端量到的内容高,用于下一次展开先摆个大概位置。
static LAST_CONTENT_HEIGHT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new((PANEL_FALLBACK_HEIGHT * 100.0) as u32);

fn last_content_height() -> f64 {
    std::sync::atomic::AtomicU32::load(&LAST_CONTENT_HEIGHT, std::sync::atomic::Ordering::Relaxed)
        as f64
        / 100.0
}

// 托盘点击坐标,resize 命令回来精调时按此重新定位。保持最新锚点，绝不丢弃。
static LAST_ANCHOR: std::sync::Mutex<Option<(f64, f64)>> = std::sync::Mutex::new(None);

/// 面板即托盘唯一入口,轻量模式下也必须可用,所以随 app 常驻,不走 WebView 销毁。
pub fn ensure_tray_panel_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window(TRAY_PANEL_LABEL) {
        return Ok(w);
    }
    let window = WebviewWindowBuilder::new(app, TRAY_PANEL_LABEL, WebviewUrl::App("/".into()))
        .title("NapCatQQ Desktop 托盘")
        .inner_size(PANEL_WIDTH, PANEL_FALLBACK_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| e.to_string())?;

    let window_for_event = window.clone();
    window.on_window_event(move |event| {
        // 点击面板外区域失焦 = 用户想收起,等价于旧菜单的"点别处关闭"。
        if let WindowEvent::Focused(false) = event {
            if crate::lightweight::is_lightweight_mode() {
                let _ = window_for_event.destroy();
            } else {
                let _ = window_for_event.hide();
            }
        }
    });
    Ok(window)
}

/// 把面板摆到托盘图标附近。坐标来自 TrayIconEvent,含物理像素与显示器。
/// 水平对到图标中心;垂直按图标所在半区决定面板弹在图标上方还是下方。
fn position_tray_panel(
    window: &WebviewWindow,
    click: PhysicalPosition<f64>,
    height: f64,
) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let click_x = click.x / scale;
    let click_y = click.y / scale;
    let x = click_x - PANEL_WIDTH / 2.0;
    let mut y = click_y - height - EDGE_GAP;
    if let Some(monitor) = window.current_monitor().map_err(|e| e.to_string())? {
        let mon_y = monitor.position().y as f64 / scale;
        let mon_h = monitor.size().height as f64 / scale;
        if click_y < mon_y + mon_h / 2.0 {
            y = click_y + EDGE_GAP;
        }
    }
    let _ = window.set_position(LogicalPosition::new(x, y));
    Ok(())
}

/// 右键展开:记下锚点,先按上次的内容高摆位显示,前端展开前量好真实高再 resize 精调。
pub fn show_tray_panel_at(app: &AppHandle, position: PhysicalPosition<f64>) -> Result<(), String> {
    let window = ensure_tray_panel_window(app)?;
    if let Ok(mut slot) = LAST_ANCHOR.lock() {
        *slot = Some((position.x, position.y));
    }
    let height = last_content_height();
    let _ = window.set_size(LogicalSize::new(PANEL_WIDTH, height));
    position_tray_panel(&window, position, height)?;
    let _ = window.emit("tray_panel_show", ());
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

/// 前端在进场动画前量完 scrollHeight 调这个:记高、调窗高、按锚点重摆,消掉展开时的跳动。
#[tauri::command]
pub fn tray_panel_resize(app: AppHandle, height: f64) -> Result<(), String> {
    // 下限对齐"状态行+三命令"的最小内容高,挡掉脏数据把面板压没。
    if !(60.0..=800.0).contains(&height) {
        return Ok(());
    }
    std::sync::atomic::AtomicU32::store(
        &LAST_CONTENT_HEIGHT,
        (height * 100.0) as u32,
        std::sync::atomic::Ordering::Relaxed,
    );
    let Some(window) = app.get_webview_window(TRAY_PANEL_LABEL) else {
        return Ok(());
    };
    let anchor = LAST_ANCHOR.lock().ok().and_then(|s| *s);
    let _ = window.set_size(LogicalSize::new(PANEL_WIDTH, height));
    if let Some((ax, ay)) = anchor {
        let _ = position_tray_panel(&window, PhysicalPosition::new(ax, ay), height);
    }
    Ok(())
}

pub fn hide_tray_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(TRAY_PANEL_LABEL) {
        if crate::lightweight::is_lightweight_mode() {
            let _ = w.destroy();
        } else {
            let _ = w.hide();
        }
    }
}

/// 托盘图标左键=显示主窗,右键=自绘面板。取代了原 .menu() 原生菜单。
pub fn handle_tray_icon_event(tray: &tauri::tray::TrayIcon, event: TrayIconEvent) {
    let TrayIconEvent::Click {
        button,
        button_state,
        position,
        ..
    } = &event
    else {
        return;
    };
    if *button_state != MouseButtonState::Up {
        return;
    }
    let app = tray.app_handle().clone();
    match button {
        MouseButton::Left => {
            tauri::async_runtime::spawn(async move {
                let _ = window_show(app).await;
            });
        }
        MouseButton::Right => {
            let _ = show_tray_panel_at(&app, *position);
        }
        _ => {}
    }
}
