// 托盘 tooltip:Bot 运行摘要(不依赖 WebView)

use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

use crate::commands::tray::TRAY_ID;
use crate::lightweight;

static TRAY_REFRESH_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

pub async fn tray_tooltip_text(app: &AppHandle) -> String {
    let state = app.state::<crate::AppState>();
    let running = state.bot_manager.active_count().await;
    let mut line = if running == 0 {
        "NapCatQQ Desktop · 后台待命".to_string()
    } else {
        format!("NapCatQQ Desktop · {running} 个 Bot 运行中")
    };
    if lightweight::is_lightweight_mode() {
        line.push_str(" · 轻量");
    }
    line
}

pub async fn refresh_tray_tooltip(app: &AppHandle) -> Result<(), String> {
    let _guard = TRAY_REFRESH_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let text = tray_tooltip_text(app).await;
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "托盘未初始化".to_string())?;
    tray.set_tooltip(Some(text)).map_err(|e| e.to_string())?;
    crate::tray_icon::refresh_tray_icon(app).await?;
    crate::tray_menu::refresh_tray_menu_status(app).await
}

pub fn spawn_tray_tooltip_refresh_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(45));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // tokio interval 的第一跳会立即返回；启动期已有一次主动刷新。
        interval.tick().await;
        loop {
            interval.tick().await;
            if app.tray_by_id(TRAY_ID).is_some() {
                let _ = refresh_tray_tooltip(&app).await;
            }
        }
    });
}
