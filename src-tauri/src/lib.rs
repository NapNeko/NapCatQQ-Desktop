use std::path::PathBuf;
use std::time::Duration;

use ncd_core::{BootstrapSnapshot, BroadcastEventBus, EventBus, EventFilter};
use tauri::Emitter;

pub mod bootstrap;
pub mod commands;
pub mod runtime;

pub use bootstrap::build_snapshot;

pub struct AppState {
    pub(crate) data_root: PathBuf,
    pub(crate) snapshot: BootstrapSnapshot,
    pub(crate) event_bus: BroadcastEventBus,
    pub(crate) runtime: runtime::AppRuntime,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_root = bootstrap::resolve_data_root();
    let snapshot = build_snapshot();
    let event_bus = BroadcastEventBus::default();
    let runtime = runtime::AppRuntime::new(&data_root, event_bus.clone());
    let runtime_watcher = runtime.clone();
    let runtime_shutdown = runtime.clone();

    tauri::Builder::default()
        .manage(AppState {
            data_root,
            snapshot,
            event_bus: event_bus.clone(),
            runtime,
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let mut subscription = event_bus.subscribe(EventFilter::all());
            tauri::async_runtime::spawn(async move {
                while let Some(event) = subscription.next().await {
                    if let Ok(payload) = serde_json::to_string(&event) {
                        let _ = handle.emit(event.tauri_event_name(), payload);
                    }
                }
            });
            tauri::async_runtime::spawn(async move {
                loop {
                    let delay = Duration::from_secs(runtime_watcher.watcher_interval_secs().await);
                    tokio::time::sleep(delay).await;
                    runtime_watcher.publish_runtime_status_changes().await;
                }
            });
            Ok(())
        })
        .on_window_event(move |_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let runtime_shutdown = runtime_shutdown.clone();
                tauri::async_runtime::spawn(async move {
                    runtime_shutdown.shutdown().await;
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::connect_remote_host,
            commands::export_migration_report,
            commands::get_all_bot_statuses,
            commands::get_bootstrap_status,
            commands::get_remote_runtime_status,
            commands::get_remote_webui_endpoint,
            commands::list_remote_files,
            commands::open_data_dir,
            commands::publish_demo_event,
            commands::publish_runtime_status,
            commands::spawn_local_bot,
            commands::stop_local_bot,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
