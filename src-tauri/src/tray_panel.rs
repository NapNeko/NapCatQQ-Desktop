// 主托盘图标事件：左键显示主窗，右键弹原生面板（tray_panel_native）。

use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

use crate::commands::tray::window_show;

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
            #[cfg(windows)]
            crate::tray_panel_native::open(&app, (position.x as i32, position.y as i32));
            #[cfg(not(windows))]
            let _ = position;
        }
        _ => {}
    }
}
