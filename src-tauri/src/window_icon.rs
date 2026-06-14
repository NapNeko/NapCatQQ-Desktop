// 任务栏 / Alt+Tab / 窗口标题区图标（与通知区托盘无关）。

use std::path::PathBuf;

use tauri::image::Image;
use tauri::AppHandle;
use tauri::Manager;

const ICON_DIR: &str = "icons";

fn icon_path(app: &AppHandle, name: &str) -> Option<PathBuf> {
    if let Ok(dir) = app.path().resource_dir() {
        let p = dir.join(ICON_DIR).join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ICON_DIR).join(name);
    if dev.is_file() {
        return Some(dev);
    }
    None
}

fn load_png(name: &str, app: &AppHandle) -> Option<Image<'static>> {
    let path = icon_path(app, name)?;
    Image::from_path(&path).ok().map(|i| i.to_owned())
}

fn embed(name: &str) -> Option<Image<'static>> {
    let bytes: &[u8] = match name {
        "32x32.png" => include_bytes!("../icons/32x32.png").as_ref(),
        "64x64.png" => include_bytes!("../icons/64x64.png").as_ref(),
        _ => return None,
    };
    Image::from_bytes(bytes).ok().map(|i| i.to_owned())
}

/// 供主窗口与轻量模式重建窗口使用；优先 32 逻辑像素，兼顾高 DPI。
pub fn main_window_icon(app: &AppHandle) -> Result<Image<'static>, String> {
    load_png("32x32.png", app)
        .or_else(|| load_png("64x64.png", app))
        .or_else(|| load_png("128x128.png", app))
        .or_else(|| embed("32x32.png"))
        .or_else(|| embed("64x64.png"))
        .ok_or_else(|| "窗口图标缺失：请执行 python script/generate_app_icons.py".to_string())
}

pub fn apply_main_window_icon(app: &AppHandle) -> Result<(), String> {
    let icon = main_window_icon(app)?;
    if let Some(window) = app.get_webview_window(crate::lightweight::MAIN_WINDOW_LABEL) {
        window.set_icon(icon).map_err(|e| e.to_string())?;
    }
    Ok(())
}