//! Tauri 命令：远端主机 ServerProfile CRUD + test_connection。

use tauri::State;

use ncd_runtime::{ProbeReport, ServerProfile};

use crate::AppState;

#[tauri::command]
pub async fn list_servers(state: State<'_, AppState>) -> Result<Vec<ServerProfile>, String> {
    Ok(state.server_manager.list_servers().await)
}

#[tauri::command]
pub async fn add_server(
    state: State<'_, AppState>,
    profile: ServerProfile,
    password: Option<String>,
) -> Result<ServerProfile, String> {
    state.server_manager.add_server(profile, password).await
}

#[tauri::command]
pub async fn update_server(
    state: State<'_, AppState>,
    profile: ServerProfile,
    password: Option<String>,
) -> Result<ServerProfile, String> {
    state.server_manager.update_server(profile, password).await
}

#[tauri::command]
pub async fn delete_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.server_manager.delete_server(&id).await
}

#[tauri::command]
pub async fn test_server_connection(
    state: State<'_, AppState>,
    id: String,
    password: Option<String>,
) -> Result<ProbeReport, String> {
    state.server_manager.test_connection(&id, password).await
}
