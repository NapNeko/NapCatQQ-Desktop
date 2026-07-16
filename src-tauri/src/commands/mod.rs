pub mod app_settings;
pub mod bot;
pub mod bot_metrics;
pub mod components;
pub mod config_transfer;
pub mod deployment_tasks;
pub mod desktop_consent;
pub mod desktop_log;
pub mod desktop_onboarding;
pub mod desktop_update;
pub mod docker;
pub mod exit;
pub mod host_resolve;
pub mod ncd_watch;
pub mod release;
pub mod servers;
pub mod snowluma;
pub mod system_metrics;
pub mod tray;
pub mod window;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ncd_domain::BootstrapSnapshot;
use ncd_runtime::{DomainEvent, EventBus};
use tauri::State;

use crate::AppState;
use crate::runtime::{
    ConnectRemoteHostRequest, GetRemoteRuntimeStatusRequest, GetRemoteWebuiEndpointRequest,
    ListRemoteFilesRequest, RemoteHostConnectionInfo, RemoteRuntimeStatusResponse,
    RemoteWebuiEndpointResponse,
};

#[tauri::command]
pub fn get_bootstrap_status(state: State<'_, AppState>) -> BootstrapSnapshot {
    state.snapshot.clone()
}

#[tauri::command]
pub async fn get_all_bot_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<ncd_domain::bot_status::BotStatus>, String> {
    Ok(state.runtime.get_all_bot_statuses().await)
}

#[tauri::command]
pub async fn connect_remote_host(
    state: State<'_, AppState>,
    request: ConnectRemoteHostRequest,
) -> Result<RemoteHostConnectionInfo, String> {
    // 兼容旧前端 contract:把 connect 请求翻译成 ServerManager add + test
    let profile = ncd_runtime::ServerProfile {
        id: request.remote_id.clone(),
        name: request.remote_id.clone(),
        host: request.host.clone(),
        port: if request.port == 0 { 22 } else { request.port },
        username: request.username.clone(),
        auth_method: if request.password.is_some() {
            ncd_runtime::AuthMethod::Password
        } else {
            ncd_runtime::AuthMethod::Key
        },
        private_key_path: None,
        remember_credential: request.password.is_some(),
        state: ncd_runtime::ServerState::Disconnected,
        health: None,
        webui_url: request.webui_url.clone(),
    };

    // 如果已存在就 update,不存在就 add
    let existing = state.server_manager.list_servers().await;
    if existing.iter().any(|p| p.id == request.remote_id) {
        state
            .server_manager
            .update_server(profile.clone(), request.password.clone())
            .await?;
    } else {
        state
            .server_manager
            .add_server(profile.clone(), request.password.clone())
            .await?;
    }

    // test_connection 会真正建立 SSH 连接并缓存
    let _report = state
        .server_manager
        .test_connection(&request.remote_id, request.password, true)
        .await?;

    Ok(RemoteHostConnectionInfo {
        remote_id: profile.id,
        host: profile.host,
        port: profile.port,
        username: profile.username,
        webui_url: profile.webui_url,
    })
}

#[tauri::command]
pub async fn list_remote_files(
    state: State<'_, AppState>,
    request: ListRemoteFilesRequest,
) -> Result<Vec<ncd_host::DirEntry>, String> {
    // 改用 ensure_connected:未命中会自动尝试建立/恢复连接(单飞 + 冷却)
    // 这样能避免拿到一个已死的缓存 host 导致后续 list_dir 立即失败
    // 失败时仍返回高层文案“远端主机未连接”,不泄露底层 SSH 细节
    let host = state
        .server_manager
        .ensure_connected(&request.remote_id)
        .await
        .map_err(|_| format!("远端主机未连接: {}", request.remote_id))?;
    let path = ncd_host::HostPath::from_posix(&request.path);
    host.list_dir(&path).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_remote_runtime_status(
    state: State<'_, AppState>,
    request: GetRemoteRuntimeStatusRequest,
) -> Result<RemoteRuntimeStatusResponse, String> {
    // 改用 ensure_connected 而非 get_host:
    // - 保证返回的 host 在此刻是可达的(死缓存会被 ensure 内部自动重连或报错)
    // - 失败时仍用“远端主机未连接”文案对齐旧行为
    // - 成功后 pgrep 失败只说明 bot 进程不在,不应被误判为“主机未连接”
    let host = state
        .server_manager
        .ensure_connected(&request.remote_id)
        .await
        .map_err(|_| format!("远端主机未连接: {}", request.remote_id))?;

    let bot_id = ncd_domain::ids::BotId::new(&request.bot_id);
    let cmd = ncd_host::HostCommand::new("pgrep")
        .arg("-f")
        .arg(format!("napcat.*{}", request.bot_id));
    let output = host.run_to_string(cmd).await;
    let status = match output {
        Ok(out) if out.success() && !out.stdout.trim().is_empty() => {
            let pid: u32 = out
                .stdout
                .trim()
                .lines()
                .next()
                .and_then(|l| l.trim().parse().ok())
                .unwrap_or(0);
            ncd_domain::bot_status::BotStatus::running(bot_id.clone(), pid, 0)
        }
        _ => ncd_domain::bot_status::BotStatus::stopped(bot_id.clone()),
    };

    Ok(RemoteRuntimeStatusResponse {
        remote_id: request.remote_id.clone(),
        bot_id: request.bot_id,
        status,
        backend_kind: Some(ncd_domain::kinds::BackendKind::RemoteSsh),
        runtime_target: Some(ncd_domain::kinds::RuntimeTarget::server(request.remote_id)),
    })
}

#[tauri::command]
pub async fn get_remote_webui_endpoint(
    state: State<'_, AppState>,
    request: GetRemoteWebuiEndpointRequest,
) -> Result<RemoteWebuiEndpointResponse, String> {
    // 从 ServerProfile 读 webui_url
    let servers = state.server_manager.list_servers().await;
    let webui_url = servers
        .iter()
        .find(|p| p.id == request.remote_id)
        .and_then(|p| p.webui_url.clone());

    Ok(RemoteWebuiEndpointResponse {
        remote_id: request.remote_id,
        bot_id: request.bot_id,
        webui_url,
    })
}

#[tauri::command]
pub fn open_data_dir(state: State<'_, AppState>) -> Result<PathBuf, String> {
    fs::create_dir_all(&state.data_root).map_err(|err| format!("创建数据目录失败: {err}"))?;
    open_in_file_manager(&state.data_root)?;
    Ok(state.data_root.clone())
}

#[tauri::command]
pub fn export_migration_report(state: State<'_, AppState>) -> Result<PathBuf, String> {
    let export_dir = ncd_runtime::DataPaths::new(&state.data_root).export_dir();
    fs::create_dir_all(&export_dir).map_err(|err| format!("创建导出目录失败: {err}"))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let export_path = export_dir.join(format!("migration-report-{stamp}.json"));
    let payload = serde_json::to_vec_pretty(&state.snapshot.report)
        .map_err(|err| format!("序列化迁移报告失败: {err}"))?;
    fs::write(&export_path, payload).map_err(|err| format!("写出迁移报告失败: {err}"))?;

    Ok(export_path)
}

#[tauri::command]
pub async fn publish_runtime_status(state: State<'_, AppState>) -> Result<(), String> {
    state.runtime.publish_runtime_statuses().await;
    Ok(())
}

#[tauri::command]
pub fn publish_demo_event(state: State<'_, AppState>) -> Result<(), String> {
    state
        .event_bus
        .publish(DomainEvent::task_progress("demo-event", 50, "demo event"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ncd_domain::{
        BackendKind, BootstrapSnapshot, BotFlavor, BotId, BotStatus, DesktopNotifySettings,
        StopMode, WebUiPollerSettings,
    };
    use ncd_runtime::{
        BotManager, BroadcastEventBus, DispatchRenderer, EventBus, EventFilter,
        FileSystemRuntimeLaunchPlanner, InMemoryCredentialStore, LocalBotConfigRepo,
        LocalConfigStore, NoopOfflineNotifier, ReqwestNapCatWebUiClient, SecretStoreImpl,
        ServerManager,
    };
    use ncd_traits::{
        ConfigStore, SecretStore,
        runtime_backend::{
            BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LogSnapshot, TailOpts,
        },
    };
    use std::sync::Arc;
    use tempfile::tempdir;

    struct FakeBackend;

    #[async_trait]
    impl BotBackend for FakeBackend {
        fn id(&self) -> &BotId {
            static ID: std::sync::OnceLock<BotId> = std::sync::OnceLock::new();
            ID.get_or_init(|| BotId::new("fake-backend"))
        }
        fn kind(&self) -> BackendKind {
            BackendKind::Local
        }
        fn flavor(&self) -> BotFlavor {
            BotFlavor::NapCat
        }
        async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
            Ok(BotStatus::running(ctx.config.bot_id.clone(), 1, 1))
        }
        async fn stop(&self, _bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
            Ok(())
        }
        async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
            Ok(BotStatus::stopped(bot_id))
        }
        async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
            Err(BotBackendError::ConfigNotFound(bot_id))
        }
        async fn write_config(
            &self,
            _bot_id: BotId,
            _cfg: &BotRuntimeConfig,
        ) -> Result<(), BotBackendError> {
            Ok(())
        }
        async fn tail_log(
            &self,
            _bot_id: BotId,
            _opts: TailOpts,
        ) -> Result<LogSnapshot, BotBackendError> {
            Ok(LogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            })
        }
    }

    fn make_test_state(root: &std::path::Path) -> (AppState, BroadcastEventBus) {
        let bus = BroadcastEventBus::default();
        let runtime = crate::runtime::AppRuntime::new(root, bus.clone());
        let store = Arc::new(LocalConfigStore::new(root));
        let secrets: Arc<dyn SecretStore + Send + Sync> =
            Arc::new(SecretStoreImpl::new(root.join("secrets")));
        let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
        let renderer = Arc::new(DispatchRenderer::new(
            store.config_dir(),
            store.config_dir(),
        ));
        let backend: Arc<dyn BotBackend> = Arc::new(FakeBackend);
        // 注入假 QQ 安装目录,CI 无本机 QQ 注册表时仍能测到 NapCat 缺组件
        let qq_install = root.join("fake-qqnt");
        let _ = std::fs::create_dir_all(&qq_install);
        let _ = std::fs::write(qq_install.join("QQ.exe"), b"");
        let launch_planner = Arc::new(
            FileSystemRuntimeLaunchPlanner::new(ncd_runtime::DataPaths::new(root).components_dir())
                .with_qq_install_path(qq_install),
        );
        let webui_client: Arc<dyn ncd_runtime::NapCatWebUiClient> =
            Arc::new(ReqwestNapCatWebUiClient::new().expect("init webui client"));
        let offline_notifier: Arc<dyn ncd_runtime::OfflineNotifier> = Arc::new(NoopOfflineNotifier);
        let poller_settings = Arc::new(tokio::sync::RwLock::new(WebUiPollerSettings::default()));
        let desktop_notify = Arc::new(tokio::sync::RwLock::new(DesktopNotifySettings::default()));
        let app_settings = Arc::new(tokio::sync::RwLock::new(ncd_domain::AppSettings::default()));
        let composite_offline = ncd_runtime::CompositeOfflineNotifier::new(
            None,
            Arc::clone(&poller_settings),
            Arc::new(tokio::sync::RwLock::new(
                ncd_domain::OfflineWebhookSettings::default(),
            )),
            Arc::new(tokio::sync::RwLock::new(
                ncd_domain::OfflineEmailSettings::default(),
            )),
            Arc::new(tokio::sync::RwLock::new(
                ncd_domain::OfflineOneBotSettings::default(),
            )),
            Arc::new(ncd_runtime::NoopOneBotEndpointResolver),
        );
        let lightweight_scheduler = Arc::new(
            crate::lightweight_scheduler::LightweightScheduler::new(Arc::clone(&app_settings)),
        );
        let metrics_prefs = {
            let mut p = ncd_runtime::metrics::BotRuntimeMetricsPrefs::default();
            p.normalize();
            Arc::new(tokio::sync::RwLock::new(p))
        };
        let metrics_collector =
            ncd_runtime::metrics::MetricsCollector::new(root, Arc::clone(&metrics_prefs));
        let bot_manager = Arc::new(BotManager::new(
            repo,
            Arc::clone(&store),
            renderer,
            backend,
            launch_planner,
            Arc::new(bus.clone()),
            webui_client,
            offline_notifier,
            poller_settings,
            Arc::clone(&desktop_notify),
        ));
        let state = AppState {
            data_root: root.to_path_buf(),
            snapshot: BootstrapSnapshot::ready(),
            event_bus: bus.clone(),
            runtime,
            bot_manager,
            server_manager: Arc::new(ServerManager::new(
                root,
                Arc::new(InMemoryCredentialStore::default()),
            )),
            active_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            deployment_tasks: ncd_runtime::DeploymentTaskManager::new(bus.clone()),
            host_probe_cache: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            desktop_notify: Arc::clone(&desktop_notify),
            app_settings: Arc::clone(&app_settings),
            offline_notifier: composite_offline,
            lightweight_scheduler: Arc::clone(&lightweight_scheduler),
            health_probe_cancel: Arc::new(tokio::sync::Mutex::new(None)),
            metrics_collector,
        };
        (state, bus)
    }

    #[tokio::test]
    async fn publish_runtime_status_emits_events() {
        let root = tempdir().unwrap();
        let (state, bus) = make_test_state(root.path());
        let mut subscription = bus.subscribe(EventFilter::kind(
            ncd_domain::domain_event::DomainEventKind::BotStatusChanged,
        ));

        state
            .runtime
            .record_external_status_for_test(BotStatus::running("10001", 42, 1))
            .await;

        state.runtime.publish_runtime_statuses().await;
        let event = subscription.next().await.expect("expected status event");
        match event {
            DomainEvent::BotStatusChanged { status, .. } => {
                assert_eq!(status.bot_id.as_str(), "10001");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}

fn open_in_file_manager(path: &Path) -> Result<(), String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("explorer");
        cmd.arg(path);
        cmd
    } else if cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        cmd.arg(path);
        cmd
    } else {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(path);
        cmd
    };

    let status = command
        .status()
        .map_err(|err| format!("打开数据目录失败: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("文件管理器退出失败: {status}"))
    }
}
