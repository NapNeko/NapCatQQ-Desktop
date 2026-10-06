// WebView2 那一侧：读窗口主帧 id、列各进程承载的帧、按 pid 取内存。
// WebView2 的 COM 对象只能在它的 UI 线程上碰，统一经 with_webview 派过去，结果走 oneshot 回来。
// 整个文件都是 WebView2 / Win32 FFI，unsafe 免不掉，每处写明前提。
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::webview::PlatformWebview;
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::oneshot;
use webview2_com::GetProcessExtendedInfosCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    COREWEBVIEW2_PROCESS_KIND, COREWEBVIEW2_PROCESS_KIND_BROWSER, COREWEBVIEW2_PROCESS_KIND_GPU,
    COREWEBVIEW2_PROCESS_KIND_RENDERER, COREWEBVIEW2_PROCESS_KIND_UTILITY, ICoreWebView2_19,
    ICoreWebView2_20, ICoreWebView2Environment13, ICoreWebView2FrameInfo2,
    ICoreWebView2ProcessExtendedInfoCollection,
};
use windows::Win32::Foundation::{CloseHandle, E_POINTER, HANDLE};
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::{BOOL, Interface};

use super::{RawProcess, WebviewMemoryReport, WebviewProcessKind, assemble};

// UI 线程正忙（比如在建窗）时回调会晚到，给足余量但别让命令挂死
const CALL_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) async fn memory_report(app: &AppHandle) -> Result<WebviewMemoryReport, String> {
    let windows: Vec<(String, WebviewWindow)> = app.webview_windows().into_iter().collect();
    let mut frames = HashMap::new();
    for (label, window) in &windows {
        match on_webview(window, |pw| {
            read_main_frame_id(&pw).map_err(|e| e.to_string())
        })
        .await
        {
            Ok(id) => {
                frames.insert(id, label.clone());
            }
            Err(err) => {
                tracing::debug!(target: "ncd_tauri::webview_scheduler", %label, "读主帧 id 失败: {err}")
            }
        }
    }

    let mut raw = match windows.first() {
        // 所有窗口共用一个 WebView2 环境，随便拿一个窗口问就是全量
        Some((_, window)) => process_infos(window).await?,
        None => Vec::new(),
    };
    for p in &mut raw {
        if let Some((private_bytes, working_set_bytes)) = memory_of(p.pid) {
            p.private_bytes = private_bytes;
            p.working_set_bytes = working_set_bytes;
        }
    }
    let (private_bytes, working_set_bytes) = memory_of_current().unwrap_or_default();
    raw.push(RawProcess {
        pid: std::process::id(),
        kind: WebviewProcessKind::Host,
        frame_ids: Vec::new(),
        private_bytes,
        working_set_bytes,
    });
    Ok(assemble(raw, &frames))
}

/// 在窗口的 UI 线程上跑一个同步的 WebView2 调用，带超时取回结果。
async fn on_webview<T, F>(window: &WebviewWindow, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(PlatformWebview) -> Result<T, String> + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    window
        .with_webview(move |pw| {
            let _ = tx.send(f(pw));
        })
        .map_err(|e| e.to_string())?;
    wait(rx).await
}

async fn wait<T>(rx: oneshot::Receiver<Result<T, String>>) -> Result<T, String> {
    match tokio::time::timeout(CALL_TIMEOUT, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("WebView 已关闭".into()),
        Err(_) => Err("WebView 响应超时".into()),
    }
}

/// 休眠 = `MemoryUsageTargetLevel` 设 Low：WebView2 会丢掉能重建的缓存、压低渲染进程的内存，
/// 页面照常可用，只是回来时图片之类要重新解码。设 Normal 即恢复。
pub(super) fn set_dormant(window: &WebviewWindow, dormant: bool) {
    let label = window.label().to_owned();
    let dispatched = window.with_webview(move |pw| {
        let level = if dormant {
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
        } else {
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
        };
        // SAFETY: 在 with_webview 派到的 UI 线程上调用，controller 由仍存活的窗口持有
        let result = unsafe {
            pw.controller()
                .CoreWebView2()
                .and_then(|core| core.cast::<ICoreWebView2_19>())
                .and_then(|core| core.SetMemoryUsageTargetLevel(level))
        };
        match result {
            Ok(()) => tracing::debug!(target: "ncd_tauri::webview_scheduler", %label, dormant, "WebView 内存级别已切换"),
            Err(err) => tracing::warn!(target: "ncd_tauri::webview_scheduler", %label, dormant, "切换 WebView 内存级别失败: {err}"),
        }
    });
    if let Err(err) = dispatched {
        tracing::debug!(target: "ncd_tauri::webview_scheduler", "WebView 已不在，跳过内存级别切换: {err}");
    }
}

fn read_main_frame_id(pw: &PlatformWebview) -> windows::core::Result<u32> {
    // SAFETY: 在 with_webview 派到的 UI 线程上调用，controller 由仍存活的窗口持有
    unsafe {
        let core: ICoreWebView2_20 = pw.controller().CoreWebView2()?.cast()?;
        let mut id = 0u32;
        core.FrameId(&mut id)?;
        Ok(id)
    }
}

/// 列出 WebView2 环境下的全部进程。这是个异步 COM 调用，完成回调回到 UI 线程，
/// 所以发起时只把 sender 交出去，结果从回调里送回。
type ProcessReply = Arc<Mutex<Option<oneshot::Sender<Result<Vec<RawProcess>, String>>>>>;

fn reply(slot: &ProcessReply, result: Result<Vec<RawProcess>, String>) {
    if let Some(tx) = slot.lock().ok().and_then(|mut slot| slot.take()) {
        let _ = tx.send(result);
    }
}

async fn process_infos(window: &WebviewWindow) -> Result<Vec<RawProcess>, String> {
    let (tx, rx) = oneshot::channel();
    // 发起失败要在发起处回错，成功才由回调回，两边共享同一个 sender
    let tx: ProcessReply = Arc::new(Mutex::new(Some(tx)));
    window
        .with_webview(move |pw| {
            let in_callback = Arc::clone(&tx);
            let handler = GetProcessExtendedInfosCompletedHandler::create(Box::new(
                move |code, collection| {
                    let result = code
                        .and_then(|()| read_processes(collection))
                        .map_err(|e| e.to_string());
                    reply(&in_callback, result);
                    Ok(())
                },
            ));
            // SAFETY: UI 线程上调用；handler 是 COM 对象，WebView2 持有到回调结束
            let started = unsafe {
                pw.environment()
                    .cast::<ICoreWebView2Environment13>()
                    .and_then(|env| env.GetProcessExtendedInfos(&handler))
            };
            if let Err(err) = started {
                reply(&tx, Err(err.to_string()));
            }
        })
        .map_err(|e| e.to_string())?;
    wait(rx).await
}

fn read_processes(
    collection: Option<ICoreWebView2ProcessExtendedInfoCollection>,
) -> windows::core::Result<Vec<RawProcess>> {
    let collection = collection.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
    let mut out = Vec::new();
    // SAFETY: 在完成回调里（UI 线程）读 WebView2 刚交回的集合
    unsafe {
        let mut count = 0u32;
        collection.Count(&mut count)?;
        for index in 0..count {
            let info = collection.GetValueAtIndex(index)?;
            let process = info.ProcessInfo()?;
            let mut pid = 0i32;
            process.ProcessId(&mut pid)?;
            let mut kind = COREWEBVIEW2_PROCESS_KIND::default();
            process.Kind(&mut kind)?;

            let mut frame_ids = Vec::new();
            // 集合要活到遍历结束：迭代器不替集合续命，集合先释放的话遍历会读到野指针
            let collection = info.AssociatedFrameInfos()?;
            let frames = collection.GetIterator()?;
            let mut has = BOOL::default();
            frames.HasCurrent(&mut has)?;
            while has.as_bool() {
                if let Ok(frame) = frames.GetCurrent()?.cast::<ICoreWebView2FrameInfo2>() {
                    let mut id = 0u32;
                    frame.FrameId(&mut id)?;
                    frame_ids.push(id);
                }
                frames.MoveNext(&mut has)?;
            }

            out.push(RawProcess {
                pid: u32::try_from(pid).unwrap_or_default(),
                kind: map_kind(kind),
                frame_ids,
                private_bytes: 0,
                working_set_bytes: 0,
            });
        }
    }
    Ok(out)
}

fn map_kind(kind: COREWEBVIEW2_PROCESS_KIND) -> WebviewProcessKind {
    match kind {
        COREWEBVIEW2_PROCESS_KIND_BROWSER => WebviewProcessKind::Browser,
        COREWEBVIEW2_PROCESS_KIND_GPU => WebviewProcessKind::Gpu,
        COREWEBVIEW2_PROCESS_KIND_RENDERER => WebviewProcessKind::Renderer,
        COREWEBVIEW2_PROCESS_KIND_UTILITY => WebviewProcessKind::Utility,
        _ => WebviewProcessKind::Other,
    }
}

fn memory_of(pid: u32) -> Option<(u64, u64)> {
    // SAFETY: 句柄只在本函数里用，用完关掉；进程刚好退出时 OpenProcess 失败，返回 None
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let counters = counters(handle);
        let _ = CloseHandle(handle);
        counters
    }
}

fn memory_of_current() -> Option<(u64, u64)> {
    // SAFETY: GetCurrentProcess 是伪句柄，不用也不能关
    unsafe { counters(GetCurrentProcess()) }
}

unsafe fn counters(handle: HANDLE) -> Option<(u64, u64)> {
    let mut c = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    // SAFETY: EX 结构以基础结构开头，cb 写明了实际大小，API 按 cb 判断填哪一版
    unsafe {
        GetProcessMemoryInfo(handle, (&raw mut c).cast::<PROCESS_MEMORY_COUNTERS>(), c.cb).ok()?;
    }
    Some((c.PrivateUsage as u64, c.WorkingSetSize as u64))
}
