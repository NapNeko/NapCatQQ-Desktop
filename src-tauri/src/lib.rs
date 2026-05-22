use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ncd_core::{
    BootstrapSnapshot, BotManager, BroadcastEventBus, ConfigStore, DispatchRenderer, EventBus,
    EventFilter, LocalBotConfigRepo, LocalConfigStore, LocalRuntimeBackend, SecretStoreImpl,
};
use tauri::Emitter;

pub mod bootstrap;
pub mod commands;
pub mod runtime;

pub use bootstrap::build_snapshot;

pub type AppBotManager = BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>;

pub struct AppState {
    pub(crate) data_root: PathBuf,
    pub(crate) snapshot: BootstrapSnapshot,
    pub(crate) event_bus: BroadcastEventBus,
    pub(crate) runtime: runtime::AppRuntime,
    pub(crate) bot_manager: Arc<AppBotManager>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_root = bootstrap::resolve_data_root();
    let snapshot = build_snapshot();
    let event_bus = BroadcastEventBus::default();
    let runtime = runtime::AppRuntime::new(&data_root, event_bus.clone());
    let runtime_watcher = runtime.clone();
    let runtime_shutdown = runtime.clone();

    let store = Arc::new(LocalConfigStore::new(&data_root));
    let secrets: Arc<dyn ncd_core::SecretStore + Send + Sync> =
        Arc::new(SecretStoreImpl::new(data_root.join("secrets")));
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let renderer = Arc::new(DispatchRenderer::new(store.config_dir()));
    let bot_backend = Arc::new(LocalRuntimeBackend::new(&data_root, "bot-manager-local"));
    let launch_planner = Arc::new(ncd_core::FileSystemRuntimeLaunchPlanner::new(
        data_root.join("runtime"),
    ));
    let bot_manager = Arc::new(BotManager::new(
        repo,
        Arc::clone(&store),
        renderer,
        bot_backend,
        launch_planner,
        Arc::new(event_bus.clone()),
    ));

    let bot_manager_bootstrap = Arc::clone(&bot_manager);

    tauri::Builder::default()
        .manage(AppState {
            data_root,
            snapshot,
            event_bus: event_bus.clone(),
            runtime,
            bot_manager,
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
            tauri::async_runtime::spawn(async move {
                match bot_manager_bootstrap.bootstrap().await {
                    Ok(result) => {
                        if !result.skipped.is_empty() {
                            eprintln!(
                                "[bot_manager] bootstrap skipped {} bot(s) (over limit)",
                                result.skipped.len()
                            );
                        }
                        if !result.started.failed.is_empty() {
                            eprintln!(
                                "[bot_manager] bootstrap auto-start failed for {} bot(s)",
                                result.started.failed.len()
                            );
                        }
                    }
                    Err(err) => {
                        eprintln!("[bot_manager] bootstrap failed: {err}");
                    }
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
            commands::bot::bootstrap_bot_manager,
            commands::bot::list_bot_snapshots,
            commands::bot::get_bot_snapshot,
            commands::bot::get_bot_config,
            commands::bot::upsert_bot_config,
            commands::bot::delete_bot_config,
            commands::bot::start_bot,
            commands::bot::stop_bot,
            commands::bot::batch_start_bots,
            commands::bot::batch_stop_bots,
            commands::bot::batch_delete_bots,
            commands::bot::count_bot_configs,
            commands::bot::active_bot_count,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
