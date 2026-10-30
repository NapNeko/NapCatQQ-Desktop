// 主窗口几何：Linux 按显示器工作区适配，其它平台保留最小尺寸与居中策略。
// 另含调试台弹出窗(独立工具窗)的创建 / 显示 / 聚焦。

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewWindowBuilder};

/// 调试台弹出窗 label，与 capabilities/main.json 的 windows、前端 main.tsx 的 label 分发一致
pub const DEBUG_WINDOW_LABEL: &str = "debug-console";

/// legacy-python MainWindow._set_window: setMinimumSize(1148, 720) + availableGeometry 居中
const MAIN_MIN_WIDTH: f64 = 1148.0;
const MAIN_MIN_HEIGHT: f64 = 720.0;

/// 启动及轻量模式重建主窗时应用，已有窗口仅恢复显示时不重设用户尺寸。
pub fn apply_main_window_startup_geometry(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口未找到".to_string())?;

    #[cfg(target_os = "linux")]
    {
        fit_linux_main_window(&window)
    }
    #[cfg(not(target_os = "linux"))]
    {
        window
            .set_min_size(Some(LogicalSize::new(MAIN_MIN_WIDTH, MAIN_MIN_HEIGHT)))
            .map_err(|e| e.to_string())?;
        center_on_work_area(&window)
    }
}

#[cfg(target_os = "linux")]
struct MainWindowGeometry {
    size: LogicalSize<f64>,
    min_size: LogicalSize<f64>,
    position: LogicalPosition<f64>,
}

#[cfg(target_os = "linux")]
fn linux_startup_geometry(work: &tauri::PhysicalRect<i32, u32>, scale: f64) -> MainWindowGeometry {
    // 系统 DPI 只换算一次；WebView 的 uiScale 不参与原生窗体尺寸计算。
    let work_width = f64::from(work.size.width) / scale;
    let work_height = f64::from(work.size.height) / scale;
    let available_width = (work_width - 48.0).max(1.0);
    let available_height = (work_height - 48.0).max(1.0);
    let size = LogicalSize::new(
        (work_width * 0.8).max(1280.0).min(available_width).round(),
        (work_height * 0.85)
            .max(800.0)
            .min(available_height)
            .round(),
    );
    MainWindowGeometry {
        size,
        min_size: LogicalSize::new(
            MAIN_MIN_WIDTH.min(size.width),
            MAIN_MIN_HEIGHT.min(size.height),
        ),
        position: LogicalPosition::new(
            f64::from(work.position.x) / scale + (work_width - size.width) / 2.0,
            f64::from(work.position.y) / scale + (work_height - size.height) / 2.0,
        ),
    }
}

#[cfg(target_os = "linux")]
fn fit_linux_main_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    let monitor = match window.current_monitor().map_err(|e| e.to_string())? {
        Some(monitor) => Some(monitor),
        // Wayland 不一定声明 primary monitor，仍可从有效显示器列表取得工作区。
        None => window
            .available_monitors()
            .map_err(|e| e.to_string())?
            .into_iter()
            .next(),
    };
    let Some(monitor) = monitor else {
        return Ok(());
    };
    let geometry = linux_startup_geometry(monitor.work_area(), monitor.scale_factor());
    window
        .set_min_size(Some(geometry.min_size))
        .map_err(|e| e.to_string())?;
    window.set_size(geometry.size).map_err(|e| e.to_string())?;
    // GTK 的尺寸请求异步生效，直接用目标尺寸居中，不能立刻回读旧的 inner_size。
    // Wayland 下位置由合成器决定。
    window
        .set_position(geometry.position)
        .map_err(|e| e.to_string())
}

/// 前端就绪后调用:显示主窗口(避免透明窗口启动闪烁)
#[tauri::command]
pub async fn show_main_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口未找到".to_string())?;

    crate::webview_scheduler::show_window(&window).await?;
    let _ = window.set_focus();
    Ok(())
}

#[cfg(not(target_os = "linux"))]
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
/// 必须 async：sync 命令跑在主线程执行器上,建 WebView 的窗口过程要把消息泵回主事件循环,
/// 从 IPC 里这么干会把整个泵卡死——前端 invoke 永不返回、主窗失去响应（连拖动都不行）
#[tauri::command]
pub async fn open_debug_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(DEBUG_WINDOW_LABEL) {
        let _ = crate::webview_scheduler::show_window(&window).await;
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
    crate::ui_scale::apply_to_window(&window)?;
    crate::window_frame::apply(&window)?;
    let _ = crate::window_icon::apply_window_icon(&app, &window);
    Ok(())
}

/// 弹出窗前端画好首帧后调用:显示 + 聚焦。对齐主窗 show_main_window 的时序,
/// 起步隐藏是防透明壳在内容就绪前闪白
#[tauri::command]
pub async fn reveal_debug_window(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(DEBUG_WINDOW_LABEL)
        .ok_or_else(|| "调试台窗口未找到".to_string())?;
    crate::webview_scheduler::show_window(&window).await?;
    let _ = window.unminimize();
    let _ = window.set_focus();
    Ok(())
}

/// 每个 WebView 窗口各占多少内存（渲染进程按窗口对上 label），连同共用的浏览器 / GPU 进程和本体
#[tauri::command]
pub async fn webview_memory_report(
    app: AppHandle,
) -> Result<crate::webview_scheduler::WebviewMemoryReport, String> {
    crate::webview_scheduler::memory_report(&app).await
}

/// 主窗的调试台入口让位用:弹出窗开着就聚焦它并回 true,主窗不再进调试页。
/// 同一时间只留一个调试台页面(工作区 / 收藏落盘 JSON 是两窗同一份文件)
#[tauri::command]
pub async fn focus_debug_window(app: AppHandle) -> bool {
    let Some(window) = app.get_webview_window(DEBUG_WINDOW_LABEL) else {
        return false;
    };
    let _ = crate::webview_scheduler::show_window(&window).await;
    let _ = window.unminimize();
    let _ = window.set_focus();
    true
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux_startup_geometry;
    use tauri::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalRect, PhysicalSize};

    #[test]
    fn linux_startup_fits_the_logical_work_area_at_different_system_scales() {
        for (scale, physical, expected_size, expected_position) in [
            (1.0, (2560, 1600), (2048.0, 1360.0), (-2304.0, 120.0)),
            (2.0, (3840, 2160), (1536.0, 918.0), (-1728.0, 81.0)),
        ] {
            let work = PhysicalRect {
                position: PhysicalPosition::new(-(physical.0 as i32), 0),
                size: PhysicalSize::new(physical.0, physical.1),
            };
            let geometry = linux_startup_geometry(&work, scale);
            assert_eq!(geometry.size, LogicalSize::from(expected_size));
            assert_eq!(geometry.position, LogicalPosition::from(expected_position));
            assert_eq!(geometry.min_size, LogicalSize::new(1148.0, 720.0));
        }
    }

    #[test]
    fn linux_startup_lowers_the_minimum_to_fit_small_work_areas() {
        let work = PhysicalRect {
            position: PhysicalPosition::new(0, 24),
            size: PhysicalSize::new(1280, 696),
        };
        let geometry = linux_startup_geometry(&work, 1.0);
        assert_eq!(geometry.size, LogicalSize::new(1232.0, 648.0));
        assert_eq!(geometry.min_size, LogicalSize::new(1148.0, 648.0));
        assert_eq!(geometry.position, LogicalPosition::new(24.0, 48.0));
    }
}
