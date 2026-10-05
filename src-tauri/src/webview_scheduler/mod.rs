//! WebView 调度：所有 WebView 窗口的资源账，以及之后的分级休眠，都收在这里。
//!
//! 各窗口共用一个浏览器进程和一个 GPU 进程，每个窗口各自一个渲染进程。按窗口算账要把
//! 渲染进程对回窗口：WebView2 能列出每个渲染进程上跑着哪些帧，窗口这边拿得到自己主帧的
//! id，两边一对就知道哪个渲染进程是谁的。

use std::collections::HashMap;

use serde::Serialize;
use tauri::AppHandle;
use ts_rs::TS;

#[cfg(windows)]
mod win;

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

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct WebviewMemoryReport {
    pub v: u32,
    pub processes: Vec<WebviewProcessUsage>,
    #[ts(type = "number")]
    pub total_private_bytes: u64,
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
    WebviewMemoryReport { v: 1, processes, total_private_bytes }
}

pub async fn memory_report(app: &AppHandle) -> Result<WebviewMemoryReport, String> {
    #[cfg(windows)]
    {
        win::memory_report(app).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("按窗口统计内存只支持 Windows".into())
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
