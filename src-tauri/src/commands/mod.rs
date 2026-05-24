pub mod bot;
pub mod components;
pub mod release;
pub mod snowluma;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ncd_runtime::{BootstrapSnapshot, DomainEvent, EventBus, RemoteFileEntry};
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
) -> Result<Vec<ncd_runtime::BotStatus>, String> {
    Ok(state.runtime.get_all_bot_statuses().await)
}

#[tauri::command]
pub async fn connect_remote_host(
    state: State<'_, AppState>,
    request: ConnectRemoteHostRequest,
) -> Result<RemoteHostConnectionInfo, String> {
    state.runtime.connect_remote_host(request).await
}

#[tauri::command]
pub async fn list_remote_files(
    state: State<'_, AppState>,
    request: ListRemoteFilesRequest,
) -> Result<Vec<RemoteFileEntry>, String> {
    state.runtime.list_remote_files(request).await
}

#[tauri::command]
pub async fn get_remote_runtime_status(
    state: State<'_, AppState>,
    request: GetRemoteRuntimeStatusRequest,
) -> Result<RemoteRuntimeStatusResponse, String> {
    state.runtime.get_remote_runtime_status(request).await
}

#[tauri::command]
pub async fn get_remote_webui_endpoint(
    state: State<'_, AppState>,
    request: GetRemoteWebuiEndpointRequest,
) -> Result<RemoteWebuiEndpointResponse, String> {
    state.runtime.get_remote_webui_endpoint(request).await
}

#[tauri::command]
pub fn open_data_dir(state: State<'_, AppState>) -> Result<PathBuf, String> {
    fs::create_dir_all(&state.data_root).map_err(|err| format!("创建数据目录失败: {err}"))?;
    open_in_file_manager(&state.data_root)?;
    Ok(state.data_root.clone())
}

#[tauri::command]
pub fn export_migration_report(state: State<'_, AppState>) -> Result<PathBuf, String> {
    let export_dir = state.data_root.join("runtime").join("tmp").join("exports");
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
        .publish(DomainEvent::task_progress("p1-demo", 50, "demo event"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_runtime::ConfigStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_test_state(root: &std::path::Path) -> (AppState, ncd_runtime::BroadcastEventBus) {
        let bus = ncd_runtime::BroadcastEventBus::default();
        let runtime = crate::runtime::AppRuntime::new(root, bus.clone());
        let store = Arc::new(ncd_runtime::LocalConfigStore::new(root));
        let secrets: Arc<dyn ncd_runtime::SecretStore + Send + Sync> =
            Arc::new(ncd_runtime::SecretStoreImpl::new(root.join("secrets")));
        let repo = Arc::new(ncd_runtime::LocalBotConfigRepo::new(
            Arc::clone(&store),
            secrets,
        ));
        let renderer = Arc::new(ncd_runtime::DispatchRenderer::new(store.config_dir()));
        let backend = Arc::new(ncd_runtime::LocalRuntimeBackend::new(root, "test-local"));
        let launch_planner = Arc::new(ncd_runtime::FileSystemRuntimeLaunchPlanner::new(
            root.join("runtime"),
        ));
        let webui_client: Arc<dyn ncd_runtime::NapCatWebUiClient> =
            Arc::new(ncd_runtime::ReqwestNapCatWebUiClient::new().expect("init webui client"));
        let offline_notifier: Arc<dyn ncd_runtime::OfflineNotifier> =
            Arc::new(ncd_runtime::NoopOfflineNotifier);
        let poller_settings = Arc::new(tokio::sync::RwLock::new(
            ncd_runtime::WebUiPollerSettings::default(),
        ));
        let bot_manager = Arc::new(ncd_runtime::BotManager::new(
            repo,
            Arc::clone(&store),
            renderer,
            backend,
            launch_planner,
            Arc::new(bus.clone()),
            webui_client,
            offline_notifier,
            poller_settings,
        ));
        let state = AppState {
            data_root: root.to_path_buf(),
            snapshot: BootstrapSnapshot::ready(),
            event_bus: bus.clone(),
            runtime,
            bot_manager,
            active_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        };
        (state, bus)
    }

    #[tokio::test]
    async fn publish_runtime_status_emits_events() {
        let root = tempdir().unwrap();
        let (state, bus) = make_test_state(root.path());
        let mut subscription = bus.subscribe(ncd_runtime::EventFilter::kind(
            ncd_runtime::DomainEventKind::BotStatusChanged,
        ));

        state
            .runtime
            .record_external_status_for_test(ncd_runtime::BotStatus::running("10001", 42, 1))
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

    #[tokio::test]
    async fn remote_commands_use_registered_remote_service() {
        let root = tempdir().unwrap();
        let bus = ncd_runtime::BroadcastEventBus::default();
        let runtime = crate::runtime::AppRuntime::new(root.path(), bus);

        let connection = runtime
            .connect_remote_host(ConnectRemoteHostRequest {
                remote_id: "remote-a".to_string(),
                host: "127.0.0.1".to_string(),
                port: 22,
                username: "napcat".to_string(),
                password: None,
                webui_url: Some("http://127.0.0.1:3000".to_string()),
            })
            .await
            .unwrap();
        assert_eq!(connection.remote_id, "remote-a");

        let files = runtime
            .list_remote_files(ListRemoteFilesRequest {
                remote_id: "remote-a".to_string(),
                path: "/etc".to_string(),
            })
            .await
            .unwrap();
        assert!(files.is_empty());

        let status = runtime
            .get_remote_runtime_status(GetRemoteRuntimeStatusRequest {
                remote_id: "remote-a".to_string(),
                bot_id: "20001".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(status.remote_id, "remote-a");
        assert_eq!(status.bot_id, "20001");

        let webui = runtime
            .get_remote_webui_endpoint(GetRemoteWebuiEndpointRequest {
                remote_id: "remote-a".to_string(),
                bot_id: "20001".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(webui.remote_id, "remote-a");
        assert_eq!(webui.webui_url.as_deref(), Some("http://127.0.0.1:3000"));
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
