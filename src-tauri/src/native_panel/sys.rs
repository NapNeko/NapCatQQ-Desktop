// 原生面板的 Win32 / Direct2D FFI 边界：窗口层的裸调用、指针解引用和成对调用都收在这里。
#![expect(unsafe_code, reason = "Win32 / D2D FFI 统一收在这里审计")]
#![warn(clippy::undocumented_unsafe_blocks)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_PRESENT_OPTIONS_RETAIN_CONTENTS, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, ID2D1Factory1, ID2D1RenderTarget,
};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetCursorPos, GetWindowRect, IDC_ARROW, IDC_HAND, IsWindowVisible, KillTimer, LoadCursorW,
    PostMessageW, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOZORDER, SetCursor, SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW,
};
use windows::core::BOOL;
use windows::core::{PCWSTR, w};
use windows_numerics::Matrix3x2;

// 类名只在这里用：create_panel_window 拿的就是 register_panel_class 注册的那个类
const CLASS_NAME: PCWSTR = w!("NCD.NativeTrayPanel.v1");

/// 窗口句柄值。user32 / gdi32 会校验句柄，窗口销毁后再调只是返回失败，不会踩内存。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hwnd(HWND);

impl Hwnd {
    pub fn from_raw(hwnd: HWND) -> Self {
        Self(hwnd)
    }

    pub fn raw(self) -> HWND {
        self.0
    }

    /// 注册表用的键。
    pub fn key(self) -> isize {
        self.0.0 as isize
    }

    pub fn show_no_activate(self) {
        // SAFETY: 句柄由 user32 校验；ShowWindow 不涉及调用方内存。
        let _ = unsafe { ShowWindow(self.0, SW_SHOWNOACTIVATE) };
    }

    /// 收起时顺手放掉鼠标捕获：ESC、失焦、点完动作都走这里，捕获不能跟着隐藏的窗口留下。
    pub fn hide(self) {
        // SAFETY: GetCapture 无参数；ReleaseCapture 只放本线程的捕获；ShowWindow 的句柄由 user32 校验。
        unsafe {
            if GetCapture() == self.0 {
                let _ = ReleaseCapture();
            }
            let _ = ShowWindow(self.0, SW_HIDE);
        }
    }

    pub fn destroy(self) {
        // SAFETY: 句柄由 user32 校验；不是本线程建的窗口会直接失败。
        let _ = unsafe { DestroyWindow(self.0) };
    }

    pub fn set_foreground(self) {
        // SAFETY: 句柄由 user32 校验。
        let _ = unsafe { SetForegroundWindow(self.0) };
    }

    pub fn invalidate(self) {
        // SAFETY: 句柄由 user32 校验；lprect 传空表示整个客户区。
        let _ = unsafe { InvalidateRect(Some(self.0), None, false) };
    }

    /// 只改位置和大小，不动 Z 序、不激活。
    pub fn set_pos(self, x: i32, y: i32, w: i32, h: i32) {
        // SAFETY: 句柄由 user32 校验；参数都是值。
        let _ = unsafe { SetWindowPos(self.0, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    }

    /// 只改大小。
    pub fn set_size(self, w: i32, h: i32) {
        // SAFETY: 句柄由 user32 校验；参数都是值。
        let _ = unsafe {
            SetWindowPos(
                self.0,
                None,
                0,
                0,
                w,
                h,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
    }

    /// Win11 起 DWM 给弹出窗画抗锯齿圆角、细边框，阴影也跟着圆角走。Win10 没有这个属性，返回 false。
    /// 不能再用 SetWindowRgn：设了窗口区域的窗口 DWM 不再圆角，也不画阴影。
    pub fn set_corner_round(self) -> bool {
        let pref: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
        // SAFETY: pv 指向栈上的枚举值，cb 是它的大小，DWM 只读不留指针。
        unsafe {
            DwmSetWindowAttribute(
                self.0,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&raw const pref).cast(),
                size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            )
        }
        .is_ok()
    }

    /// 深色主题下让 DWM 的边框也走深色，不然暗底面板外面是一圈亮灰。
    pub fn set_dark_frame(self, dark: bool) {
        let value = BOOL::from(dark);
        // SAFETY: pv 指向栈上的 BOOL，cb 是它的大小，DWM 只读不留指针。
        let _ = unsafe {
            DwmSetWindowAttribute(
                self.0,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&raw const value).cast(),
                size_of::<BOOL>() as u32,
            )
        };
    }

    /// 客户区 (宽, 高) 物理像素。
    pub fn client_size(self) -> Option<(i32, i32)> {
        let mut rect = RECT::default();
        // SAFETY: rect 是局部变量，GetClientRect 只往里写。
        unsafe { GetClientRect(self.0, &mut rect) }.ok()?;
        Some((rect.right - rect.left, rect.bottom - rect.top))
    }

    pub fn is_visible(self) -> bool {
        // SAFETY: 句柄由 user32 校验，失效句柄返回 FALSE。
        unsafe { IsWindowVisible(self.0) }.as_bool()
    }

    /// 窗口外框的屏幕物理坐标 (左, 上, 右, 下)。
    pub fn screen_rect(self) -> Option<(i32, i32, i32, i32)> {
        let mut rect = RECT::default();
        // SAFETY: rect 是局部变量，GetWindowRect 只往里写；句柄由 user32 校验。
        unsafe { GetWindowRect(self.0, &mut rect) }.ok()?;
        Some((rect.left, rect.top, rect.right, rect.bottom))
    }

    pub fn dpi(self) -> u32 {
        // SAFETY: 句柄由 user32 校验，失效句柄返回 0。
        unsafe { GetDpiForWindow(self.0) }
    }

    /// 定时器走 WM_TIMER，不带回调函数指针。
    pub fn set_timer(self, id: usize, ms: u32) {
        // SAFETY: 句柄由 user32 校验；不传 TIMERPROC。
        let _ = unsafe { SetTimer(Some(self.0), id, ms, None) };
    }

    pub fn kill_timer(self, id: usize) {
        // SAFETY: 句柄由 user32 校验。
        let _ = unsafe { KillTimer(Some(self.0), id) };
    }

    pub fn capture(self) {
        // SAFETY: 句柄由 user32 校验。
        let _ = unsafe { SetCapture(self.0) };
    }

    /// 投递一条不带参数的自定义消息。
    pub fn post(self, msg: u32) {
        // SAFETY: wparam / lparam 都是 0，不携带指针。
        let _ = unsafe { PostMessageW(Some(self.0), msg, WPARAM(0), LPARAM(0)) };
    }

    /// 光标离开时收一次 WM_MOUSELEAVE。
    pub fn track_mouse_leave(self) {
        let mut track = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.0,
            dwHoverTime: 0,
        };
        // SAFETY: track 是局部结构体，cbSize 已填。
        let _ = unsafe { TrackMouseEvent(&mut track) };
    }

    pub fn begin_paint(self) -> PaintGuard {
        let mut ps = PAINTSTRUCT::default();
        // SAFETY: ps 是局部结构体，BeginPaint 只往里写。
        let hdc = unsafe { BeginPaint(self.0, &mut ps) };
        PaintGuard {
            hwnd: self.0,
            ps,
            began: !hdc.is_invalid(),
        }
    }
}

/// BeginPaint 到 EndPaint 之间。HWND 渲染目标的 EndDraw 不会把更新区域标成有效，靠这对调用清掉。
#[must_use]
pub struct PaintGuard {
    hwnd: HWND,
    ps: PAINTSTRUCT,
    began: bool,
}

impl Drop for PaintGuard {
    fn drop(&mut self) {
        if self.began {
            // SAFETY: ps 是 BeginPaint 填好的那份，EndPaint 只读。
            let _ = unsafe { EndPaint(self.hwnd, &self.ps) };
        }
    }
}

impl PaintGuard {
    pub fn rect(&self) -> RECT {
        self.ps.rcPaint
    }
    pub fn dc(&self) -> windows::Win32::Graphics::Gdi::HDC {
        self.ps.hdc
    }
}

/// 光标的屏幕物理坐标。
pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    // SAFETY: point 是局部变量，GetCursorPos 只往里写。
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some((point.x, point.y))
}

pub fn release_capture() {
    // SAFETY: 无参数；没有捕获时只是返回失败。
    let _ = unsafe { ReleaseCapture() };
}

pub fn set_hand_cursor() {
    // SAFETY: IDC_HAND 是系统内置光标，不需要 HINSTANCE。
    unsafe { SetCursor(LoadCursorW(None, IDC_HAND).ok()) };
}

/// 系统发来的一条消息。只能在 [`trampoline`] 里构造，所以 lparam 的指针语义可以按消息号信任。
pub struct Message {
    hwnd: Hwnd,
    id: u32,
    wparam: WPARAM,
    lparam: LPARAM,
}

impl Message {
    pub fn hwnd(&self) -> Hwnd {
        self.hwnd
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn wparam(&self) -> usize {
        self.wparam.0
    }

    pub fn lparam(&self) -> isize {
        self.lparam.0
    }
}

/// 窗口过程的安全实现。返回 None 交给 DefWindowProcW。
pub trait WndHandler {
    fn handle(msg: &Message) -> Option<LRESULT>;
}

#[cfg(test)]
thread_local! {
    static CAUGHT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// 本线程 trampoline 兜住的 panic 次数，测试用来确认没有 panic 被悄悄吞掉。
#[cfg(test)]
pub fn caught_panics() -> u32 {
    CAUGHT.with(std::cell::Cell::get)
}

// panic 不能展开穿过 extern "system"。release 配置是 panic = "abort"，这里什么都兜不住，
// panic hook 记完日志进程就终止；只有 dev / test 能回落 DefWindowProcW。所以内容层回调不许有 panic 路径。
extern "system" fn trampoline<H: WndHandler>(
    hwnd: HWND,
    id: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let msg = Message {
        hwnd: Hwnd(hwnd),
        id,
        wparam,
        lparam,
    };
    let handled = catch_unwind(AssertUnwindSafe(|| H::handle(&msg))).unwrap_or_else(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        tracing::error!(
            msg = id,
            "原生面板消息处理 panic，回落 DefWindowProcW: {detail}"
        );
        #[cfg(test)]
        CAUGHT.with(|count| count.set(count.get() + 1));
        None
    });
    match handled {
        Some(result) => result,
        // SAFETY: 原样转发系统刚发来的消息，lparam 的含义由 DefWindowProcW 按消息号解释。
        None => unsafe { DefWindowProcW(hwnd, id, wparam, lparam) },
    }
}

/// 注册面板窗口类。同名类已存在（别的线程注册过）时 RegisterClassExW 失败，忽略即可。
/// CS_DROPSHADOW 是系统给菜单、提示这类短时弹出窗的阴影；Win11 上它跟着 DWM 圆角走。
pub fn register_panel_class<H: WndHandler>() {
    register_window_class::<H>(CLASS_NAME);
}

pub fn register_window_class<H: WndHandler>(class_name: PCWSTR) {
    register_window_class_with_style::<H>(class_name, CS_DBLCLKS | CS_DROPSHADOW);
}

pub fn register_window_class_with_style<H: WndHandler>(
    class_name: PCWSTR,
    style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES,
) {
    // SAFETY: GetModuleHandleW(None) 取本进程 exe 模块；IDC_ARROW 是系统内置光标。
    let (instance, cursor) = unsafe {
        (
            GetModuleHandleW(None)
                .map(|h| h.0)
                .unwrap_or(std::ptr::null_mut()),
            LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        )
    };
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style,
        lpfnWndProc: Some(trampoline::<H>),
        hInstance: HINSTANCE(instance),
        hCursor: cursor,
        lpszClassName: class_name,
        ..Default::default()
    };
    // SAFETY: class 在栈上活到调用结束；类名是 'static 的 w! 字面量，窗口过程是 'static 函数。
    let _ = unsafe { RegisterClassExW(&class) };
}

/// 建一个面板类的顶层窗口，不带 lpCreateParams：内容由调用方经线程局部交给 WM_CREATE。
pub fn create_panel_window(
    ex_style: WINDOW_EX_STYLE,
    style: WINDOW_STYLE,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> windows::core::Result<Hwnd> {
    // SAFETY: 类名是 'static 字面量，标题为空，没有父窗口、菜单和创建参数指针。
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            CLASS_NAME,
            PCWSTR::null(),
            style,
            x,
            y,
            w,
            h,
            None,
            None,
            None,
            None,
        )
    }?;
    Ok(Hwnd(hwnd))
}

/// 绑在窗口上的 D2D 渲染目标。软件渲染：面板小，免得为它拉起 GPU 设备。
pub fn hwnd_render_target(
    factory: &ID2D1Factory1,
    hwnd: Hwnd,
    width: u32,
    height: u32,
    scale: f32,
) -> windows::core::Result<ID2D1RenderTarget> {
    create_hwnd_target(
        factory,
        hwnd,
        width,
        height,
        scale,
        D2D1_RENDER_TARGET_TYPE_SOFTWARE,
        D2D1_PRESENT_OPTIONS_NONE,
    )
}

// 截图覆盖整屏，优先硬件；不可用时由 D2D 回落软件。
pub fn screenshot_render_target(
    factory: &ID2D1Factory1,
    hwnd: Hwnd,
    width: u32,
    height: u32,
) -> windows::core::Result<ID2D1RenderTarget> {
    create_hwnd_target(
        factory,
        hwnd,
        width,
        height,
        1.0,
        D2D1_RENDER_TARGET_TYPE_DEFAULT,
        D2D1_PRESENT_OPTIONS_RETAIN_CONTENTS,
    )
}

fn create_hwnd_target(
    factory: &ID2D1Factory1,
    hwnd: Hwnd,
    width: u32,
    height: u32,
    scale: f32,
    target_type: D2D1_RENDER_TARGET_TYPE,
    present: D2D1_PRESENT_OPTIONS,
) -> windows::core::Result<ID2D1RenderTarget> {
    let props = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: target_type,
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0 * scale,
        dpiY: 96.0 * scale,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    };
    let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
        hwnd: hwnd.0,
        pixelSize: D2D_SIZE_U { width, height },
        presentOptions: present,
    };
    // SAFETY: 两个属性结构体在栈上活到调用结束；句柄失效时 D2D 返回错误。
    let target = unsafe { factory.CreateHwndRenderTarget(&props, &hwnd_props) }?;
    Ok(target.into())
}

/// BeginDraw 到 EndDraw 之间。正常收尾用 [`DrawGuard::finish`] 拿 EndDraw 的结果；
/// 提前返回或 panic 时 Drop 补一次 EndDraw，渲染目标不会卡在绘制状态。
#[must_use]
pub struct DrawGuard<'a> {
    target: &'a ID2D1RenderTarget,
    done: bool,
}

impl<'a> DrawGuard<'a> {
    pub fn begin(target: &'a ID2D1RenderTarget) -> Self {
        // SAFETY: target 是活着的 COM 引用，单线程工厂只在本 UI 线程上用。
        unsafe { target.BeginDraw() };
        Self {
            target,
            done: false,
        }
    }

    pub fn finish(mut self) -> windows::core::Result<()> {
        self.done = true;
        // SAFETY: 和 begin 里的 BeginDraw 成对；done 置位后 Drop 不再重复调用。
        unsafe { self.target.EndDraw(None, None) }
    }
}

impl Drop for DrawGuard<'_> {
    fn drop(&mut self) {
        if !self.done {
            // SAFETY: 和 begin 里的 BeginDraw 成对，finish 没走到时才会进来。
            let _ = unsafe { self.target.EndDraw(None, None) };
        }
    }
}

/// 轴对齐裁剪，作用域结束时弹出。
#[must_use]
pub struct ClipGuard<'a> {
    target: &'a ID2D1RenderTarget,
}

impl<'a> ClipGuard<'a> {
    pub fn push(target: &'a ID2D1RenderTarget, rect: D2D_RECT_F) -> Self {
        // SAFETY: rect 在栈上活到调用结束，D2D 拷走数值不留指针。
        unsafe { target.PushAxisAlignedClip(&rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
        Self { target }
    }
}

impl Drop for ClipGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: 和 push 里的 PushAxisAlignedClip 成对。
        unsafe { self.target.PopAxisAlignedClip() };
    }
}

/// 临时换变换矩阵，作用域结束时恢复成换之前的那个。
#[must_use]
pub struct TransformGuard<'a> {
    target: &'a ID2D1RenderTarget,
    previous: Matrix3x2,
}

impl<'a> TransformGuard<'a> {
    pub fn set(target: &'a ID2D1RenderTarget, transform: &Matrix3x2) -> Self {
        let mut previous = Matrix3x2::identity();
        // SAFETY: previous 是局部变量，GetTransform 只往里写；transform 是调用方的引用。
        unsafe {
            target.GetTransform(&mut previous);
            target.SetTransform(transform);
        }
        Self { target, previous }
    }
}

impl Drop for TransformGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: previous 是 set 时存下的矩阵，参数是自身字段的引用。
        unsafe { self.target.SetTransform(&self.previous) };
    }
}

/// 测试和截图用的 Win32 / GDI 裸调用：DPI 感知、屏幕截取、离屏渲染目标。
#[cfg(test)]
pub mod testing {
    use super::*;
    use windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget;
    use windows::Win32::Graphics::Dwm::DwmFlush;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, HBITMAP, HDC,
        HGDIOBJ, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    /// cargo test 的 exe 没有 DPI 清单，不切过来的话窗口按 96 DPI 建，再被 DWM 位图拉伸。
    pub fn per_monitor_dpi_thread() {
        // SAFETY: 只改本线程之后新建窗口的 DPI 感知方式。
        let _ = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    }

    pub fn dwm_flush() {
        // SAFETY: 无参数。
        let _ = unsafe { DwmFlush() };
    }

    /// 把本线程队列里的消息派发完（截图前让 WM_PAINT 等跑掉）。
    pub fn pump_messages() {
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
        };
        let mut msg = MSG::default();
        for _ in 0..200 {
            // SAFETY: msg 是局部变量；派发的是本线程刚取出的消息。
            unsafe {
                if !PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    pub fn window_rect(hwnd: Hwnd) -> Option<RECT> {
        let mut rect = RECT::default();
        // SAFETY: rect 是局部变量，GetWindowRect 只往里写。
        unsafe { GetWindowRect(hwnd.0, &mut rect) }.ok()?;
        Some(rect)
    }

    /// 32bpp 自顶向下的 DIB，挂在一个内存 DC 上。
    struct Dib {
        dc: HDC,
        bitmap: HBITMAP,
        old: HGDIOBJ,
        bits: *mut u8,
        w: i32,
        h: i32,
    }

    impl Dib {
        fn new(w: i32, h: i32) -> Option<Self> {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            // SAFETY: info 在栈上活到调用结束；bits 由系统分配，随 bitmap 一起在 Drop 里释放。
            unsafe {
                let dc = CreateCompatibleDC(None);
                if dc.is_invalid() {
                    return None;
                }
                let Ok(bitmap) =
                    CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                else {
                    let _ = DeleteDC(dc);
                    return None;
                };
                let old = SelectObject(dc, bitmap.into());
                Some(Self {
                    dc,
                    bitmap,
                    old,
                    bits: bits.cast(),
                    w,
                    h,
                })
            }
        }

        /// 读出 RGBA（DIB 里是预乘 BGRA，截屏时 alpha 无意义，统一写成不透明）。
        fn rgba(&self) -> Vec<u8> {
            let len = (self.w * self.h * 4) as usize;
            // SAFETY: GdiFlush 让挂起的 GDI 绘制落到位图；bits 指向 w × h × 4 字节、活到 self 释放。
            let src = unsafe {
                let _ = GdiFlush();
                std::slice::from_raw_parts(self.bits, len)
            };
            let mut out = Vec::with_capacity(len);
            for px in src.chunks_exact(4) {
                if let [b, g, r, a] = *px {
                    let a = a.max(1);
                    let un = |c: u8| {
                        ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8
                    };
                    out.extend_from_slice(&[un(r), un(g), un(b), 255]);
                }
            }
            out
        }
    }

    impl Drop for Dib {
        fn drop(&mut self) {
            // SAFETY: 恢复 DC 原来的位图后再释放自己建的 DIB 和 DC。
            unsafe {
                SelectObject(self.dc, self.old);
                let _ = DeleteObject(self.bitmap.into());
                let _ = DeleteDC(self.dc);
            }
        }
    }

    /// 截屏幕上一块（物理像素），连同 DWM 画的阴影一起，返回 RGBA。
    pub fn capture_screen(x: i32, y: i32, w: i32, h: i32) -> Option<Vec<u8>> {
        let dib = Dib::new(w, h)?;
        // SAFETY: 屏幕 DC 用完即还；BitBlt 的源和目标都是活着的 DC。
        unsafe {
            let screen = GetDC(None);
            let ok = BitBlt(dib.dc, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT).is_ok();
            ReleaseDC(None, screen);
            if !ok {
                return None;
            }
        }
        Some(dib.rgba())
    }

    /// 离屏画一帧：DC 渲染目标绑到内存 DIB 上，`draw` 拿到的渲染目标已经 BindDC、设好 DPI。
    pub fn render_offscreen(
        factory: &ID2D1Factory1,
        w: i32,
        h: i32,
        scale: f32,
        draw: impl FnOnce(ID2D1RenderTarget),
    ) -> Option<Vec<u8>> {
        let dib = Dib::new(w, h)?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0 * scale,
            dpiY: 96.0 * scale,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let rect = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        // SAFETY: props / rect 在栈上活到调用结束；DIB 的 DC 活到函数结束。
        let target: ID2D1DCRenderTarget = unsafe {
            let target = factory.CreateDCRenderTarget(&props).ok()?;
            target.BindDC(dib.dc, &rect).ok()?;
            target
        };
        draw(target.into());
        Some(dib.rgba())
    }
}
