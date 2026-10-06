// 面板弹出窗：hwnd 注册表、消息调度和重排，Win32 裸调用在 sys；上层先切到主线程再调。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use windows::Win32::Foundation::{E_FAIL, HWND, LRESULT};
// WM_MOUSELEAVE 在 UI::Controls 里
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    WM_APP, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT,
    WM_SETCURSOR, WM_SIZE, WM_TIMER, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

use super::gfx::{Canvas, shared};
use super::sys::{self, Hwnd, Message, WndHandler};
use super::theme::{Theme, theme};

const TIMER_ANIM: usize = 1;
const ANIM_INTERVAL_MS: u32 = 33; // ~30fps，只在动画时才跑
// 悬停面板轮询光标：图标和面板之间隔着一条缝，离开后留 4 拍（约 320ms）让光标走过去
const TIMER_HOVER: usize = 2;
const HOVER_POLL_MS: u32 = 80;
const HOVER_GRACE_TICKS: u32 = 4;
// 内容层在消息回调里要求重排时，WindowData 正被借着，只能投递这条消息延后处理
const WM_PANEL_RELAYOUT: u32 = WM_APP + 1;

/// 按 (锚点, 窗口尺寸, scale) 算窗口左上角的定位函数，全部是屏幕物理像素。
pub type PositionFn = fn((i32, i32), (i32, i32), f64) -> (i32, i32);

/// 面板内容。坐标都是 DIP（CSS 像素）。
pub trait PanelContent {
    /// 按 width 重新排版，返回 (宽, 高)，不取整。
    fn measure(&mut self, width: f32, theme: &Theme) -> (f32, f32);
    fn paint(&mut self, canvas: &Canvas, x: f32, y: f32, w: f32, theme: &Theme);
    /// true：触发了一次动作，马上收起。false：没产生动作（点到了空区 / 禁用项）。
    fn click(&mut self, x: f32, y: f32, theme: &Theme) -> bool;
    fn hover(&mut self, x: f32, y: f32, inside: bool, theme: &Theme);
    /// true：吃掉了键盘事件。ESC 由窗口统一收起，不会进来。
    fn key(&mut self, vk: u16, theme: &Theme) -> bool;
    /// true：动画还在跑。false：动画结束，停掉定时器。
    fn animate(&mut self, dt_ms: u32, theme: &Theme) -> bool;
    /// 重排后要不要开动画定时器（加载圈、呼吸点）。
    fn is_animating(&self) -> bool {
        false
    }
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
    width_dip: f32,
    anchor: (i32, i32),
    mouse_tracked: bool,
    // 最近一次 show 用的定位函数，面板开着时重排要按它重新摆位置
    position: Option<PositionFn>,
    animating: bool,
    hover_outside_ticks: u32,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<isize, Rc<RefCell<WindowData>>>> = RefCell::new(HashMap::new());
    // CreateWindowExW 期间交给 WM_CREATE 的内容。不走 lpCreateParams 裸指针，建窗中途失败时只丢一次
    static PENDING: RefCell<Option<Box<dyn PanelContent>>> = const { RefCell::new(None) };
}

fn with_window<R>(hwnd: Hwnd, f: impl FnOnce(&Rc<RefCell<WindowData>>) -> R) -> Option<R> {
    // 先拿出 Rc 再放掉注册表借用：回调里同步建 / 销毁面板时 register / unregister 要 borrow_mut
    let data = WINDOWS.with(|slot| slot.borrow().get(&hwnd.key()).cloned());
    data.map(|data| f(&data))
}

fn register(hwnd: Hwnd, data: WindowData) {
    WINDOWS.with(|slot| {
        slot.borrow_mut()
            .insert(hwnd.key(), Rc::new(RefCell::new(data)))
    });
}

fn unregister(hwnd: Hwnd) {
    WINDOWS.with(|slot| slot.borrow_mut().remove(&hwnd.key()));
}

/// 把内容挂到 PENDING 上跑 `create`；WM_CREATE 没取走的（建窗中途失败）在这里丢掉。
fn create_with_pending<T>(content: Box<dyn PanelContent>, create: impl FnOnce() -> T) -> T {
    PENDING.with(|slot| *slot.borrow_mut() = Some(content));
    let result = create();
    // 先出借用再 drop，内容的析构里碰 PENDING 也不会撞上
    let leftover = PENDING.with(|slot| slot.borrow_mut().take());
    drop(leftover);
    result
}

fn ensure_class() {
    thread_local! {
        static REGISTERED: Cell<bool> = const { Cell::new(false) };
    }
    REGISTERED.with(|slot| {
        if !slot.replace(true) {
            sys::register_panel_class::<PanelProc>();
        }
    });
}

struct PanelProc;

// 内容层回调可能经 resize_to_content / hide 同步重入这里，所以 WindowData 一律 try_borrow，
// 借不到就跳过这一次。
impl WndHandler for PanelProc {
    fn handle(msg: &Message) -> Option<LRESULT> {
        let hwnd = msg.hwnd();
        match msg.id() {
            WM_CREATE => {
                let Some(content) = PENDING.with(|slot| slot.borrow_mut().take()) else {
                    return Some(LRESULT(-1));
                };
                register(
                    hwnd,
                    WindowData {
                        content,
                        canvas: None,
                        scale: 1.0,
                        width_dip: 260.0,
                        anchor: (0, 0),
                        mouse_tracked: false,
                        position: None,
                        animating: false,
                        hover_outside_ticks: 0,
                    },
                );
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                let dpi = (msg.wparam() >> 16) as u16;
                with_window(hwnd, |data| {
                    if let Ok(mut data) = data.try_borrow_mut() {
                        data.scale = f64::from(dpi.max(1)) / 96.0;
                        data.canvas = None;
                    }
                });
                // 不套系统建议的矩形：它按旧尺寸等比缩放，和按锚点重新定位打架
                hwnd.post(WM_PANEL_RELAYOUT);
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_PAINT => {
                // 不走 BeginPaint/EndPaint 的话更新区域一直无效，系统会不停补发 WM_PAINT，主线程空转重画
                let paint = hwnd.begin_paint();
                let painted = with_window(hwnd, |data| repaint(hwnd, data)).unwrap_or(true);
                drop(paint);
                if !painted {
                    // 正被借着（重入），留到下一轮再画
                    hwnd.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_SIZE => {
                with_window(hwnd, |data| {
                    if let Ok(mut data) = data.try_borrow_mut() {
                        data.canvas = None;
                    }
                });
                Some(LRESULT(0))
            }
            WM_PANEL_RELAYOUT => {
                with_window(hwnd, |data| relayout(hwnd, data));
                Some(LRESULT(0))
            }
            WM_TIMER => {
                if msg.wparam() == TIMER_ANIM {
                    with_window(hwnd, |data| {
                        let keep = {
                            let Ok(mut data) = data.try_borrow_mut() else {
                                return;
                            };
                            let keep = data.content.animate(ANIM_INTERVAL_MS, &theme());
                            if !keep {
                                data.animating = false;
                            }
                            keep
                        };
                        if !keep {
                            hwnd.kill_timer(TIMER_ANIM);
                        }
                        hwnd.invalidate();
                    });
                } else if msg.wparam() == TIMER_HOVER {
                    poll_hover(hwnd);
                }
                Some(LRESULT(0))
            }
            WM_SETCURSOR => {
                // 面板里所有可点的位置都是手型，和 Web 版一致
                sys::set_hand_cursor();
                Some(LRESULT(1))
            }
            WM_MOUSEMOVE => {
                with_window(hwnd, |data| {
                    let Ok(mut data) = data.try_borrow_mut() else {
                        return;
                    };
                    let scale = data.scale as f32;
                    if !data.mouse_tracked {
                        data.mouse_tracked = true;
                        hwnd.track_mouse_leave();
                    }
                    let (x, y) = mouse_dip(msg, scale);
                    data.content.hover(x, y, true, &theme());
                    hwnd.invalidate();
                });
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                with_window(hwnd, |data| {
                    let Ok(mut data) = data.try_borrow_mut() else {
                        return;
                    };
                    data.mouse_tracked = false;
                    data.content.hover(-1.0, -1.0, false, &theme());
                    hwnd.invalidate();
                });
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                hwnd.capture();
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                // 先放捕获再调内容层：click 里出事（dev 下被 trampoline 兜住）也不会把鼠标捕获留在面板上
                sys::release_capture();
                let consumed = with_window(hwnd, |data| {
                    let Ok(mut data) = data.try_borrow_mut() else {
                        return false;
                    };
                    let (x, y) = mouse_dip(msg, data.scale as f32);
                    data.content.click(x, y, &theme())
                });
                if consumed.unwrap_or(false) {
                    hwnd.hide();
                } else {
                    hwnd.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let vk = msg.wparam() as u16;
                if vk == VK_ESCAPE.0 {
                    hwnd.hide();
                    return Some(LRESULT(0));
                }
                with_window(hwnd, |data| {
                    let consumed = match data.try_borrow_mut() {
                        Ok(mut data) => data.content.key(vk, &theme()),
                        Err(_) => false,
                    };
                    if consumed {
                        hwnd.invalidate();
                    }
                });
                Some(LRESULT(0))
            }
            WM_KILLFOCUS => {
                let hover = with_window(hwnd, |data| {
                    data.try_borrow()
                        .map(|d| d.content.is_hover_panel())
                        .unwrap_or(false)
                })
                .unwrap_or(false);
                if !hover {
                    hwnd.hide();
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                hwnd.hide();
                Some(LRESULT(0))
            }
            WM_DESTROY => {
                unregister(hwnd);
                None
            }
            // 没单独处理的消息必须交给 DefWindowProcW：WM_NCCREATE 返回 0 会让 CreateWindowExW 失败。
            // 分支里不要写没 import 的大写常量，Rust 会把它当成变量绑定，变成吞掉所有消息的兜底分支。
            _ => None,
        }
    }
}

/// 悬停面板的一拍：光标在图标附近或面板上就续命，连续 HOVER_GRACE_TICKS 拍在外面就收起。
/// 托盘图标的 Leave 不能用来收：光标从图标往面板走时一定先触发它。
fn poll_hover(hwnd: Hwnd) {
    let probe = with_window(hwnd, |data| {
        data.try_borrow()
            .ok()
            .map(|d| d.content.hover_anchor().map(|a| (a, d.scale)))
    })
    .flatten()
    .flatten();
    let Some(((ax, ay, radius), scale)) = probe.filter(|_| hwnd.is_visible()) else {
        hwnd.kill_timer(TIMER_HOVER);
        return;
    };
    // 半径至少盖住图标到面板那条 18 DIP 的缝
    let radius = radius.max((24.0 * scale).round() as i32);
    let inside = sys::cursor_pos().is_some_and(|(cx, cy)| {
        ((cx - ax).abs() <= radius && (cy - ay).abs() <= radius)
            || hwnd
                .screen_rect()
                .is_some_and(|(l, t, r, b)| cx >= l && cx < r && cy >= t && cy < b)
    });
    let expired = with_window(hwnd, |data| {
        data.try_borrow_mut().ok().map(|mut d| {
            d.hover_outside_ticks = if inside { 0 } else { d.hover_outside_ticks + 1 };
            d.hover_outside_ticks >= HOVER_GRACE_TICKS
        })
    })
    .flatten()
    .unwrap_or(false);
    if expired {
        hwnd.kill_timer(TIMER_HOVER);
        hwnd.hide();
    }
}

/// 鼠标消息 lparam 里的客户区坐标（有符号 16 位）换成 DIP。
fn mouse_dip(msg: &Message, scale: f32) -> (f32, f32) {
    let lparam = msg.lparam();
    let x = (lparam as i16) as f32 / scale;
    let y = ((lparam >> 16) as i16) as f32 / scale;
    (x, y)
}

/// 画一帧。返回 false 表示 WindowData 正被借着（重入），这次没画。
fn repaint(hwnd: Hwnd, data: &Rc<RefCell<WindowData>>) -> bool {
    let Ok(mut data) = data.try_borrow_mut() else {
        return false;
    };
    if data.canvas.is_none() {
        let Some(canvas) = build_target(hwnd, data.scale as f32) else {
            return true;
        };
        data.canvas = Some(canvas);
    }
    let Some(canvas) = data.canvas.take() else {
        return true;
    };
    let theme = theme();
    let width = data.width_dip;
    let frame = canvas.begin_draw();
    canvas.clear(theme.elevated);
    data.content.paint(&canvas, 0.0, 0.0, width, &theme);
    // EndDraw 出错（如 D2DERR_RECREATE_TARGET）时丢掉渲染目标，下一帧重建
    if frame.finish().is_ok() {
        data.canvas = Some(canvas);
    }
    true
}

/// 重新量一次内容，按 宽 DIP × 缩放 调窗口大小；面板开着时按 show 时的锚点重新摆位置。
/// 返回 (宽, 高) 物理像素。WindowData 借不到时投递 WM_PANEL_RELAYOUT 延后处理。
fn relayout(hwnd: Hwnd, data: &Rc<RefCell<WindowData>>) -> (i32, i32) {
    let theme = theme();
    let (px_w, px_h, scale, anchor, position, start_anim) = {
        let Ok(mut data) = data.try_borrow_mut() else {
            hwnd.post(WM_PANEL_RELAYOUT);
            return hwnd.client_size().unwrap_or((0, 0));
        };
        let width = data.width_dip;
        let (w_dip, h_dip) = data.content.measure(width, &theme);
        if w_dip > 0.0 {
            data.width_dip = w_dip;
        }
        let scale = data.scale;
        // 宽取整、高向上取整：最后半个像素被裁掉时底边那条留白会少一截
        let px_w = (f64::from(data.width_dip) * scale).round().max(1.0) as i32;
        let px_h = (f64::from(h_dip) * scale).ceil().max(1.0) as i32;
        let start_anim = !data.animating && data.content.is_animating();
        if start_anim {
            data.animating = true;
        }
        (px_w, px_h, scale, data.anchor, data.position, start_anim)
    };
    // 下面的 Win32 调用会同步发 WM_SIZE 等消息回来，所以必须先放掉借用
    match position.filter(|_| hwnd.is_visible()) {
        Some(position) => {
            let (x, y) = position(anchor, (px_w, px_h), scale);
            hwnd.set_pos(x, y, px_w, px_h);
        }
        None => hwnd.set_size(px_w, px_h),
    }
    if start_anim {
        hwnd.set_timer(TIMER_ANIM, ANIM_INTERVAL_MS);
    }
    hwnd.invalidate();
    (px_w, px_h)
}

/// 一条轴上把面板夹进工作区，留 gap 的边距；工作区比面板还窄时贴最小边。
fn clamp_axis(v: i32, start: i32, size: i32, panel: i32, gap: i32) -> i32 {
    let (min, max) = (start + gap, start + size - panel - gap);
    if max < min { min } else { v.clamp(min, max) }
}

/// 主托盘（右键菜单）的定位：水平对着图标中心，图标在工作区下半就弹在上方，否则弹在下方。
pub fn tray_panel_xy(anchor: (i32, i32), panel: (i32, i32), scale: f64) -> (i32, i32) {
    let (wx, wy, ww, wh) = crate::windows_ui::monitor_work_area_px(anchor.0, anchor.1);
    let gap = (10.0 * scale.max(0.25)).round() as i32;
    let (pw, ph) = panel;
    let x = clamp_axis(anchor.0 - pw / 2, wx, ww, pw, gap);
    let y = if anchor.1 < wy + wh / 2 {
        anchor.1 + gap
    } else {
        anchor.1 - ph - gap
    };
    (x, clamp_axis(y, wy, wh, ph, gap))
}

/// 账号托盘（菜单和悬停预览）的定位：右边缘对着图标右侧 20，优先放在图标上方。
pub fn hover_panel_xy(anchor: (i32, i32), panel: (i32, i32), scale: f64) -> (i32, i32) {
    let (wx, wy, ww, wh) = crate::windows_ui::monitor_work_area_px(anchor.0, anchor.1);
    let scale = scale.max(0.25);
    let (pw, ph) = panel;
    let nudge = (20.0 * scale).round() as i32;
    let gap = (18.0 * scale).round() as i32;
    let x = clamp_axis(anchor.0 - pw + nudge, wx, ww, pw, 0);
    let above = anchor.1 - ph - gap;
    let y = if above >= wy { above } else { anchor.1 + gap };
    (x, clamp_axis(y, wy, wh, ph, 0))
}

fn build_target(hwnd: Hwnd, scale: f32) -> Option<Canvas> {
    let shared = shared().ok()?;
    let (w, h) = hwnd.client_size()?;
    if w <= 0 || h <= 0 {
        return None;
    }
    let target = sys::hwnd_render_target(&shared.d2d, hwnd, w as u32, h as u32, scale).ok()?;
    Canvas::new(target, shared, scale).ok()
}

pub fn hide(hwnd: HWND) {
    Hwnd::from_raw(hwnd).hide();
}

pub fn destroy(hwnd: HWND) {
    Hwnd::from_raw(hwnd).destroy();
}

#[derive(Clone)]
pub struct NativePanel {
    hwnd: Hwnd,
    data: Rc<RefCell<WindowData>>,
}

impl NativePanel {
    pub fn create(
        anchor: (i32, i32),
        width_dip: f32,
        content: Box<dyn PanelContent>,
    ) -> windows::core::Result<Self> {
        ensure_class();
        // 初始尺寸按锚点所在显示器的缩放算，免得起窗时先按主屏大小闪一下
        let scale = crate::windows_ui::monitor_dpi_scale_at(anchor.0, anchor.1).max(0.25);
        let theme = theme();
        let mut content = content;
        let (_w, h_dip) = content.measure(width_dip, &theme);
        let px_w = (f64::from(width_dip) * scale).round().max(1.0) as i32;
        let px_h = (f64::from(h_dip) * scale).ceil().max(1.0) as i32;
        // 不用 WS_EX_LAYERED：分层窗口在调 SetLayeredWindowAttributes / UpdateLayeredWindow
        // 之前一直不可见，HWND 渲染目标也不支持分层窗口。圆角和阴影交给 DWM。
        let hwnd = create_with_pending(content, || {
            sys::create_panel_window(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                anchor.0,
                anchor.1,
                px_w,
                px_h,
            )
        })?;
        let Some(data) = with_window(hwnd, Rc::clone) else {
            hwnd.destroy();
            return Err(windows::core::Error::new(
                E_FAIL,
                "create window failed to register",
            ));
        };
        if !hwnd.set_corner_round() {
            tracing::debug!("native panel: DWM corner preference unavailable (pre-Win11)");
        }
        hwnd.set_dark_frame(theme.is_dark());
        if let Ok(mut data) = data.try_borrow_mut() {
            data.anchor = anchor;
            data.width_dip = width_dip;
            let dpi = hwnd.dpi();
            data.scale = if dpi == 0 {
                scale
            } else {
                f64::from(dpi) / 96.0
            };
        }
        Ok(Self { hwnd, data })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd.raw()
    }

    /// 最近一次 show 时的锚点（屏幕物理坐标）。
    pub fn anchor(&self) -> (i32, i32) {
        self.data.try_borrow().map(|d| d.anchor).unwrap_or((0, 0))
    }

    /// 面板当前是否可见。
    pub fn is_visible(&self) -> bool {
        // 失焦、ESC、点了动作都直接 ShowWindow(SW_HIDE)，以窗口真实状态为准
        self.hwnd.is_visible()
    }

    /// 只重画，不重排（头像到了这类不改尺寸的变化）。
    pub fn invalidate(&self) {
        self.hwnd.invalidate();
    }

    /// 重新量一次内容；面板开着时一并按锚点调位置和大小。返回 (宽, 高) 物理像素。
    /// 从内容层回调里调（消息处理正借着 WindowData）时改成投递消息，当前消息处理完再重排。
    pub fn resize_to_content(&self) -> (i32, i32) {
        relayout(self.hwnd, &self.data)
    }

    /// 按 anchor 摆位置 + 显示；focus=true 时 SetForegroundWindow 抢焦点。
    /// `position` 是给 (anchor, size, scale) 算 (x, y) 的纯函数，
    /// 由调用方决定是主托盘还是聊天托盘的路径。
    pub fn show_with(&self, anchor: (i32, i32), focus: bool, position: PositionFn) {
        // 锚点换了显示器时先按新屏的缩放排版，窗口挪过去后 WM_DPICHANGED 再校一次
        let target_scale = crate::windows_ui::monitor_dpi_scale_at(anchor.0, anchor.1).max(0.25);
        let Some(scale) = self.data.try_borrow_mut().ok().map(|mut data| {
            data.anchor = anchor;
            data.position = Some(position);
            data.hover_outside_ticks = 0;
            if (data.scale - target_scale).abs() > f64::EPSILON {
                data.scale = target_scale;
                data.canvas = None;
            }
            data.scale
        }) else {
            return;
        };
        self.hwnd.set_dark_frame(theme().is_dark());
        let (px_w, px_h) = relayout(self.hwnd, &self.data);
        let (x, y) = position((anchor.0, anchor.1), (px_w, px_h), scale);
        let hover_panel = self
            .data
            .try_borrow()
            .map(|d| d.content.is_hover_panel())
            .unwrap_or(false);
        self.hwnd.set_pos(x, y, px_w, px_h);
        self.hwnd.show_no_activate();
        self.hwnd.invalidate();
        if focus {
            self.hwnd.set_foreground();
        }
        if hover_panel {
            self.hwnd.set_timer(TIMER_HOVER, HOVER_POLL_MS);
        } else {
            self.hwnd.kill_timer(TIMER_HOVER);
        }
    }

    pub fn hide(&self) {
        self.hwnd.hide();
    }

    #[cfg(test)]
    pub(crate) fn has_canvas(&self) -> bool {
        self.data.borrow().canvas.is_some()
    }

    pub fn destroy(&self) {
        self.hwnd.destroy();
    }

    pub fn refresh_theme(&self) {
        self.hwnd.set_dark_frame(theme().is_dark());
        self.hwnd.invalidate();
    }
}

#[cfg(all(test, windows))]
#[expect(unsafe_code, reason = "测试直接驱动 Win32 消息循环、查询窗口状态")]
mod tests {
    use super::*;
    use crate::native_panel::gfx::Rect;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::{GetUpdateRect, InvalidateRect, UpdateWindow};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, VK_DOWN};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GWL_EXSTYLE, GetWindowLongPtrW, IsWindow, MSG, PM_REMOVE, PeekMessageW,
        SendMessageW, TranslateMessage, WS_EX_LAYERED,
    };

    thread_local! {
        static PANEL: RefCell<Option<NativePanel>> = const { RefCell::new(None) };
    }

    fn resize_current_panel() {
        PANEL.with(|slot| {
            if let Some(panel) = slot.borrow().as_ref() {
                panel.resize_to_content();
            }
        });
    }

    // 测试用内容：click / key 里回调 resize_to_content，模拟主托盘 StartStop、翻页 -> mark_dirty 的重入
    struct Probe;

    impl PanelContent for Probe {
        fn measure(&mut self, _width: f32, _theme: &Theme) -> (f32, f32) {
            (260.0, 120.0)
        }
        fn paint(&mut self, canvas: &Canvas, _x: f32, _y: f32, w: f32, theme: &Theme) {
            canvas.fill_round(
                Rect::new(8.0, 8.0, w - 16.0, 40.0),
                theme.radius_sm,
                theme.muted,
            );
        }
        fn click(&mut self, _x: f32, _y: f32, _theme: &Theme) -> bool {
            resize_current_panel();
            false
        }
        fn hover(&mut self, _x: f32, _y: f32, _inside: bool, _theme: &Theme) {}
        fn key(&mut self, _vk: u16, _theme: &Theme) -> bool {
            resize_current_panel();
            true
        }
        fn animate(&mut self, _dt_ms: u32, _theme: &Theme) -> bool {
            false
        }
    }

    // 析构时计数，确认内容只被丢一次；click 里 panic
    struct Counted(Rc<Cell<u32>>);

    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    impl PanelContent for Counted {
        fn measure(&mut self, _width: f32, _theme: &Theme) -> (f32, f32) {
            (260.0, 120.0)
        }
        fn paint(&mut self, _canvas: &Canvas, _x: f32, _y: f32, _w: f32, _theme: &Theme) {}
        #[expect(clippy::panic, reason = "测的就是 click 里 panic")]
        fn click(&mut self, _x: f32, _y: f32, _theme: &Theme) -> bool {
            panic!("content callback panicked");
        }
        fn hover(&mut self, _x: f32, _y: f32, _inside: bool, _theme: &Theme) {}
        fn key(&mut self, _vk: u16, _theme: &Theme) -> bool {
            false
        }
        fn animate(&mut self, _dt_ms: u32, _theme: &Theme) -> bool {
            false
        }
    }

    // 第一次 paint panic，之后正常；记下 paint 次数
    struct PaintsOnceBadly(Rc<Cell<u32>>);

    impl PanelContent for PaintsOnceBadly {
        fn measure(&mut self, _width: f32, _theme: &Theme) -> (f32, f32) {
            (260.0, 120.0)
        }
        #[expect(clippy::panic, reason = "测的就是 paint 里 panic")]
        fn paint(&mut self, canvas: &Canvas, _x: f32, _y: f32, w: f32, theme: &Theme) {
            let n = self.0.get() + 1;
            self.0.set(n);
            let _clip = canvas.clip(Rect::new(0.0, 0.0, w, 60.0));
            if n == 1 {
                panic!("paint panicked");
            }
            canvas.fill(Rect::new(0.0, 0.0, w, 60.0), theme.muted);
        }
        fn click(&mut self, _x: f32, _y: f32, _theme: &Theme) -> bool {
            false
        }
        fn hover(&mut self, _x: f32, _y: f32, _inside: bool, _theme: &Theme) {}
        fn key(&mut self, _vk: u16, _theme: &Theme) -> bool {
            false
        }
        fn animate(&mut self, _dt_ms: u32, _theme: &Theme) -> bool {
            false
        }
    }

    fn pump() {
        let mut msg = MSG::default();
        for _ in 0..50 {
            if !unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                break;
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                let _ = DispatchMessageW(&msg);
            }
        }
    }

    #[test]
    fn panel_creates_visible_non_layered_window() {
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Probe)).expect("create panel");
        assert!(
            PENDING.with(|slot| slot.borrow().is_none()),
            "WM_CREATE must take the pending content"
        );
        let hwnd = panel.hwnd();
        let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
        assert_eq!(
            ex & WS_EX_LAYERED.0,
            0,
            "panel must not be a layered window"
        );
        panel.show_with((400, 400), false, tray_panel_xy);
        assert!(panel.is_visible());
        let _ = unsafe { UpdateWindow(hwnd) };
        assert!(
            panel.has_canvas(),
            "D2D render target should be created on first paint"
        );
        assert!(
            !unsafe { GetUpdateRect(hwnd, None, false) }.as_bool(),
            "WM_PAINT must validate the update region"
        );
        panel.hide();
        assert!(!panel.is_visible());
        panel.destroy();
    }

    #[test]
    fn reentrant_click_does_not_panic() {
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Probe)).expect("create panel");
        let hwnd = panel.hwnd();
        let caught = sys::caught_panics();
        PANEL.with(|slot| *slot.borrow_mut() = Some(panel.clone()));
        panel.show_with((400, 400), false, tray_panel_xy);
        unsafe {
            let _ = SendMessageW(
                hwnd,
                WM_LBUTTONUP,
                None,
                Some(LPARAM(((10 << 16) | 10) as isize)),
            );
        }
        pump();
        assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
        assert_eq!(sys::caught_panics(), caught, "click path must not panic");
        PANEL.with(|slot| slot.borrow_mut().take());
        panel.destroy();
    }

    #[test]
    fn reentrant_key_does_not_panic() {
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Probe)).expect("create panel");
        let hwnd = panel.hwnd();
        let caught = sys::caught_panics();
        PANEL.with(|slot| *slot.borrow_mut() = Some(panel.clone()));
        panel.show_with((400, 400), false, tray_panel_xy);
        unsafe {
            let _ = SendMessageW(hwnd, WM_KEYDOWN, Some(WPARAM(usize::from(VK_DOWN.0))), None);
        }
        pump();
        assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
        assert!(panel.is_visible(), "a consumed key must not hide the panel");
        assert_eq!(sys::caught_panics(), caught, "key path must not panic");
        PANEL.with(|slot| slot.borrow_mut().take());
        panel.destroy();
    }

    #[test]
    fn panic_in_content_is_caught_and_releases_borrow_and_capture() {
        let drops = Rc::new(Cell::new(0));
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Counted(drops.clone())))
            .expect("create panel");
        let hwnd = panel.hwnd();
        panel.show_with((400, 400), false, tray_panel_xy);
        let caught = sys::caught_panics();
        unsafe {
            let _ = SendMessageW(hwnd, WM_LBUTTONDOWN, None, Some(LPARAM(0)));
        }
        assert_eq!(unsafe { GetCapture() }, hwnd, "button down should capture");
        unsafe {
            let _ = SendMessageW(hwnd, WM_LBUTTONUP, None, Some(LPARAM(0)));
        }
        assert_eq!(sys::caught_panics(), caught + 1);
        assert_ne!(
            unsafe { GetCapture() },
            hwnd,
            "a panicking click must not keep mouse capture"
        );
        assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool());
        assert!(
            panel.data.try_borrow_mut().is_ok(),
            "unwinding must release the WindowData borrow"
        );
        panel.destroy();
        drop(panel);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn panic_in_paint_validates_and_recovers() {
        let paints = Rc::new(Cell::new(0));
        let panel =
            NativePanel::create((400, 400), 260.0, Box::new(PaintsOnceBadly(paints.clone())))
                .expect("create panel");
        let hwnd = panel.hwnd();
        let caught = sys::caught_panics();
        panel.show_with((400, 400), false, tray_panel_xy);
        let _ = unsafe { UpdateWindow(hwnd) };
        assert_eq!(sys::caught_panics(), caught + 1);
        assert!(
            !unsafe { GetUpdateRect(hwnd, None, false) }.as_bool(),
            "the paint guard must still EndPaint after a panic"
        );
        assert!(panel.data.try_borrow_mut().is_ok());
        pump();
        assert!(paints.get() <= 2, "no WM_PAINT storm after a panic");
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        let _ = unsafe { UpdateWindow(hwnd) };
        assert!(panel.has_canvas(), "the next frame rebuilds the canvas");
        assert_eq!(sys::caught_panics(), caught + 1);
        assert!(!unsafe { GetUpdateRect(hwnd, None, false) }.as_bool());
        panel.destroy();
    }

    #[test]
    fn pending_content_is_dropped_once_when_create_fails() {
        // WM_CREATE 之前就失败：内容留在 PENDING，由 create_with_pending 丢掉
        let drops = Rc::new(Cell::new(0));
        let result: Result<(), ()> =
            create_with_pending(Box::new(Counted(drops.clone())), || Err(()));
        assert!(result.is_err());
        assert_eq!(drops.get(), 1);
        assert!(PENDING.with(|slot| slot.borrow().is_none()));

        // WM_CREATE 取走后建窗再失败：内容跟着取走的一方走，也只丢一次
        let drops = Rc::new(Cell::new(0));
        let result: Result<(), ()> = create_with_pending(Box::new(Counted(drops.clone())), || {
            drop(PENDING.with(|slot| slot.borrow_mut().take()));
            Err(())
        });
        assert!(result.is_err());
        assert_eq!(drops.get(), 1);
        assert!(PENDING.with(|slot| slot.borrow().is_none()));
    }

    #[test]
    fn clamp_axis_never_panics_when_panel_is_wider_than_work_area() {
        assert_eq!(clamp_axis(500, 0, 300, 400, 10), 10);
        assert_eq!(clamp_axis(1900, 0, 1920, 260, 10), 1650);
        assert_eq!(clamp_axis(-40, 0, 1920, 260, 10), 10);
    }
}
