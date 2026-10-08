//! WebView 调度：所有 WebView 窗口的资源账和分级休眠都收在这里。
//!
//! 各窗口共用一个浏览器进程和一个 GPU 进程，每个窗口各自一个渲染进程。按窗口算账要把
//! 渲染进程对回窗口：WebView2 能列出每个渲染进程上跑着哪些帧，窗口这边拿得到自己主帧的
//! id，两边一对就知道哪个渲染进程是谁的。
//!
//! 显隐必须走 [`show_window`] / [`hide_window`]，焦点变化由 [`handle_window_event`] 喂进来；
//! 策略在 `policy.rs`。只藏原生窗口的话 WebView2 仍当页面可见，动画照跑、缓存不放。

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow, WindowEvent};
use ts_rs::TS;

mod policy;
#[cfg(windows)]
mod win;

use policy::{Action, Entry, Event, WebviewRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub enum WebviewProcessKind {
    /// 桌面端自己（Rust 后端）
    Host,
    Browser,
    Gpu,
    Renderer,
    Utility,
    Other,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct WebviewProcessUsage {
    pub pid: u32,
    pub kind: WebviewProcessKind,
    /// 这个进程承载的窗口 label。只有渲染进程会有；对不上任何窗口（比如刚销毁的页）时为空。
    pub windows: Vec<String>,
    #[ts(type = "number")]
    pub private_bytes: u64,
    #[ts(type = "number")]
    pub working_set_bytes: u64,
}

/// 调度器眼里各窗口当前处在哪一级。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct WebviewWindowLevel {
    pub label: String,
    /// 目标不可见状态，实际执行反馈见 applied_hidden。
    pub hidden: bool,
    /// 目标 Low 状态，实际执行反馈见 applied_dormant。
    pub dormant: bool,
    pub applied_hidden: Option<bool>,
    // 成功设置 Low 不表示内存回收已经完成。
    pub applied_dormant: Option<bool>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct WebviewMemoryReport {
    pub v: u32,
    pub processes: Vec<WebviewProcessUsage>,
    #[ts(type = "number")]
    pub total_private_bytes: u64,
    /// 调度器登记过的窗口；还没收到过任何显隐 / 焦点事件的窗口不在里面
    pub levels: Vec<WebviewWindowLevel>,
}

/// 平台层查回来的一条进程记录，内存读不到时为 0（进程刚好退出）。
#[derive(Debug, Clone)]
pub(crate) struct RawProcess {
    pub pid: u32,
    pub kind: WebviewProcessKind,
    pub frame_ids: Vec<u32>,
    pub private_bytes: u64,
    pub working_set_bytes: u64,
}

/// 把进程记录和「主帧 id → 窗口 label」拼成报表：本体排最前，其次浏览器、GPU、渲染进程。
pub(crate) fn assemble(
    mut raw: Vec<RawProcess>,
    frames: &HashMap<u32, String>,
) -> WebviewMemoryReport {
    raw.sort_by_key(|p| (p.kind, p.pid));
    let processes: Vec<WebviewProcessUsage> = raw
        .into_iter()
        .map(|p| {
            let mut windows: Vec<String> = p
                .frame_ids
                .iter()
                .filter_map(|id| frames.get(id).cloned())
                .collect();
            windows.sort();
            windows.dedup();
            WebviewProcessUsage {
                pid: p.pid,
                kind: p.kind,
                windows,
                private_bytes: p.private_bytes,
                working_set_bytes: p.working_set_bytes,
            }
        })
        .collect();
    let total_private_bytes = processes.iter().map(|p| p.private_bytes).sum();
    WebviewMemoryReport {
        v: 1,
        processes,
        total_private_bytes,
        levels: Vec::new(),
    }
}

pub async fn memory_report(app: &AppHandle) -> Result<WebviewMemoryReport, String> {
    #[cfg(windows)]
    let mut report = win::memory_report(app).await?;
    #[cfg(not(windows))]
    let mut report = assemble(Vec::new(), &HashMap::new());
    if let Some(scheduler) = app.try_state::<WebviewScheduler>() {
        report.levels = scheduler.levels();
    }
    Ok(report)
}

/// 每个窗口的调度状态，按 label 存。窗口第一次有事件时登记，销毁时摘掉。
#[derive(Default)]
pub struct WebviewScheduler {
    entries: Mutex<HashMap<String, Registration>>,
    next_window_id: AtomicU64,
}

struct Registration {
    id: u64,
    entry: Entry,
}

#[derive(Clone, Copy)]
struct Revision {
    window_id: u64,
    generation: u64,
}

impl WebviewScheduler {
    fn step(&self, label: &str, event: Event) -> Option<(Revision, Vec<Action>)> {
        let Ok(mut entries) = self.entries.lock() else {
            return None;
        };
        let registration = entries
            .entry(label.to_owned())
            .or_insert_with(|| Registration {
                id: self.next_window_id.fetch_add(1, Ordering::Relaxed),
                entry: Entry::default(),
            });
        let actions = policy::step(
            WebviewRole::from_label(label),
            &mut registration.entry,
            event,
            Instant::now(),
        );
        Some((
            Revision {
                window_id: registration.id,
                generation: registration.entry.generation(),
            },
            actions,
        ))
    }

    fn is_current(&self, label: &str, revision: Revision) -> bool {
        self.entries.lock().ok().is_some_and(|entries| {
            entries.get(label).is_some_and(|registration| {
                registration.id == revision.window_id
                    && registration.entry.generation() == revision.generation
            })
        })
    }

    fn complete(
        &self,
        label: &str,
        revision: Revision,
        action: Action,
        result: Result<(), String>,
    ) -> Option<Action> {
        let mut entries = self.entries.lock().ok()?;
        let registration = entries.get_mut(label)?;
        if registration.id != revision.window_id {
            return None;
        }
        registration
            .entry
            .complete(revision.generation, action, result)
    }

    fn levels(&self) -> Vec<WebviewWindowLevel> {
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };
        let mut levels: Vec<WebviewWindowLevel> = entries
            .iter()
            .map(|(label, registration)| WebviewWindowLevel {
                label: label.clone(),
                hidden: registration.entry.is_hidden(),
                dormant: registration.entry.is_dormant(),
                applied_hidden: registration.entry.confirmed_hidden(),
                applied_dormant: registration.entry.confirmed_dormant(),
                last_error: registration.entry.last_error().map(str::to_owned),
            })
            .collect();
        levels.sort_by(|a, b| a.label.cmp(&b.label));
        levels
    }

    fn forget(&self, label: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(label);
        }
    }

    fn needs_native_restore(&self, label: &str) -> bool {
        self.entries.lock().ok().is_some_and(|entries| {
            entries.get(label).is_some_and(|registration| {
                registration.entry.is_hidden()
                    || registration.entry.confirmed_hidden() == Some(true)
            })
        })
    }

    fn can_expose(&self, label: &str, revision: Revision) -> bool {
        self.entries.lock().ok().is_some_and(|entries| {
            entries.get(label).is_some_and(|registration| {
                registration.id == revision.window_id
                    && registration.entry.generation() == revision.generation
                    && !registration.entry.is_hidden()
                    && registration.entry.confirmed_hidden() == Some(false)
            })
        })
    }
}

/// 确认 WebView 可见后再显示原生窗口；内存级别恢复仍由调度器处理。
pub async fn show_window(window: &WebviewWindow) -> Result<(), String> {
    let scheduler = window
        .app_handle()
        .try_state::<WebviewScheduler>()
        .ok_or("WebView 调度器未初始化")?;
    let (revision, actions) = scheduler
        .step(window.label(), Event::Show)
        .ok_or("WebView 调度状态不可用")?;
    finish_show(window, revision, actions).await
}

async fn finish_show(
    window: &WebviewWindow,
    revision: Revision,
    actions: Vec<Action>,
) -> Result<(), String> {
    for action in actions {
        if action == Action::SetVisible(true) {
            let result = set_visible(window, true, revision).await;
            complete(window, revision, action, result.clone());
            result?;
        } else {
            apply(window, revision, action);
        }
    }
    if !window
        .app_handle()
        .try_state::<WebviewScheduler>()
        .is_some_and(|scheduler| scheduler.can_expose(window.label(), revision))
    {
        return Err("窗口显示请求已失效".into());
    }
    window.show().map_err(|e| e.to_string())
}

/// 藏起窗口：先藏原生窗口，再让 WebView 不可见并降到休眠。
pub fn hide_window(window: &WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())?;
    notify(window, Event::Hide);
    Ok(())
}

/// 挂在 `on_window_event` 上：焦点变化喂给策略，窗口销毁时摘掉登记。
pub fn handle_window_event(window: &tauri::Window, event: &WindowEvent) {
    match event {
        WindowEvent::Focused(focused) => {
            if *focused
                && window.is_visible().unwrap_or(false)
                && !window.is_minimized().unwrap_or(false)
                && defer_native_restore(window)
            {
                return;
            }
            if let Some(event) = policy::native_focus_event(
                *focused,
                window.is_visible().unwrap_or(false),
                window.is_minimized().unwrap_or(false),
            ) && let Some(webview) = window.app_handle().get_webview_window(window.label())
            {
                notify(&webview, event);
            }
        }
        WindowEvent::Resized(_) => {
            if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                if window.is_minimized().unwrap_or(false) {
                    notify(&webview, Event::Hide);
                } else if window.is_visible().unwrap_or(false) {
                    defer_native_restore(window);
                }
            }
        }
        WindowEvent::Destroyed => {
            if let Some(scheduler) = window.app_handle().try_state::<WebviewScheduler>() {
                scheduler.forget(window.label());
            }
        }
        _ => {}
    }
}

fn defer_native_restore(window: &tauri::Window) -> bool {
    let Some(scheduler) = window.app_handle().try_state::<WebviewScheduler>() else {
        return false;
    };
    if !scheduler.needs_native_restore(window.label()) {
        return false;
    }
    let Some(webview) = window.app_handle().get_webview_window(window.label()) else {
        return false;
    };
    let focused = window.is_focused().unwrap_or(false);
    // 任务栏恢复先露出原生窗口，先收起表面，等 controller 确认后再放出。
    if let Err(error) = window.hide() {
        tracing::warn!(target: "ncd_tauri::webview_scheduler", label = window.label(), %error, "延迟原生窗口恢复失败");
        return false;
    }
    // 在原生事件中确定代次，排队期间的隐藏/销毁可使这次恢复失效。
    let Some((revision, actions)) = scheduler.step(window.label(), Event::Show) else {
        return false;
    };
    tauri::async_runtime::spawn(async move {
        if let Err(error) = finish_show(&webview, revision, actions).await {
            tracing::warn!(target: "ncd_tauri::webview_scheduler", label = webview.label(), %error, "恢复 WebView 可见性失败");
        } else if focused {
            let _ = webview.set_focus();
        }
    });
    true
}

fn notify(window: &WebviewWindow, event: Event) {
    let Some(scheduler) = window.app_handle().try_state::<WebviewScheduler>() else {
        return;
    };
    if let Some((revision, actions)) = scheduler.step(window.label(), event) {
        for action in actions {
            apply(window, revision, action);
        }
    }
}

fn is_current(window: &WebviewWindow, revision: Revision) -> bool {
    window
        .app_handle()
        .try_state::<WebviewScheduler>()
        .is_some_and(|scheduler| scheduler.is_current(window.label(), revision))
}

fn complete(
    window: &WebviewWindow,
    revision: Revision,
    action: Action,
    result: Result<(), String>,
) {
    if let Some(scheduler) = window.app_handle().try_state::<WebviewScheduler>() {
        if let Some(retry) = scheduler.complete(window.label(), revision, action, result) {
            apply(window, revision, retry);
        }
    }
}

fn apply(window: &WebviewWindow, revision: Revision, action: Action) {
    if !is_current(window, revision) {
        return;
    }
    match action {
        Action::SetVisible(visible) => {
            let window = window.clone();
            tauri::async_runtime::spawn(async move {
                let result = set_visible(&window, visible, revision).await;
                if let Err(error) = &result {
                    tracing::warn!(target: "ncd_tauri::webview_scheduler", label = window.label(), visible, %error, "切换 WebView 可见性失败");
                }
                complete(&window, revision, action, result);
            });
        }
        Action::SetDormant(dormant) => {
            #[cfg(windows)]
            {
                let window = window.clone();
                tauri::async_runtime::spawn(async move {
                    let result = win::set_dormant(&window, dormant, revision).await;
                    complete(&window, revision, action, result);
                });
            }
            #[cfg(not(windows))]
            complete(
                window,
                revision,
                action,
                Err(format!("当前平台不支持 Low 策略: {dormant}")),
            );
        }
        Action::ScheduleTick { after, generation } => {
            let window = window.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(after).await;
                // 等的这段时间里窗口可能已经销毁，别给它重新登记
                if is_current(&window, revision) {
                    notify(&window, Event::Tick(generation));
                }
            });
        }
        Action::ScheduleRetry { generation } => {
            let window = window.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if is_current(&window, revision) {
                    notify(&window, Event::Retry(generation));
                }
            });
        }
    }
}

async fn set_visible(
    window: &WebviewWindow,
    visible: bool,
    revision: Revision,
) -> Result<(), String> {
    #[cfg(windows)]
    return win::set_visible(window, visible, revision).await;
    #[cfg(not(windows))]
    {
        if !is_current(window, revision) {
            return Err("窗口状态已改变".into());
        }
        let webview: &tauri::Webview = window.as_ref();
        if visible {
            webview.show()
        } else {
            webview.hide()
        }
        .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(
        pid: u32,
        kind: WebviewProcessKind,
        frame_ids: &[u32],
        private_bytes: u64,
    ) -> RawProcess {
        RawProcess {
            pid,
            kind,
            frame_ids: frame_ids.to_vec(),
            private_bytes,
            working_set_bytes: private_bytes * 2,
        }
    }

    #[test]
    fn maps_renderer_frames_to_window_labels_and_orders_by_kind() {
        let frames = HashMap::from([(7, "main".to_string()), (9, "tray-panel".to_string())]);
        let report = assemble(
            vec![
                raw(30, WebviewProcessKind::Renderer, &[9], 80),
                raw(20, WebviewProcessKind::Gpu, &[], 150),
                raw(31, WebviewProcessKind::Renderer, &[7, 7, 12], 120),
                raw(10, WebviewProcessKind::Browser, &[], 40),
                raw(1, WebviewProcessKind::Host, &[], 45),
            ],
            &frames,
        );
        let order: Vec<u32> = report.processes.iter().map(|p| p.pid).collect();
        assert_eq!(order, vec![1, 10, 20, 30, 31]);
        assert_eq!(report.processes[3].windows, vec!["tray-panel"]);
        // 同一窗口的重复帧只记一次，对不上的帧（iframe、已销毁的页）丢掉
        assert_eq!(report.processes[4].windows, vec!["main"]);
        assert!(report.processes[2].windows.is_empty());
        assert_eq!(report.total_private_bytes, 435);
        assert_eq!(report.v, 1);
    }

    #[test]
    fn serializes_camel_case_with_snake_case_kind() {
        let report = assemble(
            vec![raw(1, WebviewProcessKind::Host, &[], 3)],
            &HashMap::new(),
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["totalPrivateBytes"], 3);
        assert_eq!(json["processes"][0]["kind"], "host");
        assert_eq!(json["processes"][0]["workingSetBytes"], 6);
    }

    #[test]
    fn old_window_completion_cannot_update_a_recreated_label() {
        let scheduler = WebviewScheduler::default();
        let (old, _) = scheduler.step("main", Event::Hide).unwrap();
        scheduler.forget("main");
        let (current, _) = scheduler.step("main", Event::Hide).unwrap();
        assert!(!scheduler.is_current("main", old));
        assert!(scheduler.is_current("main", current));
        scheduler.complete("main", old, Action::SetDormant(true), Ok(()));
        assert_eq!(scheduler.levels()[0].applied_dormant, None);
        scheduler.complete("main", current, Action::SetDormant(true), Ok(()));
        assert_eq!(scheduler.levels()[0].applied_dormant, Some(true));
    }

    #[test]
    fn native_exposure_requires_confirmed_visibility_for_the_current_request() {
        let scheduler = WebviewScheduler::default();
        let (revision, _) = scheduler.step("main", Event::Show).unwrap();
        assert!(!scheduler.can_expose("main", revision));
        scheduler.complete("main", revision, Action::SetDormant(false), Ok(()));
        assert!(!scheduler.can_expose("main", revision));
        scheduler.complete(
            "main",
            revision,
            Action::SetVisible(true),
            Err("COM failed".into()),
        );
        assert!(!scheduler.can_expose("main", revision));
        scheduler.complete("main", revision, Action::SetVisible(true), Ok(()));
        assert!(scheduler.can_expose("main", revision));
        scheduler.step("main", Event::Hide).unwrap();
        assert!(!scheduler.can_expose("main", revision));
        scheduler.complete("main", revision, Action::SetVisible(true), Ok(()));
        assert!(!scheduler.can_expose("main", revision));
    }

    #[test]
    fn os_restore_stays_gated_after_focus_until_visibility_is_confirmed() {
        let scheduler = WebviewScheduler::default();
        assert!(!scheduler.needs_native_restore("main"));
        let (hidden, _) = scheduler.step("main", Event::Hide).unwrap();
        scheduler.complete("main", hidden, Action::SetVisible(false), Ok(()));
        assert!(scheduler.needs_native_restore("main"));
        let (focused, _) = scheduler.step("main", Event::Focus(true)).unwrap();
        assert!(scheduler.needs_native_restore("main"));
        scheduler.complete("main", hidden, Action::SetVisible(true), Ok(()));
        assert!(scheduler.needs_native_restore("main"));
        scheduler.complete("main", focused, Action::SetVisible(true), Ok(()));
        assert!(!scheduler.needs_native_restore("main"));
    }

    #[test]
    fn pending_show_survives_focus_changes_and_repeated_restore_requests() {
        let scheduler = WebviewScheduler::default();
        let label = crate::chat_window::CHAT_WINDOW_LABEL;
        let (hidden, _) = scheduler.step(label, Event::Hide).unwrap();
        scheduler.complete(label, hidden, Action::SetVisible(false), Ok(()));
        let (show, _) = scheduler.step(label, Event::Show).unwrap();
        scheduler.step(label, Event::Focus(false)).unwrap();
        scheduler.step(label, Event::Focus(true)).unwrap();
        scheduler.step(label, Event::Show).unwrap();
        assert!(scheduler.is_current(label, show));
        assert!(!scheduler.can_expose(label, show));
        scheduler.complete(label, show, Action::SetVisible(true), Ok(()));
        assert!(scheduler.can_expose(label, show));
        scheduler.step(label, Event::Hide).unwrap();
        assert!(!scheduler.can_expose(label, show));
    }
}
