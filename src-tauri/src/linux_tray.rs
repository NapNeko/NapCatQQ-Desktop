//! Linux 只注册 AppIndicator；系统菜单负责定位，不再与 XEmbed 图标和自绘菜单竞争。

use std::sync::OnceLock;

use tauri::{
    AppHandle,
    menu::{Menu, MenuBuilder, MenuEvent, MenuItem, SubmenuBuilder},
};

use crate::{
    commands::tray::{self, TRAY_ID},
    tray_panel_native,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct BotMenu {
    id: String,
    name: String,
    state: String,
    webui: bool,
    novnc: bool,
}

// 不保留 WebUI token；只有菜单内容变化时才替换，避免关闭正在操作的菜单。
static MENU_ROWS: OnceLock<tokio::sync::Mutex<Option<Vec<BotMenu>>>> = OnceLock::new();

pub fn menu(app: &AppHandle) -> Result<Menu<tauri::Wry>, String> {
    build_menu(app, &[]).map_err(|e| e.to_string())
}

fn build_menu(app: &AppHandle, rows: &[BotMenu]) -> tauri::Result<Menu<tauri::Wry>> {
    let mut menu = MenuBuilder::new(app);
    for row in rows {
        let running = row.state == "running";
        let action = if running { "stop" } else { "start" };
        let label = if running { "停止 Bot" } else { "启动 Bot" };
        let enabled = !matches!(row.state.as_str(), "starting" | "stopping");
        let state = match row.state.as_str() {
            "running" => "运行中",
            "starting" => "启动中",
            "stopping" => "停止中",
            "crashed" => "异常退出",
            _ => "已停止",
        };
        let mut bot =
            SubmenuBuilder::new(app, format!("{} · {state}", row.name)).item(&MenuItem::with_id(
                app,
                format!("{TRAY_ID}:{action}:{}", row.id),
                label,
                enabled,
                None::<&str>,
            )?);
        if row.webui {
            bot = bot.text(format!("{TRAY_ID}:webui:{}", row.id), "打开 WebUI");
        }
        if row.novnc {
            bot = bot.text(format!("{TRAY_ID}:novnc:{}", row.id), "打开 noVNC");
        }
        menu = menu.item(&bot.build()?);
    }
    if !rows.is_empty() {
        menu = menu.separator();
    }
    menu.text("main-tray:show", "显示主窗口")
        .text("main-tray:lightweight", "释放界面内存")
        .separator()
        .text("main-tray:quit", "退出")
        .build()
}

pub async fn refresh(app: &AppHandle) -> Result<(), String> {
    let mut previous = MENU_ROWS
        .get_or_init(|| tokio::sync::Mutex::new(None))
        .lock()
        .await;
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let rows: Vec<_> = tray_panel_native::full_snapshot(app)
        .await
        .cards
        .into_iter()
        .map(|card| BotMenu {
            id: card.bot_id,
            name: card.name,
            state: card.state,
            webui: card.webui.is_some(),
            novnc: card.novnc_available,
        })
        .collect();
    if previous.as_ref() == Some(&rows) {
        return Ok(());
    }
    tray.set_menu(Some(build_menu(app, &rows).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?;
    *previous = Some(rows);
    Ok(())
}

pub fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    let Some(action) = event.id().as_ref().strip_prefix("main-tray:") else {
        return;
    };
    let action = action.to_owned();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = match action.as_str() {
            "show" => tray::window_show(app.clone()).await,
            "lightweight" => tray::tray_panel_enter_lightweight(app.clone()).await,
            "quit" => tray::tray_panel_quit(app.clone()).await,
            _ => match action.split_once(':') {
                Some((verb @ ("start" | "stop"), id)) => {
                    tray_panel_native::start_stop(&app, id, verb == "stop").await
                }
                Some(("webui", id)) => {
                    tray_panel_native::open_napcat_webui(&app, id).await;
                    Ok(())
                }
                Some(("novnc", id)) => {
                    tray_panel_native::open_snowluma_novnc(&app, id).await;
                    Ok(())
                }
                _ => return,
            },
        };
        if let Err(error) = result {
            tracing::warn!(%error, "系统托盘操作失败");
        }
        if let Err(error) = refresh(&app).await {
            tracing::warn!(%error, "更新系统托盘菜单失败");
        }
    });
}
