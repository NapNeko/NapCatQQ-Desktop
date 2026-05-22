pub mod bot;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ncd_core::{BootstrapSnapshot, DomainEvent, EventBus, RemoteFileEntry};
use tauri::State;

use crate::AppState;
use crate::runtime::{
    ConnectRemoteHostRequest, GetRemoteRuntimeStatusRequest, GetRemoteWebuiEndpointRequest,
    ListRemoteFilesRequest, RemoteHostConnectionInfo, RemoteRuntimeStatusResponse,
    RemoteWebuiEndpointResponse, SpawnLocalBotRequest, StopLocalBotRequest,
};

#[tauri::command]
pub fn get_bootstrap_status(state: State<'_, AppState>) -> BootstrapSnapshot {
    state.snapshot.clone()
}

#[tauri::command]
pub async fn spawn_local_bot(
    state: State<'_, AppState>,
    request: SpawnLocalBotRequest,
) -> Result<ncd_core::BotStatus, String> {
    state.runtime.spawn_local_bot(request).await
}

#[tauri::command]
pub async fn stop_local_bot(
    state: State<'_, AppState>,
    request: StopLocalBotRequest,
) -> Result<(), String> {
    state.runtime.stop_local_bot(request).await
}

#[tauri::command]
pub async fn get_all_bot_statuses(
    state: State<'_, AppState>,
) -> Result<Vec<ncd_core::BotStatus>, String> {
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
    use crate::runtime::{SpawnLocalBotRequest, StopLocalBotRequest};
    use ncd_core::ConfigStore;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_test_state(root: &std::path::Path) -> (AppState, ncd_core::BroadcastEventBus) {
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = crate::runtime::AppRuntime::new(root, bus.clone());
        let store = Arc::new(ncd_core::LocalConfigStore::new(root));
        let secrets: Arc<dyn ncd_core::SecretStore + Send + Sync> =
            Arc::new(ncd_core::SecretStoreImpl::new(root.join("secrets")));
        let repo = Arc::new(ncd_core::LocalBotConfigRepo::new(
            Arc::clone(&store),
            secrets,
        ));
        let renderer = Arc::new(ncd_core::DispatchRenderer::new(store.config_dir()));
        let backend = Arc::new(ncd_core::LocalRuntimeBackend::new(root, "test-local"));
        let launch_planner = Arc::new(ncd_core::FileSystemRuntimeLaunchPlanner::new(
            root.join("runtime"),
        ));
        let bot_manager = Arc::new(ncd_core::BotManager::new(
            repo,
            Arc::clone(&store),
            renderer,
            backend,
            launch_planner,
            Arc::new(bus.clone()),
        ));
        let state = AppState {
            data_root: root.to_path_buf(),
            snapshot: BootstrapSnapshot::ready(),
            event_bus: bus.clone(),
            runtime,
            bot_manager,
        };
        (state, bus)
    }

    #[tokio::test]
    async fn publish_runtime_status_emits_events() {
        let root = tempdir().unwrap();
        let (state, bus) = make_test_state(root.path());
        let mut subscription = bus.subscribe(ncd_core::EventFilter::kind(
            ncd_core::DomainEventKind::BotStatusChanged,
        ));

        state
            .runtime
            .spawn_local_bot(SpawnLocalBotRequest {
                bot_id: "10001".to_string(),
                flavor: ncd_core::BotFlavor::NapCat,
                launch_command: if cfg!(windows) {
                    vec![
                        "timeout".to_string(),
                        "/T".to_string(),
                        "2".to_string(),
                        "/NOBREAK".to_string(),
                    ]
                } else {
                    vec!["sleep".to_string(), "2".to_string()]
                },
                working_dir: None,
                environment: std::collections::BTreeMap::new(),
            })
            .await
            .unwrap();

        state.runtime.publish_runtime_statuses().await;
        let event = subscription.next().await.expect("expected status event");
        match event {
            DomainEvent::BotStatusChanged { status, .. } => {
                assert_eq!(status.bot_id.as_str(), "10001");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        state
            .runtime
            .stop_local_bot(StopLocalBotRequest {
                bot_id: "10001".to_string(),
                mode: ncd_core::StopMode::Force,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn remote_commands_use_registered_remote_service() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
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
