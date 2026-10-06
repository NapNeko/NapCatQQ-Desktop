// 弹出窗口本体：hwnd 注册表 + WndProc + Direct2D 渲染目标。
//
// 这层只做 Win32 调度，布局计算全交给 [`PanelContent`]。上层先切到主线程
// 再调 [`NativePanel::show`] / [`hide`] / [`destroy`]。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_ALLOW_NCPAINT, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, InvalidateRect, MonitorFromWindow, SetWindowRgn, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CS_DBLCLKS, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow,
    GetClientRect, IDC_ARROW, IDC_HAND, KillTimer, LoadCursorW, RegisterClassExW, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, SetCursor, SetForegroundWindow, SetTimer, SetWindowPos,
    ShowWindow, WNDCLASSEXW, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND,
    WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT,
    WM_SETCURSOR, WM_SIZE, WM_TIMER,
};
// SetCapture / ReleaseCapture 在 UI::Input::KeyboardAndMouse
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
// WM_MOUSELEAVE 在 UI::Controls 里
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::core::PCWSTR;
use windows::core::w;

use super::gfx::{Canvas, Rect, shared};
use super::theme::{Theme, theme};

const CLASS_NAME: PCWSTR = w!("NCD.NativeTrayPanel.v1");
const TIMER_ANIM: usize = 1;
const ANIM_INTERVAL_MS: u32 = 33; // ~30fps，只OUNTOS动画时才跑

/// 面板内容。坐标都是 DIP（CSS 像素）。
pub trait PanelContent {
    /// 按 width 重新算高度；width 给 0 时由内容自己决定（返回 (宽, 高)）。
    fn measure(&mut self, width: f32, theme: &Theme) -> (f32, f32);
    fn paint(&mut self, canvas: &Canvas, x: f32, y: f32, w: f32, theme: &Theme);
    /// true：触发了一次动作，马上收起。false：没产生动作（点到了空区 / 禁用项）。
    fn click(&mut self, x: f32, y: f32, theme: &Theme) -> bool;
    fn hover(&mut self, x: f32, y: f32, inside: bool, theme: &Theme);
    /// true：吃掉了键盘事件。ESC 由窗口统一收起，不会进来。
    fn key(&mut self, vk: u16, theme: &Theme) -> bool;
    /// true：动画还在跑。false：动画结束，停掉定时器。
    fn animate(&mut self, dt_ms: u32, theme: &Theme) -> bool;
    /// true：悬停模式（不拿焦点、随光标离开收起）。
    fn is_hover_panel(&self) -> bool {
        false
    }
    /// 悬停模式：被 hover 的托盘图标中心（屏幕物理坐标）+ 半径（物理像素）。
    fn hover_anchor(&self) -> Option<(i32, i32, i32)> {
        None
    }
}

struct WindowData {
    content: Box<dyn PanelContent>,
    canvas: Option<Canvas>,
    scale: f64,
    anchor: (i32, i32),
    mouse_tracked: bool,
    hidden: bool,
    animating: bool,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<isize, Rc<RefCell<WindowData>>>> = RefCell::new(HashMap::new());
}

fn with_window<R>(hwnd: HWND, f: impl FnOnce(&Rc<RefCell<WindowData>>) -> R) -> Option<R> {
    WINDOWS.with(|slot| slot.borrow().get(&(hwnd.0 as isize)).map(|data| f(data)))
}

fn register(hwnd: HWND, data: WindowData) {
    WINDOWS.with(|slot| slot.borrow_mut().insert(hwnd.0 as isize, Rc::new(RefCell::new(data))));
}

fn unregister(hwnd: HWND) {
    WINDOWS.with(|slot| slot.borrow_mut().remove(&(hwnd.0 as isize)));
}

fn ensure_class() {
    thread_local! {
        static REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    REGISTERED.with(|slot| {
        if slot.replace(true) {
            return;
        }
        // SAFETY: WNDCLASSEXW 结构体在栈上填好再注册。
        unsafe {
            let wnd = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_DBLCLKS,
                lpfnWndProc: Some(wnd_proc),
                hInstance: HINSTANCE(GetModuleHandleW(None).map(|h| h.0).unwrap_or(std::ptr::null_mut())),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            let _ = RegisterClassExW(&wnd);
        }
    });
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            // SAFETY: lpCreateParams 里是调用方 box 好的 PanelContent 本体。
            unsafe {
                let create = lparam.0 as *const CREATESTRUCTW;
                let content_ptr = (*create).lpCreateParams as *mut Box<dyn PanelContent>;
                if content_ptr.is_null() {
                    return LRESULT(-1);
                }
                let content = *Box::from_raw(content_ptr);
                register(hwnd, WindowData {
                    content,
                    canvas: None,
                    scale: 1.0,
                    anchor: (0, 0),
                    mouse_tracked: false,
                    hidden: true,
                    animating: false,
                });
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let dpi = (wparam.0 >> 16) as u16;
            with_window(hwnd, |data| {
                let mut data = data.borrow_mut();
                data.scale = dpi as f64 / 96.0;
                if let Some(c) = data.canvas.as_mut() {
                    c.set_scale(dpi as f32 / 96.0);
                }
            });
            unsafe {
                let rect = lparam.0 as *const RECT;
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    (*rect).left,
                    (*rect).top,
                    (*rect).right - (*rect).left,
                    (*rect).bottom - (*rect).top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            with_window(hwnd, |data| repaint(hwnd, data));
            LRESULT(0)
        }
        WM_SIZE => {
            with_window(hwnd, |data| data.borrow_mut().canvas = None);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_ANIM {
                with_window(hwnd, |data| {
                    let keep = {
                        let mut data = data.borrow_mut();
                        let keep = data.content.animate(ANIM_INTERVAL_MS, &theme());
                        if !keep {
                            data.animating = false;
                        }
                        keep
                    };
                    if !keep {
                        let _ = unsafe { KillTimer(Some(hwnd), TIMER_ANIM) };
                    }
                    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
                });
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            // 面板里所有可点的位置都是手型，和 Web 版一致
            unsafe { SetCursor(LoadCursorW(None, IDC_HAND).ok()) };
            LRESULT(1)
        }
        WM_MOUSEMOVE => {
            with_window(hwnd, |data| {
                let mut data = data.borrow_mut();
                let scale = data.scale as f32;
                if !data.mouse_tracked {
                    data.mouse_tracked = true;
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut track) };
                }
                let x = (lparam.0 as i16) as f32 / scale;
                let y = ((lparam.0 >> 16) as i16) as f32 / scale;
                data.content.hover(x, y, true, &theme());
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_window(hwnd, |data| {
                let mut data = data.borrow_mut();
                data.mouse_tracked = false;
                data.content.hover(-1.0, -1.0, false, &theme());
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            unsafe { SetCapture(hwnd) };
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let consumed = with_window(hwnd, |data| {
                let mut data = data.borrow_mut();
                let scale = data.scale as f32;
                let x = (lparam.0 as i16) as f32 / scale;
                let y = ((lparam.0 >> 16) as i16) as f32 / scale;
                data.content.click(x, y, &theme())
            });
            let _ = unsafe { ReleaseCapture() };
            if consumed.unwrap_or(false) {
                hide(hwnd);
            } else {
                unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 as u16 == VK_ESCAPE.0 {
                hide(hwnd);
                return LRESULT(0);
            }
            with_window(hwnd, |data| {
                let consumed = data.borrow_mut().content.key(wparam.0 as u16, &theme());
                if consumed {
                    unsafe { let _ = InvalidateRect(Some(hwnd), None, false); }
                }
            });
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            let hover = with_window(hwnd, |data| data.borrow().content.is_hover_panel()).unwrap_or(false);
            if !hover {
                hide(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            hide(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            unregister(hwnd);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_NCDESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn repaint(hwnd: HWND, data: &Rc<RefCell<WindowData>>) {
    let mut data = data.borrow_mut();
    if data.canvas.is_none() {
        let Some(canvas) = build_target(hwnd, data.scale as f32) else { return };
        data.canvas = Some(canvas);
    }
    let Some(canvas) = data.canvas.take() else { return };
    let theme = theme();
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    unsafe {
        canvas.target().BeginDraw();
    }
    canvas.clear(theme.elevated);
    let scale = data.scale as f32;
    let w = client.right as f32 / scale;
    data.content.paint(&canvas, 0.0, 0.0, w, &theme);
    let _ = unsafe { canvas.target().EndDraw(None, None) };
    data.canvas = Some(canvas);
}

/// 主托盘（右键菜单）的定位：把面板摆在托盘图标附近，上下取图标所在半区。
pub fn tray_panel_xy(anchor: (i32, i32), panel: (i32, i32), scale: f64) -> (i32, i32) {
    let work = crate::windows_ui::monitor_work_area(anchor.0 as f64, anchor.1 as f64);
    let (wx, wy, ww, wh) = work;
    let scale = scale.max(0.25);
    let ax = anchor.0 as f64 / scale;
    let ay = anchor.1 as f64 / scale;
    let pw = panel.0 as f64 / scale;
    let ph = panel.1 as f64 / scale;
    let mut x = ax - pw / 2.0;
    let mut y = ay - ph - 10.0 * scale;
    x = x.clamp(wx + 10.0 * scale, wx + ww - pw - 10.0 * scale);
    if ay < wy + wh / 2.0 {
        y = ay + 10.0 * scale;
    }
    y = y.clamp(wy + 10.0 * scale, wy + wh - ph - 10.0 * scale);
    ((x * scale).round() as i32, (y * scale).round() as i32)
}

/// 悬停面板的定位：水平贴右边，垂直在图标上方，装不下就贴到上方。
pub fn hover_panel_xy(anchor: (i32, i32), panel: (i32, i32), scale: f64) -> (i32, i32) {
    let work = crate::windows_ui::monitor_work_area(anchor.0 as f64, anchor.1 as f64);
    let (wx, wy, ww, wh) = work;
    let scale = scale.max(0.25);
    let ax = anchor.0 as f64 / scale;
    let ay = anchor.1 as f64 / scale;
    let pw = panel.0 as f64 / scale;
    let ph = panel.1 as f64 / scale;
    let x = (ax - pw + 20.0 * scale).clamp(wx, (wx + ww - pw).max(wx));
    let y = {
        let above = ay - ph - 18.0 * scale;
        if above >= wy { above } else { ay + 18.0 * scale }
    };
    ((x * scale).round() as i32, (y.clamp(wy, (wy + wh - ph).max(wy)) * scale).round() as i32)
}

fn build_target(hwnd: HWND, scale: f32) -> Option<Canvas> {
    let shared = shared().ok()?;
    unsafe {
        let mut client = RECT::default();
        if GetClientRect(hwnd, &mut client).is_err() {
            return None;
        }
        let (w, h) = (client.right as u32, client.bottom as u32);
        if w == 0 || h == 0 {
            return None;
        }
        let rt = match shared.d2d.CreateHwndRenderTarget(
            &windows::Win32::Graphics::Direct2D::D2D1_RENDER_TARGET_PROPERTIES {
                r#type: windows::Win32::Graphics::Direct2D::D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: windows::Win32::Graphics::Direct2D::Common::D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: windows::Win32::Graphics::Direct2D::Common::D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0 * scale,
                dpiY: 96.0 * scale,
                usage: windows::Win32::Graphics::Direct2D::D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: windows::Win32::Graphics::Direct2D::D2D1_FEATURE_LEVEL_DEFAULT,
            },
            &windows::Win32::Graphics::Direct2D::D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U { width: w, height: h },
                presentOptions: windows::Win32::Graphics::Direct2D::D2D1_PRESENT_OPTIONS_NONE,
            },
        ) {
            Ok(rt) => rt,
            Err(_) => return None,
        };
        Canvas::new(rt.into(), shared, scale).ok()
    }
}

pub fn hide(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn destroy(hwnd: HWND) {
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[derive(Clone)]
pub struct NativePanel {
    hwnd: HWND,
    data: Rc<RefCell<WindowData>>,
}

impl NativePanel {
    pub fn create(anchor: (i32, i32), width_dip: f32, content: Box<dyn PanelContent>) -> windows::core::Result<Self> {
        ensure_class();
        // 初始 DPI 按 anchor 所在显示器取，免得起窗时一下就放到错屏
        let scale = crate::windows_ui::monitor_dpi_scale(unsafe {
            MonitorFromWindow(HWND(std::ptr::null_mut()), MONITOR_DEFAULTTONEAREST)
        }) as f32;
        let theme = theme();
        let mut content_mut = content;
        let (_w, h_dip) = content_mut.measure(width_dip, &theme);
        let px_w = (width_dip * scale).round().max(1.0) as i32;
        let px_h = (h_dip * scale).round().max(1.0) as i32;
        let content_ptr = Box::into_raw(Box::new(content_mut));
        // SAFETY: lpCreateParams 里是堆上的 PanelContent box，WM_CREATE 里用 Box::from_raw 收回。
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
                CLASS_NAME,
                PCWSTR::null(),
                WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                anchor.0,
                anchor.1,
                px_w,
                px_h,
                None,
                None,
                None,
                Some(content_ptr as *const core::ffi::c_void),
            )?
        };
        let data = with_window(hwnd, |d| d.clone()).ok_or_else(|| {
            // WM_CREATE 里没注册上：回收 content 防 leak，再销毁
            unsafe {
                let _ = Box::from_raw(content_ptr);
                let _ = DestroyWindow(hwnd);
            }
            windows::core::Error::new(windows::Win32::Foundation::E_FAIL, "create window failed to register")
        })?;
        {
            let mut data = data.borrow_mut();
            data.anchor = anchor;
            data.scale = unsafe { GetDpiForWindow(hwnd) as f64 / 96.0 };
        }
        Ok(Self { hwnd, data })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// 最近一次 show 时的锚点（屏幕物理坐标）。
    pub fn anchor(&self) -> (i32, i32) {
        self.data.borrow().anchor
    }

    /// 面板当前是否可见。
    pub fn is_visible(&self) -> bool {
        let data = self.data.borrow();
        !data.hidden
    }

    /// 重新量一次内容高度，按 scale 设窗口尺寸和圆角。返回窗口当前 (宽, 高) 物理像素。
    /// 重新量一次内容高度，按 scale 设窗口尺寸和圆角。返回窗口当前 (宽, 高) 物理像素。
    pub fn resize_to_content(&self) -> (i32, i32) {
        let theme = theme();
        let mut data = self.data.borrow_mut();
        let scale = data.scale as f32;
        let mut client = RECT::default();
        if unsafe { GetClientRect(self.hwnd, &mut client) }.is_err() {
            return (0, 0);
        }
        let client_w = (client.right - client.left) as f32;
        let width_dip = if client_w <= 0.0 { 260.0 } else { client_w / scale };
        let (_w_dip, h_dip) = data.content.measure(width_dip, &theme);
        let px_h = (h_dip * scale).round().max(1.0) as i32;
        let px_w = client_w.max(1.0) as i32;
        let r = (theme.radius_md * scale).round() as i32;
        unsafe {
            let rgn = CreateRoundRectRgn(0, 0, px_w + 1, px_h + 1, r * 2, r * 2);
            let _ = SetWindowRgn(self.hwnd, Some(rgn), true);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        (px_w, px_h)
    }

    fn resize_to_content_unused(&self) -> (i32, i32) {
        let theme = theme();
        let mut data = self.data.borrow_mut();
        let scale = data.scale as f32;
        let mut client = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut client);
        }
        let client_w = (client.right - client.left) as f32;
        let width_dip = if client_w <= 0.0 { 260.0 } else { client_w / scale };
        let (_w_dip, h_dip) = data.content.measure(width_dip, &theme);
        let px_h = (h_dip * scale).round().max(1.0) as i32;
        let px_w = client_w.max(1.0) as i32;
        let r = (theme.radius_md * scale).round() as i32;
        unsafe {
            let rgn = CreateRoundRectRgn(0, 0, px_w + 1, px_h + 1, r * 2, r * 2);
            let _ = SetWindowRgn(self.hwnd, Some(rgn), true);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        (px_w, px_h)
    }

    /// 按 anchor 摆位置 + 显示；focus=true 时 SetForegroundWindow 抢焦点。
    /// `position` 是给 (anchor, size, work_area, scale) 算 (x, y) 的纯函数，
    /// 由调用方决定是主托盘还是聊天托盘的路径。
    pub fn show_with(&self, anchor: (i32, i32), focus: bool, position: fn((i32, i32), (i32, i32), f64) -> (i32, i32)) {
        let (px_w, px_h) = self.resize_to_content();
        let scale = self.data.borrow().scale;
        let (x, y) = position((anchor.0, anchor.1), (px_w, px_h), scale);
        self.data.borrow_mut().anchor = anchor;
        self.data.borrow_mut().hidden = false;
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, x, y, px_w, px_h, SWP_NOZORDER | SWP_NOACTIVATE);
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
            if focus {
                let _ = SetForegroundWindow(self.hwnd);
            }
            if self.data.borrow().content.is_hover_panel() {
                let _ = SetTimer(Some(self.hwnd), TIMER_ANIM, 80, None);
            }
        }
    }

    pub fn hide(&self) {
        self.data.borrow_mut().hidden = true;
        hide(self.hwnd);
    }

    pub fn destroy(&self) {
        destroy(self.hwnd);
    }

    pub fn refresh_theme(&self) {
        let theme = theme();
        let scale = self.data.borrow().scale as f32;
        let mut client = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut client);
        }
        let r = (theme.radius_md * scale).round() as i32;
        unsafe {
            let rgn = CreateRoundRectRgn(0, 0, client.right - client.left + 1, client.bottom - client.top + 1, r * 2, r * 2);
            let _ = SetWindowRgn(self.hwnd, Some(rgn), true);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
}
