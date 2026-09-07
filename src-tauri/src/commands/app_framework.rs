//! 应用端框架 Tauri commands：薄壳，只做参数转换 + 错误转 String。
//! 业务在 `ncd_runtime::AppManager`；安装走既有 ComponentExecutor（R12）。

use ncd_component::ComponentId;
use ncd_deploy::StepKind;
use ncd_domain::{
    AppConfigDocument, AppConfigIssue, AppConfigText, AppFrameworkManifest, AppInstance,
    AppInstanceId, BotId, CreateAppInstanceRequest, OneBotLinkPlan,
};
use ncd_runtime::{
    AppConfigWriteResult, AppInstanceConfig, AppInstanceConfigEnvelope, ComponentActionRequest,
};
use ncd_traits::AppFrameworkError;
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::AppState;
use crate::commands::components::{build_inputs, cached_host_probe, executor};
use crate::commands::host_resolve::{host_display_address, resolve_host_with_autoconnect};

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct AppInstanceWebUi {
    pub url: String,
    /// Karin `HTTP_AUTH_KEY`；空则前端不写剪贴板。
    pub auth_key: String,
}

/// 配置读写命令的结构化错误：前端按 `kind` 分流（冲突 → 重载/覆盖对话框；校验 → 定位字段）。
/// 其它命令仍返回 String，这里只在需要分流的地方升级。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub enum AppConfigErrorKind {
    Conflict,
    Invalid,
    Unsupported,
    Other,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct AppConfigError {
    pub kind: AppConfigErrorKind,
    pub message: String,
    pub issues: Vec<AppConfigIssue>,
}

impl From<AppFrameworkError> for AppConfigError {
    fn from(e: AppFrameworkError) -> Self {
        let message = e.to_string();
        match e {
            AppFrameworkError::ConfigConflict(_) => Self {
                kind: AppConfigErrorKind::Conflict,
                message,
                issues: Vec::new(),
            },
            AppFrameworkError::ConfigInvalid(issues) => Self {
                kind: AppConfigErrorKind::Invalid,
                message,
                issues,
            },
            AppFrameworkError::ConfigUnsupported(_) => Self {
                kind: AppConfigErrorKind::Unsupported,
                message,
                issues: Vec::new(),
            },
            _ => Self {
                kind: AppConfigErrorKind::Other,
                message,
                issues: Vec::new(),
            },
        }
    }
}

#[tauri::command]
pub fn list_app_frameworks(state: State<'_, AppState>) -> Vec<AppFrameworkManifest> {
    state.app_manager.list_frameworks()
}

#[tauri::command]
pub async fn list_app_instances(state: State<'_, AppState>) -> Result<Vec<AppInstance>, String> {
    Ok(state.app_manager.list_instances().await)
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

/// 安装 / 重装：按实例目录提交 Karin 组件任务（依赖 Node 会一起排），返回 task id
#[tauri::command]
pub async fn install_app_instance(
    instance_id: String,
    task_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let id = AppInstanceId::new(instance_id);
    let instance = state
        .app_manager
        .get_instance(&id)
        .await
        .map_err(|e| e.to_string())?;
    let component_id = state
        .app_manager
        .list_frameworks()
        .into_iter()
        .find(|m| m.id == instance.framework_id)
        .and_then(|m| ComponentId::parse(&m.component_id))
        .ok_or_else(|| format!("应用端框架未注册组件: {}", instance.framework_id))?;

    let host = resolve_host_with_autoconnect(&instance.host_id, &state).await?;
    let (probe, selected) = cached_host_probe(&instance.host_id, host.as_ref(), &state).await;
    let mut inputs = build_inputs(&state, host.as_ref(), &probe, selected, None).await;
    inputs.app_component = Some(state.app_manager.component_hint(&instance));

    let submitted = executor(&state)
        .submit(ComponentActionRequest {
            component_id,
            host_id: instance.host_id.clone(),
            kind: StepKind::EnsureInstalled,
            task_id: task_id.filter(|s| !s.trim().is_empty()),
            host,
            inputs,
        })
        .await?;
    state
        .app_manager
        .track_install(&id, submitted.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(submitted)
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

#[tauri::command]
pub async fn read_app_instance_config(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<AppInstanceConfigEnvelope, AppConfigError> {
    state
        .app_manager
        .read_config(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn write_app_instance_config(
    instance_id: String,
    config: AppInstanceConfig,
    base_revision: Option<String>,
    state: State<'_, AppState>,
) -> Result<AppConfigWriteResult, AppConfigError> {
    state
        .app_manager
        .write_config(&AppInstanceId::new(instance_id), config, base_revision)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn list_app_config_documents(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AppConfigDocument>, String> {
    state
        .app_manager
        .list_config_documents(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())
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
        .map_err(AppConfigError::from)
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
        .write_config_text(&AppInstanceId::new(instance_id), &doc_id, &text, base_revision)
        .await
        .map_err(AppConfigError::from)
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
    let local_host: Arc<dyn ncd_host::Host> = Arc::new(ncd_host::local::LocalWindowsHost::new());
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

/// WebUI 地址（前端用 opener 打开）：本机 127.0.0.1，远端用 ServerProfile.host
#[tauri::command]
pub async fn get_app_instance_webui(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<AppInstanceWebUi, String> {
    let instance = state
        .app_manager
        .get_instance(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())?;
    let public_host = host_display_address(&instance.host_id, &state).await;
    let url = state
        .app_manager
        .webui_url(&instance, &public_host)
        .ok_or_else(|| "该应用端没有 WebUI".to_string())?;
    let auth_key = state.app_manager.webui_auth_key(&instance.id).await;
    Ok(AppInstanceWebUi { url, auth_key })
}
