//! GTK 托盘面板宿主；内容与 Windows 共用，不创建 WebView。

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gtk::glib::{self, ControlFlow, Propagation, SourceId};
use gtk::prelude::*;
use gtk::{cairo, gdk};

use super::gfx::{Canvas, Rect, Shared, shared};
use super::theme::{Rgba, Theme, theme};

const SURFACE_INSET: f32 = 20.0;
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
static MOTION_DURATION: AtomicU32 = AtomicU32::new(180);
static UI_SCALE: AtomicU64 = AtomicU64::new(1.0f64.to_bits());

thread_local! {
    static WINDOWS: RefCell<Vec<Weak<WindowData>>> = const { RefCell::new(Vec::new()) };
}

fn tray_display() -> Option<gdk::Display> {
    gdk::Display::default()
}

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

pub fn set_ui_scale(scale: f64) {
    UI_SCALE.store(scale.to_bits(), Ordering::Relaxed);
}

pub(crate) fn transition_duration() -> u32 {
    if !gtk::is_initialized_main_thread() {
        return 0;
    }
    if gtk::Settings::default().is_some_and(|settings| !settings.is_gtk_enable_animations()) {
        0
    } else {
        MOTION_DURATION.load(Ordering::Relaxed)
    }
}

/// GTK 窗口及显示器 API 使用逻辑坐标，scale 只包含应用界面倍率。
pub type PositionFn = fn((i32, i32), (i32, i32), f64, &gdk::Display) -> (i32, i32);

pub trait PanelContent {
    fn measure(&mut self, width: f32, theme: &Theme) -> (f32, f32);
    fn paint(&mut self, canvas: &Canvas, x: f32, y: f32, w: f32, theme: &Theme);
    fn click(&mut self, x: f32, y: f32, theme: &Theme) -> bool;
    fn hover(&mut self, x: f32, y: f32, inside: bool, theme: &Theme);
    fn update_hover(&mut self, x: f32, y: f32, inside: bool, theme: &Theme) -> bool {
        self.hover(x, y, inside, theme);
        true
    }
    fn key(&mut self, vk: u16, theme: &Theme) -> bool;
    fn animate(&mut self, dt_ms: u32, theme: &Theme) -> bool;
    fn is_animating(&self) -> bool {
        false
    }
    fn is_hover_panel(&self) -> bool {
        false
    }
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
    window: gtk::Window,
    area: gtk::DrawingArea,
    content: RefCell<Option<Box<dyn PanelContent>>>,
    // 位图、图标及字体缓存与宿主同寿命，不随每一帧的 Cairo context 释放。
    _shared: Rc<Shared>,
    settings: RefCell<Option<gtk::Settings>>,
    settings_handlers: RefCell<Vec<glib::SignalHandlerId>>,
    width: Cell<f32>,
    height: Cell<f32>,
    scale: Cell<f64>,
    anchor: Cell<(i32, i32)>,
    position: Cell<PositionFn>,
    animation: RefCell<Option<SourceId>>,
    last_frame: Cell<Instant>,
    relayout_queued: Cell<bool>,
    hide_queued: Cell<bool>,
    pressed: Cell<bool>,
    destroyed: Cell<bool>,
    draw_error_reported: Cell<bool>,
}

impl WindowData {
    fn select_display(self: &Rc<Self>) {
        let Some(display) = tray_display().filter(|display| *display != self.window.display())
        else {
            return;
        };
        // set_screen 会重新实现底层窗口；先隐藏，避免旧窗口失焦时清除正在打开的内容。
        self.stop_animation();
        self.pressed.set(false);
        self.window.hide();
        let screen = display.default_screen();
        self.window.set_screen(&screen);
        self.window.set_visual(screen.rgba_visual().as_ref());
        bind_settings(self);
    }

    fn with_content<R>(&self, f: impl FnOnce(&mut dyn PanelContent) -> R) -> Option<R> {
        let mut slot = self.content.try_borrow_mut().ok()?;
        Some(f(slot.as_deref_mut()?))
    }

    fn point(&self, x: f64, y: f64) -> (f32, f32) {
        (
            (x / self.scale.get()) as f32 - SURFACE_INSET,
            (y / self.scale.get()) as f32 - SURFACE_INSET,
        )
    }

    fn stop_animation(&self) {
        if let Some(source) = self.animation.borrow_mut().take() {
            source.remove();
        }
    }

    fn animate(self: &Rc<Self>) {
        if self.destroyed.get() || !self.window.is_visible() || self.animation.borrow().is_some() {
            return;
        }
        if transition_duration() == 0
            || !self
                .with_content(|content| content.is_animating())
                .unwrap_or(false)
        {
            return;
        }
        self.last_frame.set(Instant::now());
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(FRAME_INTERVAL, move || {
            let Some(data) = weak.upgrade() else {
                return ControlFlow::Break;
            };
            if data.destroyed.get() || !data.window.is_visible() || transition_duration() == 0 {
                data.animation.borrow_mut().take();
                return ControlFlow::Break;
            }
            let now = Instant::now();
            let elapsed = now
                .duration_since(data.last_frame.replace(now))
                .as_millis()
                .min(u128::from(u32::MAX)) as u32;
            let active = data
                .with_content(|content| content.animate(elapsed, &theme()))
                .unwrap_or(false);
            data.area.queue_draw();
            if active {
                ControlFlow::Continue
            } else {
                data.animation.borrow_mut().take();
                ControlFlow::Break
            }
        });
        *self.animation.borrow_mut() = Some(source);
    }

    fn paint(self: &Rc<Self>, context: &cairo::Context) {
        let result = self.draw(context);
        if let Err(error) = result {
            if !self.draw_error_reported.replace(true) {
                tracing::warn!(%error, "GTK 托盘面板绘制失败");
            }
            self.stop_animation();
        } else {
            self.animate();
        }
    }

    fn draw(&self, context: &cairo::Context) -> Result<(), String> {
        let scale = self.scale.get();
        let canvas = Canvas::new(
            context.clone(),
            (f64::from(self.area.scale_factor()) * scale) as f32,
        )?;
        let _context = canvas.translate(0.0, 0.0);
        context.scale(scale, scale);
        canvas.clear(Rgba::TRANSPARENT);
        let current_theme = theme();
        let rect = Rect::new(
            SURFACE_INSET,
            SURFACE_INSET,
            self.width.get(),
            self.height.get(),
        );
        for spread in (1..=12).rev() {
            let spread = spread as f32;
            canvas.fill_round(
                Rect::new(
                    rect.x - spread,
                    rect.y - spread + 2.0,
                    rect.w + spread * 2.0,
                    rect.h + spread * 2.0,
                ),
                current_theme.radius_md + spread,
                Rgba::rgb(0, 0, 0).alpha(0.012),
            );
        }
        canvas.fill_round(rect, current_theme.radius_md, current_theme.elevated);
        canvas.ring(rect, current_theme.radius_md, current_theme.border_subtle);
        {
            let _content = canvas.translate(SURFACE_INSET, SURFACE_INSET);
            let _clip = canvas.clip(Rect::new(0.0, 0.0, self.width.get(), self.height.get()));
            self.with_content(|content| {
                content.paint(&canvas, 0.0, 0.0, self.width.get(), &current_theme)
            });
        }
        canvas.finish()
    }

    fn relayout(self: &Rc<Self>) -> (i32, i32) {
        if self.destroyed.get() {
            return (0, 0);
        }
        let Some((width, height)) =
            self.with_content(|content| content.measure(self.width.get(), &theme()))
        else {
            // 动作会从 content 回调中重入；等当前鼠标/键盘事件结束再量布局。
            if !self.relayout_queued.replace(true) {
                let weak = Rc::downgrade(self);
                glib::idle_add_local_once(move || {
                    if let Some(data) = weak.upgrade() {
                        data.relayout_queued.set(false);
                        data.relayout();
                    }
                });
            }
            return self.window.size();
        };
        self.width.set(width);
        self.height.set(height);
        self.scale
            .set(f64::from_bits(UI_SCALE.load(Ordering::Relaxed)));
        let size = (
            ((width + SURFACE_INSET * 2.0) as f64 * self.scale.get()).ceil() as i32,
            ((height + SURFACE_INSET * 2.0) as f64 * self.scale.get()).ceil() as i32,
        );
        self.area.set_size_request(size.0, size.1);
        self.window.resize(size.0, size.1);
        if is_x11(&self.window.display()) {
            let (x, y) = (self.position.get())(
                self.anchor.get(),
                size,
                self.scale.get(),
                &self.window.display(),
            );
            self.window.move_(x, y);
        }
        self.area.queue_draw();
        self.animate();
        size
    }

    fn hide(self: &Rc<Self>) {
        if self.destroyed.get() || !self.window.is_visible() {
            return;
        }
        if self.with_content(|content| content.dismiss()).is_none() {
            if !self.hide_queued.replace(true) {
                let weak = Rc::downgrade(self);
                glib::idle_add_local_once(move || {
                    if let Some(data) = weak.upgrade()
                        && data.hide_queued.replace(false)
                    {
                        data.hide();
                    }
                });
            }
            return;
        }
        self.pressed.set(false);
        self.stop_animation();
        self.window.hide();
    }

    fn clear(self: &Rc<Self>) {
        self.destroyed.set(true);
        self.stop_animation();
        if let Some(settings) = self.settings.borrow_mut().take() {
            for handler in self.settings_handlers.borrow_mut().drain(..) {
                settings.disconnect(handler);
            }
        }
        // 业务内容持有 NativePanel，先解除它，断开 GTK 宿主与业务单例的持有环。
        if let Ok(mut content) = self.content.try_borrow_mut() {
            content.take();
        } else {
            // GTK 销毁信号也可能从内容动作中同步发出，等该动作退栈后再解除内容。
            let weak = Rc::downgrade(self);
            glib::idle_add_local_once(move || {
                if let Some(data) = weak.upgrade() {
                    data.content.borrow_mut().take();
                }
            });
        }
    }
}

#[derive(Clone)]
pub struct NativePanel {
    data: Rc<WindowData>,
}

impl NativePanel {
    pub fn create(
        anchor: (i32, i32),
        width_dip: f32,
        content: Box<dyn PanelContent>,
    ) -> Result<Self, String> {
        if !gtk::is_initialized_main_thread() {
            return Err("GTK 托盘面板必须在桌面主线程创建".into());
        }
        let graphics = shared()?;
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        if let Some(display) = tray_display() {
            window.set_screen(&display.default_screen());
        }
        window.set_title("NapCatQQ Desktop · 托盘面板");
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        window.set_keep_above(true);
        window.set_accept_focus(true);
        window.set_focus_on_map(true);
        window.set_type_hint(gdk::WindowTypeHint::Utility);
        window.set_app_paintable(true);
        if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
            window.set_visual(screen.rgba_visual().as_ref());
        }
        let css = gtk::CssProvider::new();
        css.load_from_data(
            b"window.ncd-native-tray { background-color: transparent; box-shadow: none; }",
        )
        .map_err(|error| error.to_string())?;
        window.style_context().add_class("ncd-native-tray");
        window
            .style_context()
            .add_provider(&css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        let area = gtk::DrawingArea::new();
        area.set_can_focus(true);
        area.add_events(
            gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::POINTER_MOTION_MASK
                | gdk::EventMask::LEAVE_NOTIFY_MASK,
        );
        window.add(&area);
        let data = Rc::new(WindowData {
            settings: RefCell::new(None),
            settings_handlers: RefCell::new(Vec::new()),
            window,
            area,
            content: RefCell::new(Some(content)),
            _shared: graphics,
            width: Cell::new(width_dip),
            height: Cell::new(0.0),
            scale: Cell::new(f64::from_bits(UI_SCALE.load(Ordering::Relaxed))),
            anchor: Cell::new(anchor),
            position: Cell::new(tray_panel_xy),
            animation: RefCell::new(None),
            last_frame: Cell::new(Instant::now()),
            relayout_queued: Cell::new(false),
            hide_queued: Cell::new(false),
            pressed: Cell::new(false),
            destroyed: Cell::new(false),
            draw_error_reported: Cell::new(false),
        });
        connect_events(&data);
        bind_settings(&data);
        WINDOWS.with(|windows| {
            let mut windows = windows.borrow_mut();
            windows.retain(|window| window.upgrade().is_some_and(|data| !data.destroyed.get()));
            windows.push(Rc::downgrade(&data));
        });
        data.relayout();
        Ok(Self { data })
    }

    pub fn anchor(&self) -> (i32, i32) {
        self.data.anchor.get()
    }
    pub fn is_visible(&self) -> bool {
        !self.data.destroyed.get() && self.data.window.is_visible()
    }
    pub(crate) fn is_shown(&self) -> bool {
        self.is_visible()
    }
    pub(crate) fn focus(&self) {
        if !self.data.destroyed.get() {
            self.data.window.present();
            self.data.area.grab_focus();
        }
    }
    pub fn invalidate(&self) {
        if !self.data.destroyed.get() {
            self.data.area.queue_draw();
        }
    }
    pub fn resize_to_content(&self) -> (i32, i32) {
        self.data.relayout()
    }
    pub(crate) fn crossfade_content(&self, _duration_ms: u32) -> bool {
        false
    }

    pub fn show_with(&self, anchor: (i32, i32), _focus: bool, position: PositionFn) {
        if self.data.destroyed.get() {
            return;
        }
        self.data.select_display();
        self.data.anchor.set(anchor);
        self.data.position.set(position);
        self.data.hide_queued.set(false);
        self.data.draw_error_reported.set(false);
        self.data.relayout();
        // 从右键或系统菜单显式打开的预览同样拿焦点。
        self.data.window.show_all();
        self.focus();
        self.data.animate();
    }

    pub fn hide(&self) {
        self.data.hide();
    }

    #[expect(unsafe_code, reason = "销毁 GTK 原生窗口；销毁标记阻止后续宿主操作")]
    pub fn destroy(&self) {
        if self.data.destroyed.get() {
            return;
        }
        self.data.clear();
        // SAFETY: 在 GTK 主线程销毁；信号只持 Weak，内容和定时器已解除，公开方法检查销毁状态。
        unsafe {
            self.data.window.destroy();
        }
    }

    pub fn refresh_theme(&self) {
        if self.is_visible() {
            self.data
                .with_content(|content| content.finish_transition());
            self.data.relayout();
        }
    }
}

fn connect_events(data: &Rc<WindowData>) {
    let weak = Rc::downgrade(data);
    data.area.connect_draw(move |_, context| {
        if let Some(data) = weak.upgrade().filter(|data| !data.destroyed.get()) {
            data.paint(context);
        }
        Propagation::Stop
    });
    let weak = Rc::downgrade(data);
    data.area.connect_button_press_event(move |area, event| {
        if event.button() == 1
            && let Some(data) = weak.upgrade()
        {
            data.pressed.set(true);
            area.grab_focus();
            return Propagation::Stop;
        }
        Propagation::Proceed
    });
    let weak = Rc::downgrade(data);
    data.area.connect_button_release_event(move |_, event| {
        let Some(data) = weak
            .upgrade()
            .filter(|data| event.button() == 1 && data.pressed.replace(false))
        else {
            return Propagation::Proceed;
        };
        let (x, y) = event.position();
        let (x, y) = data.point(x, y);
        let inside = Rect::new(0.0, 0.0, data.width.get(), data.height.get()).contains(x, y);
        if !inside
            || data
                .with_content(|content| content.click(x, y, &theme()))
                .unwrap_or(false)
        {
            data.hide();
        } else {
            data.relayout();
        }
        Propagation::Stop
    });
    let weak = Rc::downgrade(data);
    data.area.connect_motion_notify_event(move |_, event| {
        if let Some(data) = weak.upgrade() {
            let (x, y) = event.position();
            let (x, y) = data.point(x, y);
            if data
                .with_content(|content| content.update_hover(x, y, true, &theme()))
                .unwrap_or(false)
            {
                data.area.queue_draw();
            }
        }
        Propagation::Proceed
    });
    let weak = Rc::downgrade(data);
    data.area.connect_leave_notify_event(move |_, _| {
        if let Some(data) = weak.upgrade()
            && data
                .with_content(|content| content.update_hover(0.0, 0.0, false, &theme()))
                .unwrap_or(false)
        {
            data.area.queue_draw();
        }
        Propagation::Proceed
    });
    let weak = Rc::downgrade(data);
    data.window.connect_focus_out_event(move |_, _| {
        if let Some(data) = weak.upgrade() {
            data.hide();
        }
        Propagation::Proceed
    });
    let weak = Rc::downgrade(data);
    data.window.connect_key_press_event(move |_, event| {
        let Some(data) = weak.upgrade() else {
            return Propagation::Proceed;
        };
        if event.keyval() == gdk::keys::constants::Escape {
            data.hide();
            return Propagation::Stop;
        }
        let Some(vk) = virtual_key(event) else {
            return Propagation::Proceed;
        };
        if data
            .with_content(|content| content.key(vk, &theme()))
            .unwrap_or(false)
        {
            data.relayout();
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    let weak = Rc::downgrade(data);
    data.window.connect_delete_event(move |_, _| {
        if let Some(data) = weak.upgrade() {
            data.hide();
        }
        Propagation::Stop
    });
    let weak = Rc::downgrade(data);
    data.window.connect_destroy(move |_| {
        if let Some(data) = weak.upgrade().filter(|data| !data.destroyed.get()) {
            data.clear();
        }
    });
    let weak = Rc::downgrade(data);
    data.area.connect_scale_factor_notify(move |_| {
        if let Some(data) = weak.upgrade() {
            data.relayout();
        }
    });
}

fn bind_settings(data: &Rc<WindowData>) {
    let settings = data.window.settings();
    let previous = data.settings.replace(settings.clone());
    if let Some(previous) = previous {
        for handler in data.settings_handlers.borrow_mut().drain(..) {
            previous.disconnect(handler);
        }
    }
    if let Some(settings) = settings {
        for property in [
            "gtk-theme-name",
            "gtk-application-prefer-dark-theme",
            "gtk-enable-animations",
        ] {
            let weak = Rc::downgrade(data);
            let handler = settings.connect_notify_local(Some(property), move |_, _| {
                if let Some(data) = weak.upgrade() {
                    data.stop_animation();
                    NativePanel { data }.refresh_theme();
                }
            });
            data.settings_handlers.borrow_mut().push(handler);
        }
    }
}

fn virtual_key(event: &gdk::EventKey) -> Option<u16> {
    use gdk::keys::constants as key;
    match event.keyval() {
        key::Return | key::KP_Enter | key::space => Some(0x0d),
        key::Left | key::KP_Left => Some(0x25),
        key::Up | key::KP_Up => Some(0x26),
        key::Right | key::KP_Right => Some(0x27),
        key::Down | key::KP_Down => Some(0x28),
        key::Home | key::KP_Home => Some(0x24),
        key::End | key::KP_End => Some(0x23),
        key::ISO_Left_Tab => Some(0x26),
        key::Tab if event.state().contains(gdk::ModifierType::SHIFT_MASK) => Some(0x26),
        key::Tab => Some(0x28),
        _ => None,
    }
}

fn is_x11(display: &gdk::Display) -> bool {
    display.type_().name() == "GdkX11Display"
}

/// 只从 GTK 主线程调用。Wayland 不提供全局鼠标坐标，交给合成器放置窗口。
pub fn menu_anchor() -> (i32, i32) {
    tray_display()
        .filter(is_x11)
        .and_then(|display| display.default_seat())
        .and_then(|seat| seat.pointer())
        .map(|pointer| {
            let (_, x, y) = pointer.position();
            (x, y)
        })
        .unwrap_or((0, 0))
}

fn work_area(display: &gdk::Display, anchor: (i32, i32)) -> Option<gdk::Rectangle> {
    display
        .monitor_at_point(anchor.0, anchor.1)
        .map(|monitor| monitor.workarea())
}

fn clamp_axis(value: i32, start: i32, size: i32, panel: i32, gap: i32) -> i32 {
    let minimum = start + gap;
    let maximum = (start + size - panel - gap).max(minimum);
    value.clamp(minimum, maximum)
}

pub fn tray_panel_xy(
    anchor: (i32, i32),
    panel: (i32, i32),
    scale: f64,
    display: &gdk::Display,
) -> (i32, i32) {
    let Some(work) = work_area(display, anchor) else {
        return anchor;
    };
    let gap = (10.0 * scale).round() as i32;
    let y = if anchor.1 < work.y() + work.height() / 2 {
        anchor.1 + gap
    } else {
        anchor.1 - panel.1 - gap
    };
    (
        clamp_axis(anchor.0 - panel.0 / 2, work.x(), work.width(), panel.0, gap),
        clamp_axis(y, work.y(), work.height(), panel.1, gap),
    )
}

pub fn hover_panel_xy(
    anchor: (i32, i32),
    panel: (i32, i32),
    scale: f64,
    display: &gdk::Display,
) -> (i32, i32) {
    tray_panel_xy(anchor, panel, scale, display)
}

pub(crate) fn refresh_visible_themes() {
    let windows = WINDOWS.with(|windows| {
        let mut windows = windows.borrow_mut();
        windows.retain(|window| window.upgrade().is_some_and(|data| !data.destroyed.get()));
        windows.iter().filter_map(Weak::upgrade).collect::<Vec<_>>()
    });
    for data in windows {
        NativePanel { data }.refresh_theme();
    }
}
