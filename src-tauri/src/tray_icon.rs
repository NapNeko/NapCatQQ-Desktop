// 托盘图标：任务栏小尺寸专用资源，与窗口 ICO 区分；运行中 Bot 时换带状态点的变体。

use std::path::PathBuf;

use tauri::image::Image;
use tauri::AppHandle;
use tauri::Manager;

const TRAY_DIR: &str = "icons/tray";

fn tray_png_path(app: &AppHandle, name: &str) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?;
    let p = dir.join(TRAY_DIR).join(name);
    if p.is_file() {
        return Some(p);
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev = manifest_dir.join(TRAY_DIR).join(name);
    if dev.is_file() {
        return Some(dev);
    }
    None
}

fn load_png_at(path: &std::path::Path) -> Option<Image<'static>> {
    Image::from_path(path).ok().map(|img| img.to_owned())
}

fn load_png(app: &AppHandle, name: &str) -> Option<Image<'static>> {
    let path = tray_png_path(app, name)?;
    load_png_at(&path)
}

fn load_bundle_app_icon(app: &AppHandle) -> Option<Image<'static>> {
    let names = ["32x32.png", "64x64.png", "128x128.png"];
    for name in names {
        if let Some(dir) = app.path().resource_dir().ok() {
            let p = dir.join("icons").join(name);
            if let Some(img) = load_png_at(&p) {
                return Some(img);
            }
        }
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons").join(name);
        if let Some(img) = load_png_at(&dev) {
            return Some(img);
        }
    }
    None
}

fn embed_tray_png(name: &str) -> Option<Image<'static>> {
    let bytes: &[u8] = match name {
        "tray-32.png" => include_bytes!("../icons/tray/tray-32.png").as_ref(),
        "tray-16.png" => include_bytes!("../icons/tray/tray-16.png").as_ref(),
        "tray-24.png" => include_bytes!("../icons/tray/tray-24.png").as_ref(),
        "tray-active-32.png" => include_bytes!("../icons/tray/tray-active-32.png").as_ref(),
        _ => return None,
    };
    Image::from_bytes(bytes).ok().map(|img| img.to_owned())
}

/// 主托盘图标（无运行中 Bot、非轻量）。优先 32/16 原生尺寸，避免 48→16 二次缩放发糊。
pub fn idle_tray_icon(app: &AppHandle) -> Result<Image<'static>, String> {
    load_png(app, "tray-32.png")
        .or_else(|| load_png(app, "tray-16.png"))
        .or_else(|| load_png(app, "tray-24.png"))
        .or_else(|| load_png(app, "tray-20.png"))
        .or_else(|| embed_tray_png("tray-32.png"))
        .or_else(|| embed_tray_png("tray-16.png"))
        .or_else(|| embed_tray_png("tray-24.png"))
        .or_else(|| load_bundle_app_icon(app))
        .ok_or_else(|| {
            "托盘图标缺失：请在项目根执行 python script/generate_app_icons.py".to_string()
        })
}

/// 有 Bot 运行中：带暖色状态点，与主题 brand 色一致。
pub fn active_tray_icon(app: &AppHandle) -> Image<'static> {
    load_png(app, "tray-active-32.png")
        .or_else(|| load_png(app, "tray-active-16.png"))
        .or_else(|| load_png(app, "tray-active-24.png"))
        .or_else(|| load_png(app, "tray-active-20.png"))
        .or_else(|| embed_tray_png("tray-active-32.png"))
        .or_else(|| embed_tray_png("tray-32.png"))
        .unwrap_or_else(|| {
            idle_tray_icon(app).unwrap_or_else(|_| {
                embed_tray_png("tray-32.png").expect("embed tray-32")
            })
        })
}

/// 轻量模式：略降饱和，与 tooltip「轻量」一致。
pub fn lightweight_tray_icon(app: &AppHandle) -> Image<'static> {
    load_png(app, "tray-light-32.png")
        .or_else(|| load_png(app, "tray-light-16.png"))
        .or_else(|| load_png(app, "tray-light-24.png"))
        .or_else(|| load_png(app, "tray-light-20.png"))
        .unwrap_or_else(|| active_tray_icon(app))
}

/// 按当前 Bot / 轻量状态选择托盘图。
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

/// 刷新托盘图标（与 tooltip 同周期调用即可）。
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