//! 隔离的原生窗口资源测试：不装生产 AppState，不读取真实账号或配置。
use std::{collections::{BTreeMap, HashSet}, sync::{atomic::{AtomicBool, Ordering}, Mutex}, time::Duration};
use ncd_domain::{AppSettings, AppSettingsDto};
use ncd_domain::chat_archive::ChatArchive;
use ncd_domain::chat_desktop::*;
use ncd_domain::onebot_debug::{DebugHost, DebugReceiverState, DebugTarget};
use tauri::{Manager, State};
use sysinfo::{Pid, ProcessesToUpdate, System};

fn target() -> DebugTarget {
    DebugTarget { bot_id: "smoke-account".into(), name: "窗口测试账号".into(), qq_id: 10001, backend: ncd_domain::BackendType::NapCat, host: DebugHost::Local, running: false, online: Some(false) }
}
fn view() -> ChatViewState {
    ChatViewState { v: 1, revision: 0, selected_bot: Some("smoke-account".into()), accounts: vec![ChatAccountView { bot_id: "smoke-account".into(), self_id: "10001".into(), active: Some("private:22".into()), drafts: BTreeMap::new(), scroll: BTreeMap::new(), reading: BTreeMap::new() }] }
}
#[derive(Default)]
struct Fixture { view: Mutex<Option<ChatViewState>>, preference: Mutex<ChatAccountPreference>, finished: AtomicBool }
#[tauri::command]
fn get_app_settings() -> AppSettingsDto { AppSettingsDto { settings: AppSettings::default(), github_pat: String::new() } }
#[tauri::command]
fn chat_targets() -> Vec<DebugTarget> { vec![target()] }
#[tauri::command]
fn chat_desktop_status(state: State<'_, Fixture>) -> ChatDesktopStatus {
    let mut preference = state.preference.lock().unwrap().clone();
    preference.bot_id = "smoke-account".into(); preference.self_id = "10001".into();
    ChatDesktopStatus { v: 1, accounts: vec![ChatAccountStatus { target: target(), preference, unread: 0, notification_unread: 0, groups: vec![], connection: DebugReceiverState::Stopped { reason: "测试账号离线".into() }, error: None }] }
}
#[tauri::command]
fn chat_set_preference(state: State<'_, Fixture>, preference: ChatAccountPreference) -> Result<(), String> { preference.validate().map_err(str::to_owned)?; *state.preference.lock().unwrap() = preference; Ok(()) }
#[tauri::command]
fn chat_view_load(state: State<'_, Fixture>) -> ChatViewState { state.view.lock().unwrap().clone().unwrap_or_else(view) }
#[tauri::command]
fn chat_view_save(state: State<'_, Fixture>, view: ChatViewState) -> Result<(), String> { view.validate().map_err(str::to_owned)?; *state.view.lock().unwrap() = Some(view); Ok(()) }
#[tauri::command]
fn chat_archive_load() -> ChatArchive {
    serde_json::from_value(serde_json::json!({"v":1,"selfId":"10001","conversations":[{"key":"private:22","type":"private","id":"22","name":"测试联系人","unread":0,"pinned":false,"lastAt":1000,"preview":"窗口资源测试","boxed":false}],"messages":[{"key":"private:22/1","session":"private:22","id":"1","senderId":"22","senderName":"测试联系人","at":1000,"mine":false,"segments":[{"type":"text","data":{"text":"窗口资源测试；没有连接真实账号。"}}],"status":"sent"}]})).unwrap()
}
#[tauri::command]
fn chat_archive_save() {}
#[tauri::command]
fn chat_set_reading() {}
#[tauri::command]
fn chat_release_account() {}
#[tauri::command]
fn chat_flush() {}
#[tauri::command]
fn reveal_chat_window() {} // 保持隐藏，不抢用户焦点。

fn sample(system: &mut System, stage: &str) -> usize {
    system.refresh_processes(ProcessesToUpdate::All);
    let root = Pid::from_u32(std::process::id());
    let mut ids = HashSet::from([root]);
    loop {
        let children: Vec<_> = system.processes().iter().filter(|(id, p)| !ids.contains(id) && p.parent().is_some_and(|parent| ids.contains(&parent))).map(|(id, _)| *id).collect();
        if children.is_empty() { break; } ids.extend(children);
    }
    let mut memory = 0; let mut cpu = 0.0; let mut webviews = 0;
    for id in ids { if let Some(p) = system.process(id) { memory += p.memory(); cpu += p.cpu_usage(); if p.name().to_string_lossy().contains("msedgewebview2") { webviews += 1; } } }
    eprintln!("CHAT_SMOKE {}", serde_json::json!({"stage":stage,"workingSetMiB":memory as f64 / 1048576.0,"cpuCorePercent":cpu,"webviewProcesses":webviews}));
    webviews
}
async fn run_test(app: tauri::AppHandle) -> Result<(), String> {
    let mut system = System::new_all();
    tokio::time::sleep(Duration::from_secs(3)).await; sample(&mut system, "no-window");
    let window = ncd_tauri::chat_window::create_chat_window(&app)?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    if sample(&mut system, "one-chat-window") == 0 { return Err("聊天窗没有创建 WebView".into()); }
    window.destroy().map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    if sample(&mut system, "closed") != 0 { return Err("关闭后仍有 WebView 进程".into()); }
    for _ in 0..30 {
        let window = ncd_tauri::chat_window::create_chat_window(&app)?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        window.destroy().map_err(|e| e.to_string())?;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    tokio::time::sleep(Duration::from_secs(6)).await;
    if sample(&mut system, "after-30-cycles") != 0 { return Err("反复开关后仍有 WebView 进程".into()); }
    if app.get_webview_window(ncd_tauri::chat_window::CHAT_WINDOW_LABEL).is_some() { return Err("聊天窗未回收".into()); }
    Ok(())
}
fn main() {
    eprintln!("CHAT_SMOKE_START");
    let mut context = tauri::generate_context!();
    let cache = std::env::temp_dir().join(format!("ncd-chat-window-smoke-{}", std::process::id()));
    for window in &mut context.config_mut().app.windows { window.create = false; window.data_directory = Some(cache.clone()); }
    let app = tauri::Builder::default().manage(Fixture::default())
        .invoke_handler(tauri::generate_handler![get_app_settings, chat_targets, chat_desktop_status, chat_set_preference, chat_view_load, chat_view_save, chat_archive_load, chat_archive_save, chat_set_reading, chat_release_account, chat_flush, reveal_chat_window])
        .setup(|app| {
            if let Some(main) = app.get_webview_window("main") { main.destroy()?; }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let result = run_test(handle.clone()).await;
                eprintln!("CHAT_SMOKE_RESULT {result:?}");
                handle.state::<Fixture>().finished.store(true, Ordering::SeqCst);
                handle.exit(if result.is_ok() { 0 } else { 1 });
            });
            Ok(())
        }).build(context).expect("build smoke application");
    app.run(|app, event| { if let tauri::RunEvent::ExitRequested { api, .. } = event { if !app.state::<Fixture>().finished.load(Ordering::SeqCst) { api.prevent_exit(); } } });
}
