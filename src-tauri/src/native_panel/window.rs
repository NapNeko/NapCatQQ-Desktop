// 面板弹出窗：hwnd 注册表、消息调度和重排，Win32 裸调用在 sys；上层先切到主线程再调。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use windows::Win32::Foundation::{E_FAIL, HWND, LRESULT};
// WM_MOUSELEAVE 在 UI::Controls 里
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    WM_APP, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_KEYDOWN,
    WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT,
    WM_SETCURSOR, WM_SETTINGCHANGE, WM_SIZE, WM_THEMECHANGED, WM_TIMER, WS_CLIPCHILDREN,
    WS_CLIPSIBLINGS, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use super::gfx::{Canvas, Rect, shared};
use super::sys::{self, Hwnd, Message, WndHandler};
use super::theme::{Rgba, Theme, theme};

const TIMER_TRANSITION: usize = 3;
const TIMER_CONTENT_END: usize = 4;
const SURFACE_INSET: f32 = 20.0;
const WM_PANEL_HIDE: u32 = WM_APP + 2;
static MOTION_DURATION: AtomicU32 = AtomicU32::new(180);

pub fn set_motion(enabled: bool, speed: f64) {
    let speed = if speed.is_finite() {
        speed.clamp(0.5, 1.5)
    } else {
        0.5
    };
    MOTION_DURATION.store(
        if enabled {
            (90.0 / speed).round() as u32
        } else {
            0
        },
        Ordering::Relaxed,
    );
}

struct Transition {
    start: Instant,
    duration_ms: u32,
    from: f32,
    to: f32,
    closing: bool,
}

impl Transition {
    fn sample(&self, now: Instant) -> (f32, bool) {
        let progress = (now.saturating_duration_since(self.start).as_secs_f32() * 1000.0
            / self.duration_ms.max(1) as f32)
            .min(1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        (self.from + (self.to - self.from) * eased, progress >= 1.0)
    }
}

const TIMER_ANIM: usize = 1;
const ANIM_INTERVAL_MS: u32 = 16;
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
    fn update_hover(&mut self, x: f32, y: f32, inside: bool, theme: &Theme) -> bool {
        self.hover(x, y, inside, theme);
        true
    }
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
    fn is_tray_menu(&self) -> bool {
        false
    }
    fn dismiss(&mut self) {}
    fn finish_transition(&mut self) {}
}

struct WindowData {
    handle_slot: Rc<Cell<Hwnd>>,
    content: Box<dyn PanelContent>,
    canvas: Option<Canvas>,
    composition: Option<sys::CompositionPanel>,
    scale: f64,
    width_dip: f32,
    height_dip: f32,
    anchor: (i32, i32),
    mouse_tracked: bool,
    // 最近一次 show 用的定位函数，面板开着时重排要按它重新摆位置
    position: Option<PositionFn>,
    animating: bool,
    hover_outside_ticks: u32,
    transition: Option<Transition>,
    visibility: f32,
    settled_rect: Option<(i32, i32, i32, i32)>,
    paints: u32,
    draw_us: u64,
    measure_us: u64,
    position_updates: u32,
    lifecycle_submissions: u32,
    crossfade_ms: Option<u32>,
}

thread_local! {
    static WINDOWS: RefCell<HashMap<isize, Rc<RefCell<WindowData>>>> = RefCell::new(HashMap::new());
    // CreateWindowExW 期间交给 WM_CREATE 的内容。不走 lpCreateParams 裸指针，建窗中途失败时只丢一次
    static PENDING: RefCell<Option<Box<dyn PanelContent>>> = const { RefCell::new(None) };
    static DISPOSED: RefCell<Vec<Rc<RefCell<WindowData>>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn transition_duration() -> u32 {
    if sys::client_area_animations() {
        MOTION_DURATION.load(Ordering::Relaxed)
    } else {
        0
    }
}

fn start_hide(hwnd: Hwnd) {
    if !hwnd.is_visible() {
        return;
    }
    let hide_now = with_window(hwnd, |data| {
        let Ok(mut data) = data.try_borrow_mut() else {
            hwnd.post(WM_PANEL_HIDE);
            return false;
        };
        data.content.dismiss();
        if data.transition.as_ref().is_some_and(|t| t.closing) {
            return false;
        }
        let duration = if data.composition.is_some() {
            transition_duration()
        } else {
            0
        };
        if duration == 0 {
            return true;
        }
        let now = Instant::now();
        let visibility = data
            .transition
            .as_ref()
            .map_or(data.visibility, |t| t.sample(now).0);
        data.visibility = visibility;
        data.transition = Some(Transition {
            start: now,
            duration_ms: ((duration * 2 / 3) as f32 * visibility).round().max(16.0) as u32,
            from: visibility,
            to: 0.0,
            closing: true,
        });
        hwnd.kill_timer(TIMER_HOVER);
        let duration_ms = data.transition.as_ref().map_or(0, |t| t.duration_ms);
        let offset = animation_offset(&data);
        if let Some(composition) = data.composition.as_mut() {
            if let Err(error) = composition.animate_visibility(visibility, 0.0, offset, duration_ms)
            {
                tracing::warn!(%error, "native panel: closing composition failed");
                return true;
            }
            data.lifecycle_submissions += 1;
            hwnd.set_timer(TIMER_TRANSITION, duration_ms + 16);
        }
        false
    })
    .unwrap_or(false);
    if hide_now {
        finish_hide(hwnd);
    }
}

fn finish_hide(hwnd: Hwnd) {
    hwnd.kill_timer(TIMER_TRANSITION);
    hwnd.kill_timer(TIMER_ANIM);
    hwnd.kill_timer(TIMER_HOVER);
    hwnd.kill_timer(TIMER_CONTENT_END);
    with_window(hwnd, |data| {
        if let Ok(mut data) = data.try_borrow_mut() {
            data.transition = None;
            data.animating = false;
            data.visibility = 0.0;
            data.crossfade_ms = None;
            if let Some(composition) = data.composition.as_mut() {
                let _ = composition.hide();
            }
            report_motion(&data, "hidden");
        }
    });
    hwnd.hide();
}

fn tick_transition(hwnd: Hwnd) {
    tick_transition_at(hwnd, Instant::now());
}

fn tick_transition_at(hwnd: Hwnd, now: Instant) {
    let frame = with_window(hwnd, |data| {
        let Ok(mut data) = data.try_borrow_mut() else {
            return None;
        };
        let Some(t) = data.transition.as_ref() else {
            hwnd.kill_timer(TIMER_TRANSITION);
            return None;
        };
        let (visibility, done) = t.sample(now);
        let closing = t.closing;
        let remaining = t.duration_ms.saturating_sub(
            now.saturating_duration_since(t.start)
                .as_millis()
                .min(u128::from(u32::MAX)) as u32,
        );
        let composed = data.composition.is_some();
        data.visibility = visibility;
        let frame = animated_rect(&data).map(|rect| (rect, done, closing, composed, remaining));
        if done {
            data.transition = None;
        }
        frame
    })
    .flatten();
    if let Some(((x, y, _, _), done, closing, composed, remaining)) = frame {
        if done && closing {
            finish_hide(hwnd);
            return;
        }
        // 放掉 WindowData 借用后才移动，SetWindowPos 会同步投递窗口消息。
        if !composed {
            hwnd.move_to(x, y);
            with_window(hwnd, |data| {
                if let Ok(mut data) = data.try_borrow_mut() {
                    data.position_updates += 1;
                }
            });
        } else if !done {
            hwnd.set_timer(TIMER_TRANSITION, remaining.max(1) + 16);
        }
        if done {
            hwnd.kill_timer(TIMER_TRANSITION);
        }
    }
}

fn animated_rect(data: &WindowData) -> Option<(i32, i32, i32, i32)> {
    if data.composition.is_some() {
        return data.settled_rect;
    }
    Some(reveal_rect(
        data.settled_rect?,
        data.anchor,
        data.scale,
        data.visibility,
    ))
}

fn animation_offset(data: &WindowData) -> f32 {
    let below_anchor = data
        .settled_rect
        .is_some_and(|(_, y, _, h)| data.anchor.1 > y + h / 2);
    (if below_anchor { 6.0 } else { -6.0 }) * data.scale as f32
}

fn report_motion(data: &WindowData, phase: &str) {
    static PROFILE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !PROFILE.get_or_init(|| std::env::var_os("NCD_PANEL_PROFILE").is_some()) {
        return;
    }
    let composition = data.composition.as_ref();
    tracing::info!(
        phase,
        paints = data.paints,
        draw_us = data.draw_us,
        measure_us = data.measure_us,
        position_updates = data.position_updates,
        lifecycle_submissions = data.lifecycle_submissions,
        texture_bytes = composition.map_or(0, |c| c.texture_bytes()),
        commits = composition.map_or(0, |c| c.commit_count),
        commit_us = composition.map_or(0, |c| c.commit_us),
        warp = composition.is_some_and(|c| c.is_warp()),
        "native panel motion profile"
    );
}

fn reveal_rect(
    rect: (i32, i32, i32, i32),
    anchor: (i32, i32),
    scale: f64,
    visibility: f32,
) -> (i32, i32, i32, i32) {
    let (x, y, w, h) = rect;
    let visibility = visibility.clamp(0.0, 1.0);
    let direction = if anchor.1 > y + h / 2 { 1.0 } else { -1.0 };
    let offset = (direction * 6.0 * scale * f64::from(1.0 - visibility)).round() as i32;
    (x, y + offset, w, h)
}

fn with_window<R>(hwnd: Hwnd, f: impl FnOnce(&Rc<RefCell<WindowData>>) -> R) -> Option<R> {
    // 先拿出 Rc 再放掉注册表借用：回调里同步建 / 销毁面板时 register / unregister 要 borrow_mut
    let data = WINDOWS.with(|slot| slot.borrow().get(&hwnd.key()).cloned());
    data.map(|data| f(&data))
}

fn refresh_window_theme(hwnd: Hwnd, data: &Rc<RefCell<WindowData>>) {
    if data
        .try_borrow()
        .is_ok_and(|data| data.composition.is_none())
    {
        hwnd.set_dark_frame(theme().is_dark());
    }
    hwnd.invalidate();
}

/// 偏好同步后刷新已显示的面板，不改变显隐或焦点。只从窗口所属主线程调用。
pub(crate) fn refresh_visible_themes() {
    // 不持有注册表或 WindowData 借用调用 Win32，防止同步消息重入或窗口销毁。
    let windows = WINDOWS.with(|slot| slot.borrow().values().cloned().collect::<Vec<_>>());
    for data in windows {
        let hwnd = data.try_borrow().ok().map(|data| data.handle_slot.get());
        if let Some(hwnd) = hwnd.filter(|hwnd| hwnd.is_visible()) {
            refresh_window_theme(hwnd, &data);
        }
    }
}

fn register(hwnd: Hwnd, data: WindowData) {
    WINDOWS.with(|slot| {
        slot.borrow_mut()
            .insert(hwnd.key(), Rc::new(RefCell::new(data)))
    });
}

fn unregister(hwnd: Hwnd) {
    let data = WINDOWS.with(|slot| slot.borrow_mut().remove(&hwnd.key()));
    if let Some(data) = data {
        if !dispose_content(&data) {
            DISPOSED.with(|slot| slot.borrow_mut().push(data));
        }
    }
}

struct Closed;

impl PanelContent for Closed {
    fn measure(&mut self, _: f32, _: &Theme) -> (f32, f32) {
        (1.0, 1.0)
    }
    fn paint(&mut self, _: &Canvas, _: f32, _: f32, _: f32, _: &Theme) {}
    fn click(&mut self, _: f32, _: f32, _: &Theme) -> bool {
        false
    }
    fn hover(&mut self, _: f32, _: f32, _: bool, _: &Theme) {}
    fn key(&mut self, _: u16, _: &Theme) -> bool {
        false
    }
    fn animate(&mut self, _: u32, _: &Theme) -> bool {
        false
    }
}

fn dispose_content(data: &Rc<RefCell<WindowData>>) -> bool {
    let Ok(mut state) = data.try_borrow_mut() else {
        return false;
    };
    state.canvas = None;
    state.composition = None;
    let content = std::mem::replace(&mut state.content, Box::new(Closed));
    drop(state);
    // content 可能持着 NativePanel 的 Rc；先放借用再断环，回收设备/纹理。
    drop(content);
    true
}

struct DisposalGuard;

impl Drop for DisposalGuard {
    fn drop(&mut self) {
        let pending = DISPOSED.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
        for data in pending {
            if !dispose_content(&data) {
                DISPOSED.with(|slot| slot.borrow_mut().push(data));
            }
        }
    }
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
        let _dispose = DisposalGuard;
        let hwnd = msg.hwnd();
        if matches!(
            msg.id(),
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_LBUTTONUP | WM_KEYDOWN
        ) && with_window(hwnd, |data| {
            data.try_borrow()
                .is_ok_and(|d| d.transition.as_ref().is_some_and(|t| t.closing))
        })
        .unwrap_or(false)
        {
            return Some(LRESULT(0));
        }
        match msg.id() {
            WM_CREATE => {
                let Some(content) = PENDING.with(|slot| slot.borrow_mut().take()) else {
                    return Some(LRESULT(-1));
                };
                register(
                    hwnd,
                    WindowData {
                        handle_slot: Rc::new(Cell::new(hwnd)),
                        content,
                        canvas: None,
                        composition: None,
                        scale: 1.0,
                        width_dip: 260.0,
                        height_dip: 1.0,
                        anchor: (0, 0),
                        mouse_tracked: false,
                        position: None,
                        animating: false,
                        hover_outside_ticks: 0,
                        transition: None,
                        visibility: 0.0,
                        settled_rect: None,
                        paints: 0,
                        draw_us: 0,
                        measure_us: 0,
                        position_updates: 0,
                        lifecycle_submissions: 0,
                        crossfade_ms: None,
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
            WM_SETTINGCHANGE | WM_THEMECHANGED => {
                with_window(hwnd, |data| refresh_window_theme(hwnd, data));
                Some(LRESULT(0))
            }
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
                let size = hwnd.client_size();
                with_window(hwnd, |data| {
                    if let Ok(mut data) = data.try_borrow_mut() {
                        if data.composition.is_some() {
                            return;
                        }
                        let resized =
                            size.zip(data.canvas.as_mut())
                                .is_some_and(|((w, h), canvas)| {
                                    w > 0 && h > 0 && canvas.resize(w as u32, h as u32)
                                });
                        if !resized {
                            data.canvas = None;
                        }
                    }
                });
                Some(LRESULT(0))
            }
            WM_PANEL_RELAYOUT => {
                with_window(hwnd, |data| relayout(hwnd, data));
                Some(LRESULT(0))
            }
            WM_PANEL_HIDE => {
                start_hide(hwnd);
                Some(LRESULT(0))
            }
            WM_TIMER => {
                if msg.wparam() == TIMER_TRANSITION {
                    tick_transition(hwnd);
                } else if msg.wparam() == TIMER_CONTENT_END {
                    hwnd.kill_timer(TIMER_CONTENT_END);
                    with_window(hwnd, |data| {
                        if let Ok(mut data) = data.try_borrow_mut() {
                            data.crossfade_ms = None;
                            data.content.finish_transition();
                            if let Some(composition) = data.composition.as_mut() {
                                let _ = composition.finish_crossfade();
                            }
                        }
                    });
                } else if msg.wparam() == TIMER_ANIM {
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
                    if !data.mouse_tracked {
                        data.mouse_tracked = true;
                        hwnd.track_mouse_leave();
                    }
                    let (x, y) = content_point(msg, &data);
                    if data.content.update_hover(x, y, true, &theme()) {
                        hwnd.invalidate();
                    }
                });
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                with_window(hwnd, |data| {
                    let Ok(mut data) = data.try_borrow_mut() else {
                        return;
                    };
                    data.mouse_tracked = false;
                    if data.content.update_hover(-1.0, -1.0, false, &theme()) {
                        hwnd.invalidate();
                    }
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
                    let (x, y) = content_point(msg, &data);
                    if data.composition.is_some()
                        && !Rect::new(0.0, 0.0, data.width_dip, data.height_dip).contains(x, y)
                    {
                        return true;
                    }
                    data.content.click(x, y, &theme())
                });
                if consumed.unwrap_or(false) {
                    start_hide(hwnd);
                } else {
                    hwnd.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let vk = msg.wparam() as u16;
                if vk == VK_ESCAPE.0 {
                    start_hide(hwnd);
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
                let keep = with_window(hwnd, |data| {
                    data.try_borrow()
                        .map(|d| {
                            d.content.is_hover_panel()
                                || (d.content.is_tray_menu()
                                    && keep_menu_for_context_click(
                                        d.anchor,
                                        d.scale,
                                        sys::cursor_pos(),
                                        sys::context_button_down(),
                                    ))
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false);
                if !keep {
                    start_hide(hwnd);
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                start_hide(hwnd);
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

fn keep_menu_for_context_click(
    anchor: (i32, i32),
    scale: f64,
    cursor: Option<(i32, i32)>,
    context_down: bool,
) -> bool {
    let radius = (24.0 * scale).ceil() as i32;
    context_down
        && cursor
            .is_some_and(|(x, y)| (x - anchor.0).abs() <= radius && (y - anchor.1).abs() <= radius)
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
        start_hide(hwnd);
    }
}

/// 鼠标消息 lparam 里的客户区坐标（有符号 16 位）换成 DIP。
fn mouse_dip(msg: &Message, scale: f32) -> (f32, f32) {
    let lparam = msg.lparam();
    let x = (lparam as i16) as f32 / scale;
    let y = ((lparam >> 16) as i16) as f32 / scale;
    (x, y)
}

fn content_point(msg: &Message, data: &WindowData) -> (f32, f32) {
    let scale = data.scale as f32;
    let (x, y) = mouse_dip(msg, scale);
    if data.composition.is_none() {
        return (x, y);
    }
    let visibility = data
        .transition
        .as_ref()
        .map_or(data.visibility, |t| t.sample(Instant::now()).0);
    (
        x - SURFACE_INSET,
        y - SURFACE_INSET - animation_offset(data) * (1.0 - visibility) / scale,
    )
}

/// 画一帧。返回 false 表示 WindowData 正被借着（重入），这次没画。
fn repaint(hwnd: Hwnd, data: &Rc<RefCell<WindowData>>) -> bool {
    if !hwnd.is_visible() {
        return true;
    }
    let Ok(mut data) = data.try_borrow_mut() else {
        return false;
    };
    let started = Instant::now();
    let scale = data.scale as f32;
    if data.canvas.is_none() {
        data.canvas = match data.composition.as_ref() {
            Some(composition) => shared()
                .ok()
                .and_then(|shared| Canvas::new(composition.render_target(), shared, scale).ok()),
            None => build_target(hwnd, scale),
        };
    }
    let composition_draw = if let Some(composition) = data.composition.as_mut() {
        let (w, h) = hwnd.client_size().unwrap_or((1, 1));
        match composition.begin_draw((w.max(1) as u32, h.max(1) as u32), scale) {
            Ok(draw) => Some(draw),
            Err(error) => {
                fallback_to_hwnd(hwnd, &mut data, &error);
                return false;
            }
        }
    } else {
        None
    };
    let Some(canvas) = data.canvas.take() else {
        return true;
    };
    let theme = theme();
    let width = data.width_dip;
    let frame = canvas.begin_draw();
    let offset = composition_draw
        .as_ref()
        .map_or((0.0, 0.0), |draw| draw.offset);
    {
        let _surface = canvas.translate(offset.0, offset.1);
        let (px_w, px_h) = hwnd.client_size().unwrap_or((1, 1));
        let _update = composition_draw.as_ref().map(|_| {
            canvas.clip(Rect::new(
                0.0,
                0.0,
                px_w as f32 / scale,
                px_h as f32 / scale,
            ))
        });
        if composition_draw.is_some() {
            canvas.clear(Rgba::TRANSPARENT);
            let rect = Rect::new(SURFACE_INSET, SURFACE_INSET, width, data.height_dip);
            for spread in (1..=12).rev() {
                let spread = spread as f32;
                canvas.fill_round(
                    Rect::new(
                        rect.x - spread,
                        rect.y - spread + 2.0,
                        rect.w + spread * 2.0,
                        rect.h + spread * 2.0,
                    ),
                    theme.radius_md + spread,
                    Rgba::rgb(0, 0, 0).alpha(0.012),
                );
            }
            canvas.fill_round(rect, theme.radius_md, theme.elevated);
            canvas.ring(rect, theme.radius_md, theme.border_subtle);
            let _content = canvas.translate(SURFACE_INSET, SURFACE_INSET);
            let _clip = canvas.clip(Rect::new(0.0, 0.0, width, data.height_dip));
            data.content.paint(&canvas, 0.0, 0.0, width, &theme);
        } else {
            canvas.clear(theme.elevated);
            data.content.paint(&canvas, 0.0, 0.0, width, &theme);
        }
    }
    // EndDraw 出错（如 D2DERR_RECREATE_TARGET）时丢掉渲染目标，下一帧重建
    let drawn = frame.finish();
    if let Err(error) = drawn {
        drop(composition_draw);
        if data.composition.is_some() {
            fallback_to_hwnd(hwnd, &mut data, &error);
        }
    } else {
        // 新表面与淡入曲线一起发布，避免 opacity=0 的中间提交多占一帧。
        let crossfade_ms = data.crossfade_ms.take();
        let committed = composition_draw
            .map(|draw| {
                data.composition
                    .as_mut()
                    .map(|composition| composition.finish_draw(draw, crossfade_ms))
            })
            .flatten()
            .unwrap_or(Ok(()));
        match committed {
            Ok(()) => {
                data.canvas = Some(canvas);
                if let Some(duration_ms) = crossfade_ms {
                    hwnd.set_timer(TIMER_CONTENT_END, duration_ms + 16);
                }
            }
            Err(error) => fallback_to_hwnd(hwnd, &mut data, &error),
        }
    }
    data.paints += 1;
    data.draw_us += started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    true
}

fn fallback_to_hwnd(hwnd: Hwnd, data: &mut WindowData, error: &windows::core::Error) {
    tracing::warn!(%error, "native panel: compositor unavailable, restoring HWND renderer");
    let hwnd = match hwnd.enable_redirection() {
        Ok(()) => hwnd,
        Err(_) => match replace_window_host(hwnd, data) {
            Ok(hwnd) => hwnd,
            Err(error) => {
                tracing::error!(%error, "native panel: fallback host creation failed");
                if let Some(composition) = data.composition.as_mut() {
                    let _ = composition.reveal_static();
                }
                return;
            }
        },
    };
    data.composition = None;
    data.canvas = None;
    data.crossfade_ms = None;
    data.transition = None;
    data.visibility = 1.0;
    data.content.finish_transition();
    hwnd.kill_timer(TIMER_TRANSITION);
    hwnd.kill_timer(TIMER_CONTENT_END);
    hwnd.set_corner_round();
    hwnd.set_dark_frame(theme().is_dark());
    hwnd.post(WM_PANEL_RELAYOUT);
    hwnd.invalidate();
}

fn replace_window_host(hwnd: Hwnd, data: &mut WindowData) -> windows::core::Result<Hwnd> {
    replace_window_host_using(hwnd, data, |(x, y, w, h)| {
        sys::create_panel_window(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            x,
            y,
            w,
            h,
        )
    })
}

fn replace_window_host_using(
    hwnd: Hwnd,
    data: &mut WindowData,
    create: impl FnOnce((i32, i32, i32, i32)) -> windows::core::Result<Hwnd>,
) -> windows::core::Result<Hwnd> {
    let owner = WINDOWS
        .with(|slot| slot.borrow().get(&hwnd.key()).cloned())
        .ok_or_else(|| windows::core::Error::new(E_FAIL, "fallback window state missing"))?;
    let size = (
        (f64::from(data.width_dip) * data.scale).round().max(1.0) as i32,
        (f64::from(data.height_dip) * data.scale).ceil().max(1.0) as i32,
    );
    let (x, y) = data.position.map_or(data.anchor, |position| {
        position(data.anchor, size, data.scale)
    });
    let rect = (x, y, size.0, size.1);
    let visible = hwnd.is_visible();
    let focused = hwnd.is_foreground();
    // 创建失败时旧宿主/表面保持原状；成功后才迁移注册表与句柄。
    let replacement = create_with_pending(Box::new(Closed), || create(rect))?;
    let placeholder = WINDOWS.with(|slot| {
        let mut windows = slot.borrow_mut();
        let placeholder = windows.insert(replacement.key(), owner);
        windows.remove(&hwnd.key());
        placeholder
    });
    drop(placeholder);
    data.handle_slot.set(replacement);
    data.canvas = None;
    data.composition = None;
    data.settled_rect = Some(rect);
    for timer in [TIMER_TRANSITION, TIMER_ANIM, TIMER_HOVER, TIMER_CONTENT_END] {
        hwnd.kill_timer(timer);
    }
    // 旧键已摘除，WM_DESTROY 不会 dispose 正迁移的 WindowData/content。
    hwnd.destroy();
    replacement.disable_dwm_transitions();
    replacement.set_composition_frame(false);
    replacement.set_corner_round();
    replacement.set_dark_frame(theme().is_dark());
    if visible {
        replacement.show_no_activate();
        if data.animating {
            replacement.set_timer(TIMER_ANIM, ANIM_INTERVAL_MS);
        }
        if data.content.is_hover_panel() {
            replacement.set_timer(TIMER_HOVER, HOVER_POLL_MS);
        }
        if focused {
            replacement.set_foreground();
        }
    }
    replacement.invalidate();
    Ok(replacement)
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
        let measure_started = Instant::now();
        let (w_dip, h_dip) = data.content.measure(width, &theme);
        data.measure_us += measure_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        if w_dip > 0.0 {
            data.width_dip = w_dip;
        }
        data.height_dip = h_dip;
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
            let rect = data.try_borrow_mut().ok().and_then(|mut data| {
                data.settled_rect = Some(surface_rect((x, y, px_w, px_h), &data));
                animated_rect(&data)
            });
            if let Some((x, y, w, h)) = rect {
                place_window(hwnd, data, (x, y, w, h));
            }
        }
        None => {
            let inset = data.try_borrow().map_or(0, |d| surface_inset_px(&d));
            hwnd.set_size(px_w + inset * 2, px_h + inset * 2);
        }
    }
    if start_anim {
        hwnd.set_timer(TIMER_ANIM, ANIM_INTERVAL_MS);
    }
    hwnd.invalidate();
    (px_w, px_h)
}

fn surface_inset_px(data: &WindowData) -> i32 {
    if data.composition.is_some() {
        (f64::from(SURFACE_INSET) * data.scale).round() as i32
    } else {
        0
    }
}

fn surface_rect((x, y, w, h): (i32, i32, i32, i32), data: &WindowData) -> (i32, i32, i32, i32) {
    let inset = surface_inset_px(data);
    (x - inset, y - inset, w + inset * 2, h + inset * 2)
}

fn place_window(hwnd: Hwnd, data: &Rc<RefCell<WindowData>>, rect: (i32, i32, i32, i32)) {
    let (x, y, w, h) = rect;
    let changed = hwnd.screen_rect() != Some((x, y, x + w, y + h));
    if !changed {
        return;
    }
    hwnd.set_pos(x, y, w, h);
    if let Ok(mut data) = data.try_borrow_mut() {
        data.position_updates += 1;
    }
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
    start_hide(Hwnd::from_raw(hwnd));
}

pub fn destroy(hwnd: HWND) {
    Hwnd::from_raw(hwnd).destroy();
}

#[derive(Clone)]
pub struct NativePanel {
    hwnd: Rc<Cell<Hwnd>>,
    data: Rc<RefCell<WindowData>>,
}

impl NativePanel {
    pub fn create(
        anchor: (i32, i32),
        width_dip: f32,
        content: Box<dyn PanelContent>,
    ) -> windows::core::Result<Self> {
        let started = Instant::now();
        ensure_class();
        // 初始尺寸按锚点所在显示器的缩放算，免得起窗时先按主屏大小闪一下
        let scale = crate::windows_ui::monitor_dpi_scale_at(anchor.0, anchor.1).max(0.25);
        let theme = theme();
        let mut content = content;
        let (_w, h_dip) = content.measure(width_dip, &theme);
        let px_w = (f64::from(width_dip) * scale).round().max(1.0) as i32;
        let px_h = (f64::from(h_dip) * scale).ceil().max(1.0) as i32;
        let composited = std::env::var("NCD_PANEL_RENDERER").ok().as_deref() != Some("hwnd");
        let hwnd = create_with_pending(content, || {
            sys::create_panel_window(
                WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | if composited {
                        WS_EX_NOREDIRECTIONBITMAP
                    } else {
                        Default::default()
                    },
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
        if let Ok(mut state) = data.try_borrow_mut() {
            state.anchor = anchor;
            state.width_dip = width_dip;
            state.height_dip = h_dip;
            let dpi = hwnd.dpi();
            state.scale = if dpi == 0 {
                scale
            } else {
                f64::from(dpi) / 96.0
            };
        }
        hwnd.disable_dwm_transitions();
        let composition = if composited {
            match shared().and_then(|shared| sys::CompositionPanel::new(hwnd, &shared.d2d)) {
                Ok(composition) => Some(composition),
                Err(error) => {
                    tracing::warn!(%error, "native panel: composition initialization failed");
                    if hwnd.enable_redirection().is_err() {
                        let replaced = {
                            let mut state = data.borrow_mut();
                            replace_window_host(hwnd, &mut state)
                        };
                        if let Err(error) = replaced {
                            hwnd.destroy();
                            return Err(error);
                        }
                    }
                    None
                }
            }
        } else {
            None
        };
        let hwnd = data.borrow().handle_slot.get();
        hwnd.set_composition_frame(composition.is_some());
        if composition.is_none() {
            hwnd.set_corner_round();
            hwnd.set_dark_frame(theme.is_dark());
        }
        if let Ok(mut data) = data.try_borrow_mut() {
            data.anchor = anchor;
            data.width_dip = width_dip;
            data.height_dip = h_dip;
            data.composition = composition;
            let dpi = hwnd.dpi();
            data.scale = if dpi == 0 {
                scale
            } else {
                f64::from(dpi) / 96.0
            };
        }
        tracing::debug!(
            initialization_us = started.elapsed().as_micros(),
            "native panel initialized"
        );
        let hwnd = data.borrow().handle_slot.clone();
        Ok(Self { hwnd, data })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd.get().raw()
    }

    /// 最近一次 show 时的锚点（屏幕物理坐标）。
    pub fn anchor(&self) -> (i32, i32) {
        self.data.try_borrow().map(|d| d.anchor).unwrap_or((0, 0))
    }

    /// 面板当前是否可见。
    pub fn is_visible(&self) -> bool {
        // 收起动画开始后业务层立即视为关闭；再次展开可以反向接续动画。
        self.hwnd.get().is_visible()
            && self
                .data
                .try_borrow()
                .is_ok_and(|d| !d.transition.as_ref().is_some_and(|t| t.closing))
    }

    pub(crate) fn is_shown(&self) -> bool {
        self.hwnd.get().is_visible()
    }

    pub(crate) fn focus(&self) {
        self.hwnd.get().set_foreground();
    }

    /// 只重画，不重排（头像到了这类不改尺寸的变化）。
    pub fn invalidate(&self) {
        self.hwnd.get().invalidate();
    }

    /// 重新量一次内容；面板开着时一并按锚点调位置和大小。返回 (宽, 高) 物理像素。
    /// 从内容层回调里调（消息处理正借着 WindowData）时改成投递消息，当前消息处理完再重排。
    pub fn resize_to_content(&self) -> (i32, i32) {
        relayout(self.hwnd.get(), &self.data)
    }

    pub(crate) fn crossfade_content(&self, duration_ms: u32) -> bool {
        if duration_ms == 0 {
            return false;
        }
        if let Ok(mut data) = self.data.try_borrow_mut() {
            if let Some(composition) = data.composition.as_mut() {
                if composition.prepare_crossfade().is_ok() {
                    data.crossfade_ms = Some(duration_ms);
                    return true;
                }
            }
        }
        false
    }

    /// 按 anchor 摆位置 + 显示；focus=true 时 SetForegroundWindow 抢焦点。
    /// `position` 是给 (anchor, size, scale) 算 (x, y) 的纯函数，
    /// 由调用方决定是主托盘还是聊天托盘的路径。
    pub fn show_with(&self, anchor: (i32, i32), focus: bool, position: PositionFn) {
        // 锚点换了显示器时先按新屏的缩放排版，窗口挪过去后 WM_DPICHANGED 再校一次
        let target_scale = crate::windows_ui::monitor_dpi_scale_at(anchor.0, anchor.1).max(0.25);
        let was_visible = self.hwnd.get().is_visible();
        let duration = if self
            .data
            .try_borrow()
            .is_ok_and(|data| data.composition.is_some())
        {
            transition_duration()
        } else {
            0
        };
        let Some((scale, restart, settle_visibility)) =
            self.data.try_borrow_mut().ok().map(|mut data| {
                let now = Instant::now();
                let closing = data.transition.as_ref().is_some_and(|t| t.closing);
                let restart = !was_visible || closing;
                let from = if was_visible {
                    data.transition
                        .as_ref()
                        .map_or(data.visibility, |t| t.sample(now).0)
                } else {
                    0.0
                };
                data.visibility = from;
                let settle_visibility = duration == 0 && data.transition.is_some();
                if duration == 0 {
                    data.visibility = 1.0;
                    data.transition = None;
                } else if !was_visible || closing {
                    data.transition = Some(Transition {
                        start: now,
                        duration_ms: (duration as f32 * (1.0 - from)).round().max(16.0) as u32,
                        from,
                        to: 1.0,
                        closing: false,
                    });
                }
                data.anchor = anchor;
                data.position = Some(position);
                data.hover_outside_ticks = 0;
                if (data.scale - target_scale).abs() > f64::EPSILON {
                    data.scale = target_scale;
                    data.canvas = None;
                }
                (data.scale, restart, settle_visibility)
            })
        else {
            return;
        };
        if self
            .data
            .try_borrow()
            .is_ok_and(|data| data.composition.is_none())
        {
            self.hwnd.get().set_dark_frame(theme().is_dark());
        }
        let (px_w, px_h) = relayout(self.hwnd.get(), &self.data);
        let (x, y) = position((anchor.0, anchor.1), (px_w, px_h), scale);
        let hover_panel = self
            .data
            .try_borrow()
            .map(|d| d.content.is_hover_panel())
            .unwrap_or(false);
        let frame = self.data.try_borrow_mut().ok().and_then(|mut data| {
            data.settled_rect = Some(surface_rect((x, y, px_w, px_h), &data));
            animated_rect(&data).map(|rect| (rect, data.transition.is_some()))
        });
        if let Some(((x, y, w, h), transitioning)) = frame {
            place_window(self.hwnd.get(), &self.data, (x, y, w, h));
            if transitioning && self.data.borrow().composition.is_none() {
                self.hwnd.get().set_timer(TIMER_TRANSITION, 16);
            } else if !transitioning {
                self.hwnd.get().kill_timer(TIMER_TRANSITION);
            }
        }
        if !was_visible {
            self.hwnd.get().show_no_activate();
        }
        self.hwnd.get().invalidate();
        self.hwnd.get().paint_now();
        let mut failed_composition = false;
        if let Ok(mut data) = self.data.try_borrow_mut() {
            if restart {
                if let Some(transition) = data.transition.as_mut() {
                    transition.start = Instant::now();
                }
            }
            let curve = data
                .transition
                .as_ref()
                .map(|t| (t.from, t.to, t.duration_ms));
            let offset = animation_offset(&data);
            if let Some(composition) = data.composition.as_mut() {
                let mut failure = None;
                if restart || settle_visibility {
                    let (from, to, duration_ms) = curve.unwrap_or((1.0, 1.0, 0));
                    if let Err(error) =
                        composition.animate_visibility(from, to, offset, duration_ms)
                    {
                        failure = Some(error);
                    }
                    if duration_ms > 0 {
                        self.hwnd
                            .get()
                            .set_timer(TIMER_TRANSITION, duration_ms + 16);
                    }
                }
                if let Some(error) = failure {
                    fallback_to_hwnd(self.hwnd.get(), &mut data, &error);
                    failed_composition = true;
                }
            }
            if restart {
                data.lifecycle_submissions += 1;
            }
            report_motion(&data, "shown");
        }
        if failed_composition {
            relayout(self.hwnd.get(), &self.data);
            self.hwnd.get().paint_now();
        }
        if focus {
            self.hwnd.get().set_foreground();
        }
        if hover_panel {
            self.hwnd.get().set_timer(TIMER_HOVER, HOVER_POLL_MS);
        } else {
            self.hwnd.get().kill_timer(TIMER_HOVER);
        }
    }

    pub fn hide(&self) {
        start_hide(self.hwnd.get());
    }

    #[cfg(test)]
    pub(crate) fn has_canvas(&self) -> bool {
        self.data.borrow().canvas.is_some()
    }

    pub fn destroy(&self) {
        self.hwnd.get().destroy();
    }

    pub fn refresh_theme(&self) {
        refresh_window_theme(self.hwnd.get(), &self.data);
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
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageW, SetWindowPos, TranslateMessage,
        WS_EX_LAYERED,
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

    fn inside_point(panel: &NativePanel) -> LPARAM {
        let data = panel.data.borrow();
        let inset = if data.composition.is_some() {
            SURFACE_INSET
        } else {
            0.0
        };
        let point = ((inset + 30.0) as f64 * data.scale).round() as isize;
        LPARAM(point | (point << 16))
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
        let until = Instant::now() + std::time::Duration::from_millis(500);
        while panel.hwnd.get().is_visible() && Instant::now() < until {
            pump();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!panel.hwnd.get().is_visible());
        panel.destroy();
    }

    #[test]
    fn system_theme_notifications_invalidate_visible_panel_during_reentry() {
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Probe)).expect("create panel");
        panel.show_with((400, 400), false, tray_panel_xy);
        let hwnd = panel.hwnd();
        let focused = panel.hwnd.get().is_foreground();
        for message in [WM_SETTINGCHANGE, WM_THEMECHANGED] {
            panel.hwnd.get().paint_now();
            assert!(!unsafe { GetUpdateRect(hwnd, None, false) }.as_bool());
            let paints = panel.data.borrow().paints;
            let caught = sys::caught_panics();
            {
                // 系统通知可能在内容回调持有 WindowData 时同步重入。
                let _borrow = panel.data.borrow_mut();
                unsafe {
                    let _ = SendMessageW(hwnd, message, None, None);
                }
            }
            assert_eq!(sys::caught_panics(), caught);
            assert!(unsafe { GetUpdateRect(hwnd, None, false) }.as_bool());
            panel.hwnd.get().paint_now();
            assert!(panel.data.borrow().paints > paints);
            assert_eq!(panel.hwnd(), hwnd);
            assert!(panel.is_visible());
            assert_eq!(panel.hwnd.get().is_foreground(), focused);
        }
        panel.destroy();
    }

    #[test]
    fn preference_refresh_repaints_visible_panels_without_showing_hidden_panels() {
        let first = NativePanel::create((400, 400), 260.0, Box::new(Probe)).expect("first panel");
        let second = NativePanel::create((700, 400), 260.0, Box::new(Probe)).expect("second panel");
        let hidden = NativePanel::create((1000, 400), 260.0, Box::new(Probe)).expect("hidden panel");
        first.show_with((400, 400), false, tray_panel_xy);
        second.show_with((700, 400), false, tray_panel_xy);
        first.hwnd.get().paint_now();
        second.hwnd.get().paint_now();
        let first_paints = first.data.borrow().paints;
        let second_paints = second.data.borrow().paints;
        let hidden_paints = hidden.data.borrow().paints;
        let focused = (
            first.hwnd.get().is_foreground(),
            second.hwnd.get().is_foreground(),
        );

        refresh_visible_themes();

        assert!(unsafe { GetUpdateRect(first.hwnd(), None, false) }.as_bool());
        assert!(unsafe { GetUpdateRect(second.hwnd(), None, false) }.as_bool());
        first.hwnd.get().paint_now();
        second.hwnd.get().paint_now();
        assert!(first.data.borrow().paints > first_paints);
        assert!(second.data.borrow().paints > second_paints);
        assert_eq!(hidden.data.borrow().paints, hidden_paints);
        assert!(!hidden.is_shown());
        assert_eq!(
            (
                first.hwnd.get().is_foreground(),
                second.hwnd.get().is_foreground(),
            ),
            focused
        );
        first.destroy();
        second.destroy();
        hidden.destroy();
    }

    #[test]
    fn reentrant_click_does_not_panic() {
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Probe)).expect("create panel");
        let hwnd = panel.hwnd();
        let caught = sys::caught_panics();
        PANEL.with(|slot| *slot.borrow_mut() = Some(panel.clone()));
        panel.show_with((400, 400), false, tray_panel_xy);
        unsafe {
            let _ = SendMessageW(hwnd, WM_LBUTTONUP, None, Some(inside_point(&panel)));
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
            let _ = SendMessageW(hwnd, WM_LBUTTONUP, None, Some(inside_point(&panel)));
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

    #[test]
    fn reveal_moves_toward_the_anchor_without_resizing() {
        let full = (100, 200, 260, 300);
        let bottom = reveal_rect(full, (340, 520), 1.0, 0.5);
        assert_eq!(bottom, (100, 203, 260, 300));
        let top = reveal_rect(full, (340, 180), 1.0, 0.5);
        assert_eq!(top, (100, 197, 260, 300));
        assert_eq!(reveal_rect(full, (340, 520), 1.0, 1.0), full);
        assert_eq!(
            reveal_rect(full, (340, 520), 1.0, 0.0),
            (100, 206, 260, 300)
        );
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for frame in 0..=180 {
                let rect = reveal_rect(full, (340, 520), scale, frame as f32 / 180.0);
                assert_eq!((rect.2, rect.3), (full.2, full.3));
            }
        }
    }

    #[test]
    fn tray_context_click_keeps_the_menu_but_outside_focus_loss_closes_it() {
        assert!(keep_menu_for_context_click(
            (400, 400),
            1.5,
            Some((420, 400)),
            true
        ));
        assert!(!keep_menu_for_context_click(
            (400, 400),
            1.5,
            Some((450, 400)),
            true
        ));
        assert!(!keep_menu_for_context_click(
            (400, 400),
            1.5,
            Some((400, 400)),
            false
        ));
        assert!(!keep_menu_for_context_click((400, 400), 1.5, None, true));
    }

    #[test]
    fn lifecycle_motion_preserves_the_render_target_and_composed_hide_state() {
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Probe)).expect("create panel");
        panel.show_with((400, 400), false, tray_panel_xy);
        let size = panel.hwnd.get().client_size();
        let diagnostics = panel
            .data
            .borrow()
            .canvas
            .as_ref()
            .unwrap()
            .target_diagnostics();
        let position_baseline = panel.data.borrow().position_updates;
        let start = Instant::now();
        panel.data.borrow_mut().transition = Some(Transition {
            start,
            duration_ms: 180,
            from: 0.0,
            to: 1.0,
            closing: false,
        });
        for ms in (0..=192).step_by(16) {
            tick_transition_at(
                panel.hwnd.get(),
                start + std::time::Duration::from_millis(ms),
            );
            assert_eq!(panel.hwnd.get().client_size(), size);
            assert_eq!(
                panel
                    .data
                    .borrow()
                    .canvas
                    .as_ref()
                    .unwrap()
                    .target_diagnostics(),
                diagnostics
            );
        }
        finish_hide(panel.hwnd.get());
        assert!(!panel.is_shown());
        assert!(panel.data.borrow().transition.is_none());
        if panel.data.borrow().composition.is_some() {
            assert_eq!(
                panel.data.borrow().position_updates,
                position_baseline,
                "compositor frames must not move the HWND"
            );
        }
        panel.show_with((400, 400), false, tray_panel_xy);
        assert!(panel.is_shown());
        panel.destroy();
        assert!(panel.data.borrow().composition.is_none());
        assert!(panel.data.borrow().canvas.is_none());
    }

    #[test]
    fn compositor_failure_restores_a_visible_hwnd_render_target() {
        let drops = Rc::new(Cell::new(0));
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Counted(drops.clone())))
            .expect("create");
        let clone = panel.clone();
        let original = panel.hwnd();
        let composed = panel.data.borrow().composition.is_some();
        panel.show_with((400, 400), false, tray_panel_xy);
        {
            let mut data = panel.data.borrow_mut();
            fallback_to_hwnd(
                panel.hwnd.get(),
                &mut data,
                &windows::core::Error::new(E_FAIL, "forced fallback"),
            );
        }
        pump();
        panel.hwnd.get().paint_now();
        assert!(panel.is_shown());
        assert!(panel.has_canvas());
        assert!(panel.data.borrow().composition.is_none());
        assert_eq!(
            clone.hwnd(),
            panel.hwnd(),
            "all clones must follow the fallback host"
        );
        if composed {
            assert_ne!(original, panel.hwnd());
            assert!(!unsafe { IsWindow(Some(original)) }.as_bool());
        }
        assert_eq!(
            drops.get(),
            0,
            "old WM_DESTROY must not dispose the migrated content"
        );
        let ex = unsafe { GetWindowLongPtrW(panel.hwnd(), GWL_EXSTYLE) } as u32;
        assert_eq!(ex & WS_EX_NOREDIRECTIONBITMAP.0, 0);
        let size = panel.hwnd.get().client_size().unwrap();
        assert!(
            panel
                .data
                .borrow_mut()
                .canvas
                .as_mut()
                .unwrap()
                .resize(size.0 as u32, size.1 as u32),
            "fallback must be an actual HWND target"
        );
        panel.destroy();
        assert_eq!(drops.get(), 1);
        clone.destroy();
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn failed_fallback_host_creation_keeps_the_original_visible_state() {
        let drops = Rc::new(Cell::new(0));
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Counted(drops.clone())))
            .expect("create");
        panel.show_with((400, 400), false, tray_panel_xy);
        let original = panel.hwnd.get();
        {
            let mut data = panel.data.borrow_mut();
            let result = replace_window_host_using(original, &mut data, |_| {
                Err(windows::core::Error::new(E_FAIL, "forced creation failure"))
            });
            assert!(result.is_err());
            assert_eq!(data.handle_slot.get(), original);
            assert!(data.canvas.is_some());
        }
        assert!(original.is_visible());
        assert!(WINDOWS.with(|slot| slot.borrow().contains_key(&original.key())));
        assert_eq!(drops.get(), 0);
        panel.destroy();
        assert_eq!(drops.get(), 1);
    }

    struct DestroyOnClick(Rc<Cell<u32>>);

    impl Drop for DestroyOnClick {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    impl PanelContent for DestroyOnClick {
        fn measure(&mut self, _: f32, _: &Theme) -> (f32, f32) {
            (260.0, 120.0)
        }
        fn paint(&mut self, _: &Canvas, _: f32, _: f32, _: f32, _: &Theme) {}
        fn click(&mut self, _: f32, _: f32, _: &Theme) -> bool {
            PANEL.with(|slot| {
                if let Some(panel) = slot.borrow().as_ref() {
                    panel.destroy();
                }
            });
            false
        }
        fn hover(&mut self, _: f32, _: f32, _: bool, _: &Theme) {}
        fn key(&mut self, _: u16, _: &Theme) -> bool {
            false
        }
        fn animate(&mut self, _: u32, _: &Theme) -> bool {
            false
        }
    }

    #[test]
    fn reentrant_destroy_breaks_the_content_cycle_and_releases_the_surface() {
        let drops = Rc::new(Cell::new(0));
        let panel = NativePanel::create((400, 400), 260.0, Box::new(DestroyOnClick(drops.clone())))
            .expect("create");
        PANEL.with(|slot| *slot.borrow_mut() = Some(panel.clone()));
        panel.show_with((400, 400), false, tray_panel_xy);
        unsafe {
            let _ = SendMessageW(panel.hwnd(), WM_LBUTTONUP, None, Some(inside_point(&panel)));
        }
        assert_eq!(drops.get(), 1);
        assert!(panel.data.borrow().composition.is_none());
        assert!(panel.data.borrow().canvas.is_none());
        assert!(DISPOSED.with(|slot| slot.borrow().is_empty()));
        PANEL.with(|slot| slot.borrow_mut().take());
    }

    struct ProfileContent(Box<dyn PanelContent>, Rgba);

    struct ProfileBackground;

    impl PanelContent for ProfileBackground {
        fn measure(&mut self, _: f32, _: &Theme) -> (f32, f32) {
            (600.0, 450.0)
        }
        fn paint(&mut self, canvas: &Canvas, _: f32, _: f32, width: f32, _: &Theme) {
            canvas.fill(Rect::new(0.0, 0.0, width, 450.0), Rgba::rgb(240, 240, 240));
        }
        fn click(&mut self, _: f32, _: f32, _: &Theme) -> bool {
            false
        }
        fn hover(&mut self, _: f32, _: f32, _: bool, _: &Theme) {}
        fn key(&mut self, _: u16, _: &Theme) -> bool {
            false
        }
        fn animate(&mut self, _: u32, _: &Theme) -> bool {
            false
        }
    }

    fn profile_background_xy(anchor: (i32, i32), size: (i32, i32), _: f64) -> (i32, i32) {
        let (x, y, w, h) = crate::windows_ui::monitor_work_area_px(anchor.0, anchor.1);
        (
            clamp_axis(anchor.0 - size.0 / 2, x, w, size.0, 0),
            clamp_axis(anchor.1 - size.1 / 2, y, h, size.1, 0),
        )
    }

    impl PanelContent for ProfileContent {
        fn measure(&mut self, width: f32, theme: &Theme) -> (f32, f32) {
            self.0.measure(width, theme)
        }
        fn paint(&mut self, canvas: &Canvas, x: f32, y: f32, width: f32, theme: &Theme) {
            self.0.paint(canvas, x, y, width, theme);
            canvas.fill(Rect::new(18.0, 18.0, 10.0, 10.0), self.1);
        }
        fn click(&mut self, _: f32, _: f32, _: &Theme) -> bool {
            false
        }
        fn hover(&mut self, _: f32, _: f32, _: bool, _: &Theme) {}
        fn key(&mut self, _: u16, _: &Theme) -> bool {
            false
        }
        fn animate(&mut self, _: u32, _: &Theme) -> bool {
            false
        }
    }

    fn sample_composed_pixel(
        point: (i32, i32),
        millis: u64,
    ) -> std::thread::JoinHandle<Vec<(u128, [u8; 3])>> {
        std::thread::spawn(move || {
            sys::testing::per_monitor_dpi_thread();
            let start = Instant::now();
            let mut samples = Vec::new();
            while start.elapsed().as_millis() < u128::from(millis) {
                if let Some(rgba) = sys::testing::capture_screen(point.0, point.1, 1, 1) {
                    if let Some(pixel) = rgba.get(..3) {
                        samples.push((start.elapsed().as_millis(), [pixel[0], pixel[1], pixel[2]]));
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(6));
            }
            samples
        })
    }

    #[test]
    #[ignore = "需要空闲交互桌面，报告真实合成像素和阶段计时"]
    fn profile_native_tray_composition() {
        use crate::chat_tray_panel_native::{
            Mode,
            tests::{MotionShot, conv, view_with as chat_view},
        };
        use crate::tray_panel_native::tests::{Shot, card, view_with};
        sys::testing::per_monitor_dpi_thread();
        set_motion(true, 0.5);
        assert!(
            transition_duration() > 0,
            "enable Windows client-area animations for the diagnostic"
        );
        let baseline_memory = sys::testing::private_bytes();
        let anchor = (600, 700);
        let main_content = Shot(view_with(vec![
            card("10001", "主托盘诊断", "running", false),
            card("10002", "另一个账号", "stopped", false),
            card("10003", "第三个账号", "stopped", false),
        ]));
        let chat_content = MotionShot(chat_view(
            Mode::Preview,
            (0..5).map(|i| conv(i, i % 2 == 0, 3)).collect(),
            7,
        ));
        let mut panels = Vec::new();
        let mut background = None;
        for (name, content, position) in [
            (
                "main",
                Box::new(main_content) as Box<dyn PanelContent>,
                tray_panel_xy as PositionFn,
            ),
            (
                "chat",
                Box::new(chat_content) as Box<dyn PanelContent>,
                hover_panel_xy as PositionFn,
            ),
        ] {
            let create = Instant::now();
            let panel = NativePanel::create(
                anchor,
                260.0,
                Box::new(ProfileContent(content, Rgba::rgb(255, 0, 0))),
            )
            .expect("create");
            let create_us = create.elapsed().as_micros();
            let (shadow_point, background_anchor) = {
                let data = panel.data.borrow();
                let size = (
                    (data.width_dip as f64 * data.scale).round() as i32,
                    (data.height_dip as f64 * data.scale).ceil() as i32,
                );
                let (x, y) = position(anchor, size, data.scale);
                (
                    (
                        x - (5.0 * data.scale).round() as i32,
                        y + (60.0 * data.scale).round() as i32,
                    ),
                    (x + size.0 / 2, y + size.1 / 2),
                )
            };
            if background.is_none() {
                background = Some(
                    NativePanel::create(background_anchor, 600.0, Box::new(ProfileBackground))
                        .expect("create profile background"),
                );
            }
            // 两种托盘弹出方向不同，底色要跟着待测面板移动，避免采到动态桌面。
            set_motion(false, 0.5);
            background
                .as_ref()
                .unwrap()
                .show_with(background_anchor, false, profile_background_xy);
            unsafe {
                SetWindowPos(
                    background.as_ref().unwrap().hwnd(),
                    Some(panel.hwnd()),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
                .expect("place profile background behind panel");
            }
            sys::testing::dwm_flush();
            set_motion(true, 0.5);
            let shadow_before = sys::testing::capture_screen(shadow_point.0, shadow_point.1, 1, 1)
                .expect("desktop capture");
            assert_eq!(&shadow_before[..3], &[240, 240, 240]);
            eprintln!(
                "SHADOW_BASELINE name={name} point={shadow_point:?} before={shadow_before:?} background_rect={:?} background_visible={:?}",
                background.as_ref().and_then(|b| b.hwnd.get().screen_rect()),
                background.as_ref().map(|b| b.is_shown())
            );
            let show = Instant::now();
            panel.show_with(anchor, false, position);
            let show_us = show.elapsed().as_micros();
            let (point, counts, texture_bytes, device_key, commit_us, warp) = {
                let data = panel.data.borrow();
                let composition = data
                    .composition
                    .as_ref()
                    .expect("DirectComposition required for this diagnostic");
                let (x, y, _, _) = data.settled_rect.unwrap();
                let point = (
                    x + ((SURFACE_INSET + 23.0) * data.scale as f32).round() as i32,
                    y + ((SURFACE_INSET + 23.0) * data.scale as f32).round() as i32,
                );
                (
                    point,
                    (data.paints, data.position_updates, composition.commit_count),
                    composition.texture_bytes(),
                    composition.device_key(),
                    composition.commit_us,
                    composition.is_warp(),
                )
            };
            // 不 pump UI 消息，像素仍应连续变化；这段不依赖 WM_TIMER 驱动画面。
            let opening = sample_composed_pixel(point, 240).join().unwrap();
            let unique = opening
                .iter()
                .map(|(_, color)| *color)
                .collect::<std::collections::HashSet<_>>()
                .len();
            let data = panel.data.borrow();
            let composition = data.composition.as_ref().unwrap();
            assert_eq!(
                (data.paints, data.position_updates, composition.commit_count),
                counts,
                "lifecycle must not require UI repaint, position, or per-frame Commit"
            );
            eprintln!(
                "PANEL_PROFILE name={name} create_us={create_us} show_us={show_us} draw_us={} measure_us={} commit_us={commit_us} surface_logical_bytes={texture_bytes} private_bytes={:?} warp={warp} opening_unique_pixels={unique} opening_ui_paints=0 opening_ui_position_updates=0 opening_ui_commits=0 opening_samples={opening:?}",
                data.draw_us,
                data.measure_us,
                sys::testing::private_bytes()
            );
            drop(data);
            assert!(
                unique >= 4,
                "animation must continue while the UI queue is not pumped"
            );
            pump();
            panel.hide();
            let closing_counts = {
                let data = panel.data.borrow();
                (
                    data.paints,
                    data.position_updates,
                    data.composition.as_ref().unwrap().commit_count,
                )
            };
            let closing = sample_composed_pixel(point, 200).join().unwrap();
            let unique = closing
                .iter()
                .map(|(_, color)| *color)
                .collect::<std::collections::HashSet<_>>()
                .len();
            let data = panel.data.borrow();
            assert_eq!(
                (
                    data.paints,
                    data.position_updates,
                    data.composition.as_ref().unwrap().commit_count
                ),
                closing_counts
            );
            drop(data);
            let shadow_after = sys::testing::capture_screen(shadow_point.0, shadow_point.1, 1, 1)
                .expect("desktop capture");
            eprintln!(
                "SHADOW_RESULT name={name} point={shadow_point:?} before={shadow_before:?} after={shadow_after:?} panel_rect={:?}",
                panel.hwnd.get().screen_rect()
            );
            for (before, after) in shadow_before.iter().zip(&shadow_after).take(3) {
                assert!(
                    before.abs_diff(*after) <= 2,
                    "shadow must fade out with content before the delayed HWND hide"
                );
            }
            eprintln!(
                "PANEL_PROFILE name={name} closing_unique_pixels={unique} closing_ui_paints=0 closing_ui_position_updates=0 closing_ui_commits=0 shadow_restored=true closing_samples={closing:?}"
            );
            assert!(
                unique >= 4,
                "closing must be a composed fade, not immediate hiding"
            );
            pump();
            assert!(!panel.is_shown());
            let warm = Instant::now();
            panel.show_with(anchor, false, position);
            eprintln!(
                "PANEL_PROFILE name={name} warm_show_us={}",
                warm.elapsed().as_micros()
            );
            finish_hide(panel.hwnd.get());
            std::thread::sleep(std::time::Duration::from_millis(32));
            panels.push((panel, device_key));
        }
        assert_eq!(
            panels[0].1, panels[1].1,
            "main and chat must share the rendering device"
        );
        for (panel, _) in panels {
            panel.destroy();
        }
        if let Some(background) = background {
            background.destroy();
        }
        assert!(
            !sys::testing::composition_device_alive(),
            "destroyed panels must release the shared device"
        );
        super::super::gfx::release_shared();
        eprintln!(
            "PANEL_PROFILE memory_before={baseline_memory:?} memory_after_release={:?}",
            sys::testing::private_bytes()
        );
    }

    #[test]
    fn reversed_transition_starts_at_the_current_frame() {
        let start = Instant::now();
        let opening = Transition {
            start,
            duration_ms: 180,
            from: 0.0,
            to: 1.0,
            closing: false,
        };
        let now = start + std::time::Duration::from_millis(60);
        let current = opening.sample(now).0;
        let closing = Transition {
            start: now,
            duration_ms: 120,
            from: current,
            to: 0.0,
            closing: true,
        };
        assert_eq!(closing.sample(now), (current, false));
        let later = now + std::time::Duration::from_millis(40);
        let current = closing.sample(later).0;
        let reopened = Transition {
            start: later,
            duration_ms: 180,
            from: current,
            to: 1.0,
            closing: false,
        };
        assert_eq!(reopened.sample(later), (current, false));
        assert_eq!(
            reopened.sample(later + std::time::Duration::from_millis(180)),
            (1.0, true)
        );
    }

    #[test]
    fn relayout_and_repeated_show_preserve_the_opening_transition() {
        let panel = NativePanel::create((400, 400), 260.0, Box::new(Probe)).expect("create panel");
        panel.show_with((400, 400), false, tray_panel_xy);
        let start = panel.data.borrow().transition.as_ref().map(|t| t.start);
        panel.resize_to_content();
        panel.show_with((400, 400), false, tray_panel_xy);
        assert_eq!(
            panel.data.borrow().transition.as_ref().map(|t| t.start),
            start
        );
        panel.hide();
        let closing = panel.data.borrow().transition.as_ref().map(|t| t.start);
        panel.resize_to_content();
        assert_eq!(
            panel.data.borrow().transition.as_ref().map(|t| t.start),
            closing
        );
        panel.destroy();
    }

    #[test]
    fn chat_preview_to_menu_preserves_window_bounds_and_paints_once() {
        use crate::chat_tray_panel_native::{
            Mode,
            tests::{MotionShot, conv, view_with},
        };
        let anchor = (600, 700);
        let conversations: Vec<_> = (0..5).map(|i| conv(i, i % 2 == 0, 3)).collect();
        let preview = MotionShot(view_with(Mode::Preview, conversations.clone(), 7));
        let panel = NativePanel::create(anchor, 260.0, Box::new(preview)).expect("create panel");
        panel.show_with(anchor, false, hover_panel_xy);
        let (bounds, rect, paints, positions, submissions, commits) = {
            let data = panel.data.borrow();
            (
                panel.hwnd.get().screen_rect(),
                data.settled_rect,
                data.paints,
                data.position_updates,
                data.lifecycle_submissions,
                data.composition.as_ref().map(|c| c.commit_count),
            )
        };
        panel.crossfade_content(180);
        panel.data.borrow_mut().content =
            Box::new(MotionShot(view_with(Mode::Menu, conversations, 7)));
        panel.show_with(anchor, false, hover_panel_xy);
        {
            let data = panel.data.borrow();
            assert_eq!(panel.hwnd.get().screen_rect(), bounds);
            assert_eq!(data.settled_rect, rect);
            assert_eq!(data.paints - paints, 1, "draw only the new body");
            assert_eq!(data.position_updates, positions);
            assert_eq!(data.lifecycle_submissions, submissions);
            if let Some(commits) = commits {
                assert_eq!(
                    data.composition.as_ref().unwrap().commit_count - commits,
                    1,
                    "publish the new surface and its crossfade in one commit"
                );
            }
        }
        panel.destroy();
    }

    #[test]
    #[ignore = "需要空闲交互桌面，测量预览转菜单的真实合成像素和阶段计时"]
    fn profile_chat_preview_to_menu() {
        use crate::chat_tray_panel_native::{
            Mode,
            tests::{MotionShot, conv, view_with},
        };
        sys::testing::per_monitor_dpi_thread();
        set_motion(true, 0.5);
        let anchor = (600, 700);
        let conversations: Vec<_> = (0..5).map(|i| conv(i, i % 2 == 0, 3)).collect();
        let preview = MotionShot(view_with(Mode::Preview, conversations.clone(), 7));
        let panel = NativePanel::create(
            anchor,
            260.0,
            Box::new(ProfileContent(Box::new(preview), Rgba::rgb(255, 0, 0))),
        )
        .expect("create panel");
        panel.show_with(anchor, false, hover_panel_xy);
        std::thread::sleep(std::time::Duration::from_millis(230));
        pump();
        let bounds = panel.hwnd.get().screen_rect();
        for iteration in 0..4 {
            let mode = if iteration % 2 == 0 {
                Mode::Menu
            } else {
                Mode::Preview
            };
            let color = if mode == Mode::Menu {
                Rgba::rgb(0, 0, 255)
            } else {
                Rgba::rgb(255, 0, 0)
            };
            let counts = {
                let data = panel.data.borrow();
                (
                    data.paints,
                    data.position_updates,
                    data.lifecycle_submissions,
                    data.composition.as_ref().unwrap().commit_count,
                )
            };
            let switch = Instant::now();
            assert!(panel.crossfade_content(180), "DirectComposition required");
            panel.data.borrow_mut().content = Box::new(ProfileContent(
                Box::new(MotionShot(view_with(mode, conversations.clone(), 7))),
                color,
            ));
            panel.show_with(anchor, false, hover_panel_xy);
            let switch_us = switch.elapsed().as_micros();
            let (point, committed) = {
                let data = panel.data.borrow();
                assert_eq!(panel.hwnd.get().screen_rect(), bounds);
                assert_eq!(data.paints - counts.0, 1);
                assert_eq!(data.position_updates, counts.1);
                assert_eq!(data.lifecycle_submissions, counts.2);
                assert_eq!(
                    data.composition.as_ref().unwrap().commit_count - counts.3,
                    1
                );
                let (x, y, _, _) = data.settled_rect.unwrap();
                (
                    (
                        x + ((SURFACE_INSET + 23.0) * data.scale as f32).round() as i32,
                        y + ((SURFACE_INSET + 23.0) * data.scale as f32).round() as i32,
                    ),
                    (data.paints, data.composition.as_ref().unwrap().commit_count),
                )
            };
            let samples = sample_composed_pixel(point, 230).join().unwrap();
            let unique = samples
                .iter()
                .map(|(_, color)| *color)
                .collect::<std::collections::HashSet<_>>()
                .len();
            eprintln!(
                "CHAT_MODE_SAMPLES iteration={iteration} point={point:?} unique={unique} samples={samples:?}"
            );
            let data = panel.data.borrow();
            assert_eq!(
                (data.paints, data.composition.as_ref().unwrap().commit_count),
                committed
            );
            assert!(
                unique >= 4,
                "crossfade must continue while UI messages are paused"
            );
            eprintln!(
                "CHAT_MODE_PROFILE iteration={iteration} mode={mode:?} switch_us={switch_us} bounds_unchanged=true switch_paints=1 position_updates=0 lifecycle_submissions=0 crossfade_unique_pixels={unique} crossfade_ui_paints=0 crossfade_ui_commits=0"
            );
            drop(data);
            pump();
        }
        panel.destroy();
    }
}
