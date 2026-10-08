// 按显示器采集桌面，离屏导出与 GDI 资源都在调用线程内释放。
#![expect(unsafe_code, reason = "Win32 GDI / Direct2D 的 FFI 边界")]
#![warn(clippy::undocumented_unsafe_blocks)]

use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT};
use windows::Win32::Graphics::Direct2D::{
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, ID2D1RenderTarget,
};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmFlush, DwmGetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, EnumDisplayMonitors, GdiFlush, GetDC, GetMonitorInfoW,
    HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFO, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::Shell::GetScaleFactorForMonitor;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindowVisible,
    WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, Interface};

use super::model::PixelRect;

const MAX_PIXEL_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug)]
pub struct MonitorImage {
    pub bounds: PixelRect,
    pub scale: f32,
    pub bgra: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct WindowTarget {
    pub bounds: PixelRect,
}

#[derive(Debug)]
pub struct Desktop {
    pub monitors: Vec<MonitorImage>,
    pub windows: Vec<WindowTarget>,
    pub bounds: PixelRect,
}

pub fn capture_desktop() -> Result<Desktop, String> {
    let _dpi = ThreadDpiContext::new()?;
    // SAFETY: 调用方已隐藏发起窗口，等待合成完成后才创建截图覆盖窗。
    unsafe { DwmFlush() }.map_err(|error| format!("等待桌面合成失败：{error}"))?;
    let specs = enumerate_monitors()?;
    let bounds = desktop_bounds(&specs)?;
    let screen = ScreenDc::new()?;
    let mut monitors = Vec::new();
    monitors
        .try_reserve_exact(specs.len())
        .map_err(|_| "截图内存不足".to_string())?;
    for spec in specs {
        let surface = DibSurface::new(Some(screen.0), spec.bounds.width, spec.bounds.height)?;
        // SAFETY: 两个 DC 都在当前线程有效，目标 DIB 尺寸与复制区域一致。
        unsafe {
            BitBlt(
                surface.dc,
                0,
                0,
                spec.bounds.width as i32,
                spec.bounds.height as i32,
                Some(screen.0),
                spec.bounds.left,
                spec.bounds.top,
                SRCCOPY | CAPTUREBLT,
            )
        }
        .map_err(|error| format!("采集屏幕失败：{error}"))?;
        let mut bgra = surface.read_pixels()?;
        for pixel in bgra.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        monitors.push(MonitorImage {
            bounds: spec.bounds,
            scale: spec.scale,
            bgra,
        });
    }
    Ok(Desktop {
        monitors,
        windows: enumerate_windows()?,
        bounds,
    })
}

pub fn render_offscreen(
    width: u32,
    height: u32,
    draw: impl FnOnce(ID2D1RenderTarget) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    // Direct2D 的工厂不依赖 COM apartment，这里不改变调用线程的 COM 生命周期。
    let surface = DibSurface::new(None, width, height)?;
    let props = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_IGNORE,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    };
    let rect = RECT {
        left: 0,
        top: 0,
        right: width as i32,
        bottom: height as i32,
    };
    // SAFETY: 工厂、目标与 DC 同属当前线程，绑定区域在已分配的 DIB 内。
    let target = unsafe {
        let factory = crate::native_panel::gfx::shared()
            .map_err(|error| format!("创建导出画布失败：{error}"))?
            .d2d
            .clone();
        let target = factory
            .CreateDCRenderTarget(&props)
            .map_err(|error| format!("创建导出目标失败：{error}"))?;
        target
            .BindDC(surface.dc, &rect)
            .map_err(|error| format!("绑定导出画布失败：{error}"))?;
        target
    };
    // Canvas 回调负责成对 BeginDraw / EndDraw，避免嵌套绘制会话。
    draw(
        target
            .cast()
            .map_err(|error| format!("导出接口转换失败：{error}"))?,
    )?;
    drop(target);
    let mut rgba = surface.read_pixels()?;
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    Ok(rgba)
}

pub fn capture_region(bounds: PixelRect) -> Result<MonitorImage, String> {
    let _dpi = ThreadDpiContext::new()?;
    let bytes = pixel_bytes(bounds.width, bounds.height)?;
    if bytes > 32 * 1024 * 1024 {
        return Err("滚动选区超过 32 MiB 像素限制，请缩小范围".into());
    }
    // SAFETY: 等待上一轮桌面合成后，只采集当前选区；DC 与 DIB 均由同线程守卫持有。
    unsafe { DwmFlush() }.map_err(|e| e.to_string())?;
    let screen = ScreenDc::new()?;
    let surface = DibSurface::new(Some(screen.0), bounds.width, bounds.height)?;
    unsafe {
        BitBlt(
            surface.dc,
            0,
            0,
            bounds.width as i32,
            bounds.height as i32,
            Some(screen.0),
            bounds.left,
            bounds.top,
            SRCCOPY | CAPTUREBLT,
        )
    }
    .map_err(|e| format!("采集滚动选区失败：{e}"))?;
    let mut bgra = surface.read_pixels()?;
    for pixel in bgra.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    Ok(MonitorImage {
        bounds,
        scale: 1.0,
        bgra,
    })
}

#[derive(Clone, Copy)]
struct MonitorSpec {
    bounds: PixelRect,
    scale: f32,
}

#[derive(Default)]
struct MonitorEnumeration {
    monitors: Vec<MonitorSpec>,
    total_bytes: usize,
    error: Option<String>,
}

fn enumerate_monitors() -> Result<Vec<MonitorSpec>, String> {
    let mut state = MonitorEnumeration::default();
    // SAFETY: 回调同步执行，state 的独占借用与地址在整个枚举调用期间有效。
    let completed = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(monitor_callback),
            LPARAM((&mut state as *mut MonitorEnumeration) as isize),
        )
    };
    if let Some(error) = state.error {
        return Err(error);
    }
    if !completed.as_bool() || state.monitors.is_empty() {
        return Err("没有可截图的显示器".into());
    }
    Ok(state.monitors)
}

unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: data 是 enumerate_monitors 传入的独占 state 指针，回调不会保留它。
    let state = unsafe { &mut *(data.0 as *mut MonitorEnumeration) };
    match monitor_spec(monitor, state.total_bytes) {
        Ok((spec, total_bytes)) => {
            if state.monitors.try_reserve(1).is_err() {
                state.error = Some("截图内存不足".into());
                return BOOL(0);
            }
            state.monitors.push(spec);
            state.total_bytes = total_bytes;
            BOOL(1)
        }
        Err(error) => {
            state.error = Some(error);
            BOOL(0)
        }
    }
}

fn monitor_spec(monitor: HMONITOR, existing_bytes: usize) -> Result<(MonitorSpec, usize), String> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: monitor 来自当前枚举；info 的 cbSize 和可写大小匹配 Win32 结构。
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err("读取显示器信息失败".into());
    }
    let bounds = pixel_rect(info.rcMonitor).ok_or("显示器尺寸无效")?;
    let bytes = pixel_bytes(bounds.width, bounds.height)?;
    let total = existing_bytes
        .checked_add(bytes)
        .filter(|total| *total <= MAX_PIXEL_BYTES)
        .ok_or("显示器总像素超过截图内存限制（256 MiB）")?;
    // SAFETY: monitor 是有效显示器句柄；读取缩放不会改变线程或显示器状态。
    let scale = unsafe { GetScaleFactorForMonitor(monitor) }
        .map(|scale| (scale.0 as f32 / 100.0).clamp(1.0, 5.0))
        .unwrap_or(1.0);
    Ok((MonitorSpec { bounds, scale }, total))
}

fn desktop_bounds(monitors: &[MonitorSpec]) -> Result<PixelRect, String> {
    let first = monitors.first().ok_or("没有可截图的显示器")?.bounds;
    let mut left = first.left;
    let mut top = first.top;
    let mut right = i64::from(first.left) + i64::from(first.width);
    let mut bottom = i64::from(first.top) + i64::from(first.height);
    for monitor in monitors.iter().skip(1) {
        left = left.min(monitor.bounds.left);
        top = top.min(monitor.bounds.top);
        right = right.max(i64::from(monitor.bounds.left) + i64::from(monitor.bounds.width));
        bottom = bottom.max(i64::from(monitor.bounds.top) + i64::from(monitor.bounds.height));
    }
    let width = u32::try_from(right - i64::from(left)).map_err(|_| "桌面宽度无效")?;
    let height = u32::try_from(bottom - i64::from(top)).map_err(|_| "桌面高度无效")?;
    if width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err("桌面坐标范围过大".into());
    }
    Ok(PixelRect {
        left,
        top,
        width,
        height,
    })
}

#[derive(Default)]
struct WindowEnumeration {
    windows: Vec<WindowTarget>,
    allocation_failed: bool,
}

fn enumerate_windows() -> Result<Vec<WindowTarget>, String> {
    let mut state = WindowEnumeration::default();
    // SAFETY: EnumWindows 同步按 Z 顺序调用，state 在整个调用期间独占且有效。
    let result = unsafe {
        EnumWindows(
            Some(window_callback),
            LPARAM((&mut state as *mut WindowEnumeration) as isize),
        )
    };
    if state.allocation_failed {
        return Err("截图窗口列表内存不足".into());
    }
    result.map_err(|error| format!("读取窗口列表失败：{error}"))?;
    Ok(state.windows)
}

unsafe extern "system" fn window_callback(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: hwnd 由 Windows 枚举；查询对已关闭窗口只会返回失败，不持有其内存。
    let bounds = unsafe {
        if !IsWindowVisible(hwnd).as_bool()
            || IsIconic(hwnd).as_bool()
            || GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW.0 as isize != 0
        {
            return BOOL(1);
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            size_of::<u32>() as u32,
        );
        if cloaked != 0 {
            return BOOL(1);
        }
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut rect as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        )
        .is_err()
            && GetWindowRect(hwnd, &mut rect).is_err()
        {
            return BOOL(1);
        }
        pixel_rect(rect)
    };
    if let Some(bounds) = bounds {
        // SAFETY: data 是 enumerate_windows 的独占 state 指针，不逃逸出回调。
        let state = unsafe { &mut *(data.0 as *mut WindowEnumeration) };
        if state.windows.try_reserve(1).is_err() {
            state.allocation_failed = true;
            return BOOL(0);
        }
        state.windows.push(WindowTarget { bounds });
    }
    BOOL(1)
}

fn pixel_rect(rect: RECT) -> Option<PixelRect> {
    let width = u32::try_from(i64::from(rect.right) - i64::from(rect.left)).ok()?;
    let height = u32::try_from(i64::from(rect.bottom) - i64::from(rect.top)).ok()?;
    (width > 0 && height > 0).then_some(PixelRect {
        left: rect.left,
        top: rect.top,
        width,
        height,
    })
}

fn pixel_bytes(width: u32, height: u32) -> Result<usize, String> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err("截图尺寸无效".into());
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes <= MAX_PIXEL_BYTES)
        .ok_or_else(|| "截图像素超过内存限制（256 MiB）".into())
}

struct ThreadDpiContext(DPI_AWARENESS_CONTEXT);

impl ThreadDpiContext {
    fn new() -> Result<Self, String> {
        // SAFETY: 只修改当前线程，旧上下文由守卫在同一线程恢复。
        let old =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if old.0.is_null() {
            return Err("设置截图 DPI 上下文失败".into());
        }
        Ok(Self(old))
    }
}

impl Drop for ThreadDpiContext {
    fn drop(&mut self) {
        // SAFETY: 存储的上下文来自成功调用，原生指针使守卫不能跨线程移动。
        unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

struct ScreenDc(HDC);

impl ScreenDc {
    fn new() -> Result<Self, String> {
        // SAFETY: 获取当前桌面 DC，交给守卫在当前线程 ReleaseDC。
        let dc = unsafe { GetDC(None) };
        if dc.is_invalid() {
            return Err("获取桌面画布失败".into());
        }
        Ok(Self(dc))
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: 只释放本守卫通过 GetDC(None) 获取的有效 DC。
        unsafe { ReleaseDC(None, self.0) };
    }
}

struct MemoryDc(HDC);

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: 守卫只持有通过 CreateCompatibleDC 创建、尚未转移的 DC。
        let _ = unsafe { DeleteDC(self.0) };
    }
}

struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: DIB 在销毁前已从 DC 中选出，只有此守卫拥有该句柄。
        let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
    }
}

struct DibSurface {
    dc: HDC,
    _bitmap: Bitmap,
    previous: HGDIOBJ,
    bits: *mut u8,
    len: usize,
}

impl DibSurface {
    fn new(source: Option<HDC>, width: u32, height: u32) -> Result<Self, String> {
        let len = pixel_bytes(width, height)?;
        // SAFETY: source 在调用期间有效或为 None，新 DC 立即获得单一守卫所有者。
        let dc = MemoryDc(unsafe { CreateCompatibleDC(source) });
        if dc.0.is_invalid() {
            return Err("创建截图画布失败".into());
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: len as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: info 描述已检查的 32 位顶向下 DIB，bits 是有效可写输出指针。
        let bitmap = Bitmap(
            unsafe { CreateDIBSection(Some(dc.0), &info, DIB_RGB_COLORS, &mut bits, None, 0) }
                .map_err(|error| format!("分配截图位图失败：{error}"))?,
        );
        if bits.is_null() {
            return Err("截图位图没有像素缓冲".into());
        }
        // SAFETY: DIB 已分配 len 字节；初始化后即便绘制失败，也不会读取未初始化内存。
        unsafe { std::ptr::write_bytes(bits.cast::<u8>(), 0, len) };
        // SAFETY: 位图还未选入其他 DC，成功时保存旧对象用于销毁前恢复。
        let previous = unsafe { SelectObject(dc.0, HGDIOBJ(bitmap.0.0)) };
        if previous.is_invalid() {
            return Err("绑定截图位图失败".into());
        }
        let surface = Self {
            dc: dc.0,
            _bitmap: bitmap,
            previous,
            bits: bits.cast(),
            len,
        };
        // 所有权由创建失败守卫转交给 DibSurface，后者先选出位图再删除 DC。
        std::mem::forget(dc);
        Ok(surface)
    }

    fn read_pixels(&self) -> Result<Vec<u8>, String> {
        // SAFETY: GDI 调用和直接像素访问在同一线程，flush 完成后才借用 DIB 像素。
        if !unsafe { GdiFlush() }.as_bool() {
            return Err("同步截图像素失败".into());
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(self.len)
            .map_err(|_| "截图内存不足".to_string())?;
        // SAFETY: 位图所有者仍存活，bits 包含 len 个已初始化字节，无并发写入。
        pixels.extend_from_slice(unsafe { std::slice::from_raw_parts(self.bits, self.len) });
        Ok(pixels)
    }
}

impl Drop for DibSurface {
    fn drop(&mut self) {
        // SAFETY: 当前线程仍拥有 DC；先恢复旧对象，随后删除 DC 和字段中的位图。
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteDC(self.dc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

    #[test]
    fn monitor_bounds_preserve_negative_coordinates_and_gaps() {
        let monitors = [
            MonitorSpec {
                bounds: PixelRect {
                    left: -2560,
                    top: -300,
                    width: 2560,
                    height: 1440,
                },
                scale: 1.5,
            },
            MonitorSpec {
                bounds: PixelRect {
                    left: 800,
                    top: 0,
                    width: 1920,
                    height: 1080,
                },
                scale: 1.0,
            },
        ];
        assert_eq!(
            desktop_bounds(&monitors).unwrap(),
            PixelRect {
                left: -2560,
                top: -300,
                width: 5280,
                height: 1440
            },
        );
    }

    #[test]
    fn pixel_budget_accepts_boundary_and_rejects_overflow_or_empty_dimensions() {
        assert_eq!(pixel_bytes(8192, 8192).unwrap(), MAX_PIXEL_BYTES);
        assert!(pixel_bytes(8193, 8192).is_err());
        assert!(pixel_bytes(u32::MAX, u32::MAX).is_err());
        assert!(pixel_bytes(1, 0).is_err());
    }

    #[test]
    fn native_rect_rejects_inverted_edges_without_signed_overflow() {
        assert!(
            pixel_rect(RECT {
                left: 10,
                top: 0,
                right: -10,
                bottom: 20
            })
            .is_none()
        );
        assert_eq!(
            pixel_rect(RECT {
                left: i32::MIN,
                top: 0,
                right: i32::MAX,
                bottom: 1
            })
            .unwrap()
            .width,
            u32::MAX,
        );
    }

    #[test]
    fn native_offscreen_roundtrip_preserves_rgba_channel_order_and_opacity() {
        let pixels = render_offscreen(3, 2, |target| {
            let color = D2D1_COLOR_F {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            };
            // SAFETY: 测试工厂与画布同属当前线程，成对结束绘制后再读像素。
            unsafe {
                target.BeginDraw();
                target.Clear(Some(&color));
                target
                    .EndDraw(None, None)
                    .map_err(|error| error.to_string())
            }
        })
        .unwrap();
        assert_eq!(pixels, [255, 0, 0, 255].repeat(6));
        crate::native_panel::gfx::release_shared();
    }

    #[test]
    fn native_offscreen_repeated_fresh_workers_release_shared_resources() {
        for expected in [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 255, 255],
        ] {
            let pixels = std::thread::spawn(move || {
                let resources = crate::native_panel::gfx::shared().unwrap();
                let weak = std::rc::Rc::downgrade(&resources);
                drop(resources);
                let draw = |target: ID2D1RenderTarget| {
                    let color = D2D1_COLOR_F {
                        r: f32::from(expected[0]) / 255.0,
                        g: f32::from(expected[1]) / 255.0,
                        b: f32::from(expected[2]) / 255.0,
                        a: 1.0,
                    };
                    // SAFETY: 每个线程只访问自己的工厂与目标，绘制会话成对结束。
                    unsafe {
                        target.BeginDraw();
                        target.Clear(Some(&color));
                        target
                            .EndDraw(None, None)
                            .map_err(|error| error.to_string())
                    }
                };
                let first = render_offscreen(3, 2, draw).unwrap();
                let second = render_offscreen(3, 2, draw).unwrap();
                assert_eq!(first, second);
                crate::native_panel::gfx::release_shared();
                assert!(weak.upgrade().is_none(), "导出结束后绘制资源仍被引用");
                second
            })
            .join()
            .unwrap();
            assert_eq!(pixels, expected.repeat(6));
        }
    }
}
