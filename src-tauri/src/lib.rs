use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::{BootstrapSnapshot, DesktopNotifySettings};
use ncd_runtime::{
    BotManager, BroadcastEventBus, DispatchRenderer, EventBus, EventFilter, LocalBotConfigRepo,
    LocalConfigStore, ReqwestNapCatWebUiClient, SecretStoreImpl,
};
use tauri::Emitter;
use tauri::Manager;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

pub mod autostart;
pub mod bootstrap;
pub mod bot_host_resolver;
pub mod commands;
pub mod desktop_consent;
pub mod desktop_log;
pub mod desktop_log_format;
pub mod desktop_notify;
pub mod desktop_onboarding;
pub mod desktop_update;
pub mod legacy_install_cleanup;
pub mod lightweight;
pub mod lightweight_scheduler;
pub mod onebot_endpoint_resolver;
pub mod product_registry;
pub mod runtime;
pub mod single_instance;
pub mod snowluma_offline_listener;
pub mod tray_icon;
pub mod tray_menu;
pub mod tray_summary;
pub mod window_icon;
pub mod windows_toast;

pub use bootstrap::{build_snapshot, build_snapshot_for_data_root};

pub type AppBotManager = BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>;

pub struct AppState {
    pub(crate) data_root: PathBuf,
    pub(crate) snapshot: BootstrapSnapshot,
    pub(crate) event_bus: BroadcastEventBus,
    pub(crate) runtime: runtime::AppRuntime,
    pub(crate) bot_manager: Arc<AppBotManager>,
    pub(crate) server_manager: Arc<ncd_runtime::ServerManager>,
    /// Components 页活跃 task 注册表,task_id → CancellationToken
    /// run_component_action 启动时插入;plan 完成 / 取消时移除
    pub(crate) active_tasks: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// 部署/安装任务事实源与资源调度器。
    pub(crate) deployment_tasks: ncd_runtime::DeploymentTaskManager,
    /// 远端主机布局探测缓存:host_id → (home, layout)
    /// detect_component 对同一台机器的 home/layout 探测结果是稳定的,缓存后
    /// 5 个并发组件 detect 只探一次,不再各跑一遍 echo $HOME + layout 检查
    /// run_component_action 会清掉对应条目,因为安装可能改变布局
    pub(crate) host_probe_cache: Arc<Mutex<HashMap<String, ncd_runtime::RemoteHostProbe>>>,
    pub(crate) desktop_notify: Arc<RwLock<DesktopNotifySettings>>,
    pub(crate) app_settings: Arc<RwLock<ncd_domain::AppSettings>>,
    /// 离线告警 fan-out(桌面 Toast / Webhook / Email / OneBot)
    pub(crate) offline_notifier: Arc<ncd_runtime::CompositeOfflineNotifier>,
    pub(crate) lightweight_scheduler: Arc<lightweight_scheduler::LightweightScheduler>,
    /// 远程主机健康探活 walker 的取消令牌
    /// 由启动 wiring 和 set_app_settings 根据 enabled 变化来条件 spawn / cancel + restart
    pub(crate) health_probe_cancel: Arc<Mutex<Option<CancellationToken>>>,
    /// 本机 Bot 指标采集（读 net-stats + 节流写 history.jsonl）；远端历史由 ncd-watch 写
    pub(crate) metrics_collector: ncd_runtime::metrics::MetricsCollector,
    /// 数据根整树迁移闸门(进行中拒绝其它写盘 command)
    pub(crate) migrate_gate: Arc<commands::data_root_migrate::DataRootMigrateGate>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_root = bootstrap::resolve_data_root();
    let snapshot = build_snapshot_for_data_root(&data_root);
    let event_bus = BroadcastEventBus::default();
    desktop_log::init_desktop_logging(&data_root, event_bus.clone());
    #[cfg(windows)]
    {
        let reg = product_registry::ensure_product_paths_registered(&data_root);
        if reg.wrote_install_dir || reg.wrote_data_root {
            tracing::info!(
                target: "ncd_tauri::product_registry",
                install_dir = ?reg.install_dir.as_ref().map(|p| p.display().to_string()),
                data_root = ?reg.data_root.as_ref().map(|p| p.display().to_string()),
                wrote_install_dir = reg.wrote_install_dir,
                wrote_data_root = reg.wrote_data_root,
                "ensured product path registry values"
            );
        }
        for err in &reg.errors {
            tracing::debug!(
                target: "ncd_tauri::product_registry",
                error = %err,
                "product path registry ensure skipped or failed"
            );
        }
        let cleanup = legacy_install_cleanup::purge_legacy_install_orphans();

        if let Some(reason) = cleanup.skipped_reason.as_deref() {
            tracing::debug!(
                target: "ncd_tauri::install_cleanup",
                install_dir = %cleanup.install_dir.display(),
                reason,
                "legacy install orphan cleanup skipped"
            );
        }
        for name in &cleanup.removed {
            tracing::info!(
                target: "ncd_tauri::install_cleanup",
                install_dir = %cleanup.install_dir.display(),
                name = %name,
                "removed legacy install orphan"
            );
        }
        for (name, err) in &cleanup.failed {
            tracing::warn!(
                target: "ncd_tauri::install_cleanup",
                install_dir = %cleanup.install_dir.display(),
                name = %name,
                error = %err,
                "failed to remove legacy install orphan"
            );
        }

        let broken_reg = legacy_install_cleanup::purge_broken_hkcu_product_key();
        if broken_reg.removed {
            tracing::info!(
                target: "ncd_tauri::install_cleanup",
                "removed broken HKCU product key Software{{{{product_name}}}}"
            );
        } else if let Some(err) = broken_reg.error.as_deref() {
            tracing::warn!(
                target: "ncd_tauri::install_cleanup",
                error = %err,
                "failed to remove broken HKCU product key Software{{{{product_name}}}}"
            );
        } else if broken_reg.found {
            tracing::debug!(
                target: "ncd_tauri::install_cleanup",
                "broken HKCU product key present but not removed"
            );
        }
    }
    tracing::info!(
        target: "ncd_tauri",
        data_root = %data_root.display(),
        "NapCatQQ Desktop 进程已启动"
    );
    desktop_log::write_session_line(
        "INFO",
        "ncd::desktop",
        &format!("NapCatQQ Desktop 已启动，data_root={}", data_root.display()),
    );
    let runtime = runtime::AppRuntime::new(&data_root, event_bus.clone());
    let runtime_watcher = runtime.clone();

    let store = Arc::new(LocalConfigStore::new(&data_root));
    let paths = ncd_runtime::DataPaths::new(&data_root);
    let secrets: Arc<dyn ncd_traits::SecretStore + Send + Sync> =
        Arc::new(SecretStoreImpl::new(paths.secrets_dir()));
    let repo = Arc::new(LocalBotConfigRepo::new(
        Arc::clone(&store),
        Arc::clone(&secrets),
    ));
    let renderer = Arc::new(DispatchRenderer::new(
        paths.napcat_config_dir(),
        paths.snowluma_config_dir(),
    ));
    let launch_planner = Arc::new(
        ncd_runtime::FileSystemRuntimeLaunchPlanner::new(paths.components_dir())
            .with_snowluma_runtime_root(paths.snowluma_install_dir())
            .with_snowluma_data_root(paths.snowluma_data_dir()),
    );
    // NativeDeployment 替代旧 LocalRuntimeBackend:通过适配器壳对外仍是 BotBackend
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
        Some(paths.bot_log_dir()),
    ));
    let bot_backend: Arc<dyn ncd_traits::runtime_backend::BotBackend> =
        Arc::new(ncd_runtime::NativeDeploymentBackend::new(
            native_deployment,
            Arc::clone(&local_host),
            "bot-manager-local",
            ncd_domain::kinds::BotFlavor::NapCat,
        ));
    // NapCat WebUI 登录轮询依赖:WebUI client + 离线告警 fan-out(桌面 Toast / Webhook / Email)
    let webui_client: Arc<dyn ncd_runtime::NapCatWebUiClient> = Arc::new(
        ReqwestNapCatWebUiClient::new()
            .expect("初始化 NapCat WebUI HTTP 客户端失败：rustls-tls 构建异常"),
    );
    let app_settings = commands::app_settings::read_app_settings(&data_root);
    // 以 app-settings 为准收敛 HKCU Run(开=刷新路径,关=删本产品值)
    autostart::reconcile_launch_on_startup(app_settings.launch_on_startup);
    let app_settings_shared = Arc::new(RwLock::new(app_settings.clone()));
    let metrics_prefs = {
        let mut p = ncd_runtime::metrics::BotRuntimeMetricsPrefs::from_app(&app_settings);
        p.normalize();
        Arc::new(RwLock::new(p))
    };
    let metrics_collector =
        ncd_runtime::metrics::MetricsCollector::new(data_root.clone(), Arc::clone(&metrics_prefs));
    let lightweight_scheduler = Arc::new(lightweight_scheduler::LightweightScheduler::new(
        Arc::clone(&app_settings_shared),
    ));
    let startup_tray_only = matches!(
        app_settings.ui_mode_on_startup,
        ncd_domain::UiModeOnStartup::TrayOnly
    );
    let poller_settings = Arc::new(RwLock::new(app_settings.poller.clone()));
    let desktop_notify = Arc::new(RwLock::new(app_settings.desktop_notify_flags()));
    let tauri_notifier = desktop_notify::TauriOfflineNotifier::new(Arc::clone(&desktop_notify));
    let webhook_settings = Arc::new(RwLock::new(app_settings.offline_webhook.clone()));
    let email_settings = Arc::new(RwLock::new(app_settings.offline_email.clone()));
    let onebot_settings = Arc::new(RwLock::new(app_settings.offline_onebot.clone()));
    let onebot_resolver = ncd_runtime::SwappableOneBotEndpointResolver::new(Arc::new(
        ncd_runtime::NoopOneBotEndpointResolver,
    ));
    let offline_notifier = ncd_runtime::CompositeOfflineNotifier::new(
        Some(ncd_runtime::DesktopToastSink::new(
            tauri_notifier.clone() as Arc<dyn ncd_runtime::OfflineNotifier>,
            Arc::clone(&desktop_notify),
        )),
        Arc::clone(&poller_settings),
        Arc::clone(&webhook_settings),
        Arc::clone(&email_settings),
        Arc::clone(&onebot_settings),
        onebot_resolver.clone() as Arc<dyn ncd_runtime::OneBotEndpointResolver>,
    );
    let offline_notifier_trait: Arc<dyn ncd_runtime::OfflineNotifier> = offline_notifier.clone();
    // ServerManager 提前构造,既给下面 AppState 用,也给 HostResolver 用(让
    // BotManager 能按 runtime_target 把 bot 启到本机 / 远端)
    // 注入 event_bus,用于发布 HostConnectionLost / Recovered
    let mut server_mgr =
        ncd_runtime::ServerManager::new(&data_root, Arc::new(ncd_runtime::KeyringCredentialStore));
    server_mgr.set_event_bus(Arc::new(event_bus.clone()));
    let server_manager = Arc::new(server_mgr);
    let host_resolver: Arc<dyn ncd_runtime::HostResolver> =
        Arc::new(bot_host_resolver::TauriHostResolver::new(
            Arc::clone(&server_manager),
            Arc::clone(&local_host),
        ));
    let bot_manager = Arc::new(
        BotManager::new(
            repo,
            Arc::clone(&store),
            renderer,
            bot_backend,
            launch_planner,
            Arc::new(event_bus.clone()),
            webui_client,
            offline_notifier_trait,
            poller_settings,
            Arc::clone(&desktop_notify),
        )
        .with_host_resolver(host_resolver)
        .with_docker_webui_secret_store(Arc::clone(&secrets)),
    );

    // SnowLuma daemon + backend wiring
    // 路径来自 DataPaths 布局 v1:
    // - 数据根:state/snowluma
    // - 安装根:components/SnowLuma
    let snowluma_data_root = paths.snowluma_data_dir();
    let snowluma_runtime_root = paths.snowluma_install_dir();
    let snowluma_factory: Arc<dyn ncd_runtime::SnowLumaWebUiClientFactory> =
        Arc::new(ncd_runtime::ReqwestSnowLumaWebUiClientFactory::new());
    let snowluma_daemon = ncd_runtime::SnowLumaDaemon::new(
        snowluma_data_root,
        snowluma_runtime_root,
        Arc::new(event_bus.clone()),
        snowluma_factory,
    );
    let snowluma_backend: Arc<dyn ncd_traits::runtime_backend::BotBackend> =
        Arc::new(ncd_runtime::SnowLumaRuntimeBackend::new(
            ncd_domain::ids::BotId::new("snowluma-backend-local"),
            Arc::clone(&snowluma_daemon),
            Arc::new(event_bus.clone()),
        ));
    let bot_manager = Arc::new(
        Arc::try_unwrap(bot_manager)
            .ok()
            .expect("bot_manager Arc not yet shared")
            .with_snowluma(snowluma_backend, Arc::clone(&snowluma_daemon)),
    );
    // BotManager 就绪后再挂 OneBot messenger 解析
    tauri::async_runtime::block_on(onebot_resolver.set(Arc::new(
        onebot_endpoint_resolver::BotManagerOneBotEndpointResolver::new(Arc::clone(&bot_manager)),
    )));

    let bot_manager_bootstrap = Arc::clone(&bot_manager);
    let data_root_for_bootstrap = data_root.clone();
    let bot_manager_listener = Arc::clone(&bot_manager);
    let bot_manager_login_listener = Arc::clone(&bot_manager);
    let bot_manager_snowluma_listener = Arc::clone(&bot_manager);
    let bot_manager_offline_listener = Arc::clone(&bot_manager);

    let mut builder = tauri::Builder::default();
    #[cfg(desktop)]
    {
        builder = builder.plugin(single_instance::plugin());
    }
    builder
        // 用系统默认浏览器打开外部 URL(例如 NapCat WebUI)
        // webview 自身不支持 target=_blank
        .plugin(tauri_plugin_opener::init())
        // 配置导入导出用原生文件 / 目录选择对话框(webview 无法拿真实文件系统路径)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState {
            data_root,
            snapshot,
            event_bus: event_bus.clone(),
            runtime,
            bot_manager,
            server_manager,
            active_tasks: Arc::new(Mutex::new(HashMap::new())),
            deployment_tasks: ncd_runtime::DeploymentTaskManager::new(event_bus.clone()),
            host_probe_cache: Arc::new(Mutex::new(HashMap::new())),
            desktop_notify: Arc::clone(&desktop_notify),
            app_settings: Arc::clone(&app_settings_shared),
            offline_notifier: Arc::clone(&offline_notifier),
            lightweight_scheduler: Arc::clone(&lightweight_scheduler),
            health_probe_cancel: Arc::new(Mutex::new(None)),
            metrics_collector: metrics_collector.clone(),
            migrate_gate: Arc::new(commands::data_root_migrate::DataRootMigrateGate::default()),
        })
        .setup(move |app| {
            if startup_tray_only {
                if let Err(err) = lightweight::enter_lightweight_mode(app.handle()) {
                    desktop_log::write_session_line(
                        "WARN",
                        "ncd::lightweight",
                        &format!("startup tray_only failed: {err}"),
                    );
                }
            }

            windows_toast::prepare_windows_toast_identity(app.handle());
            tauri_notifier.bind_app(app.handle().clone());
            desktop_notify::spawn_desktop_notify_listener(
                app.handle().clone(),
                event_bus.clone(),
                Arc::clone(&desktop_notify),
            );
            snowluma_offline_listener::spawn_snowluma_offline_listener(
                event_bus.clone(),
                Arc::clone(&offline_notifier),
                bot_manager_offline_listener,
            );
            let handle = app.handle().clone();
            let mut subscription = event_bus.subscribe(EventFilter::all());
            tauri::async_runtime::spawn(async move {
                while let Some(event) = subscription.next().await {
                    let event_name = event.tauri_event_name();
                    // 带顶层 v envelope 序列化(R14):payload 形如 {"v":1,"kind":...}
                    if let Ok(payload) = event.to_envelope_json() {
                        let emit_result = handle.emit(event_name, payload);
                        if let Err(err) = emit_result {
                            desktop_log::write_session_line(
                                "WARN",
                                "ncd::event_emit",
                                &format!("FAILED to emit {event_name}: {err}"),
                            );
                        }
                    } else {
                        desktop_log::write_session_line(
                            "WARN",
                            "ncd::event_emit",
                            &format!("FAILED to serialize event {event_name}"),
                        );
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
            // 订阅必须先于 bootstrap:reconcile attach 会立刻发 napcat_webui_available,
            // broadcast 无 backlog。login listener 在 subscribe 后 oneshot ready,
            // bootstrap 等 ready 再跑,避免多实例真实 port(+1) 丢失。
            // event emit 的 subscribe 在上面已同步完成(spawn 前 register)。
            let (login_ready_tx, login_ready_rx) = tokio::sync::oneshot::channel::<()>();
            let (sl_ready_tx, sl_ready_rx) = tokio::sync::oneshot::channel::<()>();
            tauri::async_runtime::spawn(async move {
                (*bot_manager_listener)
                    .clone()
                    .run_runtime_event_listener()
                    .await;
            });
            tauri::async_runtime::spawn(async move {
                bot_manager_login_listener
                    .run_napcat_login_listener(Some(login_ready_tx))
                    .await;
            });
            tauri::async_runtime::spawn(async move {
                bot_manager_snowluma_listener
                    .run_snowluma_listener(Some(sl_ready_tx))
                    .await;
            });
            tauri::async_runtime::spawn(async move {
                if login_ready_rx.await.is_err() {
                    desktop_log::write_session_line(
                        "WARN",
                        "ncd::bot_manager",
                        "login listener ready signal dropped; bootstrap continues",
                    );
                }
                if sl_ready_rx.await.is_err() {
                    desktop_log::write_session_line(
                        "WARN",
                        "ncd::bot_manager",
                        "snowluma listener ready signal dropped; bootstrap continues",
                    );
                }
                // 未同意当前 Desktop 协议时只恢复 Actor / reconcile，不 auto_start
                let allow_auto_start =
                    !crate::desktop_consent::is_consent_required(&data_root_for_bootstrap);
                if !allow_auto_start {
                    desktop_log::write_session_line(
                        "INFO",
                        "ncd::bot_manager",
                        "desktop consent pending; bootstrap skips auto_start",
                    );
                }
                match bot_manager_bootstrap
                    .bootstrap_with_auto_start(allow_auto_start)
                    .await
                {
                    Ok(result) => {
                        if !result.skipped.is_empty() {
                            desktop_log::write_session_line(
                                "WARN",
                                "ncd::bot_manager",
                                &format!(
                                    "bootstrap skipped {} bot(s) (over limit)",
                                    result.skipped.len()
                                ),
                            );
                        }
                        if !result.started.failed.is_empty() {
                            desktop_log::write_session_line(
                                "WARN",
                                "ncd::bot_manager",
                                &format!(
                                    "bootstrap auto-start failed for {} bot(s)",
                                    result.started.failed.len()
                                ),
                            );
                        }
                    }
                    Err(err) => {
                        desktop_log::write_session_line(
                            "EROR",
                            "ncd::bot_manager",
                            &format!("bootstrap failed: {err}"),
                        );
                    }
                }
            });

            if let Err(err) = commands::window::apply_main_window_startup_geometry(&app.handle()) {
                desktop_log::write_session_line(
                    "WARN",
                    "ncd::window",
                    &format!("startup geometry failed: {err}"),
                );
            }

            if let Err(err) = window_icon::apply_main_window_icon(&app.handle()) {
                desktop_log::write_session_line(
                    "WARN",
                    "ncd::window",
                    &format!("set window icon failed: {err}"),
                );
            }

            if let Err(err) = commands::tray::attach_tray(&app.handle()) {
                desktop_log::write_session_line(
                    "WARN",
                    "ncd::tray",
                    &format!("attach failed: {err}"),
                );
            }
            // 远端 ncd-watch:周期写 desktop_present + 同步 notify.json
            commands::ncd_watch::spawn_ncd_watch_heartbeat(app.handle().clone());
            // 本机实例指标：读 net-stats 并节流写 history（远端 history 由 ncd-watch）
            commands::bot_metrics::spawn_local_metrics_collector(app.handle().clone());
            // 主动探活:启动期根据初始 AppSettings 决定是否 spawn 后台健康 walker
            // 由于 .setup 闭包是 sync 的,这里 spawn 一个一次性 async 任务来做条件判断 + spawn walker
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        let enabled = state
                            .app_settings
                            .read()
                            .await
                            .remote_host_health_probe_enabled;
                        if enabled {
                            let cancel_token = CancellationToken::new();
                            let child = cancel_token.child_token();
                            {
                                let mut guard = state.health_probe_cancel.lock().await;
                                *guard = Some(cancel_token);
                            }
                            let sm = Arc::clone(&state.server_manager);
                            let settings = Arc::clone(&state.app_settings);
                            // 注意:这里 spawn 的 walker 任务会一直跑,直到 cancel
                            tauri::async_runtime::spawn(async move {
                                sm.run_health_probe_loop(settings, child).await;
                            });
                        }
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let close_action = state.app_settings.read().await.close_action.clone();
                    if matches!(close_action, ncd_domain::CloseAction::Tray) {
                        if let Err(err) =
                            commands::tray::hide_main_window_to_tray(app.clone()).await
                        {
                            eprintln!("[window] hide to tray failed: {err}");
                        }
                        return;
                    }
                    let _ = app.emit("desktop-request-close", ());
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
            commands::data_root_migrate::preview_migrate_data_root,
            commands::data_root_migrate::start_migrate_data_root,
            commands::data_root_migrate::cancel_migrate_data_root,
            commands::data_root_migrate::delete_retired_data_root,
            commands::data_root_migrate::restart_after_data_root_migrate,
            commands::desktop_log::open_desktop_log_location,
            commands::desktop_log::tail_desktop_log,
            commands::publish_demo_event,
            commands::publish_runtime_status,
            commands::release::get_release_snapshot,
            commands::app_settings::get_app_settings,
            commands::app_settings::set_app_settings,
            commands::desktop_consent::get_desktop_agreements,
            commands::desktop_consent::accept_desktop_agreements,
            commands::desktop_onboarding::get_desktop_onboarding,
            commands::desktop_onboarding::start_desktop_onboarding,
            commands::desktop_onboarding::skip_desktop_onboarding,
            commands::desktop_onboarding::complete_desktop_onboarding,
            commands::desktop_onboarding::reopen_desktop_onboarding,
            commands::app_settings::sync_close_action_preference,
            commands::app_settings::test_offline_webhook,
            commands::app_settings::test_offline_email,
            commands::app_settings::list_offline_delivery_history,
            commands::app_settings::clear_offline_delivery_history,
            commands::app_settings::list_onebot_messenger_candidates,
            commands::app_settings::ensure_onebot_messenger_http,
            commands::system_metrics::get_system_resource_snapshot,
            commands::config_transfer::export_config,
            commands::config_transfer::import_config,
            commands::config_transfer::preview_config_import,
            commands::components::list_components,
            commands::components::detect_component,
            commands::components::qq_deps::detect_qq_dependencies,
            commands::components::qq_deps::install_qq_dependencies,
            commands::components::qq_deps::remember_sudo_password,
            commands::components::run_component_action,
            commands::components::cancel_component_action,
            commands::desktop_update::check_desktop_update,
            commands::desktop_update::precheck_desktop_update,
            commands::desktop_update::install_desktop_update,
            commands::desktop_update::consume_desktop_update_startup_notice,
            commands::ncd_watch::sync_ncd_watch_notify,
            commands::ncd_watch::touch_ncd_watch_present,
            commands::deployment_tasks::list_deployment_tasks,
            commands::deployment_tasks::cancel_deployment_task,
            commands::deployment_tasks::delete_deployment_task,
            commands::deployment_tasks::clear_finished_deployment_tasks,
            commands::bot::bootstrap_bot_manager,
            commands::bot::list_bot_snapshots,
            commands::bot_metrics::get_bot_runtime_metrics,
            commands::bot_metrics::get_bot_runtime_metrics_history,
            commands::bot_metrics::list_bot_runtime_metrics,
            commands::bot::list_bot_flavors,
            commands::bot::list_napcat_webui_bindings,
            commands::bot::list_snowluma_ui_snapshot,
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
            commands::snowluma::get_snowluma_agreements,
            commands::snowluma::accept_snowluma_agreements,
            commands::snowluma::prepare_snowluma_agreements,
            commands::snowluma::release_snowluma_agreement_session,
            commands::snowluma::open_snowluma_webui,
            commands::snowluma::open_snowluma_novnc,
            commands::servers::list_servers,
            commands::servers::add_server,
            commands::servers::update_server,
            commands::servers::setup_server_key_auth,
            commands::servers::delete_server,
            commands::servers::test_server_connection,
            commands::servers::confirm_server_host_key,
            commands::servers::scan_local_ssh_keys,
            commands::docker::ops::docker_probe,
            commands::docker::install::docker_install,
            commands::docker::ops::docker_list_containers,
            commands::docker::ops::docker_list_images,
            commands::docker::ops::docker_remove_image,
            commands::docker::ops::docker_container_action,
            commands::docker::ops::docker_logs,
            commands::docker::ops::docker_image_ready_for_flavor,
            commands::docker::deploy::docker_deploy,
            commands::docker::ops::docker_compose_down,
            commands::tray::window_show,
            commands::tray::window_hide_to_tray,
            commands::tray::count_local_active_bots,
            commands::exit::prepare_exit_desktop,
            commands::exit::request_exit_app,
            commands::window::show_main_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if lightweight::should_prevent_exit(app) {
                    api.prevent_exit();
                }
            }
        });
}
