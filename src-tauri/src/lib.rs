use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ncd_runtime::{
    BootstrapSnapshot, BotManager, BroadcastEventBus, DispatchRenderer, EventBus,
    EventFilter, LocalBotConfigRepo, LocalConfigStore, NoopOfflineNotifier,
    ReqwestNapCatWebUiClient, SecretStoreImpl,
};
use tauri::Emitter;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

pub mod bootstrap;
pub mod bot_host_resolver;
pub mod commands;
pub mod runtime;

pub use bootstrap::{build_snapshot, build_snapshot_for_data_root};

pub type AppBotManager = BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>;

pub struct AppState {
    pub(crate) data_root: PathBuf,
    pub(crate) snapshot: BootstrapSnapshot,
    pub(crate) event_bus: BroadcastEventBus,
    pub(crate) runtime: runtime::AppRuntime,
    pub(crate) bot_manager: Arc<AppBotManager>,
    pub(crate) server_manager: Arc<ncd_runtime::ServerManager>,
    /// Components 页活跃 task 注册表，task_id → CancellationToken。
    /// `run_component_action` 启动时插入；plan 完成 / 取消时移除。
    pub(crate) active_tasks: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// 远端主机布局探测缓存：host_id → (home, layout)。
    /// detect_component 对同一台机器的 home/layout 探测结果是稳定的，缓存后
    /// 5 个并发组件 detect 只探一次，不再各跑一遍 `echo $HOME` + layout 检查。
    /// run_component_action 会清掉对应条目，因为安装可能改变布局。
    pub(crate) host_probe_cache: Arc<Mutex<HashMap<String, commands::components::RemoteHostProbe>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_root = bootstrap::resolve_data_root();
    let snapshot = build_snapshot_for_data_root(&data_root);
    let event_bus = BroadcastEventBus::default();
    let runtime = runtime::AppRuntime::new(&data_root, event_bus.clone());
    let runtime_watcher = runtime.clone();

    let store = Arc::new(LocalConfigStore::new(&data_root));
    let secrets: Arc<dyn ncd_runtime::SecretStore + Send + Sync> =
        Arc::new(SecretStoreImpl::new(data_root.join("secrets")));
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let renderer = Arc::new(DispatchRenderer::new(
        data_root.join("runtime").join("NapCatQQ").join("config"),
        data_root.join("runtime").join("SnowLuma").join("config"),
    ));
    let launch_planner = Arc::new(
        ncd_runtime::FileSystemRuntimeLaunchPlanner::new(data_root.join("runtime"))
            .with_snowluma_runtime_root(data_root.join("runtime").join("SnowLuma"))
            .with_snowluma_data_root(data_root.join("snowluma")),
    );
    // NativeDeployment 替代旧 LocalRuntimeBackend：通过适配器壳对外仍是 BotBackend。
    let local_host: Arc<dyn ncd_host::Host> = Arc::new(ncd_host::local::LocalWindowsHost::new());
    let event_sink: Arc<dyn ncd_deploy::NativeRuntimeEventSink> =
        Arc::new(ncd_runtime::EventBusSink::new(Arc::new(event_bus.clone())));
    let translator: Arc<dyn ncd_deploy::NativeLaunchTranslator> =
        Arc::new(ncd_runtime::RuntimeLaunchPlannerAdapter::new(
            launch_planner.clone() as Arc<dyn ncd_runtime::RuntimeLaunchPlanner>,
        ));
    let native_deployment = Arc::new(ncd_deploy::NativeDeployment::new(
        translator,
        event_sink,
        Some(data_root.join("runtime").join("log")),
    ));
    let bot_backend: Arc<dyn ncd_runtime::BotBackend> =
        Arc::new(ncd_runtime::NativeDeploymentBackend::new(
            native_deployment,
            Arc::clone(&local_host),
            "bot-manager-local",
            ncd_runtime::BotFlavor::NapCat,
        ));
    // NapCat WebUI 登录轮询所需依赖（design.md §15.1）。
    // - `ReqwestNapCatWebUiClient` 走 rustls-tls，仅访问 127.0.0.1。
    // - `NoopOfflineNotifier` 是占位实现，真实通道由后续 Spec 接入。
    // - `WebUiPollerSettings` 默认轮询 5s + 关闭离线通知，调用方可热更新。
    let webui_client: Arc<dyn ncd_runtime::NapCatWebUiClient> = Arc::new(
        ReqwestNapCatWebUiClient::new()
            .expect("初始化 NapCat WebUI HTTP 客户端失败：rustls-tls 构建异常"),
    );
    let offline_notifier: Arc<dyn ncd_runtime::OfflineNotifier> = Arc::new(NoopOfflineNotifier);
    // 轮询设置启动期从磁盘加载（app-settings.json），不再每次都是 default：
    // 用户在设置页改的 Bot 登录检查间隔重启后仍生效。文件缺失回落 default。
    let app_settings = commands::app_settings::read_app_settings(&data_root);
    let poller_settings = Arc::new(RwLock::new(app_settings.poller.clone()));
    // ServerManager 提前构造,既给下面 AppState 用,也给 HostResolver 用(让
    // BotManager 能按 runtime_target 把 bot 启到本机 / 远端)。
    let server_manager = Arc::new(ncd_runtime::ServerManager::new(
        &data_root,
        Arc::new(ncd_runtime::KeyringCredentialStore),
    ));
    let host_resolver: Arc<dyn ncd_runtime::HostResolver> = Arc::new(
        bot_host_resolver::TauriHostResolver::new(
            Arc::clone(&server_manager),
            Arc::clone(&local_host),
        ),
    );
    let bot_manager = Arc::new(
        BotManager::new(
            repo,
            Arc::clone(&store),
            renderer,
            bot_backend,
            launch_planner,
            Arc::new(event_bus.clone()),
            webui_client,
            offline_notifier,
            poller_settings,
        )
        .with_host_resolver(host_resolver),
    );

    // SnowLuma daemon + backend wiring。
    //
    // 路径起源严格来自 `bootstrap::resolve_data_root()`：
    // - SnowLuma 持久化数据根：`<data_root>/snowluma/`
    // - SnowLuma 安装根：`<data_root>/runtime/snowluma`（与 `runtime_launch_plan`
    // 建图时使用的 runtime_root 同源；后续如果 PathProbe 暴露 SnowLuma
    // 单独路径，再切到 PathProbe 输出）。
    let snowluma_data_root = data_root.join("snowluma");
    let snowluma_runtime_root = data_root.join("runtime").join("SnowLuma");
    let snowluma_factory: Arc<dyn ncd_runtime::SnowLumaWebUiClientFactory> =
        Arc::new(ncd_runtime::ReqwestSnowLumaWebUiClientFactory::new());
    let snowluma_daemon = ncd_runtime::SnowLumaDaemon::new(
        snowluma_data_root,
        snowluma_runtime_root,
        Arc::new(event_bus.clone()),
        snowluma_factory,
    );
    let snowluma_backend: Arc<dyn ncd_runtime::BotBackend> =
        Arc::new(ncd_runtime::SnowLumaRuntimeBackend::new(
            ncd_runtime::BotId::new("snowluma-backend-local"),
            Arc::clone(&snowluma_daemon),
            Arc::new(event_bus.clone()),
        ));
    let bot_manager = Arc::new(
        Arc::try_unwrap(bot_manager)
            .ok()
            .expect("bot_manager Arc not yet shared")
            .with_snowluma(snowluma_backend, Arc::clone(&snowluma_daemon)),
    );

    let bot_manager_bootstrap = Arc::clone(&bot_manager);
    let bot_manager_listener = Arc::clone(&bot_manager);
    let bot_manager_login_listener = Arc::clone(&bot_manager);
    let bot_manager_snowluma_listener = Arc::clone(&bot_manager);

    tauri::Builder::default()
        // 用系统默认浏览器打开外部 URL（例如 NapCat WebUI）
        // webview 自身不支持 target=_blank。
        .plugin(tauri_plugin_opener::init())
        // 配置导入导出用原生文件 / 目录选择对话框（webview 无法拿真实文件系统路径）。
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            data_root,
            snapshot,
            event_bus: event_bus.clone(),
            runtime,
            bot_manager,
            server_manager,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            host_probe_cache: Arc::new(Mutex::new(HashMap::new())),
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let mut subscription = event_bus.subscribe(EventFilter::all());
            tauri::async_runtime::spawn(async move {
                while let Some(event) = subscription.next().await {
                    let event_name = event.tauri_event_name();
                    if let Ok(payload) = serde_json::to_string(&event) {
                        // 诊断日志：确认事件链是否真的把事件发到 webview。
                        // 稳定后可改成 tracing::debug。
                        eprintln!(
                            "[event-emit] {} bot={:?} payload_len={}",
                            event_name,
                            event.bot_id().map(|b| b.as_str().to_string()),
                            payload.len()
                        );
                        let emit_result = handle.emit(event_name, payload);
                        if let Err(err) = emit_result {
                            eprintln!("[event-emit] FAILED to emit {event_name}: {err}");
                        }
                    } else {
                        eprintln!("[event-emit] FAILED to serialize event {event_name}");
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
            // 订阅运行时事件总线，把 BotProcessExited 转成 actor 状态机转移
            // 防止 UI 残留假 Running。必须用 tauri::async_runtime::spawn
            // setup 回调本身没有 tokio current handle，直接 tokio::spawn 会 panic。
            tauri::async_runtime::spawn(async move {
                (*bot_manager_listener)
                    .clone()
                    .run_runtime_event_listener()
                    .await;
            });
            // NapCat WebUI 登录轮询监听（design.md §15.3 / §15.4）：
            // 同时订阅 NapCatWebuiAvailable / BotProcessExited 两路事件，分别
            // 驱动 NapCatLoginPoller 的创建与回收。`run_napcat_login_listener`
            // 需要 `Arc<Self>` 作为接收者（用于 cast 到 `Arc<dyn RestartHandle>`）。
            tauri::async_runtime::spawn(async move {
                bot_manager_login_listener.run_napcat_login_listener().await;
            });

            // SnowLuma daemon Crashed 级联级 actor。
            tauri::async_runtime::spawn(async move {
                bot_manager_snowluma_listener.run_snowluma_listener().await;
            });

            if let Err(err) = commands::tray::attach_tray(&app.handle()) {
                eprintln!("[tray] attach failed: {err}");
            }

            Ok(())
        })
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                let close_action = app_settings.close_action.clone();
                tauri::async_runtime::spawn(async move {
                    if close_action == "tray" {
                        if let Err(err) = commands::tray::window_hide_to_tray(app) {
                            eprintln!("[window] hide to tray failed: {err}");
                        }
                        return;
                    }
                    let state = app.state::<AppState>();
                    let result = state.bot_manager.shutdown_all().await;
                    if !result.failed.is_empty() {
                        eprintln!(
                            "[bot_manager] shutdown_all: {} bot(s) failed to stop cleanly",
                            result.failed.len()
                        );
                    }
                    state.runtime.shutdown().await;
                    app.exit(0);
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
            commands::release::get_release_snapshot,
            commands::app_settings::get_app_settings,
            commands::app_settings::set_app_settings,
            commands::app_settings::sync_close_action_preference,
            commands::system_metrics::get_system_resource_snapshot,
            commands::config_transfer::export_config,
            commands::config_transfer::import_config,
            commands::config_transfer::preview_config_import,
            commands::components::list_components,
            commands::components::detect_component,
            commands::components::run_component_action,
            commands::components::cancel_component_action,
            commands::bot::bootstrap_bot_manager,
            commands::bot::list_bot_snapshots,
            commands::bot::list_bot_flavors,
            commands::bot::get_bot_snapshot,
            commands::bot::get_bot_config,
            commands::bot::upsert_bot_config,
            commands::bot::delete_bot_config,
            commands::bot::start_bot,
            commands::bot::detect_bot_config_drift,
            commands::bot::start_bot_with_drift_decisions,
            commands::bot::upsert_bot_config_with_decisions,
            commands::bot::stop_bot,
            commands::bot::batch_start_bots,
            commands::bot::batch_stop_bots,
            commands::bot::batch_delete_bots,
            commands::bot::count_bot_configs,
            commands::bot::active_bot_count,
            commands::bot::tail_bot_log,
            commands::snowluma::list_qq_processes,
            commands::snowluma::probe_qq_login_info,
            commands::snowluma::get_snowluma_app_config,
            commands::snowluma::set_snowluma_app_config,
            commands::snowluma::set_snowluma_password_override,
            commands::snowluma::open_snowluma_webui,
            commands::servers::list_servers,
            commands::servers::add_server,
            commands::servers::update_server,
            commands::servers::setup_server_key_auth,
            commands::servers::delete_server,
            commands::servers::test_server_connection,
            commands::servers::scan_local_ssh_keys,
            commands::docker::docker_probe,
            commands::docker::docker_install,
            commands::docker::docker_list_containers,
            commands::docker::docker_container_action,
            commands::docker::docker_logs,
            commands::docker::docker_deploy,
            commands::docker::docker_compose_down,
            commands::tray::window_show,
            commands::tray::window_hide_to_tray,
            commands::tray::count_local_active_bots,
            commands::tray::request_exit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
