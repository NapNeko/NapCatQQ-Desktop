//! 应用端框架 Tauri commands：薄壳，只做参数转换 + 错误转 String。
//! 业务在 `ncd_runtime::AppManager`，安装、排插件任务、开 WebUI 隧道都在那边；
//! 远端主机没连上时由它的主机解析器现连，这里不用预热。

use ncd_domain::{
    AppConfigDocument, AppConfigError, AppConfigText, AppFrameworkId, AppFrameworkManifest,
    AppInstance, AppInstanceId, AppInstanceWebUi, AppPendingTerms, AppProjectProbe,
    AppWebUiAccount, BotId, CreateAppInstanceRequest, ImportAppInstanceRequest, OneBotLinkPlan,
};
use ncd_runtime::PackageVersions;
use ncd_traits::AppFrameworkError;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn list_app_frameworks(state: State<'_, AppState>) -> Vec<AppFrameworkManifest> {
    state.app_manager.list_frameworks()
}

#[tauri::command]
pub async fn list_app_instances(state: State<'_, AppState>) -> Result<Vec<AppInstance>, String> {
    Ok(state.app_manager.list_instances().await)
}

#[tauri::command]
pub async fn list_app_config_documents(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AppConfigDocument>, AppConfigError> {
    state
        .app_manager
        .list_config_documents(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn read_app_config_text(
    instance_id: String,
    doc_id: String,
    state: State<'_, AppState>,
) -> Result<AppConfigText, AppConfigError> {
    state
        .app_manager
        .read_config_text(&AppInstanceId::new(instance_id), &doc_id)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn write_app_config_text(
    instance_id: String,
    doc_id: String,
    text: String,
    base_revision: Option<String>,
    state: State<'_, AppState>,
) -> Result<AppConfigText, AppConfigError> {
    state
        .app_manager
        .write_config_text(
            &AppInstanceId::new(instance_id),
            &doc_id,
            &text,
            base_revision,
        )
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn open_app_instance_directory(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let instance = state
        .app_manager
        .get_instance(&AppInstanceId::new(instance_id))
        .await
        .map_err(|error| error.to_string())?;
    if instance.host_id != "local" || instance.placement != ncd_domain::AppPlacement::LocalNative {
        return Err("只能打开本机目录".into());
    }
    let host = super::host_resolve::local_host()?;
    let directory = ncd_host::HostPath::from_posix(&instance.install_dir).render_for(host.os());
    super::open_local_directory(std::path::Path::new(&directory)).await
}

#[tauri::command]
pub async fn create_app_instance(
    request: CreateAppInstanceRequest,
    state: State<'_, AppState>,
) -> Result<AppInstance, String> {
    state
        .app_manager
        .create_instance(request)
        .await
        .map_err(|e| e.to_string())
}

/// 某框架支持的可安装版本；None = 该框架不支持按版本安装（UI 隐藏选择器）
#[tauri::command]
pub async fn list_app_framework_versions(
    framework_id: String,
    state: State<'_, AppState>,
) -> Result<Option<PackageVersions>, String> {
    state
        .app_manager
        .available_versions(&AppFrameworkId::new(framework_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn probe_app_project(
    host_id: String,
    framework_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<AppProjectProbe, String> {
    state
        .app_manager
        .probe_project(&host_id, &AppFrameworkId::new(framework_id), &path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_app_instance(
    request: ImportAppInstanceRequest,
    state: State<'_, AppState>,
) -> Result<AppInstance, String> {
    state
        .app_manager
        .import_instance(request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_app_install_dir(
    host_id: String,
    framework_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .app_manager
        .preview_install_dir(&host_id, &AppFrameworkId::new(framework_id))
        .await
        .map(|p| p.as_posix().to_string())
        .map_err(|e| e.to_string())
}

/// 安装 / 重装：按实例目录提交应用端组件任务（缺的依赖会一起排），返回 task id
#[tauri::command]
pub async fn install_app_instance(
    instance_id: String,
    task_id: Option<String>,
    // 指定安装的版本；不传 = 最新正式版
    version: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .app_manager
        .install_instance(&AppInstanceId::new(instance_id), task_id, version)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh_app_instance(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<AppInstance, String> {
    state
        .app_manager
        .refresh_instance(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tail_app_instance_log(
    instance_id: String,
    lines: Option<usize>,
    state: State<'_, AppState>,
) -> Result<ncd_traits::runtime_backend::LogSnapshot, String> {
    state
        .app_manager
        .tail_log(&AppInstanceId::new(instance_id), lines.unwrap_or(1000))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_app_instance(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<AppInstance, String> {
    state
        .app_manager
        .start_instance(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn app_pending_terms(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AppPendingTerms>, String> {
    state
        .app_manager
        .pending_terms(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn accept_app_terms(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .app_manager
        .accept_terms(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_app_instance(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<AppInstance, String> {
    state
        .app_manager
        .stop_instance(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_app_instance(
    instance_id: String,
    remove_files: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .app_manager
        .delete_instance(&AppInstanceId::new(instance_id), remove_files)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_app_link(
    instance_id: String,
    bot_id: String,
    state: State<'_, AppState>,
) -> Result<OneBotLinkPlan, String> {
    state
        .app_manager
        .preview_link(&AppInstanceId::new(instance_id), &BotId::new(bot_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_app_link(
    instance_id: String,
    bot_id: String,
    state: State<'_, AppState>,
) -> Result<AppInstance, String> {
    state
        .app_manager
        .apply_link(&AppInstanceId::new(instance_id), &BotId::new(bot_id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unlink_app_instance(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<AppInstance, String> {
    state
        .app_manager
        .unlink(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())
}

/// 测试用：本机-only 的 AppManager（空实例表 + LocalOnlyHostResolver）
#[cfg(test)]
pub(crate) fn test_app_manager(
    root: &std::path::Path,
    bus: &ncd_runtime::BroadcastEventBus,
    bot_manager: std::sync::Arc<dyn ncd_runtime::BotConfigPort>,
) -> std::sync::Arc<ncd_runtime::AppManager> {
    use std::sync::Arc;
    let app_store = Arc::new(ncd_runtime::AppInstanceStore::empty(root));
    let local_host: Arc<dyn ncd_host::Host> = ncd_host::local::local_host();
    Arc::new(ncd_runtime::AppManager::new(
        Arc::new(ncd_runtime::AppFrameworkRegistry::with_builtin()),
        Arc::clone(&app_store),
        Arc::new(ncd_runtime::NativeAppRuntime::new(
            Arc::new(bus.clone()),
            app_store,
        )),
        Arc::new(ncd_runtime::LocalOnlyHostResolver::new(local_host)),
        bot_manager,
        Arc::new(bus.clone()),
        root,
    ))
}

/// WebUI 地址（前端用 opener 打开）：一律本机 loopback；远端经 SSH 隧道。
#[tauri::command]
pub async fn get_app_instance_webui(
    instance_id: String,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<AppInstanceWebUi, String> {
    state
        .app_manager
        .open_webui(&AppInstanceId::new(instance_id), path.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// 只看账号不开隧道：实例停着时从详情页查看 / 重置密码用。
#[tauri::command]
pub async fn get_app_instance_webui_account(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Option<AppWebUiAccount>, String> {
    let instance = state
        .app_manager
        .get_instance(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())?;
    Ok(state.app_manager.webui_account(&instance).await)
}

/// 重置账号密码类 WebUI 的密码（`password` 空 = 随机生成）；实例须已停止。
#[tauri::command]
pub async fn reset_app_instance_webui_password(
    instance_id: String,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<AppWebUiAccount, AppConfigError> {
    state
        .app_manager
        .reset_webui_password(&AppInstanceId::new(instance_id), password)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

/// 修改实例的开机自启设置
#[tauri::command]
pub async fn set_app_instance_auto_start(
    instance_id: String,
    auto_start: bool,
    state: State<'_, AppState>,
) -> Result<AppInstance, String> {
    state
        .app_manager
        .set_instance_auto_start(&AppInstanceId::new(instance_id), auto_start)
        .await
        .map_err(|e| e.to_string())
}
