//! Tauri 命令：远端主机 ServerProfile CRUD + test_connection。
//!
//! 这层只管"档案"——服务器列表、连接测试、连接缓存。组件部署走 components.rs
//! 的 run_component_action（host_id = "remote:<server_id>"）。

use tauri::State;

use ncd_runtime::{ProbeReport, ServerProfile};

use crate::AppState;

#[tauri::command]
pub async fn list_servers(state: State<'_, AppState>) -> Result<Vec<ServerProfile>, String> {
    Ok(state.server_manager.list_servers().await)
}

#[tauri::command]
pub async fn test_server_connection(
    state: State<'_, AppState>,
    id: String,
    password: Option<String>,
) -> Result<ProbeReport, String> {
    state.server_manager.test_connection(&id, password).await
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

/// 扫描本地 ~/.ssh/ 下的标准命名私钥（id_ed25519 / id_ecdsa / id_rsa / id_dsa）。
/// 仅返回已存在的文件绝对路径，按现代算法优先级排序。
/// 不读取私钥内容，纯路径枚举——给 UI 做候选项下拉用。
#[tauri::command]
pub async fn scan_local_ssh_keys() -> Result<Vec<String>, String> {
    const STANDARD_KEY_NAMES: &[&str] = &["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"];

    let ssh_dir = match dirs::home_dir() {
        Some(home) => home.join(".ssh"),
        None => return Ok(Vec::new()),
    };
    if !ssh_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for name in STANDARD_KEY_NAMES {
        let candidate = ssh_dir.join(name);
        if candidate.is_file() {
            found.push(candidate.to_string_lossy().into_owned());
        }
    }
    Ok(found)
}
