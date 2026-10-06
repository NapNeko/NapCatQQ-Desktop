// Windows 平台小工具：系统明暗、屏幕上某点的工作区，面板和主题都在这里拿。
// 故意不进 native_panel：主窗口用 WebView2 时也可能要系统明暗。

#[cfg(windows)]
#[expect(unsafe_code, reason = "RegGetValueW 的 out 缓冲区是裸指针")]
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

#[cfg(windows)]
#[expect(unsafe_code, reason = "MonitorFromPoint 是 Win32 FFI")]
fn monitor_at(x: i32, y: i32) -> windows::Win32::Graphics::Gdi::HMONITOR {
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
    // SAFETY: 参数都是值；DEFAULTTONEAREST 保证总能拿到一个显示器。
    unsafe {
        MonitorFromPoint(
            windows::Win32::Foundation::POINT { x, y },
            MONITOR_DEFAULTTONEAREST,
        )
    }
}

/// 屏幕物理坐标 (x, y) 所在显示器的工作区，(left, top, width, height) 物理像素。
/// 取不到时给 1280×800 的兜底。
#[cfg(windows)]
#[expect(unsafe_code, reason = "GetMonitorInfoW 是 Win32 FFI")]
pub fn monitor_work_area_px(x: i32, y: i32) -> (i32, i32, i32, i32) {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO};
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: info 是局部结构体，cbSize 已填。
    if unsafe { GetMonitorInfoW(monitor_at(x, y), &mut info) }.as_bool() {
        let r = info.rcWork;
        return (r.left, r.top, r.right - r.left, r.bottom - r.top);
    }
    (0, 0, 1280, 800)
}

/// 屏幕物理坐标 (x, y) 所在显示器的缩放。
#[cfg(windows)]
pub fn monitor_dpi_scale_at(x: i32, y: i32) -> f64 {
    monitor_dpi_scale(monitor_at(x, y))
}

/// 屏幕的 dip / 像素比；面板定位、内联缓存用。
#[cfg(windows)]
#[expect(unsafe_code, reason = "GetDpiForMonitor 是 Win32 FFI")]
pub fn monitor_dpi_scale(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> f64 {
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    let mut x = 0u32;
    let mut y = 0u32;
    // SAFETY: 取有效 DPI，输出是局部变量。
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }
        .map(|_| x as f64 / 96.0)
        .unwrap_or(1.0)
}
