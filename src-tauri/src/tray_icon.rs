// 托盘图标:任务栏小尺寸专用资源,与窗口 ICO 区分;运行中 Bot 时换带状态点的变体
//
// 全部 embed 进二进制,安装目录不再落 icons/tray。旧版 MSI 若残留 icons 目录,
// 由 wix/legacy_install_cleanup 白名单删掉。

use tauri::AppHandle;
use tauri::Manager;
use tauri::image::Image;

fn embed_tray_png(name: &str) -> Option<Image<'static>> {
    let bytes: &[u8] = match name {
        "tray-16.png" => include_bytes!("../icons/tray/tray-16.png").as_ref(),
        "tray-20.png" => include_bytes!("../icons/tray/tray-20.png").as_ref(),
        "tray-24.png" => include_bytes!("../icons/tray/tray-24.png").as_ref(),
        "tray-32.png" => include_bytes!("../icons/tray/tray-32.png").as_ref(),
        "tray-48.png" => include_bytes!("../icons/tray/tray-48.png").as_ref(),
        "tray-active-16.png" => include_bytes!("../icons/tray/tray-active-16.png").as_ref(),
        "tray-active-20.png" => include_bytes!("../icons/tray/tray-active-20.png").as_ref(),
        "tray-active-24.png" => include_bytes!("../icons/tray/tray-active-24.png").as_ref(),
        "tray-active-32.png" => include_bytes!("../icons/tray/tray-active-32.png").as_ref(),
        "tray-active-48.png" => include_bytes!("../icons/tray/tray-active-48.png").as_ref(),
        "tray-light-16.png" => include_bytes!("../icons/tray/tray-light-16.png").as_ref(),
        "tray-light-20.png" => include_bytes!("../icons/tray/tray-light-20.png").as_ref(),
        "tray-light-24.png" => include_bytes!("../icons/tray/tray-light-24.png").as_ref(),
        "tray-light-32.png" => include_bytes!("../icons/tray/tray-light-32.png").as_ref(),
        "tray-light-48.png" => include_bytes!("../icons/tray/tray-light-48.png").as_ref(),
        _ => return None,
    };
    Image::from_bytes(bytes).ok().map(|img| img.to_owned())
}

fn load_png(name: &str) -> Option<Image<'static>> {
    embed_tray_png(name)
}

/// 主托盘图标(无运行中 Bot,非轻量)优先 32/16 原生尺寸,避免 48→16 二次缩放发糊
pub fn idle_tray_icon(_app: &AppHandle) -> Result<Image<'static>, String> {
    load_png("tray-32.png")
        .or_else(|| load_png("tray-16.png"))
        .or_else(|| load_png("tray-24.png"))
        .or_else(|| load_png("tray-20.png"))
        .or_else(|| load_png("tray-48.png"))
        .ok_or_else(|| {
            "托盘图标缺失：请在项目根执行 python script/generate_app_icons.py".to_string()
        })
}

/// 有 Bot 运行中:带暖色状态点,与主题 brand 色一致
pub fn active_tray_icon(app: &AppHandle) -> Image<'static> {
    load_png("tray-active-32.png")
        .or_else(|| load_png("tray-active-16.png"))
        .or_else(|| load_png("tray-active-24.png"))
        .or_else(|| load_png("tray-active-20.png"))
        .or_else(|| load_png("tray-active-48.png"))
        .or_else(|| load_png("tray-32.png"))
        .unwrap_or_else(|| {
            idle_tray_icon(app).unwrap_or_else(|_| load_png("tray-32.png").expect("embed tray-32"))
        })
}

/// 轻量模式:略降饱和,与 tooltip「轻量」一致
pub fn lightweight_tray_icon(app: &AppHandle) -> Image<'static> {
    load_png("tray-light-32.png")
        .or_else(|| load_png("tray-light-16.png"))
        .or_else(|| load_png("tray-light-24.png"))
        .or_else(|| load_png("tray-light-20.png"))
        .or_else(|| load_png("tray-light-48.png"))
        .unwrap_or_else(|| active_tray_icon(app))
}

/// 按当前 Bot / 轻量状态选择托盘图
pub async fn icon_for_state(
    app: &AppHandle,
    running_bots: usize,
    lightweight: bool,
) -> Image<'static> {
    if lightweight {
        return lightweight_tray_icon(app);
    }
    if running_bots > 0 {
        return active_tray_icon(app);
    }
    idle_tray_icon(app).unwrap_or_else(|_| active_tray_icon(app))
}

/// 刷新托盘图标(与 tooltip 同周期调用即可)
pub async fn refresh_tray_icon(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let running = state.bot_manager.active_count().await;
    let lightweight = crate::lightweight::is_lightweight_mode();
    let icon = icon_for_state(app, running, lightweight).await;
    let tray = app
        .tray_by_id(crate::commands::tray::TRAY_ID)
        .ok_or_else(|| "托盘未初始化".to_string())?;
    tray.set_icon(Some(icon)).map_err(|e| e.to_string())
}
