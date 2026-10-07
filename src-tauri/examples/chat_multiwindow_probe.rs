//! 隔离测量多份 Chat 页面；不启动生产 runtime，不读真实账号与配置。
#![allow(unsafe_code)]
use ncd_domain::chat_archive::ChatArchive;
use ncd_domain::chat_desktop::*;
use ncd_domain::onebot_debug::{DebugHost, DebugTarget};
use ncd_domain::{AppSettings, AppSettingsDto, BackendType};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tokio::sync::oneshot;
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
use webview2_com::TrySuspendCompletedHandler;
use windows::core::Interface;

#[derive(Default)]
struct ProbeState {
    finished: AtomicBool,
    native_events: AtomicBool,
}

#[tauri::command]
fn get_app_settings() -> AppSettingsDto {
    AppSettingsDto {
        settings: AppSettings::default(),
        github_pat: String::new(),
    }
}

#[tauri::command]
fn chat_targets() -> Vec<DebugTarget> {
    vec![DebugTarget {
        bot_id: "multiwindow-probe".into(),
        name: "隔离测量账号".into(),
        qq_id: 10001,
        backend: BackendType::NapCat,
        host: DebugHost::Local,
        running: false,
        online: Some(false),
    }]
}

#[tauri::command]
fn chat_view_load() -> ChatViewState {
    ChatViewState {
        v: 1,
        revision: 1,
        selected_bot: Some("multiwindow-probe".into()),
        accounts: vec![ChatAccountView {
            bot_id: "multiwindow-probe".into(),
            self_id: "10001".into(),
            active: Some("private:22".into()),
            drafts: BTreeMap::new(),
            scroll: BTreeMap::new(),
            reading: BTreeMap::new(),
        }],
    }
}

#[tauri::command]
fn chat_archive_load() -> Result<ChatArchive, String> {
    serde_json::from_value(serde_json::json!({
        "v":1,"selfId":"10001",
        "conversations":[{"key":"private:22","type":"private","id":"22","name":"测试联系人","unread":0,"pinned":false,"lastAt":1000,"preview":"多窗口资源测量","boxed":false}],
        "messages":[{"key":"private:22/1","session":"private:22","id":"1","senderId":"22","senderName":"测试联系人","at":1000,"mine":false,"segments":[{"type":"text","data":{"text":"隔离探针，不连接真实账号。"}}],"status":"sent"}]
    })).map_err(|error| error.to_string())
}

#[tauri::command]
fn chat_desktop_status() -> ChatDesktopStatus {
    ChatDesktopStatus {
        v: 1,
        accounts: Vec::new(),
    }
}

#[tauri::command]
fn chat_take_tray_navigation() -> Option<ChatTrayNavigation> {
    None
}
#[tauri::command]
fn chat_select_account() {}
#[tauri::command]
fn chat_set_reading() {}
#[tauri::command]
fn chat_mark_read() {}
#[tauri::command]
fn chat_archive_save() {}
#[tauri::command]
fn chat_view_save() {}
#[tauri::command]
fn chat_release_account() {}
#[tauri::command]
fn chat_flush() {}
#[tauri::command]
fn reveal_chat_window() {}

async fn record(app: &AppHandle, stage: &str) -> Result<(), String> {
    for sample in 0..3 {
        let report = ncd_tauri::webview_scheduler::memory_report(app).await?;
        eprintln!(
            "MULTIWINDOW_SAMPLE {}",
            serde_json::json!({
                "stage": stage, "sample": sample, "report": report
            })
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(())
}

async fn suspend(window: &WebviewWindow) -> Result<bool, String> {
    let (send, receive) = oneshot::channel();
    window
        .with_webview(move |platform| {
            // SAFETY: COM 调用与回调均在 WebView UI 线程，controller 由窗口持有。
            let result = unsafe {
                platform
                    .controller()
                    .CoreWebView2()
                    .and_then(|core| core.cast::<ICoreWebView2_3>())
            };
            match result {
                Ok(core) => {
                    let send = std::sync::Arc::new(std::sync::Mutex::new(Some(send)));
                    let callback_send = send.clone();
                    let callback =
                        TrySuspendCompletedHandler::create(Box::new(move |code, success| {
                            if let Some(send) =
                                callback_send.lock().ok().and_then(|mut slot| slot.take())
                            {
                                let _ = send.send(
                                    code.map(|()| success).map_err(|error| error.to_string()),
                                );
                            }
                            Ok(())
                        }));
                    // SAFETY: 该 core 与 callback 在同一 UI 线程，WebView2 接管回调引用。
                    if let Err(error) = unsafe { core.TrySuspend(&callback) } {
                        if let Some(send) = send.lock().ok().and_then(|mut slot| slot.take()) {
                            let _ = send.send(Err(error.to_string()));
                        }
                    }
                }
                Err(error) => {
                    let _ = send.send(Err(error.to_string()));
                }
            }
        })
        .map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(5), receive)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?
}

async fn experiment(app: AppHandle, blank: bool, two: bool, legacy: bool) -> Result<(), String> {
    record(&app, "no-window").await?;
    let mut windows = Vec::new();
    let counts: &[usize] = if two { &[1, 2] } else { &[1, 2, 4, 8] };
    for &count in counts {
        while windows.len() < count {
            let mut conf = ncd_tauri::chat_window::chat_window_config(&app)?;
            if legacy {
                let main = app.config().app.windows.first().ok_or("缺少窗口配置")?;
                conf.transparent = main.transparent;
                conf.window_effects = main.window_effects.clone();
            }
            conf.label = format!("probe-chat-{}", windows.len());
            conf.width = 880.0;
            conf.height = 700.0;
            conf.min_width = Some(480.0);
            conf.min_height = Some(480.0);
            conf.visible = false;
            if blank {
                conf.url = WebviewUrl::External(
                    "about:blank"
                        .parse()
                        .map_err(|error: url::ParseError| error.to_string())?,
                );
            }
            let builder = WebviewWindowBuilder::from_config(&app, &conf)
                .map_err(|error| error.to_string())?;
            windows.push(builder.build().map_err(|error| error.to_string())?);
        }
        tokio::time::sleep(Duration::from_secs(8)).await;
        record(&app, &format!("normal-{count}")).await?;
    }
    eprintln!("MULTIWINDOW_INSPECT pid={}", std::process::id());
    tokio::time::sleep(Duration::from_secs(20)).await;
    for window in &windows {
        ncd_tauri::webview_scheduler::hide_window(window)?;
    }
    tokio::time::sleep(Duration::from_secs(10)).await;
    record(&app, &format!("hidden-low-{}", windows.len())).await?;
    ncd_tauri::webview_scheduler::show_window(&windows[0]).await?;
    let restored = ncd_tauri::webview_scheduler::memory_report(&app).await?;
    if !windows[0].is_visible().map_err(|error| error.to_string())?
        || !restored
            .levels
            .iter()
            .any(|level| level.label == windows[0].label() && level.applied_hidden == Some(false))
    {
        return Err("原生窗口显示前未确认 WebView 可见".into());
    }
    eprintln!("MULTIWINDOW_RESTORE_CONFIRMED");
    app.state::<ProbeState>()
        .native_events
        .store(true, Ordering::SeqCst);
    let native_restore = async {
        windows[0].minimize().map_err(|error| error.to_string())?;
        confirm_native_visibility(&app, &windows[0], true).await?;
        windows[0].unminimize().map_err(|error| error.to_string())?;
        confirm_native_visibility(&app, &windows[0], false).await?;
        eprintln!("MULTIWINDOW_OS_RESTORE_CONFIRMED");
        Ok::<(), String>(())
    }
    .await;
    app.state::<ProbeState>()
        .native_events
        .store(false, Ordering::SeqCst);
    native_restore?;
    ncd_tauri::webview_scheduler::hide_window(&windows[0])?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    for window in &windows {
        let success = suspend(window).await?;
        eprintln!("MULTIWINDOW_SUSPEND {} {success}", window.label());
    }
    tokio::time::sleep(Duration::from_secs(10)).await;
    record(&app, &format!("suspended-{}", windows.len())).await?;
    for window in windows.iter().skip(1) {
        window.destroy().map_err(|error| error.to_string())?;
    }
    tokio::time::sleep(Duration::from_secs(8)).await;
    record(&app, "retained-one").await?;
    for window in &windows {
        window.destroy().ok();
    }
    tokio::time::sleep(Duration::from_secs(8)).await;
    record(&app, "all-closed").await?;
    Ok(())
}

async fn confirm_native_visibility(
    app: &AppHandle,
    window: &WebviewWindow,
    hidden: bool,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline {
        let report = ncd_tauri::webview_scheduler::memory_report(app).await?;
        let native_ready = if hidden {
            window.is_minimized().map_err(|error| error.to_string())?
        } else {
            window.is_visible().map_err(|error| error.to_string())?
                && !window.is_minimized().map_err(|error| error.to_string())?
        };
        if native_ready
            && report
                .levels
                .iter()
                .any(|level| level.label == window.label() && level.applied_hidden == Some(hidden))
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("原生恢复状态未确认: hidden={hidden}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let blank = std::env::args().any(|argument| argument == "blank");
    let two = std::env::args().any(|argument| argument == "two");
    let legacy = std::env::args().any(|argument| argument == "legacy");
    let mut context = tauri::generate_context!();
    let data = std::env::temp_dir().join(format!("ncd-multiwindow-probe-{}", std::process::id()));
    for window in &mut context.config_mut().app.windows {
        window.create = false;
        window.data_directory = Some(data.clone());
    }
    let app = tauri::Builder::default()
        .manage(ProbeState::default())
        .manage(ncd_tauri::webview_scheduler::WebviewScheduler::default())
        .invoke_handler(tauri::generate_handler![get_app_settings, chat_targets, chat_view_load,
            chat_archive_load, chat_desktop_status, chat_take_tray_navigation, chat_select_account,
            chat_set_reading, chat_mark_read, chat_archive_save, chat_view_save,
            chat_release_account, chat_flush, reveal_chat_window])
        // 常规阶段隔离焦点干扰；最小化/恢复验收阶段单独接入原生事件。
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed)
                || window.app_handle().state::<ProbeState>().native_events.load(Ordering::SeqCst)
            {
                ncd_tauri::webview_scheduler::handle_window_event(window, event);
            }
        })
        .setup(move |app| {
            app.add_capability(r#"{"identifier":"multiwindow-probe","windows":["probe-chat-*"],"permissions":["core:default"]}"#)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let result = experiment(handle.clone(), blank, two, legacy).await;
                eprintln!("MULTIWINDOW_RESULT {result:?}");
                for window in handle.webview_windows().values() { let _ = window.destroy(); }
                handle.state::<ProbeState>().finished.store(true, Ordering::SeqCst);
                handle.exit(if result.is_ok() { 0 } else { 1 });
            });
            Ok(())
        })
        .build(context)?;
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app.state::<ProbeState>().finished.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
    Ok(())
}
