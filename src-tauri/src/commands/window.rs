// 主窗口几何:对齐 legacy MainWindow._set_window(最小尺寸 + 工作区居中)
// 另含调试台弹出窗(独立工具窗)的创建 / 显示 / 聚焦。

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewWindowBuilder};

/// 调试台弹出窗 label，与 capabilities/main.json 的 windows、前端 main.tsx 的 label 分发一致
pub const DEBUG_WINDOW_LABEL: &str = "debug-console";

/// legacy-python MainWindow._set_window: setMinimumSize(1148, 720) + availableGeometry 居中
const MAIN_MIN_WIDTH: f64 = 1148.0;
const MAIN_MIN_HEIGHT: f64 = 720.0;

/// 在 setup 中调用一次:限制最小尺寸,并按当前显示器工作区居中(排除任务栏)
pub fn apply_main_window_startup_geometry(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口未找到".to_string())?;

    window
        .set_min_size(Some(LogicalSize::new(MAIN_MIN_WIDTH, MAIN_MIN_HEIGHT)))
        .map_err(|e| e.to_string())?;

    center_on_work_area(&window)?;
    Ok(())
}

/// 前端就绪后调用:显示主窗口(避免透明窗口启动闪烁)
#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口未找到".to_string())?;

    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

fn center_on_work_area(window: &tauri::WebviewWindow) -> Result<(), String> {
    let monitor = match window.current_monitor().map_err(|e| e.to_string())? {
        Some(m) => m,
        None => {
            window.center().map_err(|e| e.to_string())?;
            return Ok(());
        }
    };

    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let outer = window.outer_size().map_err(|e| e.to_string())?;

    let win_w = outer.width as f64 / scale;
    let win_h = outer.height as f64 / scale;
    let work_x = work.position.x as f64 / scale;
    let work_y = work.position.y as f64 / scale;
    let work_w = work.size.width as f64 / scale;
    let work_h = work.size.height as f64 / scale;

    let x = work_x + (work_w - win_w) / 2.0;
    let y = work_y + (work_h - win_h) / 2.0;

    window
        .set_position(LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

/// 调试台顶栏「弹出为独立窗口」:已存在就只聚焦,不重复建窗。
/// 新窗直接克隆主窗配置(无边框 + 透明 + Mica + 起步隐藏),换 label 和标题。
#[tauri::command]
pub fn open_debug_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(DEBUG_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return Ok(());
    }
    let mut conf = app
        .config()
        .app
        .windows
        .first()
        .cloned()
        .ok_or_else(|| "tauri.conf.json 未配置 app.windows".to_string())?;
    conf.label = DEBUG_WINDOW_LABEL.to_string();
    conf.title = "调试台 - NapCatQQ Desktop".to_string();
    let window = WebviewWindowBuilder::from_config(&app, &conf)
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    if let Ok(icon) = crate::window_icon::main_window_icon(&app) {
        let _ = window.set_icon(icon);
    }
    Ok(())
}

/// 弹出窗前端画好首帧后调用:显示 + 聚焦。对齐主窗 show_main_window 的时序,
/// 起步隐藏是防透明壳在内容就绪前闪白
#[tauri::command]
pub fn reveal_debug_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(DEBUG_WINDOW_LABEL)
        .ok_or_else(|| "调试台窗口未找到".to_string())?;
    window.show().map_err(|e| e.to_string())?;
    let _ = window.unminimize();
    let _ = window.set_focus();
    Ok(())
}

/// 主窗的调试台入口让位用:弹出窗开着就聚焦它并回 true,主窗不再进调试页。
/// 同一时间只留一个调试台页面(工作区 / 收藏落盘 JSON 是两窗同一份文件)
#[tauri::command]
pub fn focus_debug_window(app: AppHandle) -> bool {
    let Some(window) = app.get_webview_window(DEBUG_WINDOW_LABEL) else {
        return false;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    true
}
