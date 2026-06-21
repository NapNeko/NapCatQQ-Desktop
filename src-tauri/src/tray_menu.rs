// 托盘右键菜单:原生控件内做层次(抬头 / 状态 / 分组),操作项纯文字

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Manager};

use crate::commands::tray::{MENU_LIGHTWEIGHT, MENU_QUIT, MENU_SHOW};

const MENU_HEADER: &str = "tray-header";
const MENU_STATUS: &str = "tray-status";

struct TrayMenuHandles {
    status: MenuItem<tauri::Wry>,
}

static TRAY_MENU: Mutex<Option<TrayMenuHandles>> = Mutex::new(None);

/// 菜单内状态行(比 tooltip 短,不重复产品名)
pub async fn tray_menu_status_line(app: &AppHandle) -> String {
    let state = app.state::<crate::AppState>();
    let running = state.bot_manager.active_count().await;
    if crate::lightweight::is_lightweight_mode() {
        if running > 0 {
            return format!("轻量模式 · {running} 个 Bot 运行中");
        }
        return "轻量模式 · 后台托管".to_string();
    }
    if running > 0 {
        return format!("{running} 个 Bot 运行中");
    }
    "后台待命".to_string()
}

pub async fn build_tray_menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, String> {
    let status_text = tray_menu_status_line(app).await;

    let header = MenuItem::with_id(
        app,
        MENU_HEADER,
        "NapCatQQ Desktop",
        false,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;

    let status = MenuItem::with_id(
        app,
        MENU_STATUS,
        status_text.as_str(),
        false,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;

    let sep_top = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;

    let show = MenuItem::with_id(app, MENU_SHOW, "显示主窗口", true, None::<&str>)
        .map_err(|e| e.to_string())?;

    let lightweight = MenuItem::with_id(
        app,
        MENU_LIGHTWEIGHT,
        "释放界面内存…",
        true,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;

    let sep_bottom = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;

    let quit = MenuItem::with_id(
        app,
        MENU_QUIT,
        "退出 NapCatQQ Desktop",
        true,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;

    let menu = Menu::with_items(
        app,
        &[
            &header,
            &status,
            &sep_top,
            &show,
            &lightweight,
            &sep_bottom,
            &quit,
        ],
    )
    .map_err(|e| e.to_string())?;

    if let Ok(mut slot) = TRAY_MENU.lock() {
        *slot = Some(TrayMenuHandles { status });
    }

    Ok(menu)
}

pub async fn refresh_tray_menu_status(app: &AppHandle) -> Result<(), String> {
    let text = tray_menu_status_line(app).await;
    let guard = TRAY_MENU
        .lock()
        .map_err(|_| "托盘菜单锁异常".to_string())?;
    let Some(handles) = guard.as_ref() else {
        return Ok(());
    };
    handles.status.set_text(text).map_err(|e| e.to_string())
}