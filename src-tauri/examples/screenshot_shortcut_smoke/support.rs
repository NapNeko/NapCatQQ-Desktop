// 隔离验证系统热键和原生截图开窗，复用生产源码，不加载账号与配置。
#![cfg(windows)]
#![expect(unsafe_code, reason = "隔离测试的 SendInput 和窗口状态探测")]

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use ncd_domain::chat_screenshot::{
    ChatScreenshotShortcut, ChatScreenshotShortcutEvent, ChatScreenshotShortcutResult,
};
use ncd_runtime::chat_screenshots::ChatScreenshotCache;
use tauri::{Listener, Manager, WebviewUrl, WebviewWindow};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetClassNameW, GetForegroundWindow, IsWindowVisible, PostMessageW,
    SetForegroundWindow, WM_HOTKEY,
};
use windows::core::w;

pub(crate) use ncd_tauri::{chat_window, webview_scheduler, windows_ui};
#[allow(dead_code)]
#[path = "../chat_window_smoke/fixture.rs"]
mod chat_fixture;
#[allow(dead_code, unused_imports)]
#[path = "../../src/native_panel/mod.rs"]
pub(crate) mod native_panel;
#[allow(dead_code)]
#[path = "../../src/screenshot/mod.rs"]
mod screenshot;

struct FixtureChat(Arc<ChatScreenshotCache>);
impl FixtureChat {
    fn is_enabled(&self) -> bool {
        true
    }
    fn screenshot_cache(&self) -> Arc<ChatScreenshotCache> {
        Arc::clone(&self.0)
    }
    fn view(&self) -> ncd_domain::chat_desktop::ChatViewState {
        ncd_domain::chat_desktop::ChatViewState::default()
    }
    async fn stage_screenshot(
        &self,
        _: &ncd_domain::chat_desktop::ChatViewState,
        _: &ncd_domain::chat_screenshot::ChatScreenshotAttachment,
    ) -> Result<String, String> {
        Err("隔离测试不插入生产草稿".into())
    }
}
struct FixtureGate;
impl FixtureGate {
    fn ensure_idle(&self) -> Result<(), String> {
        Ok(())
    }
}
pub(crate) struct AppState {
    chat: FixtureChat,
    migrate_gate: FixtureGate,
    configured: Mutex<Option<ChatScreenshotShortcut>>,
    finished: AtomicBool,
}

#[tauri::command]
async fn chat_screenshot_shortcut(
    window: WebviewWindow,
    state: tauri::State<'_, AppState>,
    request: ChatScreenshotShortcut,
) -> Result<(), String> {
    screenshot::configure_shortcut(
        window.app_handle().clone(),
        window.label().into(),
        request.clone(),
    )
    .await?;
    *state.configured.lock().map_err(|_| "fixture unavailable")? = Some(request);
    Ok(())
}
#[tauri::command]
async fn chat_screenshot_capture(
    window: WebviewWindow,
    state: tauri::State<'_, AppState>,
    request: ncd_domain::chat_screenshot::ChatScreenshotRequest,
) -> Result<Option<ncd_domain::chat_screenshot::ChatScreenshotAttachment>, String> {
    screenshot::capture_chat(
        window.app_handle().clone(),
        window,
        state.chat.screenshot_cache(),
        request,
        None,
    )
    .await
}
#[tauri::command]
fn chat_screenshot_cancel(window: WebviewWindow) {
    screenshot::cancel(window.app_handle(), window.label());
}
#[tauri::command]
fn chat_window_state() -> ncd_tauri::chat_window::ChatWindowState {
    ncd_tauri::chat_window::ChatWindowState {
        v: 1,
        detached: true,
        embed_requested: false,
    }
}
#[tauri::command]
fn chat_take_tray_navigation() -> Option<ncd_domain::chat_desktop::ChatTrayNavigation> {
    None
}
#[tauri::command]
fn chat_select_account() {}

fn press_shortcut() -> Result<(), String> {
    if std::env::args().any(|argument| argument == "--post-message") {
        // SAFETY: 只向本测试注册的消息窗投递无指针的热键消息。
        return unsafe {
            let hwnd = FindWindowW(w!("NCD.ChatScreenshotShortcut.v1"), None)
                .map_err(|e| e.to_string())?;
            PostMessageW(Some(hwnd), WM_HOTKEY, WPARAM(1), LPARAM(0)).map_err(|e| e.to_string())
        };
    }
    let keys = if std::env::args().any(|argument| argument == "--same-shortcut") {
        vec![0x11, 0x12, 0x46]
    } else {
        vec![0x11, 0x12, 0x10, 0x7a]
    };
    // SAFETY: 只查询按键；若用户正按住修饰键，不改动用户的键盘状态。
    if keys.iter().any(|key| unsafe { GetAsyncKeyState(*key) } < 0) {
        return Err("测试组合键仍被按住".into());
    }
    let mut inputs = Vec::new();
    for (up, keys) in [
        (false, keys.clone()),
        (true, keys.into_iter().rev().collect()),
    ] {
        for key in keys {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(key as u16),
                        dwFlags: if up {
                            KEYEVENTF_KEYUP
                        } else {
                            Default::default()
                        },
                        ..Default::default()
                    },
                },
            });
        }
    }
    // SAFETY: 注入已经注册的测试热键，随后释放同一组按键，不产生文本输入。
    let count = inputs.len() / 2;
    let down = unsafe { SendInput(&inputs[..count], std::mem::size_of::<INPUT>() as i32) };
    std::thread::sleep(Duration::from_millis(100));
    let up = unsafe { SendInput(&inputs[count..], std::mem::size_of::<INPUT>() as i32) };
    let sent = down + up;
    if sent != inputs.len() as u32 {
        return Err(format!("SendInput 只投递了 {sent} 个事件"));
    }
    Ok(())
}

pub(crate) fn run() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let directory = tempfile::tempdir().expect("isolated test directory");
    let data = directory.path().to_path_buf();
    let mut context = tauri::generate_context!();
    for window in &mut context.config_mut().app.windows {
        window.create = false;
    }
    context.config_mut().app.security.capabilities.push(
        serde_json::from_value(serde_json::json!({
            "identifier": "shortcut-smoke",
            "windows": ["main", "chat-panel"],
            "remote": { "urls": ["*"] },
            "permissions": ["core:default"]
        }))
        .expect("isolated test capability"),
    );
    let app = tauri::Builder::default()
        .register_uri_scheme_protocol("shortcut-smoke", |_, _| {
            tauri::http::Response::builder()
                .header("Content-Type", "text/html")
                .body(b"<title>Shortcut smoke</title><p>Isolated screenshot test</p>".to_vec())
                .expect("isolated test page")
        })
        .manage(AppState {
            chat: FixtureChat(Arc::new(ChatScreenshotCache::new(&data))),
            migrate_gate: FixtureGate,
            configured: Mutex::new(None),
            finished: AtomicBool::new(false),
        })
        .manage(chat_fixture::Fixture::default())
        .invoke_handler(tauri::generate_handler![
            chat_fixture::get_app_settings, chat_fixture::chat_targets, chat_fixture::chat_desktop_status,
            chat_fixture::chat_set_preference, chat_fixture::chat_view_load, chat_fixture::chat_view_save,
            chat_fixture::chat_archive_load, chat_fixture::chat_archive_save, chat_fixture::chat_set_reading,
            chat_fixture::chat_release_account, chat_fixture::chat_flush, chat_fixture::reveal_chat_window,
            chat_screenshot_shortcut, chat_screenshot_capture, chat_screenshot_cancel,
            chat_window_state, chat_take_tray_navigation, chat_select_account,
        ])
        .manage(screenshot::ScreenshotCoordinator::default())
        .manage(webview_scheduler::WebviewScheduler::default())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                screenshot::owner_destroyed(window.app_handle(), window.label());
            }
            webview_scheduler::handle_window_event(window, event);
        })
        .setup(move |app| {
            let window = tauri::WebviewWindowBuilder::new(
                app,
                if std::env::args().any(|argument| argument == "--startup") { "main" } else { "chat-panel" },
                if std::env::args().any(|argument| argument == "--startup") {
                    WebviewUrl::External("http://localhost:1420/".parse()?)
                } else if std::env::args().any(|argument| argument == "--react") {
                    WebviewUrl::External("http://localhost:1420/chat.html".parse()?)
                } else {
                    WebviewUrl::CustomProtocol("shortcut-smoke://localhost".parse()?)
                },
            )
            .title("截图快捷键隔离测试")
            .inner_size(360.0, 180.0)
            .visible(false)
            .initialization_script(r#"
                localStorage.setItem('ncd.chat.screenshot.v1', JSON.stringify({hideWindow: false, globalShortcut: true, shortcut: 'Ctrl+Alt+F'}));
                window.__shortcutSmoke = { ready: false, error: null, events: 0 };
                window.__TAURI_INTERNALS__.invoke('plugin:event|listen', {
                    event: 'chat-screenshot-shortcut',
                    target: { kind: 'AnyLabel', label: window.__TAURI_INTERNALS__.metadata.currentWindow.label },
                    handler: window.__TAURI_INTERNALS__.transformCallback(() => window.__shortcutSmoke.events++)
                }).then(() => window.__shortcutSmoke.ready = true)
                  .catch(error => window.__shortcutSmoke.error = String(error));
            "#)
            .data_directory(data.join("webview"))
            .build()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let result = run_test(handle.clone(), window).await;
                eprintln!("SHORTCUT_SMOKE_RESULT {result:?}");
                handle.state::<AppState>().finished.store(true, Ordering::SeqCst);
                handle.exit(if result.is_ok() { 0 } else { 1 });
            });
            Ok(())
        })
        .build(context)
        .expect("build isolated shortcut test");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app.state::<AppState>().finished.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
    drop(directory);
}

async fn wait_event(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<ChatScreenshotShortcutEvent>,
) -> Result<ChatScreenshotShortcutEvent, String> {
    tokio::time::timeout(Duration::from_secs(5), receiver.recv())
        .await
        .map_err(|_| "截图状态事件超时".to_string())?
        .ok_or_else(|| "截图状态通道已关闭".into())
}

async fn run_test(app: tauri::AppHandle, window: WebviewWindow) -> Result<(), String> {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    app.listen_any("chat-screenshot-shortcut", move |event| {
        eprintln!("SHORTCUT_SMOKE event={}", event.payload());
        if let Ok(event) = serde_json::from_str::<ChatScreenshotShortcutEvent>(event.payload()) {
            if std::env::args().any(|argument| argument == "--delayed")
                && event.result == ChatScreenshotShortcutResult::Started
            {
                std::thread::sleep(Duration::from_millis(350));
            }
            let _ = sender.send(event);
        }
    });
    let same = std::env::args().any(|argument| argument == "--same-shortcut");
    let startup = std::env::args().any(|argument| argument == "--startup");
    if startup || std::env::args().any(|argument| argument == "--react") {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if app
                    .state::<AppState>()
                    .configured
                    .lock()
                    .ok()
                    .is_some_and(|request| request.as_ref().is_some_and(|request| request.enabled))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| "应用页面未注册快捷键".to_string())?;
        if startup {
            let configured = app.state::<AppState>().configured.lock().map_err(|_| "fixture unavailable")?.clone();
            if configured.is_none_or(|request| !request.context.is_empty()) {
                return Err("启动测试意外依赖了聊天会话".into());
            }
            eprintln!("SHORTCUT_SMOKE startup_registered_without_chat=true");
        }
    } else {
        screenshot::configure_shortcut(
            app.clone(),
            window.label().into(),
            ChatScreenshotShortcut {
                enabled: true,
                global: true,
                control: true,
                alt: true,
                shift: !same,
                key: if same { "F" } else { "F11" }.into(),
                context: "isolated-shortcut-smoke".into(),
                release_owner: false,
                hide_window: !same,
                add_to_chat: true,
            },
        )
        .await?;
    }
    // SAFETY: 只记句柄，结束后交还测试前的前台窗口。
    let previous = unsafe { GetForegroundWindow() }.0 as isize;
    let mut stages = vec!["visible", "minimized", "hidden"];
    if std::env::args().any(|argument| argument == "--repeat-hidden") {
        stages.extend(std::iter::repeat_n("hidden", 20));
    }
    stages.extend(["released", "closed"]);
    for stage in stages {
        if stage == "closed" {
            window.destroy().map_err(|e| e.to_string())?;
        } else {
            webview_scheduler::show_window(&window).await?;
            window.unminimize().map_err(|e| e.to_string())?;
            tokio::time::sleep(Duration::from_millis(400)).await;
            match stage {
                "minimized" => window.minimize().map_err(|e| e.to_string())?,
                "hidden" => webview_scheduler::hide_window(&window)?,
                _ => {
                    window.set_focus().map_err(|e| e.to_string())?;
                }
            }
            if stage == "released" {
                screenshot::configure_shortcut(
                    app.clone(),
                    window.label().into(),
                    ChatScreenshotShortcut {
                        release_owner: true,
                        ..Default::default()
                    },
                )
                .await?;
            }
        }
        if stage != "visible" {
            // SAFETY: 将前台还给测试前的其他应用，验证真实后台触发。
            let _ = unsafe {
                SetForegroundWindow(windows::Win32::Foundation::HWND(previous as *mut _))
            };
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        if stage == "visible" {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let sender = Mutex::new(Some(sender));
            window
                .eval_with_callback("window.__shortcutSmoke", move |value| {
                    if let Ok(mut sender) = sender.lock()
                        && let Some(sender) = sender.take()
                    {
                        let _ = sender.send(value);
                    }
                })
                .map_err(|e| e.to_string())?;
            let value = tokio::time::timeout(Duration::from_secs(3), receiver)
                .await
                .map_err(|_| "页面监听状态超时".to_string())?
                .map_err(|e| e.to_string())?;
            eprintln!("SHORTCUT_SMOKE javascript={value}");
            if !value.contains("\"ready\":true") {
                return Err("页面结果监听未就绪".into());
            }
        }
        if stage != "closed" {
            eprintln!(
                "SHORTCUT_SMOKE stage={stage} visible={} minimized={} hwnd={:?} focused={}",
                window.is_visible().unwrap_or(false),
                window.is_minimized().unwrap_or(false),
                window.hwnd(),
                window.is_focused().unwrap_or(false)
            );
        } else {
            eprintln!(
                "SHORTCUT_SMOKE stage=closed webview_exists={}",
                app.get_webview_window(window.label()).is_some()
            );
        }
        let triggered_at = Instant::now();
        press_shortcut()?;
        let started = match wait_event(&mut receiver).await {
            Ok(event) => event,
            Err(error) => {
                // SAFETY: 只查询测试窗口与系统当前前台窗口。
                let overlay = unsafe { FindWindowW(w!("NCD.ChatScreenshot.v1"), None) };
                let foreground = unsafe { GetForegroundWindow() };
                eprintln!("SHORTCUT_SMOKE timeout overlay={overlay:?} foreground={foreground:?}");
                return Err(error);
            }
        };
        if started.result != ChatScreenshotShortcutResult::Started {
            return Err(format!("{stage}: 收到意外事件 {:?}", started.result));
        }
        let received_ms = triggered_at.elapsed().as_millis();
        let opened = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                // SAFETY: user32 校验窗口句柄，探测只读。
                if unsafe { FindWindowW(w!("NCD.ChatScreenshot.v1"), None) }
                    .ok()
                    .is_some_and(|hwnd| unsafe { IsWindowVisible(hwnd) }.as_bool())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        })
        .await;
        let opened_ms = triggered_at.elapsed().as_millis();
        tokio::time::sleep(Duration::from_millis(150)).await;
        let mut class = [0u16; 128];
        // SAFETY: 缓冲区有效，系统只写返回的窗口类名。
        let len = unsafe { GetClassNameW(GetForegroundWindow(), &mut class) }.max(0) as usize;
        let focused = String::from_utf16_lossy(&class[..len]) == "NCD.ChatScreenshot.v1";
        eprintln!("SHORTCUT_SMOKE stage={stage} received_ms={received_ms} opened_ms={opened_ms} overlay_foreground={focused}");
        let capture_owner = if startup || matches!(stage, "released" | "closed") {
            screenshot::SHORTCUT_CAPTURE_OWNER
        } else {
            window.label()
        };
        if opened.is_err() {
            screenshot::cancel(&app, capture_owner);
            return Err(format!("{stage}: 收到了热键，但截图窗口没有显示"));
        }
        if !focused {
            screenshot::cancel(&app, capture_owner);
            return Err(format!("{stage}: 截图窗已打开，但未取得前台与键盘焦点"));
        }
        if std::env::args().any(|argument| argument == "--recall-active") {
            press_shortcut()?;
            tokio::time::sleep(Duration::from_millis(100)).await;
            // SAFETY: 重复热键应仍指向同一原生截图，不创建第二次捕获。
            let mut class = [0u16; 128];
            let len = unsafe { GetClassNameW(GetForegroundWindow(), &mut class) }.max(0) as usize;
            if String::from_utf16_lossy(&class[..len]) != "NCD.ChatScreenshot.v1" {
                return Err(format!("{stage}: 再次按热键没有召回当前截图"));
            }
            if receiver.try_recv().is_ok() {
                return Err(format!("{stage}: 再次按热键意外开始了另一轮截图"));
            }
        }
        screenshot::cancel(&app, capture_owner);
        let finished = wait_event(&mut receiver).await?;
        if finished.result != (ChatScreenshotShortcutResult::Finished { file: None }) {
            return Err(format!("{stage}: 取消未完成 {:?}", finished.result));
        }
        eprintln!("SHORTCUT_SMOKE stage={stage} opened=true cancelled=true");
        // SAFETY: 保留的句柄由系统校验；失效时调用只会失败。
        let _ =
            unsafe { SetForegroundWindow(windows::Win32::Foundation::HWND(previous as *mut _)) };
    }
    screenshot::configure_shortcut(
        app.clone(),
        window.label().into(),
        ChatScreenshotShortcut::default(),
    )
    .await?;
    screenshot::owner_destroyed(&app, window.label());
    if app.get_webview_window(window.label()).is_some() {
        window.destroy().map_err(|e| e.to_string())?;
    }
    Ok(())
}
