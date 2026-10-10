// 每块显示器一个短期原生截图窗，坐标统一为桌面物理像素。
#![expect(unsafe_code, reason = "原生窗口、输入控件和消息 FFI")]
#![warn(clippy::undocumented_unsafe_blocks)]

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::oneshot;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DeleteObject, FW_NORMAL,
    HFONT, OUT_DEFAULT_PRECIS,
};
use windows::Win32::UI::Controls::EM_LIMITTEXT;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::capture::Desktop;
use super::model::{Annotation, Editor, PixelRect, Point, Tool};
use super::render::Scene;
use super::toolbar::{self, Action, Toolbar};
use crate::native_panel::gfx::{Canvas, Rect, release_shared, shared};
use crate::native_panel::sys::{self, Hwnd, Message, WndHandler};
use crate::native_panel::theme::{Rgba, theme};

const CLASS: PCWSTR = w!("NCD.ChatScreenshot.v1");
const WM_FINISH: u32 = WM_APP + 31;
const WM_TEXT_COMMIT: u32 = WM_APP + 32;
const WM_CHECK_FOCUS: u32 = WM_APP + 33;
const WM_TEXT_CANCEL: u32 = WM_APP + 34;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputAction {
    Attach,
    Copy,
    Save,
    Scroll,
}
pub struct Output {
    pub desktop: Desktop,
    pub editor: Editor,
    pub action: OutputAction,
}
type Sender = oneshot::Sender<Result<Option<Output>, String>>;

struct Overlay {
    hwnd: Hwnd,
    bounds: PixelRect,
    scale: f32,
    canvas: Canvas,
    scene: Scene,
}
enum Gesture {
    Select { start: Point },
    Move { start: Point, original: PixelRect },
    Resize { original: PixelRect, edges: u8 },
    Draw(Annotation),
}
struct TextInput {
    hwnd: HWND,
    annotation: Annotation,
    font: HFONT,
}
impl Drop for TextInput {
    fn drop(&mut self) {
        // SAFETY: 这两个资源由本对象在 UI 线程创建，先关控件再释放其字体。
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            let _ = DeleteObject(self.font.into());
        }
    }
}
struct Session {
    owner: String,
    desktop: Option<Desktop>,
    editor: Editor,
    overlays: Vec<Overlay>,
    gesture: Option<Gesture>,
    cursor: Point,
    hover_selection: Option<PixelRect>,
    hover_action: Option<Action>,
    toolbar_monitor: usize,
    text_input: Option<TextInput>,
    sender: Option<Sender>,
    error: Option<String>,
}
thread_local! {
    static ACTIVE: RefCell<Option<Rc<RefCell<Session>>>> = const { RefCell::new(None) };
    static IME_COMPOSING: Cell<bool> = const { Cell::new(false) };
}
fn active() -> Option<Rc<RefCell<Session>>> {
    ACTIVE.with(|s| s.borrow().clone())
}

pub fn open(desktop: Desktop, owner: String, sender: Sender) -> Result<(), String> {
    if active().is_some() {
        return Err("截图工具已打开".into());
    }
    sys::register_window_class::<Handler>(CLASS);
    let cursor = sys::cursor_pos()
        .map(|(x, y)| Point {
            x: x as f32,
            y: y as f32,
        })
        .unwrap_or(desktop.bounds.origin());
    let mut session = Session {
        owner,
        desktop: Some(desktop),
        editor: Editor::new(),
        overlays: Vec::new(),
        gesture: None,
        cursor,
        hover_selection: None,
        hover_action: None,
        toolbar_monitor: 0,
        text_input: None,
        sender: Some(sender),
        error: None,
    };
    let result = (|| {
        let desktop = session.desktop.as_ref().ok_or("截图画面已释放")?;
        for monitor in &desktop.monitors {
            let r = monitor.bounds;
            // SAFETY: 类名是静态字符串，不传创建指针；坐标经过捕获层的尺寸检查。
            let hwnd = Hwnd::from_raw(
                unsafe {
                    CreateWindowExW(
                        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                        CLASS,
                        w!("截图"),
                        WS_POPUP,
                        r.left,
                        r.top,
                        r.width as i32,
                        r.height as i32,
                        None,
                        None,
                        None,
                        None,
                    )
                }
                .map_err(|e| e.to_string())?,
            );
            let resources = (|| {
                let resources = shared().map_err(|e| e.to_string())?;
                let target = sys::screenshot_render_target(&resources.d2d, hwnd, r.width, r.height)
                    .map_err(|e| e.to_string())?;
                let canvas = Canvas::new(target, resources, 1.0).map_err(|e| e.to_string())?;
                let scene = Scene::new(&canvas, desktop, r)?;
                Ok::<_, String>((canvas, scene))
            })();
            match resources {
                Ok((canvas, scene)) => session.overlays.push(Overlay {
                    hwnd,
                    bounds: r,
                    scale: monitor.scale,
                    canvas,
                    scene,
                }),
                Err(error) => {
                    hwnd.destroy();
                    return Err(error);
                }
            }
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = result {
        for overlay in session.overlays {
            overlay.hwnd.destroy();
        }
        release_shared();
        return Err(error);
    }
    session.toolbar_monitor = session
        .overlays
        .iter()
        .position(|o| o.bounds.contains(cursor))
        .unwrap_or(0);
    session.hover_selection = session.window_under_cursor(cursor);
    let hwnds: Vec<_> = session.overlays.iter().map(|o| o.hwnd).collect();
    let foreground = session
        .overlays
        .get(session.toolbar_monitor)
        .map(|o| o.hwnd);
    ACTIVE.with(|s| *s.borrow_mut() = Some(Rc::new(RefCell::new(session))));
    for hwnd in hwnds {
        hwnd.show_no_activate();
        hwnd.invalidate();
    }
    if let Some(hwnd) = foreground {
        hwnd.set_foreground();
        // SAFETY: 只探测当前前台窗口；后台触发时系统可能拒绝激活请求。
        if unsafe { GetForegroundWindow() } != hwnd.raw() {
            tracing::warn!("截图窗已打开，但未取得前台焦点");
        }
    }
    Ok(())
}

pub fn cancel_owner(owner: &str) {
    let matches = active().is_some_and(|s| s.try_borrow().is_ok_and(|s| s.owner == owner));
    if matches {
        finish(None);
    }
}

pub(super) fn activate() -> Option<bool> {
    let session = active()?;
    let session = session.try_borrow().ok()?;
    let hwnds: Vec<_> = session.overlays.iter().map(|overlay| overlay.hwnd).collect();
    let foreground = session.overlays.get(session.toolbar_monitor)?.hwnd;
    drop(session);
    for hwnd in &hwnds {
        hwnd.show_no_activate();
    }
    foreground.set_foreground();
    // SAFETY: 只比较系统校验过的 HWND；不把请求聚焦当成已经拿到焦点。
    let focused = unsafe { GetForegroundWindow() };
    Some(hwnds.iter().any(|hwnd| hwnd.raw() == focused))
}

fn finish(action: Option<OutputAction>) {
    let Some(session) = active() else {
        return;
    };
    let Ok(mut session) = session.try_borrow_mut() else {
        return;
    };
    ACTIVE.with(|s| s.borrow_mut().take());
    if action.is_some() {
        session.commit_text();
    } else {
        session.text_input.take();
    }
    sys::release_capture();
    let overlays = std::mem::take(&mut session.overlays);
    let sender = session.sender.take();
    let output = action.and_then(|action| {
        session.desktop.take().map(|desktop| Output {
            desktop,
            editor: std::mem::take(&mut session.editor),
            action,
        })
    });
    drop(session);
    // WM_DESTROY 此时查不到 ACTIVE，不能再次回调或借用正在收尾的 Session。
    for overlay in overlays {
        overlay.hwnd.destroy();
    }
    release_shared();
    if let Some(sender) = sender {
        let _ = sender.send(Ok(output));
    }
}

struct Handler;
impl WndHandler for Handler {
    fn handle(msg: &Message) -> Option<LRESULT> {
        match msg.id() {
            WM_ERASEBKGND => Some(LRESULT(1)),
            WM_PAINT => {
                let guard = msg.hwnd().begin_paint();
                if let Some(session) = active()
                    && let Ok(session) = session.try_borrow()
                {
                    session.paint(msg.hwnd(), guard.rect());
                }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONUP => {
                if let Some(session) = active()
                    && let Ok(mut session) = session.try_borrow_mut()
                {
                    let point = sys::cursor_pos()
                        .map(|(x, y)| Point {
                            x: x as f32,
                            y: y as f32,
                        })
                        .unwrap_or(session.cursor);
                    match msg.id() {
                        WM_MOUSEMOVE => session.pointer_move(point),
                        WM_LBUTTONDOWN => session.pointer_down(point, msg.hwnd()),
                        WM_LBUTTONUP => session.pointer_up(point),
                        WM_LBUTTONDBLCLK => {
                            if session.editor.selection.is_some() {
                                msg.hwnd().post(WM_FINISH);
                            }
                        }
                        WM_RBUTTONUP => {
                            session.gesture = None;
                            if session.editor.annotations.is_empty() {
                                session.editor.selection = None;
                                session.invalidate_all();
                            } else {
                                session.editor.undo();
                                session.invalidate_all();
                            }
                        }
                        _ => {}
                    }
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                if let Some(session) = active()
                    && let Ok(mut session) = session.try_borrow_mut()
                {
                    session.key(msg.wparam() as u16, msg.hwnd());
                }
                Some(LRESULT(0))
            }
            WM_SETCURSOR => {
                set_cursor();
                Some(LRESULT(1))
            }
            WM_TEXT_COMMIT => {
                if let Some(session) = active()
                    && let Ok(mut session) = session.try_borrow_mut()
                {
                    session.commit_text();
                    session.invalidate_all();
                }
                Some(LRESULT(0))
            }
            WM_TEXT_CANCEL => {
                if let Some(session) = active()
                    && let Ok(mut session) = session.try_borrow_mut()
                {
                    session.text_input.take();
                    session.invalidate_all();
                }
                Some(LRESULT(0))
            }
            WM_FINISH => {
                finish(Some(match msg.wparam() {
                    1 => OutputAction::Copy,
                    2 => OutputAction::Save,
                    3 => OutputAction::Scroll,
                    _ => OutputAction::Attach,
                }));
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                finish(None);
                Some(LRESULT(0))
            }
            WM_KILLFOCUS => {
                msg.hwnd().post(WM_CHECK_FOCUS);
                Some(LRESULT(0))
            }
            WM_CHECK_FOCUS => {
                check_focus();
                Some(LRESULT(0))
            }
            WM_DISPLAYCHANGE => {
                finish(None);
                Some(LRESULT(0))
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                if let Some(session) = active()
                    && let Ok(mut session) = session.try_borrow_mut()
                {
                    session.gesture = None;
                    if msg.id() == WM_CANCELMODE {
                        sys::release_capture();
                    }
                    session.invalidate_all();
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

impl Session {
    fn window_under_cursor(&self, p: Point) -> Option<PixelRect> {
        let desktop = self.desktop.as_ref()?;
        desktop
            .windows
            .iter()
            .find(|w| w.bounds.contains(p))
            .and_then(|w| w.bounds.intersection(desktop.bounds))
            .or_else(|| {
                desktop
                    .monitors
                    .iter()
                    .find(|m| m.bounds.contains(p))
                    .map(|m| m.bounds)
            })
    }
    fn toolbar(&self) -> Option<Toolbar> {
        let selection = self.editor.selection?;
        let monitor = self.overlays.get(self.toolbar_monitor)?;
        Some(Toolbar::new(selection, monitor.bounds, monitor.scale))
    }
    fn ui_scale(&self, point: Point) -> f32 {
        self.overlays
            .iter()
            .find(|overlay| overlay.bounds.contains(point))
            .or_else(|| self.overlays.get(self.toolbar_monitor))
            .map(|overlay| overlay.scale.clamp(1.0, 3.0))
            .unwrap_or(1.0)
    }
    fn invalidate_all(&self) {
        for overlay in &self.overlays {
            overlay.hwnd.invalidate();
        }
    }
    fn invalidate(&self, r: PixelRect) {
        for overlay in &self.overlays {
            if let Some(r) = r.intersection(overlay.bounds) {
                let rect = windows::Win32::Foundation::RECT {
                    left: r.left - overlay.bounds.left,
                    top: r.top - overlay.bounds.top,
                    right: r.right() - overlay.bounds.left,
                    bottom: r.bottom() - overlay.bounds.top,
                };
                // SAFETY: RECT 活到调用结束，user32 校验 HWND。
                let _ = unsafe { InvalidateRect(Some(overlay.hwnd.raw()), Some(&rect), false) };
            }
        }
    }
    fn paint(&self, hwnd: Hwnd, dirty: windows::Win32::Foundation::RECT) {
        let Some(overlay) = self.overlays.iter().find(|o| o.hwnd == hwnd) else {
            return;
        };
        let canvas = &overlay.canvas;
        let origin = overlay.bounds.origin();
        let s = overlay.scale.clamp(1.0, 3.0);
        let theme = theme();
        let frame = canvas.begin_draw();
        let _dirty = canvas.clip(Rect::new(
            dirty.left as f32,
            dirty.top as f32,
            (dirty.right - dirty.left) as f32,
            (dirty.bottom - dirty.top) as f32,
        ));
        overlay.scene.paint_background(canvas, origin);
        let selection = self.editor.selection.or(self.hover_selection);
        let full = Rect::new(
            0.0,
            0.0,
            overlay.bounds.width as f32,
            overlay.bounds.height as f32,
        );
        let dim = Rgba::rgb(0, 0, 0).alpha(0.48);
        if let Some(selection) = selection.and_then(|r| r.intersection(overlay.bounds)) {
            let r = rect_local(selection, origin);
            canvas.fill(Rect::new(0.0, 0.0, full.w, r.y), dim);
            canvas.fill(Rect::new(0.0, r.bottom(), full.w, full.h - r.bottom()), dim);
            canvas.fill(Rect::new(0.0, r.y, r.x, r.h), dim);
            canvas.fill(Rect::new(r.right(), r.y, full.w - r.right(), r.h), dim);
        } else {
            canvas.fill(full, dim);
        }
        if let Err(error) = overlay
            .scene
            .paint_annotations(canvas, &self.editor, origin)
        {
            tracing::warn!(%error,"截图标注绘制失败");
        }
        if let Some(Gesture::Draw(annotation)) = &self.gesture
            && let Some(selection) = self.editor.selection
        {
            let _selection = canvas.clip(rect_local(selection, origin));
            if let Err(error) = overlay.scene.paint_annotation(canvas, annotation, origin) {
                tracing::warn!(%error,"截图笔画绘制失败");
            }
            if annotation.tool == Tool::Mosaic {
                if let (Some(first), Some(last)) =
                    (annotation.points.first(), annotation.points.last())
                {
                    let range = rect_local(PixelRect::from_points(*first, *last), origin);
                    canvas.fill(range, theme.brand.alpha(0.08));
                    canvas.stroke_rect(range, 1.5 * s, theme.brand);
                }
            }
        }
        if let Some(selection) = selection {
            let r = rect_local(selection, origin);
            canvas.stroke_rect(r, 1.5, theme.brand);
            if self.editor.selection.is_some() {
                for point in handles(selection) {
                    canvas.fill_round(
                        Rect::new(
                            point.x - origin.x - 3.0 * s,
                            point.y - origin.y - 3.0 * s,
                            6.0 * s,
                            6.0 * s,
                        ),
                        1.5 * s,
                        Rgba::rgb(255, 255, 255),
                    );
                    canvas.stroke_rect(
                        Rect::new(
                            point.x - origin.x - 3.0 * s,
                            point.y - origin.y - 3.0 * s,
                            6.0 * s,
                            6.0 * s,
                        ),
                        1.0,
                        theme.brand,
                    );
                }
            }
            let x = (r.x + 2.0 * s).clamp(8.0 * s, (full.w - 135.0 * s).max(8.0 * s));
            let y = if r.y >= 34.0 * s {
                r.y - 30.0 * s
            } else {
                r.y + 10.0 * s
            };
            canvas.fill_round(
                Rect::new(x, y, 122.0 * s, 23.0 * s),
                5.0 * s,
                Rgba::rgb(20, 20, 20).alpha(0.82),
            );
            toolbar::label(
                canvas,
                &format!("{} × {}", selection.width, selection.height),
                x + 9.0 * s,
                y + 3.0 * s,
                12.0 * s,
                Rgba::rgb(255, 255, 255),
            );
        }
        if let Some(toolbar) = self.toolbar() {
            toolbar.paint(canvas, origin, &self.editor, self.hover_action, &theme);
        } else {
            canvas.fill_round(
                Rect::new(20.0 * s, 20.0 * s, 310.0 * s, 36.0 * s),
                9.0 * s,
                theme.elevated,
            );
            toolbar::label(
                canvas,
                "框选 · 单击窗口 · Space 全屏 · Esc 取消",
                32.0 * s,
                28.0 * s,
                12.0 * s,
                theme.text,
            );
        }
        if self.editor.tool == Tool::Mosaic
            && self.gesture.is_none()
            && self
                .editor
                .selection
                .is_some_and(|selection| selection.contains(self.cursor))
            && !self
                .toolbar()
                .is_some_and(|toolbar| toolbar.contains(self.cursor))
            && overlay.bounds.contains(self.cursor)
        {
            let x = (self.cursor.x - origin.x + 18.0 * s)
                .min(full.w - 150.0 * s)
                .max(8.0 * s);
            let y = (self.cursor.y - origin.y + 18.0 * s)
                .min(full.h - 40.0 * s)
                .max(8.0 * s);
            canvas.fill_round(
                Rect::new(x, y, 132.0 * s, 26.0 * s),
                6.0 * s,
                theme.elevated,
            );
            toolbar::label(
                canvas,
                "拖动框选马赛克区域",
                x + 8.0 * s,
                y + 4.0 * s,
                11.0 * s,
                theme.text_secondary,
            );
        }
        if self.editor.selection.is_none()
            || matches!(
                self.gesture,
                Some(Gesture::Select { .. } | Gesture::Resize { .. })
            )
        {
            if overlay.bounds.contains(self.cursor) {
                let x = (self.cursor.x - origin.x + 24.0 * s)
                    .min(full.w - 126.0 * s)
                    .max(8.0 * s);
                let y = (self.cursor.y - origin.y + 24.0 * s)
                    .min(full.h - 142.0 * s)
                    .max(8.0 * s);
                canvas.fill_round(
                    Rect::new(x - 3.0 * s, y - 3.0 * s, 116.0 * s, 134.0 * s),
                    7.0 * s,
                    theme.elevated,
                );
                overlay.scene.paint_magnifier(
                    canvas,
                    self.cursor,
                    Rect::new(x, y, 110.0 * s, 110.0 * s),
                );
                canvas.line(
                    (x + 55.0 * s, y),
                    (x + 55.0 * s, y + 110.0 * s),
                    1.0,
                    theme.brand,
                );
                canvas.line(
                    (x, y + 55.0 * s),
                    (x + 110.0 * s, y + 55.0 * s),
                    1.0,
                    theme.brand,
                );
                toolbar::label(
                    canvas,
                    &format!("{}, {}", self.cursor.x as i32, self.cursor.y as i32),
                    x + 5.0 * s,
                    y + 114.0 * s,
                    10.0 * s,
                    theme.text_secondary,
                );
            }
        }
        if let Some(error) = &self.error {
            toolbar::label(
                canvas,
                error,
                20.0 * s,
                full.h - 35.0 * s,
                13.0 * s,
                theme.danger,
            );
        }
        drop(_dirty);
        if let Err(error) = frame.finish() {
            tracing::warn!(%error,"截图帧绘制失败");
        }
    }
}

fn rect_local(r: PixelRect, origin: Point) -> Rect {
    Rect::new(
        r.left as f32 - origin.x,
        r.top as f32 - origin.y,
        r.width as f32,
        r.height as f32,
    )
}
fn handles(r: PixelRect) -> [Point; 8] {
    let (l, t, b, rr) = (
        r.left as f32,
        r.top as f32,
        r.bottom() as f32,
        r.right() as f32,
    );
    let (mx, my) = ((l + rr) / 2.0, (t + b) / 2.0);
    [
        Point { x: l, y: t },
        Point { x: mx, y: t },
        Point { x: rr, y: t },
        Point { x: rr, y: my },
        Point { x: rr, y: b },
        Point { x: mx, y: b },
        Point { x: l, y: b },
        Point { x: l, y: my },
    ]
}
fn padded(r: PixelRect, padding: i32) -> PixelRect {
    PixelRect {
        left: r.left - padding,
        top: r.top - padding,
        width: r.width.saturating_add(padding as u32 * 2),
        height: r.height.saturating_add(padding as u32 * 2),
    }
}
fn union(a: PixelRect, b: PixelRect) -> PixelRect {
    PixelRect::from_points(
        Point {
            x: a.left.min(b.left) as f32,
            y: a.top.min(b.top) as f32,
        },
        Point {
            x: a.right().max(b.right()) as f32,
            y: a.bottom().max(b.bottom()) as f32,
        },
    )
}

impl Session {
    fn pointer_down(&mut self, p: Point, hwnd: Hwnd) {
        self.commit_text();
        self.error = None;
        self.cursor = p;
        if let Some(toolbar) = self.toolbar()
            && toolbar.contains(p)
        {
            if let Some(action) = toolbar.hit(p) {
                self.action(action, hwnd);
            }
            self.invalidate_all();
            return;
        }
        if self.editor.selection.is_none() {
            self.editor.selection = Some(PixelRect::from_points(p, p));
            self.gesture = Some(Gesture::Select { start: p });
        } else if let Some(selection) = self.editor.selection {
            if self.editor.tool == Tool::Select {
                let edges = edge_hit(selection, p, 8.0 * self.ui_scale(p));
                if edges != 0 {
                    self.gesture = Some(Gesture::Resize {
                        original: selection,
                        edges,
                    });
                } else if selection.contains(p) {
                    self.gesture = Some(Gesture::Move {
                        start: p,
                        original: selection,
                    });
                } else {
                    self.gesture = Some(Gesture::Select { start: p });
                    self.editor.selection = Some(PixelRect::from_points(p, p));
                }
            } else if selection.contains(p) {
                let annotation =
                    Annotation::new(self.editor.tool, p, self.editor.color, self.editor.stroke);
                match self.editor.tool {
                    Tool::Text => {
                        self.start_text(annotation, hwnd);
                        return;
                    }
                    Tool::Number => {
                        if let Err(error) = self.editor.commit(annotation) {
                            self.error = Some(error);
                        }
                        self.invalidate_all();
                        return;
                    }
                    _ => self.gesture = Some(Gesture::Draw(annotation)),
                }
            }
        }
        if self.gesture.is_some() {
            hwnd.capture();
        }
        self.invalidate_all();
    }
    fn pointer_move(&mut self, p: Point) {
        let old_cursor = self.cursor;
        self.cursor = p;
        let Some(desktop) = self.desktop.as_ref() else {
            return;
        };
        let bounds = desktop.bounds;
        let p = Point {
            x: p.x.clamp(bounds.left as f32, bounds.right() as f32),
            y: p.y.clamp(bounds.top as f32, bounds.bottom() as f32),
        };
        let old_selection = self.editor.selection;
        match self.gesture.as_mut() {
            Some(Gesture::Select { start }) => {
                let p = if key_down(VK_SHIFT.0) {
                    let delta = (p.x - start.x).abs().min((p.y - start.y).abs());
                    Point {
                        x: start.x + delta * (p.x - start.x).signum(),
                        y: start.y + delta * (p.y - start.y).signum(),
                    }
                } else {
                    p
                };
                self.editor.selection = PixelRect::from_points(*start, p).intersection(bounds);
            }
            Some(Gesture::Move { start, original }) => {
                let left = (original.left as f32 + p.x - start.x).round() as i32;
                let top = (original.top as f32 + p.y - start.y).round() as i32;
                self.editor.selection = Some(PixelRect {
                    left: left.clamp(bounds.left, bounds.right() - original.width as i32),
                    top: top.clamp(bounds.top, bounds.bottom() - original.height as i32),
                    ..*original
                });
            }
            Some(Gesture::Resize { original, edges }) => {
                let mut from = original.origin();
                let mut to = Point {
                    x: original.right() as f32,
                    y: original.bottom() as f32,
                };
                if *edges & 1 != 0 {
                    from.x = p.x;
                }
                if *edges & 2 != 0 {
                    to.x = p.x;
                }
                if *edges & 4 != 0 {
                    from.y = p.y;
                }
                if *edges & 8 != 0 {
                    to.y = p.y;
                }
                self.editor.selection = PixelRect::from_points(from, to).intersection(bounds);
            }
            Some(Gesture::Draw(annotation)) => {
                let old = annotation_bounds(annotation);
                if annotation.tool == Tool::Pen {
                    annotation.push_point(p);
                } else if annotation.points.len() == 1 {
                    annotation.points.push(p);
                } else if let Some(last) = annotation.points.last_mut() {
                    *last = p;
                }
                let dirty = union(old, annotation_bounds(annotation));
                self.invalidate(padded(dirty, 24));
                return;
            }
            None => {
                if self.editor.selection.is_none() {
                    let hovered = self.window_under_cursor(p);
                    if hovered != self.hover_selection {
                        self.hover_selection = hovered;
                        self.invalidate_all();
                    } else {
                        self.invalidate(cursor_dirty(old_cursor, self.ui_scale(old_cursor)));
                        self.invalidate(cursor_dirty(p, self.ui_scale(p)));
                    }
                } else {
                    let hover = self.toolbar().and_then(|t| t.hit(p));
                    if self.editor.tool == Tool::Mosaic {
                        self.invalidate(cursor_dirty(old_cursor, self.ui_scale(old_cursor)));
                        self.invalidate(cursor_dirty(p, self.ui_scale(p)));
                    }
                    if hover != self.hover_action {
                        self.hover_action = hover;
                        if let Some(t) = self.toolbar() {
                            self.invalidate(padded(rect_pixels(t.bounds), 4));
                        }
                    }
                }
                return;
            }
        }
        if self.editor.selection != old_selection {
            self.invalidate_all();
        } else {
            self.invalidate(cursor_dirty(old_cursor, self.ui_scale(old_cursor)));
            self.invalidate(cursor_dirty(p, self.ui_scale(p)));
        }
    }
    fn pointer_up(&mut self, p: Point) {
        sys::release_capture();
        if let Some(gesture) = self.gesture.take() {
            match gesture {
                Gesture::Select { start } if start.distance_squared(p) < 9.0 => {
                    self.editor.selection = self.window_under_cursor(p)
                }
                Gesture::Draw(annotation) => {
                    if let Err(error) = self.editor.commit(annotation) {
                        self.error = Some(error);
                    }
                }
                _ => {}
            }
            if self
                .editor
                .selection
                .is_some_and(|r| r.width < 2 || r.height < 2)
            {
                self.editor.selection = None;
            }
            self.toolbar_monitor = self
                .overlays
                .iter()
                .position(|o| o.bounds.contains(p))
                .unwrap_or(self.toolbar_monitor);
            self.hover_action = None;
            self.invalidate_all();
        }
    }
    fn action(&mut self, action: Action, hwnd: Hwnd) {
        self.commit_text();
        match action {
            Action::Tool(tool) => self.editor.tool = tool,
            Action::Color(color) => self.editor.color = color,
            Action::Width(width) => self.editor.stroke = width,
            Action::Undo => {
                self.editor.undo();
            }
            Action::Redo => {
                self.editor.redo();
            }
            Action::FullScreen => {
                self.toolbar_monitor = self
                    .overlays
                    .iter()
                    .position(|o| o.bounds.contains(self.cursor))
                    .unwrap_or(self.toolbar_monitor);
                self.editor.selection = self.overlays.get(self.toolbar_monitor).map(|o| o.bounds);
            }
            Action::Long => {
                if !self.editor.annotations.is_empty() {
                    self.error = Some("请先撤销标注再开始长截图".into());
                } else if self
                    .editor
                    .selection
                    .is_some_and(|r| r.width >= 32 && r.height >= 96)
                {
                    post(hwnd, WM_FINISH, 3);
                } else {
                    self.error = Some("请框选至少 32 × 96 像素的滚动内容区域".into());
                }
            }
            Action::Copy => post(hwnd, WM_FINISH, 1),
            Action::Save => post(hwnd, WM_FINISH, 2),
            Action::Finish => post(hwnd, WM_FINISH, 0),
            Action::Cancel => hwnd.post(WM_CLOSE),
        }
    }
    fn key(&mut self, key: u16, hwnd: Hwnd) {
        let control = key_down(VK_CONTROL.0);
        if key == 27 {
            hwnd.post(WM_CLOSE);
            return;
        }
        if key == 13
            && self
                .editor
                .selection
                .is_some_and(|r| r.width >= 2 && r.height >= 2)
        {
            self.action(Action::Finish, hwnd);
            return;
        }
        if control {
            match key {
                90 => {
                    if key_down(VK_SHIFT.0) {
                        self.action(Action::Redo, hwnd)
                    } else {
                        self.action(Action::Undo, hwnd)
                    }
                }
                89 => self.action(Action::Redo, hwnd),
                67 => {
                    if self.editor.selection.is_some() {
                        self.action(Action::Copy, hwnd)
                    }
                }
                83 => {
                    if self.editor.selection.is_some() {
                        self.action(Action::Save, hwnd)
                    }
                }
                65 => {
                    self.editor.selection = self.desktop.as_ref().map(|d| d.bounds);
                }
                _ => {}
            }
        } else {
            match key {
                32 => self.action(Action::FullScreen, hwnd),
                86 => self.action(Action::Tool(Tool::Select), hwnd),
                82 => self.action(Action::Tool(Tool::Rectangle), hwnd),
                69 => self.action(Action::Tool(Tool::Ellipse), hwnd),
                65 => self.action(Action::Tool(Tool::Arrow), hwnd),
                80 => self.action(Action::Tool(Tool::Pen), hwnd),
                84 => self.action(Action::Tool(Tool::Text), hwnd),
                78 => self.action(Action::Tool(Tool::Number), hwnd),
                77 => self.action(Action::Tool(Tool::Mosaic), hwnd),
                76 => self.action(Action::Long, hwnd),
                46 => self.action(Action::Undo, hwnd),
                _ => {}
            }
        }
        self.invalidate_all();
    }
}
fn rect_pixels(r: Rect) -> PixelRect {
    PixelRect {
        left: r.x.floor() as i32,
        top: r.y.floor() as i32,
        width: r.w.ceil().max(0.0) as u32,
        height: r.h.ceil().max(0.0) as u32,
    }
}
fn cursor_dirty(p: Point, scale: f32) -> PixelRect {
    PixelRect {
        left: (p.x - 160.0 * scale).floor() as i32,
        top: (p.y - 170.0 * scale).floor() as i32,
        width: (340.0 * scale).ceil() as u32,
        height: (360.0 * scale).ceil() as u32,
    }
}
fn annotation_bounds(a: &Annotation) -> PixelRect {
    let Some(first) = a.points.first().copied() else {
        return PixelRect::default();
    };
    let (mut min, mut max) = (first, first);
    for p in &a.points {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    PixelRect::from_points(min, max)
}
fn edge_hit(r: PixelRect, p: Point, tolerance: f32) -> u8 {
    if !padded(r, tolerance as i32).contains(p) {
        return 0;
    }
    let mut edge = 0;
    if (p.x - r.left as f32).abs() <= tolerance {
        edge |= 1;
    } else if (p.x - r.right() as f32).abs() <= tolerance {
        edge |= 2;
    }
    if (p.y - r.top as f32).abs() <= tolerance {
        edge |= 4;
    } else if (p.y - r.bottom() as f32).abs() <= tolerance {
        edge |= 8;
    }
    edge
}
fn key_down(key: u16) -> bool {
    // SAFETY: GetKeyState 只读本线程的键盘状态。
    unsafe { GetKeyState(i32::from(key)) < 0 }
}
fn post(hwnd: Hwnd, message: u32, param: usize) {
    // SAFETY: 不传指针；参数在窗口过程里按数值解码。
    let _ = unsafe { PostMessageW(Some(hwnd.raw()), message, WPARAM(param), LPARAM(0)) };
}

impl Session {
    fn start_text(&mut self, annotation: Annotation, parent: Hwnd) {
        let Some(overlay) = self.overlays.iter().find(|o| o.hwnd == parent) else {
            return;
        };
        let Some(point) = annotation.points.first().copied() else {
            return;
        };
        let selection = self.editor.selection.unwrap_or(overlay.bounds);
        let width = (selection.right() as f32 - point.x).clamp(80.0, 420.0) as i32;
        let height = (selection.bottom() as f32 - point.y).clamp(45.0, 140.0) as i32;
        let font_size = (annotation.width * 3.0 + 14.0).clamp(16.0, 96.0) as i32;
        // SAFETY: EDIT 是系统窗口类，创建参数不含指针，字体为本线程持有。
        let result = unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WS_CHILD
                    | WS_VISIBLE
                    | WS_BORDER
                    | WINDOW_STYLE(
                        (ES_MULTILINE | ES_AUTOHSCROLL | ES_AUTOVSCROLL | ES_WANTRETURN) as u32,
                    ),
                (point.x - overlay.bounds.left as f32) as i32,
                (point.y - overlay.bounds.top as f32) as i32,
                width,
                height,
                Some(parent.raw()),
                None,
                None,
                None,
            );
            hwnd.map(|hwnd| {
                let font = CreateFontW(
                    -font_size,
                    0,
                    0,
                    0,
                    FW_NORMAL.0 as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    CLEARTYPE_QUALITY,
                    0,
                    w!("Microsoft YaHei UI"),
                );
                SendMessageW(
                    hwnd,
                    WM_SETFONT,
                    Some(WPARAM(font.0 as usize)),
                    Some(LPARAM(1)),
                );
                SendMessageW(
                    hwnd,
                    EM_LIMITTEXT,
                    Some(WPARAM(super::model::MAX_TEXT_CHARS)),
                    Some(LPARAM(0)),
                );
                let _ = SetWindowSubclass(hwnd, Some(edit_proc), 1, 0);
                let _ = SetFocus(Some(hwnd));
                TextInput {
                    hwnd,
                    annotation,
                    font,
                }
            })
        };
        match result {
            Ok(input) => self.text_input = Some(input),
            Err(error) => self.error = Some(format!("创建文字输入框失败：{error}")),
        }
    }
    fn commit_text(&mut self) {
        let Some(mut input) = self.text_input.take() else {
            return;
        };
        let mut text = vec![0u16; super::model::MAX_TEXT_CHARS * 2 + 1];
        // SAFETY: buffer 是有界的活切片；系统最多写入切片长度，返回实际 UTF-16 数量。
        let len = unsafe { GetWindowTextW(input.hwnd, &mut text) }.max(0) as usize;
        input.annotation.text = String::from_utf16_lossy(&text[..len.min(text.len())]);
        if !input.annotation.text.trim().is_empty() {
            if let Err(error) = self.editor.commit(input.annotation.clone()) {
                self.error = Some(error);
            }
        }
        let foreground = self.overlays.get(self.toolbar_monitor).map(|o| o.hwnd);
        drop(input);
        if let Some(hwnd) = foreground {
            // SAFETY: HWND 属于当前 UI 线程；销毁输入框后把键盘焦点还给原生画布。
            let _ = unsafe { SetFocus(Some(hwnd.raw())) };
        }
    }
}

unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    // SAFETY: 系统从 EDIT 的子类链调用；不解引用 lparam，也不让 Rust panic 穿过 FFI。
    unsafe {
        if msg == WM_IME_STARTCOMPOSITION {
            IME_COMPOSING.with(|s| s.set(true));
        }
        if msg == WM_IME_ENDCOMPOSITION {
            IME_COMPOSING.with(|s| s.set(false));
        }
        if !IME_COMPOSING.with(Cell::get)
            && (msg == WM_KEYDOWN || msg == WM_CHAR)
            && (wp.0 == 27 || (wp.0 == 13 && !key_down(VK_SHIFT.0)))
        {
            if msg == WM_KEYDOWN
                && let Ok(parent) = GetParent(hwnd)
            {
                let _ = PostMessageW(
                    Some(parent),
                    if wp.0 == 27 {
                        WM_TEXT_CANCEL
                    } else {
                        WM_TEXT_COMMIT
                    },
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            return LRESULT(0);
        }
        if msg == WM_NCDESTROY {
            IME_COMPOSING.with(|s| s.set(false));
            let _ = RemoveWindowSubclass(hwnd, Some(edit_proc), 1);
        }
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}

fn check_focus() {
    let Some(session) = active() else {
        return;
    };
    let Ok(session) = session.try_borrow() else {
        return;
    };
    // SAFETY: 只取系统当前前台句柄，不访问外部内存。
    let foreground = unsafe { GetForegroundWindow() };
    let belongs = session.overlays.iter().any(|o| o.hwnd.raw() == foreground)
        || session
            .text_input
            .as_ref()
            .is_some_and(|i| i.hwnd == foreground);
    drop(session);
    if !belongs {
        finish(None);
    }
}

fn set_cursor() {
    let cursor = active()
        .and_then(|s| {
            s.try_borrow().ok().map(|s| {
                if s.toolbar().is_some_and(|t| t.contains(s.cursor)) {
                    IDC_HAND
                } else if s.editor.tool == Tool::Text {
                    IDC_IBEAM
                } else if s.editor.tool != Tool::Select {
                    IDC_CROSS
                } else if let Some(r) = s.editor.selection {
                    match edge_hit(r, s.cursor, 8.0 * s.ui_scale(s.cursor)) {
                        1 | 2 => IDC_SIZEWE,
                        4 | 8 => IDC_SIZENS,
                        5 | 10 => IDC_SIZENWSE,
                        6 | 9 => IDC_SIZENESW,
                        _ => {
                            if r.contains(s.cursor) {
                                IDC_SIZEALL
                            } else {
                                IDC_CROSS
                            }
                        }
                    }
                } else {
                    IDC_CROSS
                }
            })
        })
        .unwrap_or(IDC_CROSS);
    // SAFETY: 只加载系统内置光标，不需要释放。
    unsafe {
        SetCursor(LoadCursorW(None, cursor).ok());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_edges_and_dirty_regions_use_negative_physical_coordinates() {
        let r = PixelRect {
            left: -500,
            top: -200,
            width: 300,
            height: 150,
        };
        assert_eq!(
            edge_hit(
                r,
                Point {
                    x: -500.0,
                    y: -200.0
                },
                8.0
            ),
            5
        );
        assert_eq!(
            edge_hit(
                r,
                Point {
                    x: -200.0,
                    y: -50.0
                },
                8.0
            ),
            10
        );
        assert_eq!(
            edge_hit(
                r,
                Point {
                    x: -350.0,
                    y: -120.0
                },
                8.0
            ),
            0
        );
        assert_eq!(
            union(
                r,
                PixelRect {
                    left: -100,
                    top: -100,
                    width: 20,
                    height: 20
                }
            ),
            PixelRect {
                left: -500,
                top: -200,
                width: 420,
                height: 150
            }
        );
    }
    #[test]
    fn optional_native_toolbar_preview() {
        let _resources = super::super::render::SharedScope;
        let Some(directory) = std::env::var_os("NCD_SCREENSHOT_SHOTS_DIR") else {
            return;
        };
        let bounds = PixelRect {
            left: -200,
            top: -100,
            width: 1200,
            height: 780,
        };
        let mut bgra = Vec::with_capacity(1200 * 780 * 4);
        for y in 0..780 {
            for x in 0..1200 {
                let c = if (120..1030).contains(&x) && (110..530).contains(&y) {
                    [255, 255, 255, 255]
                } else if x < 95 {
                    [242, 231, 235, 255]
                } else {
                    [248, 245, 246, 255]
                };
                bgra.extend_from_slice(&c);
            }
        }
        let desktop = Desktop {
            monitors: vec![super::super::capture::MonitorImage {
                bounds,
                scale: 1.0,
                bgra,
            }],
            windows: Vec::new(),
            bounds,
        };
        let rgba = super::super::capture::render_offscreen(bounds.width, bounds.height, |target| {
            let resources = shared().map_err(|e| e.to_string())?;
            let canvas = Canvas::new(target, resources, 1.0).map_err(|e| e.to_string())?;
            let scene = Scene::new(&canvas, &desktop, bounds)?;
            let hwnd = Hwnd::from_raw(HWND::default());
            let mut editor = Editor::new();
            editor.selection = Some(PixelRect {
                left: -80,
                top: 10,
                width: 910,
                height: 420,
            });
            editor.tool = Tool::Arrow;
            let mut annotation =
                Annotation::new(Tool::Arrow, Point { x: 720.0, y: 90.0 }, editor.color, 4.0);
            annotation.points.push(Point { x: 580.0, y: 205.0 });
            editor.commit(annotation)?;
            let session = Session {
                owner: "preview".into(),
                desktop: None,
                editor,
                overlays: vec![Overlay {
                    hwnd,
                    bounds,
                    scale: 1.0,
                    canvas,
                    scene,
                }],
                gesture: None,
                cursor: Point { x: 720.0, y: 90.0 },
                hover_selection: None,
                hover_action: Some(Action::Tool(Tool::Arrow)),
                toolbar_monitor: 0,
                text_input: None,
                sender: None,
                error: None,
            };
            session.paint(
                hwnd,
                windows::Win32::Foundation::RECT {
                    left: 0,
                    top: 0,
                    right: 1200,
                    bottom: 780,
                },
            );
            Ok(())
        })
        .unwrap();
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        image::save_buffer(
            directory.join("screenshot-toolbar-native.png"),
            &rgba,
            bounds.width,
            bounds.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }

    #[test]
    fn scaled_cursor_damage_covers_magnifier_and_resize_handles() {
        let cursor = Point {
            x: -210.0,
            y: -90.0,
        };
        let damage = cursor_dirty(cursor, 2.0);
        assert!(damage.contains(Point {
            x: cursor.x - 260.0,
            y: cursor.y - 288.0
        }));
        assert!(damage.contains(Point {
            x: cursor.x + 280.0,
            y: cursor.y + 316.0
        }));
        let selection = PixelRect {
            left: -200,
            top: -80,
            width: 320,
            height: 200,
        };
        assert_eq!(edge_hit(selection, cursor, 8.0), 0);
        assert_eq!(edge_hit(selection, cursor, 16.0), 5);
    }
}
