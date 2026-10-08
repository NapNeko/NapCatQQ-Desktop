//! 隔离的原生窗口资源测试：不装生产 AppState，不读取真实账号或配置。
use std::{collections::HashSet, sync::atomic::Ordering, time::Duration};
use sysinfo::{Pid, ProcessesToUpdate, System};
use tauri::Manager;

#[path = "chat_window_smoke/fixture.rs"]
mod fixture;
use fixture::Fixture;

fn sample(system: &mut System, stage: &str) -> usize {
    system.refresh_processes(ProcessesToUpdate::All);
    let root = Pid::from_u32(std::process::id());
    let mut ids = HashSet::from([root]);
    loop {
        let children: Vec<_> = system
            .processes()
            .iter()
            .filter(|(id, p)| {
                !ids.contains(id) && p.parent().is_some_and(|parent| ids.contains(&parent))
            })
            .map(|(id, _)| *id)
            .collect();
        if children.is_empty() {
            break;
        }
        ids.extend(children);
    }
    let mut memory = 0;
    let mut cpu = 0.0;
    let mut webviews = 0;
    for id in ids {
        if let Some(p) = system.process(id) {
            memory += p.memory();
            cpu += p.cpu_usage();
            if p.name().to_string_lossy().contains("msedgewebview2") {
                webviews += 1;
            }
        }
    }
    eprintln!(
        "CHAT_SMOKE {}",
        serde_json::json!({"stage":stage,"workingSetMiB":memory as f64 / 1048576.0,"cpuCorePercent":cpu,"webviewProcesses":webviews})
    );
    webviews
}
async fn run_test(app: tauri::AppHandle) -> Result<(), String> {
    let mut system = System::new_all();
    tokio::time::sleep(Duration::from_secs(3)).await;
    sample(&mut system, "no-window");
    let window = ncd_tauri::chat_window::create_chat_window(&app)?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    if sample(&mut system, "one-chat-window") == 0 {
        return Err("聊天窗没有创建 WebView".into());
    }
    window.destroy().map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    if sample(&mut system, "closed") != 0 {
        return Err("关闭后仍有 WebView 进程".into());
    }
    for _ in 0..30 {
        let window = ncd_tauri::chat_window::create_chat_window(&app)?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        window.destroy().map_err(|e| e.to_string())?;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    tokio::time::sleep(Duration::from_secs(6)).await;
    if sample(&mut system, "after-30-cycles") != 0 {
        return Err("反复开关后仍有 WebView 进程".into());
    }
    if app
        .get_webview_window(ncd_tauri::chat_window::CHAT_WINDOW_LABEL)
        .is_some()
    {
        return Err("聊天窗未回收".into());
    }
    Ok(())
}
fn main() {
    eprintln!("CHAT_SMOKE_START");
    let mut context = tauri::generate_context!();
    let cache = std::env::temp_dir().join(format!("ncd-chat-window-smoke-{}", std::process::id()));
    for window in &mut context.config_mut().app.windows {
        window.create = false;
        window.data_directory = Some(cache.clone());
    }
    let app = tauri::Builder::default()
        .manage(Fixture::default())
        .invoke_handler(tauri::generate_handler![
            fixture::get_app_settings,
            fixture::chat_targets,
            fixture::chat_desktop_status,
            fixture::chat_set_preference,
            fixture::chat_view_load,
            fixture::chat_view_save,
            fixture::chat_archive_load,
            fixture::chat_archive_save,
            fixture::chat_set_reading,
            fixture::chat_release_account,
            fixture::chat_flush,
            fixture::reveal_chat_window
        ])
        .setup(|app| {
            if let Some(main) = app.get_webview_window("main") {
                main.destroy()?;
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let result = run_test(handle.clone()).await;
                eprintln!("CHAT_SMOKE_RESULT {result:?}");
                handle
                    .state::<Fixture>()
                    .finished
                    .store(true, Ordering::SeqCst);
                handle.exit(if result.is_ok() { 0 } else { 1 });
            });
            Ok(())
        })
        .build(context)
        .expect("build smoke application");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app.state::<Fixture>().finished.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}
