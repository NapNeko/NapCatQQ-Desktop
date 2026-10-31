// 托盘小尺寸资源与窗口 ICO 分开；状态图直接内嵌，解码失败返回调用方。

use tauri::{AppHandle, Manager, image::Image};

#[derive(Clone, Copy)]
enum IconState {
    Idle,
    Active,
    Lightweight,
}

fn load_icon(state: IconState) -> Result<Image<'static>, String> {
    // Linux 面板由桌面环境缩放，48px 可覆盖高 DPI；Windows 沿用专用 32px 图。
    let bytes: &[u8] = match (state, cfg!(target_os = "linux")) {
        (IconState::Idle, false) => include_bytes!("../icons/tray/tray-32.png"),
        (IconState::Idle, true) => include_bytes!("../icons/tray/tray-48.png"),
        (IconState::Active, false) => include_bytes!("../icons/tray/tray-active-32.png"),
        (IconState::Active, true) => include_bytes!("../icons/tray/tray-active-48.png"),
        (IconState::Lightweight, false) => include_bytes!("../icons/tray/tray-light-32.png"),
        (IconState::Lightweight, true) => include_bytes!("../icons/tray/tray-light-48.png"),
    };
    Image::from_bytes(bytes).map_err(|error| format!("解码内置托盘图标失败：{error}"))
}

pub fn idle_tray_icon(_app: &AppHandle) -> Result<Image<'static>, String> {
    load_icon(IconState::Idle)
}

/// 与 tooltip 同步更新，不依赖 WebView。
pub async fn refresh_tray_icon(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let running = state.bot_manager.active_count().await;
    let icon_state = if crate::lightweight::is_lightweight_mode() {
        IconState::Lightweight
    } else if running > 0 {
        IconState::Active
    } else {
        IconState::Idle
    };
    let icon = load_icon(icon_state)?;
    let tray = app
        .tray_by_id(crate::commands::tray::TRAY_ID)
        .ok_or_else(|| "托盘未初始化".to_string())?;
    tray.set_icon(Some(icon)).map_err(|error| error.to_string())
}
