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
    /// WebView 设了不可见（页面 hidden）
    pub hidden: bool,
    /// 内存级别降到 Low
    pub dormant: bool,
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
pub(crate) fn assemble(mut raw: Vec<RawProcess>, frames: &HashMap<u32, String>) -> WebviewMemoryReport {
    raw.sort_by_key(|p| (p.kind, p.pid));
    let processes: Vec<WebviewProcessUsage> = raw
        .into_iter()
        .map(|p| {
            let mut windows: Vec<String> = p.frame_ids.iter().filter_map(|id| frames.get(id).cloned()).collect();
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
    WebviewMemoryReport { v: 1, processes, total_private_bytes, levels: Vec::new() }
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
    entries: Mutex<HashMap<String, Entry>>,
}

impl WebviewScheduler {
    fn step(&self, label: &str, event: Event) -> Vec<Action> {
        let Ok(mut entries) = self.entries.lock() else {
            return Vec::new();
        };
        let entry = entries.entry(label.to_owned()).or_default();
        policy::step(WebviewRole::from_label(label), entry, event, Instant::now())
    }

    fn levels(&self) -> Vec<WebviewWindowLevel> {
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };
        let mut levels: Vec<WebviewWindowLevel> = entries
            .iter()
            .map(|(label, e)| WebviewWindowLevel { label: label.clone(), hidden: e.is_hidden(), dormant: e.is_dormant() })
            .collect();
        levels.sort_by(|a, b| a.label.cmp(&b.label));
        levels
    }

    fn forget(&self, label: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(label);
        }
    }
}

/// 显示窗口：先放出 WebView、恢复内存级别，再显示原生窗口，免得闪一帧空白。
pub fn show_window(window: &WebviewWindow) -> Result<(), String> {
    notify(window, Event::Show);
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
            if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                notify(&webview, Event::Focus(*focused));
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

fn notify(window: &WebviewWindow, event: Event) {
    let Some(scheduler) = window.app_handle().try_state::<WebviewScheduler>() else {
        return;
    };
    for action in scheduler.step(window.label(), event) {
        apply(window, action);
    }
}

fn apply(window: &WebviewWindow, action: Action) {
    match action {
        Action::SetVisible(visible) => {
            let webview: &tauri::Webview = window.as_ref();
            let result = if visible { webview.show() } else { webview.hide() };
            if let Err(err) = result {
                tracing::warn!(target: "ncd_tauri::webview_scheduler", label = window.label(), visible, "切换 WebView 可见性失败: {err}");
            }
        }
        Action::SetDormant(dormant) => {
            #[cfg(windows)]
            win::set_dormant(window, dormant);
            #[cfg(not(windows))]
            let _ = dormant;
        }
        Action::ScheduleTick { after, generation } => {
            let window = window.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(after).await;
                // 等的这段时间里窗口可能已经销毁，别给它重新登记
                if window.app_handle().get_webview_window(window.label()).is_some() {
                    notify(&window, Event::Tick(generation));
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(pid: u32, kind: WebviewProcessKind, frame_ids: &[u32], private_bytes: u64) -> RawProcess {
        RawProcess { pid, kind, frame_ids: frame_ids.to_vec(), private_bytes, working_set_bytes: private_bytes * 2 }
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
        let report = assemble(vec![raw(1, WebviewProcessKind::Host, &[], 3)], &HashMap::new());
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["totalPrivateBytes"], 3);
        assert_eq!(json["processes"][0]["kind"], "host");
        assert_eq!(json["processes"][0]["workingSetBytes"], 6);
    }
}
