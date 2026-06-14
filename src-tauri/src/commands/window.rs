// 主窗口几何：对齐 legacy MainWindow._set_window（最小尺寸 + 工作区居中）。

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager};

/// legacy-python MainWindow._set_window: setMinimumSize(1148, 720) + availableGeometry 居中。
const MAIN_MIN_WIDTH: f64 = 1148.0;
const MAIN_MIN_HEIGHT: f64 = 720.0;

/// 在 setup 中调用一次：限制最小尺寸，并按当前显示器工作区居中（排除任务栏）。
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

/// 前端就绪后调用：显示主窗口（避免透明窗口启动闪烁）。
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