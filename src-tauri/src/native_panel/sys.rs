// 原生面板的 Win32 / Direct2D FFI 边界：窗口层的裸调用、指针解引用和成对调用都收在这里。
#![expect(unsafe_code, reason = "Win32 / D2D FFI 统一收在这里审计")]
#![warn(clippy::undocumented_unsafe_blocks)]

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::{Rc, Weak};

use windows::Win32::Foundation::{
    E_FAIL, GetLastError, HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, SetLastError,
    WIN32_ERROR, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET,
    D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS,
    D2D1_PRESENT_OPTIONS, D2D1_PRESENT_OPTIONS_NONE, D2D1_PRESENT_OPTIONS_RETAIN_CONTENTS,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, ID2D1Device,
    ID2D1DeviceContext, ID2D1Factory1, ID2D1RenderTarget,
};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice2, IDCompositionAnimation, IDCompositionDesktopDevice,
    IDCompositionEffectGroup, IDCompositionSurface, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dwm::{
    DWM_WINDOW_CORNER_PREFERENCE, DWMNCRENDERINGPOLICY, DWMNCRP_DISABLED, DWMNCRP_USEWINDOWSTYLE,
    DWMWA_NCRENDERING_POLICY, DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGISurface};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT,
    TrackMouseEvent, VK_LBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, CS_DROPSHADOW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE,
    GetClientRect, GetCursorPos, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, IDC_ARROW,
    IDC_HAND, IsWindowVisible, KillTimer, LoadCursorW, PostMessageW, RegisterClassExW,
    SM_SWAPBUTTON, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetCursor, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW,
    WS_EX_NOREDIRECTIONBITMAP,
};
use windows::core::BOOL;
use windows::core::{Interface, PCWSTR, w};
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

    pub fn disable_dwm_transitions(self) {
        let value = BOOL(1);
        // SAFETY: 本面板自行驱动过渡，禁用 DWM 的另一套窗口/阴影过渡，BOOL 只在调用中读取。
        let _ = unsafe {
            DwmSetWindowAttribute(
                self.0,
                DWMWA_TRANSITIONS_FORCEDISABLED,
                (&raw const value).cast(),
                size_of::<BOOL>() as u32,
            )
        };
    }

    pub fn paint_now(self) {
        // SAFETY: 同步绘制本窗口的无效区；调用方已放掉窗口状态借用。
        let _ = unsafe { UpdateWindow(self.0) };
    }

    pub fn enable_redirection(self) -> windows::core::Result<()> {
        // SAFETY: 初始化合成器失败时恢复普通 HWND 绘制；句柄和样式由 user32 校验。
        unsafe {
            let style = GetWindowLongPtrW(self.0, GWL_EXSTYLE);
            SetLastError(WIN32_ERROR(0));
            let previous = SetWindowLongPtrW(
                self.0,
                GWL_EXSTYLE,
                style & !(WS_EX_NOREDIRECTIONBITMAP.0 as isize),
            );
            if previous == 0 && GetLastError().0 != 0 {
                return Err(windows::core::Error::from_win32());
            }
            if GetWindowLongPtrW(self.0, GWL_EXSTYLE) & WS_EX_NOREDIRECTIONBITMAP.0 as isize != 0 {
                return Err(windows::core::Error::new(
                    E_FAIL,
                    "redirection style was not cleared",
                ));
            }
            SetWindowPos(
                self.0,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )?;
        }
        self.set_composition_frame(false);
        Ok(())
    }

    pub fn is_foreground(self) -> bool {
        // SAFETY: 查询当前前台句柄，不访问调用方内存。
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() == self.0 }
    }

    pub fn set_composition_frame(self, composited: bool) {
        let policy = if composited {
            DWMNCRP_DISABLED
        } else {
            DWMNCRP_USEWINDOWSTYLE
        };
        // SAFETY: 合成表面自己画边框/阴影，禁用独立的非客户区效果。
        let _ = unsafe {
            DwmSetWindowAttribute(
                self.0,
                DWMWA_NCRENDERING_POLICY,
                (&raw const policy).cast(),
                size_of::<DWMNCRENDERINGPOLICY>() as u32,
            )
        };
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
        if self.screen_rect() == Some((x, y, x + w, y + h)) {
            return;
        }
        // SAFETY: 句柄由 user32 校验；参数都是值。
        let _ = unsafe { SetWindowPos(self.0, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE) };
    }

    /// 只改大小。
    pub fn set_size(self, w: i32, h: i32) {
        if self.client_size() == Some((w, h)) {
            return;
        }
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

    pub fn move_to(self, x: i32, y: i32) {
        if self.screen_rect().is_some_and(|r| r.0 == x && r.1 == y) {
            return;
        }
        // SAFETY: 不改尺寸、Z 序或焦点；避免位移动画触发渲染目标和阴影的尺寸重排。
        let _ = unsafe {
            SetWindowPos(
                self.0,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
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

pub fn context_button_down() -> bool {
    // SAFETY: 只读系统鼠标映射与按键最高位，不能用不可靠的“最近按过”最低位。
    unsafe {
        let key = if GetSystemMetrics(SM_SWAPBUTTON) == 0 {
            VK_RBUTTON
        } else {
            VK_LBUTTON
        };
        GetAsyncKeyState(i32::from(key.0)) < 0
    }
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
/// 本类不附系统阴影，阴影必须跟正文处于同一个合成动画中。
pub fn register_panel_class<H: WndHandler>() {
    // 阴影在合成表面里画，不能让另一个系统阴影留在透明 HWND 后面。
    register_window_class_with_style::<H>(CLASS_NAME, CS_DBLCLKS);
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

struct CompositionShared {
    device: IDCompositionDesktopDevice,
    d2d: ID2D1Device,
    _d3d: ID3D11Device,
    warp: bool,
}

thread_local! {
    static COMPOSITION: RefCell<Weak<CompositionShared>> = const { RefCell::new(Weak::new()) };
}

fn composition_shared(factory: &ID2D1Factory1) -> windows::core::Result<Rc<CompositionShared>> {
    if let Some(shared) = COMPOSITION.with(|slot| slot.borrow().upgrade()) {
        return Ok(shared);
    }
    let mut d3d = None;
    let mut warp = false;
    // SAFETY: 输出为栈上 COM Option；BGRA 设备供 Direct2D 与 DirectComposition 共用。
    unsafe {
        if D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d3d),
            None,
            None,
        )
        .is_err()
        {
            warp = true;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
        }
        let d3d = d3d.ok_or_else(|| windows::core::Error::new(E_FAIL, "D3D11 device missing"))?;
        let dxgi: IDXGIDevice = d3d.cast()?;
        let d2d = factory.CreateDevice(&dxgi)?;
        let device: IDCompositionDesktopDevice = DCompositionCreateDevice2(&dxgi)?;
        let shared = Rc::new(CompositionShared {
            device,
            d2d,
            _d3d: d3d,
            warp,
        });
        COMPOSITION.with(|slot| *slot.borrow_mut() = Rc::downgrade(&shared));
        Ok(shared)
    }
}

struct CompositionLayer {
    visual: IDCompositionVisual,
    opacity: IDCompositionEffectGroup,
    surface: Option<IDCompositionSurface>,
    size: (u32, u32),
}

impl CompositionLayer {
    fn new(shared: &CompositionShared) -> windows::core::Result<Self> {
        // SAFETY: 所有 COM 对象在同一 UI 线程，效果随 visual 持有。
        unsafe {
            let visual: IDCompositionVisual = shared.device.CreateVisual()?.cast()?;
            let opacity = shared.device.CreateEffectGroup()?;
            visual.SetEffect(&opacity)?;
            Ok(Self {
                visual,
                opacity,
                surface: None,
                size: (0, 0),
            })
        }
    }
}

pub struct CompositionPanel {
    shared: Rc<CompositionShared>,
    target: IDCompositionTarget,
    root: IDCompositionVisual,
    opacity: IDCompositionEffectGroup,
    current: CompositionLayer,
    previous: Option<CompositionLayer>,
    context: ID2D1DeviceContext,
    pub draw_count: u32,
    pub commit_count: u32,
    pub commit_us: u64,
}

impl CompositionPanel {
    pub fn new(hwnd: Hwnd, factory: &ID2D1Factory1) -> windows::core::Result<Self> {
        let shared = composition_shared(factory)?;
        // SAFETY: HWND 为当前线程所有；D2D context 与 DXGI surface 来自同一设备。
        unsafe {
            let target = shared.device.CreateTargetForHwnd(hwnd.raw(), true)?;
            let root: IDCompositionVisual = shared.device.CreateVisual()?.cast()?;
            let opacity = shared.device.CreateEffectGroup()?;
            opacity.SetOpacity2(0.0)?;
            root.SetEffect(&opacity)?;
            let current = CompositionLayer::new(&shared)?;
            root.AddVisual(&current.visual, true, None)?;
            target.SetRoot(&root)?;
            let context = shared
                .d2d
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            Ok(Self {
                shared,
                target,
                root,
                opacity,
                current,
                previous: None,
                context,
                draw_count: 0,
                commit_count: 0,
                commit_us: 0,
            })
        }
    }

    pub fn render_target(&self) -> ID2D1RenderTarget {
        self.context.clone().into()
    }

    pub fn is_warp(&self) -> bool {
        self.shared.warp
    }

    #[cfg(test)]
    pub fn device_key(&self) -> usize {
        self.shared.device.as_raw() as usize
    }

    pub fn texture_bytes(&self) -> u64 {
        let bytes =
            |layer: &CompositionLayer| u64::from(layer.size.0) * u64::from(layer.size.1) * 4;
        bytes(&self.current) + self.previous.as_ref().map_or(0, bytes)
    }

    fn commit(&mut self) -> windows::core::Result<()> {
        let start = std::time::Instant::now();
        // SAFETY: Commit 只提交状态，不等待 compositor 或 GPU。
        unsafe {
            self.shared.device.Commit()?;
        }
        self.commit_count += 1;
        self.commit_us += start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        Ok(())
    }

    pub fn begin_draw(
        &mut self,
        size: (u32, u32),
        scale: f32,
    ) -> windows::core::Result<CompositionDraw> {
        // SAFETY: surface 与 context 属于同一个 DXGI 设备；guard 覆盖 BeginDraw 之后的错误路径。
        unsafe {
            if self.current.size != size || self.current.surface.is_none() {
                let surface = self.shared.device.CreateSurface(
                    size.0,
                    size.1,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_ALPHA_MODE_PREMULTIPLIED,
                )?;
                self.current.visual.SetContent(&surface)?;
                self.current.surface = Some(surface);
                self.current.size = size;
            }
            let surface =
                self.current.surface.as_ref().ok_or_else(|| {
                    windows::core::Error::new(E_FAIL, "composition surface missing")
                })?;
            let mut offset = POINT::default();
            let dxgi: IDXGISurface = surface.BeginDraw(None, &mut offset)?;
            let guard = CompositionDraw {
                surface: surface.clone(),
                context: self.context.clone(),
                offset: (offset.x as f32 / scale, offset.y as f32 / scale),
                done: false,
            };
            let properties = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            };
            let bitmap = self
                .context
                .CreateBitmapFromDxgiSurface(&dxgi, Some(&properties))?;
            self.context.SetTarget(&bitmap);
            self.draw_count += 1;
            Ok(guard)
        }
    }

    pub fn finish_draw(
        &mut self,
        draw: CompositionDraw,
        crossfade_ms: Option<u32>,
    ) -> windows::core::Result<()> {
        draw.finish()?;
        if let Some(duration_ms) = crossfade_ms {
            self.start_crossfade(duration_ms)
        } else {
            self.commit()
        }
    }

    pub fn animate_visibility(
        &mut self,
        from: f32,
        to: f32,
        offset: f32,
        duration_ms: u32,
    ) -> windows::core::Result<()> {
        // SAFETY: 只绑定解析动画，按刷新率采样在 compositor 中进行，无 UI 帧回调。
        unsafe {
            if duration_ms == 0 {
                self.opacity.SetOpacity2(to)?;
                self.root.SetOffsetY2(offset * (1.0 - to))?;
            } else {
                self.opacity.SetOpacity(&composition_curve(
                    &self.shared,
                    from,
                    to,
                    duration_ms,
                )?)?;
                self.root.SetOffsetY(&composition_curve(
                    &self.shared,
                    offset * (1.0 - from),
                    offset * (1.0 - to),
                    duration_ms,
                )?)?;
            }
        }
        self.commit()
    }

    pub fn prepare_crossfade(&mut self) -> windows::core::Result<()> {
        self.detach_previous()?;
        let next = CompositionLayer::new(&self.shared)?;
        // SAFETY: 旧纹理保持不变，新纹理首次完整绘制后才提交淡入。
        unsafe {
            next.opacity.SetOpacity2(0.0)?;
            // 空参考配合 insertAbove=true 会插到所有兄弟层下面；必须显式放在旧层之上。
            self.root
                .AddVisual(&next.visual, true, &self.current.visual)?;
        }
        self.previous = Some(std::mem::replace(&mut self.current, next));
        Ok(())
    }

    pub fn start_crossfade(&mut self, duration_ms: u32) -> windows::core::Result<()> {
        if let Some(previous) = self.previous.as_ref() {
            // SAFETY: 两张缓存纹理只有 opacity 在 compositor 上变化，不重新绘制正文。
            unsafe {
                // 旧层作为不透明底层，新层淡入形成线性混合，避免两层各半时背景透出来。
                previous.opacity.SetOpacity2(1.0)?;
                self.current.opacity.SetOpacity(&composition_curve(
                    &self.shared,
                    0.0,
                    1.0,
                    duration_ms,
                )?)?;
            }
        }
        self.commit()
    }

    fn detach_previous(&mut self) -> windows::core::Result<bool> {
        if let Some(previous) = self.previous.take() {
            // SAFETY: compositor 已持有帧；解除 visual 引用后旧 surface 自动释放。
            unsafe {
                self.root.RemoveVisual(&previous.visual)?;
                self.current.opacity.SetOpacity2(1.0)?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub fn finish_crossfade(&mut self) -> windows::core::Result<()> {
        if self.detach_previous()? {
            self.commit()?;
        }
        Ok(())
    }

    pub fn hide(&mut self) -> windows::core::Result<()> {
        self.finish_crossfade()?;
        // SAFETY: 阴影与正文都在 root 下，同一提交一并变为透明，隐藏 HWND 不需 DwmFlush。
        unsafe {
            self.opacity.SetOpacity2(0.0)?;
        }
        self.commit()
    }

    pub fn reveal_static(&mut self) -> windows::core::Result<()> {
        // SAFETY: 无法创建回退宿主时保留已有表面，避免丢掉最后一帧的可见退路。
        unsafe {
            self.opacity.SetOpacity2(1.0)?;
            self.root.SetOffsetY2(0.0)?;
            if self.current.surface.is_some() {
                self.current.opacity.SetOpacity2(1.0)?;
            } else if let Some(previous) = self.previous.as_ref() {
                previous.opacity.SetOpacity2(1.0)?;
            }
        }
        self.commit()
    }
}

impl Drop for CompositionPanel {
    fn drop(&mut self) {
        // SAFETY: 析构前解除 HWND 的 visual tree；weak 设备缓存不会保住已回收的设备。
        unsafe {
            let _ = self.target.SetRoot(None);
            let _ = self.shared.device.Commit();
        }
    }
}

fn composition_curve(
    shared: &CompositionShared,
    from: f32,
    to: f32,
    duration_ms: u32,
) -> windows::core::Result<IDCompositionAnimation> {
    let seconds = f64::from(duration_ms.max(1)) / 1000.0;
    let duration = seconds as f32;
    let delta = to - from;
    // SAFETY: cubic ease-out 是以秒为自变量的解析多项式，终点固定避免外推。
    unsafe {
        let animation = shared.device.CreateAnimation()?;
        animation.AddCubic(
            0.0,
            from,
            3.0 * delta / duration,
            -3.0 * delta / duration.powi(2),
            delta / duration.powi(3),
        )?;
        animation.End(seconds, to)?;
        Ok(animation)
    }
}

#[must_use]
pub struct CompositionDraw {
    surface: IDCompositionSurface,
    context: ID2D1DeviceContext,
    pub offset: (f32, f32),
    done: bool,
}

impl CompositionDraw {
    fn finish(mut self) -> windows::core::Result<()> {
        self.done = true;
        // SAFETY: 自有 context 的 D2D EndDraw 由 Canvas 完成，然后解除 bitmap 才结束 surface 更新。
        unsafe {
            self.context.SetTarget(None);
            self.surface.EndDraw()
        }
    }
}

impl Drop for CompositionDraw {
    fn drop(&mut self) {
        if !self.done {
            // SAFETY: 与 BeginDraw 成对，panic 或 CreateBitmap 失败也不能占着更新锁。
            unsafe {
                self.context.SetTarget(None);
                let _ = self.surface.EndDraw();
            }
        }
    }
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

#[must_use]
pub struct OpacityGuard<'a> {
    target: &'a ID2D1RenderTarget,
}

impl<'a> OpacityGuard<'a> {
    pub fn push(target: &'a ID2D1RenderTarget, bounds: D2D_RECT_F, opacity: f32) -> Self {
        let parameters = D2D1_LAYER_PARAMETERS {
            contentBounds: bounds,
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity: opacity.clamp(0.0, 1.0),
            layerOptions: D2D1_LAYER_OPTIONS_NONE,
            ..Default::default()
        };
        // SAFETY: 参数只含栈上数值与空 COM 引用；Direct2D 管理临时层，Drop 与之成对。
        unsafe { target.PushLayer(&parameters, None) };
        Self { target }
    }
}

impl Drop for OpacityGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: 与 push 的 PushLayer 成对，渲染目标仍由守卫借用。
        unsafe { self.target.PopLayer() };
    }
}

/// 临时换变换矩阵，作用域结束时恢复成换之前的那个。
#[must_use]
pub struct TransformGuard<'a> {
    target: &'a ID2D1RenderTarget,
    previous: Matrix3x2,
}

impl<'a> TransformGuard<'a> {
    pub fn concat(target: &'a ID2D1RenderTarget, transform: &Matrix3x2) -> Self {
        let mut previous = Matrix3x2::identity();
        // SAFETY: 用局部变换叠加表面/窗口内边距，图标不能覆盖外层 DXGI 更新偏移。
        unsafe {
            target.GetTransform(&mut previous);
        }
        Self::set(target, &(*transform * previous))
    }

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

    pub fn composition_device_alive() -> bool {
        COMPOSITION.with(|slot| slot.borrow().upgrade().is_some())
    }

    pub fn private_bytes() -> Option<usize> {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        // SAFETY: 当前进程伪句柄与完整 EX 结构体，API 按 cb 校验可写大小。
        unsafe {
            GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut counters).cast(), counters.cb)
        }
        .ok()?;
        Some(counters.PrivateUsage)
    }

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
        fn rgba(&self, premultiplied: bool) -> Vec<u8> {
            let len = (self.w * self.h * 4) as usize;
            // SAFETY: GdiFlush 让挂起的 GDI 绘制落到位图；bits 指向 w × h × 4 字节、活到 self 释放。
            let src = unsafe {
                let _ = GdiFlush();
                std::slice::from_raw_parts(self.bits, len)
            };
            let mut out = Vec::with_capacity(len);
            for px in src.chunks_exact(4) {
                if let [b, g, r, a] = *px {
                    let a = if premultiplied { a.max(1) } else { 255 };
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
        Some(dib.rgba(false))
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
        Some(dib.rgba(true))
    }
}

/// Windows 的“在 Windows 中显示动画”设置。
pub fn client_area_animations() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut enabled = BOOL(1);
    // SAFETY: 系统向有效的栈上 BOOL 写入，不保留指针。
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut enabled).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_err()
        || enabled.as_bool()
}
