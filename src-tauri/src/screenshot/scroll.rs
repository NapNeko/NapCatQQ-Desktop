// 手动滚动采集；临时输入钩子只发通知，截图与匹配留在工作线程。
#![expect(unsafe_code, reason = "滚轮通知、原生控制条和边框 FFI")]
#![warn(clippy::undocumented_unsafe_blocks)]
use super::{
    capture::{self, Desktop, MonitorImage},
    model::{Editor, PixelRect, Point},
    scroll_panel::{ControlLayout, ControlView, DETAIL_SIZE},
    stitch::{Append, Frame, Stitcher},
    window::{Output, OutputAction},
};
use crate::native_panel::{
    gfx::{Canvas, release_shared, shared},
    sys::{self, Hwnd, Message, WndHandler},
    theme::theme,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{
    Notify,
    mpsc::{Sender, channel},
};
use tokio_util::sync::CancellationToken;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::{PCWSTR, w},
};

const CLASS: PCWSTR = w!("NCD.ScrollScreenshot.v1");
const WM_ACTION: u32 = WM_APP + 42;
const SAMPLE_PERIOD: Duration = Duration::from_millis(60);
const IDLE_SAMPLE_PERIOD: Duration = Duration::from_millis(240);
const ACTIVITY_TAIL: Duration = Duration::from_millis(600);
#[derive(Clone, Copy)]
enum Command {
    Pause(bool),
    Undo,
    Finish(OutputAction),
}
struct Control {
    hwnd: Hwnd,
    canvas: Canvas,
    layout: ControlLayout,
    source: SourceWindow,
    borders: Vec<Hwnd>,
    mouse: HHOOK,
    keyboard: HHOOK,
    events: Sender<Command>,
    activity: Arc<Activity>,
    cancel: CancellationToken,
    paused: bool,
    dragging: bool,
    finishing: bool,
    hover: Option<usize>,
    can_undo: bool,
    status: String,
    height: u32,
    parts: usize,
    preview: Option<windows::Win32::Graphics::Direct2D::ID2D1Bitmap>,
    preview_size: (u32, u32),
    detail: Option<windows::Win32::Graphics::Direct2D::ID2D1Bitmap>,
    detail_size: (u32, u32),
    detail_range: (u32, u32),
}

struct Activity {
    epoch: Instant,
    latest: AtomicU64,
    wake: Notify,
}
impl Activity {
    fn new() -> Self {
        Self {
            epoch: Instant::now(),
            latest: AtomicU64::new(0),
            wake: Notify::new(),
        }
    }
    fn touch(&self) {
        self.latest.store(
            self.epoch.elapsed().as_millis() as u64 + 1,
            Ordering::Relaxed,
        );
        self.wake.notify_one();
    }
    fn active(&self) -> bool {
        let last = self.latest.load(Ordering::Relaxed);
        last != 0
            && self.epoch.elapsed().as_millis() as u64 + 1 - last
                <= ACTIVITY_TAIL.as_millis() as u64
    }
    fn stop(&self) {
        self.latest.store(0, Ordering::Relaxed);
    }
}
struct Snapshot {
    preview: Frame,
    detail: Frame,
    detail_range: (u32, u32),
    height: u32,
    parts: usize,
    can_undo: bool,
}
impl Snapshot {
    fn new(stitcher: &Stitcher) -> Self {
        let (detail, detail_range) = stitcher.detail_preview(DETAIL_SIZE.0, DETAIL_SIZE.1);
        Self {
            preview: stitcher.thumbnail(),
            detail,
            detail_range,
            height: stitcher.height(),
            parts: stitcher.parts(),
            can_undo: stitcher.can_undo(),
        }
    }
}
enum WorkerCommand {
    Sample,
    Undo,
    Finish {
        action: OutputAction,
        capture_final: bool,
    },
}
enum WorkerReply {
    Sample {
        step: Result<Append, String>,
        snapshot: Snapshot,
    },
    Undo {
        changed: bool,
        snapshot: Snapshot,
    },
    FinishRejected {
        reason: String,
        snapshot: Snapshot,
    },
    Finished {
        frame: Frame,
        action: OutputAction,
    },
}
thread_local! {
    static CONTROL:RefCell<Option<Rc<RefCell<Control>>>>=const{RefCell::new(None)};
    static CONTROL_HWND:Cell<isize>=const{Cell::new(0)};
}

pub async fn run(
    app: &tauri::AppHandle,
    output: Output,
    cancel: CancellationToken,
) -> Result<Option<Output>, String> {
    let roi = output.editor.selection.ok_or("请先框选滚动内容")?;
    let center = Point {
        x: roi.left as f32 + roi.width as f32 / 2.0,
        y: roi.top as f32 + roi.height as f32 / 2.0,
    };
    let monitor = output
        .desktop
        .monitors
        .iter()
        .find(|m| m.bounds.contains(center))
        .or_else(|| output.desktop.monitors.first())
        .ok_or("未找到滚动选区所在显示器")?;
    let monitor_bounds = monitor.bounds;
    let scale = monitor.scale;
    drop(output);
    let (first, source) = tauri::async_runtime::spawn_blocking(move || {
        Ok::<_, String>((capture_frame(roi)?, source_window(roi)?))
    })
    .await
    .map_err(|e| e.to_string())??;
    let stitcher = Stitcher::new(first)?;
    let initial = Snapshot::new(&stitcher);
    let activity = Arc::new(Activity::new());
    let layout = ControlLayout::new(roi, monitor_bounds, scale);
    let must_hide = layout.overlaps_roi;
    let (events, mut receiver) = channel(16);
    let (reply, ready) = tokio::sync::oneshot::channel();
    let ui_activity = Arc::clone(&activity);
    let ui_cancel = cancel.clone();
    app.run_on_main_thread(move || {
        let _ = reply.send(open(
            roi,
            layout,
            source,
            events,
            ui_activity,
            ui_cancel,
            initial,
        ));
    })
    .map_err(|e| e.to_string())?;
    ready.await.map_err(|e| e.to_string())??;
    let (jobs, job_receiver) = channel(1);
    let (replies, mut results) = channel(1);
    let stopped = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stopped);
    let worker_app = app.clone();
    let worker_activity = Arc::clone(&activity);
    let worker = tauri::async_runtime::spawn_blocking(move || {
        worker_loop(
            &worker_app,
            roi,
            source,
            must_hide,
            stitcher,
            job_receiver,
            replies,
            &worker_stop,
            &worker_activity,
        );
    });
    let mut paused = false;
    let mut busy = false;
    let mut pending = None;
    let mut last_sample = Instant::now();
    let mut ticker =
        tokio::time::interval_at(tokio::time::Instant::now() + SAMPLE_PERIOD, SAMPLE_PERIOD);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = async {
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(None),
                command = receiver.recv() => match command.ok_or("滚动采集窗口已关闭")? {
                    Command::Pause(value) => {
                        paused = value;
                        if paused { activity.stop(); } else { activity.touch(); }
                        set_status(app, paused, false, if paused { "已暂停" } else { "在框选区域内上下滚动" });
                    }
                    Command::Undo => {
                        paused = true;
                        activity.stop();
                        pending = Some(WorkerCommand::Undo);
                        set_status(app, true, false, "正在撤销最后一段");
                    }
                    Command::Finish(action) => {
                        pending = Some(WorkerCommand::Finish { action, capture_final: !paused });
                        activity.stop();
                        set_status(app, paused, true, "正在完成");
                    }
                },
                answer = results.recv(), if busy => {
                    busy = false;
                    match answer.ok_or("滚动采集线程已结束")? {
                        WorkerReply::Sample { step, snapshot } => {
                            let status = match step {
                                Ok(Append::Added(_)) => {
                                    if !paused && pending.is_none() { activity.touch(); }
                                    "在框选区域内上下滚动"
                                }
                                Ok(Append::Unchanged) => "在框选区域内上下滚动",
                                Ok(Append::Covered) => {
                                    if !paused && pending.is_none() { activity.touch(); }
                                    "已采集此区域，可继续上下滚动"
                                }
                                Ok(Append::NoOverlap | Append::Ambiguous) => "重叠不足，请往回滚一点",
                                Ok(Append::Limit) => { paused = true; activity.stop(); "已到长度上限，请完成" }
                                Err(error) => {
                                    paused = true;
                                    activity.stop();
                                    update(app, snapshot, true, pending.is_some(), &error).await?;
                                    if let Some(command) = pending.take() {
                                        jobs.try_send(command).map_err(|e| e.to_string())?;
                                        busy = true;
                                    }
                                    continue;
                                }
                            };
                            update(app, snapshot, paused, pending.is_some(), status).await?;
                        }
                        WorkerReply::Undo { changed, snapshot } => {
                            update(app, snapshot, true, false, if changed { "已撤销，滚回上一屏后继续" } else { "没有可撤销的内容" }).await?;
                        }
                        WorkerReply::FinishRejected { reason, snapshot } => {
                            paused = true;
                            activity.stop();
                            update(app, snapshot, true, false, &reason).await?;
                        }
                        WorkerReply::Finished { frame, action } => {
                            let bounds = PixelRect { height: frame.height, ..roi };
                            let mut editor = Editor::new();
                            editor.selection = Some(bounds);
                            return Ok(Some(Output { desktop: Desktop { monitors: vec![MonitorImage { bounds, scale, bgra: frame.bgra }], windows: Vec::new(), bounds }, editor, action }));
                        }
                    }
                },
                _ = activity.wake.notified() => {},
                _ = ticker.tick(), if !paused && pending.is_none() => {
                    // 原生手势和页面自己的滚动未必经过输入钩子，空闲时仍低频检查。
                    if !busy && (activity.active() || last_sample.elapsed() >= IDLE_SAMPLE_PERIOD) {
                        jobs.try_send(WorkerCommand::Sample).map_err(|e| e.to_string())?;
                        last_sample = Instant::now();
                        busy = true;
                    }
                },
            }
            if !busy && let Some(command) = pending.take() {
                jobs.try_send(command).map_err(|e| e.to_string())?;
                busy = true;
            }
        }
    }.await;
    stopped.store(true, Ordering::Relaxed);
    drop(jobs);
    drop(results);
    let (sender, closed) = tokio::sync::oneshot::channel();
    let _ = app.run_on_main_thread(move || {
        close();
        let _ = sender.send(());
    });
    let _ = closed.await;
    drop(worker);
    result
}
fn capture_frame(roi: PixelRect) -> Result<Frame, String> {
    let pixels = capture::capture_region(roi)?;
    Ok(Frame {
        width: roi.width,
        height: roi.height,
        bgra: pixels.bgra,
    })
}
fn worker_loop(
    app: &tauri::AppHandle,
    roi: PixelRect,
    source: SourceWindow,
    hide: bool,
    mut stitcher: Stitcher,
    mut jobs: tokio::sync::mpsc::Receiver<WorkerCommand>,
    replies: Sender<WorkerReply>,
    stopped: &AtomicBool,
    activity: &Arc<Activity>,
) {
    while let Some(command) = jobs.blocking_recv() {
        if stopped.load(Ordering::Relaxed) {
            return;
        }
        let reply = match command {
            WorkerCommand::Sample => {
                let step = sample_frame(app, roi, source, hide, activity)
                    .and_then(|frame| stitcher.append(frame));
                WorkerReply::Sample {
                    step,
                    snapshot: Snapshot::new(&stitcher),
                }
            }
            WorkerCommand::Undo => {
                let changed = stitcher.undo();
                WorkerReply::Undo {
                    changed,
                    snapshot: Snapshot::new(&stitcher),
                }
            }
            WorkerCommand::Finish {
                action,
                capture_final,
            } => {
                if capture_final {
                    std::thread::sleep(SAMPLE_PERIOD);
                    let final_step = sample_frame(app, roi, source, hide, activity)
                        .and_then(|frame| stitcher.append(frame));
                    let reason = match final_step {
                        Ok(Append::Added(_) | Append::Unchanged | Append::Covered) => None,
                        Ok(Append::NoOverlap | Append::Ambiguous) => {
                            Some("最后一屏未能拼接，回滚重试；再次完成保留当前结果".to_owned())
                        }
                        Ok(Append::Limit) => Some("已到长度上限，再次完成保留当前结果".to_owned()),
                        Err(error) => Some(format!("{error}；再次完成保留当前结果")),
                    };
                    if let Some(reason) = reason {
                        if replies
                            .blocking_send(WorkerReply::FinishRejected {
                                reason,
                                snapshot: Snapshot::new(&stitcher),
                            })
                            .is_err()
                        {
                            return;
                        }
                        continue;
                    }
                }
                if !stopped.load(Ordering::Relaxed) {
                    let _ = replies.blocking_send(WorkerReply::Finished {
                        frame: stitcher.finish(),
                        action,
                    });
                }
                return;
            }
        };
        if stopped.load(Ordering::Relaxed) || replies.blocking_send(reply).is_err() {
            return;
        }
    }
}
fn sample_frame(
    app: &tauri::AppHandle,
    roi: PixelRect,
    source: SourceWindow,
    hide: bool,
    activity: &Arc<Activity>,
) -> Result<Frame, String> {
    if hide {
        visible(app, false, activity)?;
    }
    let result = validate_source(roi, source).and_then(|_| capture_frame(roi));
    if hide {
        visible(app, true, activity)?;
    }
    result
}
#[derive(Clone, Copy)]
struct SourceWindow {
    handle: isize,
    bounds: (i32, i32, i32, i32),
}
fn source_window(roi: PixelRect) -> Result<SourceWindow, String> {
    // SAFETY: 按选区中心取原应用窗口，只读取系统句柄和范围。
    let hwnd = unsafe {
        GetAncestor(
            WindowFromPoint(POINT {
                x: roi.left + roi.width as i32 / 2,
                y: roi.top + roi.height as i32 / 2,
            }),
            GA_ROOT,
        )
    };
    let mut rect = windows::Win32::Foundation::RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|e| e.to_string())?;
    Ok(SourceWindow {
        handle: hwnd.0 as isize,
        bounds: (rect.left, rect.top, rect.right, rect.bottom),
    })
}
fn validate_source(roi: PixelRect, source: SourceWindow) -> Result<(), String> {
    let mut rect = windows::Win32::Foundation::RECT::default();
    // SAFETY: 句柄由系统校验；仅比较原窗口的位置和尺寸，不控制其他应用。
    unsafe { GetWindowRect(HWND(source.handle as *mut _), &mut rect) }
        .map_err(|_| "原窗口已关闭，请完成当前结果或重新框选".to_string())?;
    if (rect.left, rect.top, rect.right, rect.bottom) != source.bounds {
        return Err("滚动区域已移动，请完成当前结果或重新框选".into());
    }
    // SAFETY: 只检查选区中心的可见顶层窗口，控制卡在采集前已隐藏。
    let visible = unsafe {
        GetAncestor(
            WindowFromPoint(POINT {
                x: roi.left + roi.width as i32 / 2,
                y: roi.top + roi.height as i32 / 2,
            }),
            GA_ROOT,
        )
    };
    if visible.0 as isize != source.handle {
        return Err("滚动区域已被其他窗口遮挡，请移开后继续".into());
    }
    Ok(())
}
fn visible(app: &tauri::AppHandle, show: bool, activity: &Arc<Activity>) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let activity = Arc::clone(activity);
    app.run_on_main_thread(move || {
        CONTROL.with(|s| {
            if let Some(control) = s.borrow().as_ref() {
                let c = control.borrow();
                if !Arc::ptr_eq(&c.activity, &activity) {
                    return;
                }
                if show {
                    c.hwnd.show_no_activate();
                } else {
                    c.hwnd.hide();
                }
            }
        });
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(1))
        .map_err(|e| e.to_string())
}
async fn update(
    app: &tauri::AppHandle,
    snapshot: Snapshot,
    paused: bool,
    finishing: bool,
    status: &str,
) -> Result<(), String> {
    let status = status.to_owned();
    let (sent, received) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        CONTROL.with(|s| {
            if let Some(c) = s.borrow().as_ref() {
                let mut c = c.borrow_mut();
                c.paused = paused;
                c.finishing = finishing;
                c.height = snapshot.height;
                c.parts = snapshot.parts;
                c.can_undo = snapshot.can_undo;
                c.status = status;
                c.preview_size = (snapshot.preview.width, snapshot.preview.height);
                c.preview = c
                    .canvas
                    .create_bgra_bitmap(
                        snapshot.preview.width,
                        snapshot.preview.height,
                        &snapshot.preview.bgra,
                    )
                    .ok();
                c.detail_size = (snapshot.detail.width, snapshot.detail.height);
                c.detail_range = snapshot.detail_range;
                c.detail = c
                    .canvas
                    .create_bgra_bitmap(
                        snapshot.detail.width,
                        snapshot.detail.height,
                        &snapshot.detail.bgra,
                    )
                    .ok();
                c.hwnd.invalidate();
            }
        });
        let _ = sent.send(());
    })
    .map_err(|e| e.to_string())?;
    // UI 消费后再采下一帧，避免大预览在主线程消息队列里积压。
    received.await.map_err(|e| e.to_string())
}
fn set_status(app: &tauri::AppHandle, paused: bool, finishing: bool, status: &str) {
    let status = status.to_owned();
    let _ = app.run_on_main_thread(move || {
        CONTROL.with(|s| {
            if let Some(c) = s.borrow().as_ref() {
                let mut c = c.borrow_mut();
                c.paused = paused;
                c.finishing = finishing;
                c.status = status;
                c.hwnd.invalidate();
            }
        })
    });
}

fn open(
    roi: PixelRect,
    layout: ControlLayout,
    source: SourceWindow,
    events: Sender<Command>,
    activity: Arc<Activity>,
    cancel: CancellationToken,
    initial: Snapshot,
) -> Result<(), String> {
    if CONTROL.with(|s| s.borrow().is_some()) {
        return Err("滚动采集已经打开".into());
    }
    sys::register_window_class::<Handler>(CLASS);
    sys::register_window_class_with_style::<BorderHandler>(
        w!("NCD.ScrollScreenshotBorder.v1"),
        CS_DBLCLKS,
    );
    let position = layout.position;
    let make = |class: PCWSTR, r: PixelRect, ex| {
        // SAFETY: 静态类名、已校验物理像素范围，不传外部创建指针。
        unsafe {
            CreateWindowExW(
                ex,
                class,
                w!("滚动长截图"),
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
        .map(Hwnd::from_raw)
        .map_err(|e| e.to_string())
    };
    let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
    let hwnd = make(CLASS, position, ex)?;
    hwnd.set_corner_round();
    let mut created = CreatedWindows(vec![hwnd]);
    let resources = shared().map_err(|e| e.to_string())?;
    let target =
        sys::hwnd_render_target(&resources.d2d, hwnd, position.width, position.height, 1.0)
            .map_err(|e| e.to_string())?;
    let canvas = Canvas::new(target, resources, 1.0).map_err(|e| e.to_string())?;
    let thumbnail = canvas
        .create_bgra_bitmap(
            initial.preview.width,
            initial.preview.height,
            &initial.preview.bgra,
        )
        .ok();
    let detail = canvas
        .create_bgra_bitmap(
            initial.detail.width,
            initial.detail.height,
            &initial.detail.bgra,
        )
        .ok();
    let mut borders = Vec::new();
    for rect in [
        PixelRect {
            left: roi.left - 2,
            top: roi.top - 2,
            width: roi.width + 4,
            height: 2,
        },
        PixelRect {
            left: roi.left - 2,
            top: roi.bottom(),
            width: roi.width + 4,
            height: 2,
        },
        PixelRect {
            left: roi.left - 2,
            top: roi.top,
            width: 2,
            height: roi.height,
        },
        PixelRect {
            left: roi.right(),
            top: roi.top,
            width: 2,
            height: roi.height,
        },
    ] {
        match make(
            w!("NCD.ScrollScreenshotBorder.v1"),
            rect,
            ex | WS_EX_TRANSPARENT,
        ) {
            Ok(border) => {
                created.0.push(border);
                borders.push(border);
            }
            Err(error) => return Err(error),
        }
    }
    // SAFETY: 低级钩子只回到当前 UI 消息线程，exe 模块在进程整个生命周期内存在。
    let module = unsafe { GetModuleHandleW(None) }.map_err(|e| e.to_string())?;
    let mouse = match unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_hook),
            Some(windows::Win32::Foundation::HINSTANCE(module.0)),
            0,
        )
    } {
        Ok(hook) => hook,
        Err(error) => return Err(error.to_string()),
    };
    let keyboard = match unsafe {
        SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            Some(windows::Win32::Foundation::HINSTANCE(module.0)),
            0,
        )
    } {
        Ok(hook) => hook,
        Err(error) => {
            let _ = unsafe { UnhookWindowsHookEx(mouse) };
            return Err(error.to_string());
        }
    };
    CONTROL_HWND.with(|s| s.set(hwnd.key()));
    CONTROL.with(|s| {
        *s.borrow_mut() = Some(Rc::new(RefCell::new(Control {
            hwnd,
            canvas,
            layout,
            source,
            borders,
            mouse,
            keyboard,
            events,
            activity,
            cancel,
            paused: false,
            dragging: false,
            finishing: false,
            hover: None,
            can_undo: initial.can_undo,
            status: "上下滚动 · Enter 完成".into(),
            height: roi.height,
            parts: 1,
            preview: thumbnail,
            preview_size: (initial.preview.width, initial.preview.height),
            detail,
            detail_size: (initial.detail.width, initial.detail.height),
            detail_range: initial.detail_range,
        })))
    });
    created.0.clear();
    CONTROL.with(|s| {
        if let Some(c) = s.borrow().as_ref() {
            let c = c.borrow();
            for border in &c.borders {
                border.show_no_activate();
                border.invalidate();
            }
        }
    });
    hwnd.show_no_activate();
    hwnd.invalidate();
    Ok(())
}
struct CreatedWindows(Vec<Hwnd>);
impl Drop for CreatedWindows {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            for hwnd in &self.0 {
                hwnd.destroy();
            }
            release_shared();
        }
    }
}
fn close() {
    CONTROL_HWND.with(|s| s.set(0));
    let control = CONTROL.with(|s| s.borrow_mut().take());
    if let Some(control) = control {
        let c = control.borrow();
        // SAFETY: 这些钩子仅由本会话在当前线程安装，先卸载再销毁其消息窗口。
        unsafe {
            let _ = UnhookWindowsHookEx(c.mouse);
            let _ = UnhookWindowsHookEx(c.keyboard);
        }
        for border in &c.borders {
            border.destroy();
        }
        c.hwnd.destroy();
        drop(c);
        drop(control);
    }
    release_shared();
}
struct Handler;
impl WndHandler for Handler {
    fn handle(msg: &Message) -> Option<LRESULT> {
        if msg.id() == WM_ERASEBKGND {
            return Some(LRESULT(1));
        }
        let control = CONTROL.with(|s| s.borrow().clone())?;
        match msg.id() {
            WM_PAINT => {
                let _paint = msg.hwnd().begin_paint();
                if let Ok(c) = control.try_borrow() {
                    paint(&c);
                }
                Some(LRESULT(0))
            }
            WM_ACTION => {
                if let Ok(mut c) = control.try_borrow_mut() {
                    if c.finishing && msg.wparam() != 6 {
                        return Some(LRESULT(0));
                    }
                    let previous_paused = c.paused;
                    let command = match msg.wparam() {
                        1 => {
                            c.paused = !c.paused;
                            Command::Pause(c.paused)
                        }
                        2 => {
                            if !c.can_undo {
                                return Some(LRESULT(0));
                            }
                            c.paused = true;
                            Command::Undo
                        }
                        3 => {
                            c.finishing = true;
                            Command::Finish(OutputAction::Copy)
                        }
                        4 => {
                            c.finishing = true;
                            Command::Finish(OutputAction::Save)
                        }
                        5 => {
                            c.finishing = true;
                            Command::Finish(OutputAction::Attach)
                        }
                        _ => {
                            c.cancel.cancel();
                            return Some(LRESULT(0));
                        }
                    };
                    if c.events.try_send(command).is_err() {
                        c.paused = previous_paused;
                        c.finishing = false;
                    }
                    c.hwnd.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                if let Some((x, y)) = sys::cursor_pos()
                    && let Ok(c) = control.try_borrow()
                {
                    let point = Point {
                        x: (x - c.layout.position.left) as f32,
                        y: (y - c.layout.position.top) as f32,
                    };
                    if let Some(action) = c.layout.hit(point, c.can_undo) {
                        post(msg.hwnd(), WM_ACTION, action);
                    }
                }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                msg.hwnd().track_mouse_leave();
                if let Some((x, y)) = sys::cursor_pos()
                    && let Ok(mut c) = control.try_borrow_mut()
                {
                    let hover = c.layout.hit(
                        Point {
                            x: (x - c.layout.position.left) as f32,
                            y: (y - c.layout.position.top) as f32,
                        },
                        true,
                    );
                    if c.hover != hover {
                        c.hover = hover;
                        c.hwnd.invalidate();
                    }
                    if hover.is_some() {
                        sys::set_hand_cursor();
                    }
                }
                Some(LRESULT(0))
            }
            windows::Win32::UI::Controls::WM_MOUSELEAVE => {
                if let Ok(mut c) = control.try_borrow_mut() {
                    c.hover = None;
                    c.hwnd.invalidate();
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                if let Ok(c) = control.try_borrow() {
                    c.cancel.cancel();
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}
struct BorderHandler;
impl WndHandler for BorderHandler {
    fn handle(msg: &Message) -> Option<LRESULT> {
        match msg.id() {
            WM_PAINT => {
                let paint = msg.hwnd().begin_paint();
                let color = theme().brand;
                let rgb = (color.r * 255.0) as u32
                    | ((color.g * 255.0) as u32) << 8
                    | ((color.b * 255.0) as u32) << 16;
                // SAFETY: 画刷仅用于此帧的有效 HDC，填充结束后释放。
                unsafe {
                    let brush = windows::Win32::Graphics::Gdi::CreateSolidBrush(
                        windows::Win32::Foundation::COLORREF(rgb),
                    );
                    windows::Win32::Graphics::Gdi::FillRect(paint.dc(), &paint.rect(), brush);
                    let _ = windows::Win32::Graphics::Gdi::DeleteObject(brush.into());
                }
                Some(LRESULT(0))
            }
            WM_NCHITTEST => Some(LRESULT(HTTRANSPARENT as isize)),
            _ => None,
        }
    }
}

fn paint(c: &Control) {
    let canvas = &c.canvas;
    let theme = theme();
    let frame = canvas.begin_draw();
    c.layout.paint(
        canvas,
        ControlView {
            preview: c.preview.as_ref(),
            preview_size: c.preview_size,
            detail: c.detail.as_ref(),
            detail_size: c.detail_size,
            detail_range: c.detail_range,
            height: c.height,
            parts: c.parts,
            paused: c.paused,
            status: &c.status,
            hover: c.hover,
            can_undo: c.can_undo,
        },
        &theme,
    );
    if let Err(error) = frame.finish() {
        tracing::warn!(%error,"滚动截图控制条绘制失败");
    }
}
fn post(hwnd: Hwnd, message: u32, value: usize) {
    // SAFETY: 消息参数仅包含整数，不携带跨回调指针。
    let _ = unsafe { PostMessageW(Some(hwnd.raw()), message, WPARAM(value), LPARAM(0)) };
}
unsafe extern "system" fn mouse_hook(code: i32, event: WPARAM, data: LPARAM) -> LRESULT {
    if code == 0
        && matches!(
            event.0 as u32,
            WM_MOUSEWHEEL | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE
        )
    {
        // SAFETY: 系统只在本次 WH_MOUSE_LL 回调期间提供有效 MSLLHOOKSTRUCT。
        let info = unsafe { &*(data.0 as *const MSLLHOOKSTRUCT) };
        let point = Point {
            x: info.pt.x as f32,
            y: info.pt.y as f32,
        };
        CONTROL.with(|s| {
            if let Ok(s) = s.try_borrow()
                && let Some(c) = s.as_ref()
                && let Ok(mut c) = c.try_borrow_mut()
            {
                if !c.paused && !c.finishing && !c.layout.position.contains(point) {
                    let (left, top, right, bottom) = c.source.bounds;
                    let in_source = point.x >= left as f32
                        && point.x < right as f32
                        && point.y >= top as f32
                        && point.y < bottom as f32;
                    match event.0 as u32 {
                        WM_MOUSEWHEEL if in_source => c.activity.touch(),
                        WM_LBUTTONDOWN if in_source => {
                            c.dragging = true;
                            c.activity.touch();
                        }
                        WM_MOUSEMOVE if c.dragging => c.activity.touch(),
                        WM_LBUTTONUP if c.dragging => {
                            c.dragging = false;
                            c.activity.touch();
                        }
                        _ => {}
                    }
                } else if event.0 as u32 == WM_LBUTTONUP {
                    c.dragging = false;
                }
            }
        });
    }
    // SAFETY: 从不截断滚轮消息，原应用照常滚动。
    unsafe { CallNextHookEx(None, code, event, data) }
}
unsafe extern "system" fn keyboard_hook(code: i32, event: WPARAM, data: LPARAM) -> LRESULT {
    if code == 0 && (event.0 as u32 == WM_KEYDOWN || event.0 as u32 == WM_SYSKEYDOWN) {
        // SAFETY: 系统保证 KBDLLHOOKSTRUCT 在当前回调期间有效。
        let key = unsafe { &*(data.0 as *const KBDLLHOOKSTRUCT) };
        if matches!(key.vkCode, 32..=40) {
            CONTROL.with(|s| {
                if let Ok(s) = s.try_borrow()
                    && let Some(c) = s.as_ref()
                    && let Ok(c) = c.try_borrow()
                    && !c.paused
                    && !c.finishing
                {
                    // SAFETY: 只读前台句柄，导航按键原样转给原应用。
                    if unsafe { GetForegroundWindow() }.0 as isize == c.source.handle {
                        c.activity.touch();
                    }
                }
            });
        }
        let action = match key.vkCode {
            27 => Some(6),
            13 => Some(5),
            90 => {
                // SAFETY: 只读当前键盘状态，不修改用户输入。
                if unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x11) }
                    < 0
                {
                    Some(2)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(action) = action {
            let hwnd = CONTROL_HWND.with(Cell::get);
            if hwnd != 0 {
                post(Hwnd::from_raw(HWND(hwnd as *mut _)), WM_ACTION, action);
                return LRESULT(1);
            }
        }
    }
    // SAFETY: 其他按键原样交回钩子链。
    unsafe { CallNextHookEx(None, code, event, data) }
}
