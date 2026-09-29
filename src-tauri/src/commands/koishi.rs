//! Koishi 专有的 Tauri commands：薄壳。插件树的保存走通用的 `write_app_instance_config`，
//! 这里只有运行状态、插件表单来源、已装插件包和重启 worker。

use ncd_domain::{AppConfigError, AppInstanceId};
use ncd_runtime::{KoishiPackageInfo, KoishiPluginSchema, KoishiRuntimeStatus};
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
