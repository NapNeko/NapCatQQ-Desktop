//! Koishi 专有的 Tauri commands：薄壳。插件树的保存走通用的 `write_app_instance_config`，
//! 这里只有运行状态、插件表单来源、已装插件包和重启 worker。

use ncd_domain::{AppConfigError, AppInstanceId};
use ncd_runtime::{
    KoishiCommandRow, KoishiDatabaseTable, KoishiFileContent, KoishiFileEntry, KoishiPackageInfo,
    KoishiPluginSchema, KoishiRuntimeStatus, KoishiSandboxMessage,
};
use ncd_traits::AppFrameworkError;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn koishi_status(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<KoishiRuntimeStatus, AppConfigError> {
    state
        .app_manager
        .koishi_status(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_plugin_schemas(
    instance_id: String,
    names: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<KoishiPluginSchema>, AppConfigError> {
    state
        .app_manager
        .koishi_plugin_schemas(&AppInstanceId::new(instance_id), names)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_packages(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<KoishiPackageInfo>, AppConfigError> {
    state
        .app_manager
        .koishi_packages(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_restart(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .koishi_restart(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

// ---------------------------------------------------------------------------
// 控制台功能（沙盒试聊 / 文件 / 数据库 / 指令），都走控制台 WebSocket
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn koishi_sandbox_send(
    instance_id: String,
    platform: String,
    user: String,
    channel: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .koishi_sandbox_send(
            &AppInstanceId::new(instance_id),
            &platform,
            &user,
            &channel,
            &content,
        )
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_sandbox_messages(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<KoishiSandboxMessage>, AppConfigError> {
    state
        .app_manager
        .koishi_sandbox_messages(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_explorer_tree(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<KoishiFileEntry>, AppConfigError> {
    state
        .app_manager
        .koishi_explorer_tree(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_explorer_read(
    instance_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<KoishiFileContent, AppConfigError> {
    state
        .app_manager
        .koishi_explorer_read(&AppInstanceId::new(instance_id), &path)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_explorer_write(
    instance_id: String,
    path: String,
    content: String,
    binary: bool,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .koishi_explorer_write(&AppInstanceId::new(instance_id), &path, &content, binary)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_explorer_mkdir(
    instance_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .koishi_explorer_mkdir(&AppInstanceId::new(instance_id), &path)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_explorer_remove(
    instance_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .koishi_explorer_remove(&AppInstanceId::new(instance_id), &path)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_explorer_rename(
    instance_id: String,
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .koishi_explorer_rename(&AppInstanceId::new(instance_id), &from, &to)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_database_tables(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<KoishiDatabaseTable>, AppConfigError> {
    state
        .app_manager
        .koishi_database_tables(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_database_rows(
    instance_id: String,
    table: String,
    offset: u64,
    limit: u64,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, AppConfigError> {
    state
        .app_manager
        .koishi_database_rows(&AppInstanceId::new(instance_id), &table, offset, limit)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_commands(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<KoishiCommandRow>, AppConfigError> {
    state
        .app_manager
        .koishi_commands(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_command_update(
    instance_id: String,
    name: String,
    config: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .koishi_command_update(&AppInstanceId::new(instance_id), &name, config)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn koishi_command_aliases(
    instance_id: String,
    name: String,
    aliases: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .koishi_command_aliases(&AppInstanceId::new(instance_id), &name, aliases)
        .await
        .map_err(AppFrameworkError::into_config_error)
}
