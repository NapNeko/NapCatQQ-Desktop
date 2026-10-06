// Windows 平台小工具：系统明暗、屏幕上某点的工作区，面板和主题都在这里拿。
// 故意不进 native_panel：主窗口用 WebView2 时也可能要系统明暗。

#[cfg(windows)]
pub fn system_prefers_dark() -> bool {
    use windows::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, RegGetValueW};
    use windows::core::w;
    let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value_name = w!("AppsUseLightTheme");
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: 读 HKCU 下的 DWORD，缓冲区、指针类型都对。
    let status = unsafe {
        RegGetValueW(
            HKEY(HKEY_CURRENT_USER.0),
            subkey,
            value_name,
            windows::Win32::System::Registry::RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
    };
    // 读不到或值非 0（浅色）都当浅色；微软这套键是 AppsUseLightTheme
    status.is_ok() && data == 0
}

/// (left, top, width, height)，dip 坐标。找不到时给 1280×800 的兜底。
#[cfg(windows)]
pub fn monitor_work_area(x: f64, y: f64) -> (f64, f64, f64, f64) {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST, MonitorFromPoint,
    };

    // SAFETY: 读取显示器信息，结构体足够大。
    unsafe {
        let monitor = MonitorFromPoint(windows::Win32::Foundation::POINT { x: x as i32, y: y as i32 }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let scale = monitor_dpi_scale(monitor);
            let r = info.rcWork;
            return (
                r.left as f64 / scale,
                r.top as f64 / scale,
                (r.right - r.left) as f64 / scale,
                (r.bottom - r.top) as f64 / scale,
            );
        }
        (0.0, 0.0, 1280.0, 800.0)
    }
}

/// 屏幕的 dip / 像素比；面板定位、内联缓存用。
#[cfg(windows)]
pub fn monitor_dpi_scale(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> f64 {
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    let mut x = 0u32;
    let mut y = 0u32;
    // SAFETY: 取有效 DPI，输出是局部变量。
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }
        .map(|_| x as f64 / 96.0)
        .unwrap_or(1.0)
}
