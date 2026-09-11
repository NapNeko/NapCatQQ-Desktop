//! 应用端框架 Tauri commands：薄壳，只做参数转换 + 错误转 String。
//! 业务在 `ncd_runtime::AppManager`；安装走既有 ComponentExecutor（R12）。

use ncd_component::ComponentId;
use ncd_deploy::StepKind;
use ncd_domain::{
    AppConfigDocument, AppConfigIssue, AppConfigText, AppFrameworkId, AppFrameworkManifest,
    AppInstance, AppInstanceId, AppPluginAction, AppPluginConfigSchema, AppProjectProbe,
    AppStoreResource, AppWebUiAccount, BotId, CreateAppInstanceRequest, DeploymentTaskKind,
    DeploymentTaskResource, ImportAppInstanceRequest, OneBotLinkPlan,
};
use ncd_runtime::{
    AppConfigWriteResult, AppInstanceConfig, AppInstanceConfigEnvelope, AppStoreInstalled,
    AppStoreMarketEntry, AstrBotAbconfInfo, AstrBotDashboardStatus, AstrBotKbCreate,
    AstrBotKnowledgeBase, AstrBotPersona, AstrBotSessionRule, ComponentActionRequest,
    DeploymentTaskRequest, KarinPluginInstalled, KarinPluginMarketEntry, join_webui_url,
    run_app_plugin_task,
};
use ncd_traits::AppFrameworkError;
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::AppState;
use crate::commands::components::{build_inputs, cached_host_probe, executor};
use crate::commands::host_resolve::resolve_host_with_autoconnect;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct AppInstanceWebUi {
    pub url: String,
    /// Karin `HTTP_AUTH_KEY`；空则前端不写剪贴板。
    pub auth_key: String,
    /// 用户名密码类 WebUI（AstrBot）的账号；None = 该框架不是账号密码登录
    #[ts(optional)]
    pub account: Option<AppWebUiAccount>,
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
    NotRunning,
    Auth,
    Unreachable,
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
            AppFrameworkError::NotRunning(_) => Self {
                kind: AppConfigErrorKind::NotRunning,
                message,
                issues: Vec::new(),
            },
            AppFrameworkError::DashboardAuth(_) => Self {
                kind: AppConfigErrorKind::Auth,
                message,
                issues: Vec::new(),
            },
            AppFrameworkError::DashboardUnreachable(_) => Self {
                kind: AppConfigErrorKind::Unreachable,
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
    conf_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<AppConfigWriteResult, AppConfigError> {
    state
        .app_manager
        .write_config_profile(
            &AppInstanceId::new(instance_id),
            config,
            base_revision,
            conf_id,
        )
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

/// WebUI 地址（前端用 opener 打开）：一律本机 loopback；远端经 SSH 隧道。
#[tauri::command]
pub async fn get_app_instance_webui(
    instance_id: String,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<AppInstanceWebUi, String> {
    let instance = state
        .app_manager
        .get_instance(&AppInstanceId::new(instance_id))
        .await
        .map_err(|e| e.to_string())?;
    let port = state
        .app_manager
        .desktop_webui_loopback_port(&instance)
        .await
        .map_err(|e| e.to_string())?;
    let mut view = instance.clone();
    view.port = port;
    let url = state
        .app_manager
        .webui_url(&view, "127.0.0.1")
        .ok_or_else(|| "该应用端没有 WebUI".to_string())?;
    let url = join_webui_url(&url, path.as_deref());
    let auth_key = state.app_manager.webui_auth_key(&instance.id).await;
    let account = state.app_manager.webui_account(&instance).await;
    Ok(AppInstanceWebUi {
        url,
        auth_key,
        account,
    })
}

#[tauri::command]
pub async fn astrbot_dashboard_status(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<AstrBotDashboardStatus, AppConfigError> {
    state
        .app_manager
        .astrbot_dashboard_status(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_list_personas(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotPersona>, AppConfigError> {
    state
        .app_manager
        .astrbot_list_personas(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_upsert_persona(
    instance_id: String,
    persona: AstrBotPersona,
    creating: bool,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotPersona>, AppConfigError> {
    state
        .app_manager
        .astrbot_upsert_persona(&AppInstanceId::new(instance_id), persona, creating)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_delete_persona(
    instance_id: String,
    persona_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotPersona>, AppConfigError> {
    state
        .app_manager
        .astrbot_delete_persona(&AppInstanceId::new(instance_id), &persona_id)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_list_kbs(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotKnowledgeBase>, AppConfigError> {
    state
        .app_manager
        .astrbot_list_kbs(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_create_kb(
    instance_id: String,
    request: AstrBotKbCreate,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotKnowledgeBase>, AppConfigError> {
    state
        .app_manager
        .astrbot_create_kb(&AppInstanceId::new(instance_id), request)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_delete_kb(
    instance_id: String,
    kb_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotKnowledgeBase>, AppConfigError> {
    state
        .app_manager
        .astrbot_delete_kb(&AppInstanceId::new(instance_id), &kb_id)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_list_session_rules(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotSessionRule>, AppConfigError> {
    state
        .app_manager
        .astrbot_list_session_rules(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_update_session_rule(
    instance_id: String,
    rule: AstrBotSessionRule,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotSessionRule>, AppConfigError> {
    state
        .app_manager
        .astrbot_update_session_rule(&AppInstanceId::new(instance_id), rule)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_delete_session_rule(
    instance_id: String,
    umo: String,
    rule_key: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotSessionRule>, AppConfigError> {
    state
        .app_manager
        .astrbot_delete_session_rule(&AppInstanceId::new(instance_id), &umo, &rule_key)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_list_abconfs(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotAbconfInfo>, AppConfigError> {
    state
        .app_manager
        .astrbot_list_abconfs(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_create_abconf(
    instance_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotAbconfInfo>, AppConfigError> {
    state
        .app_manager
        .astrbot_create_abconf(&AppInstanceId::new(instance_id), &name)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_delete_abconf(
    instance_id: String,
    abconf_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AstrBotAbconfInfo>, AppConfigError> {
    state
        .app_manager
        .astrbot_delete_abconf(&AppInstanceId::new(instance_id), &abconf_id)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_list_source_models(
    instance_id: String,
    source_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppConfigError> {
    state
        .app_manager
        .astrbot_list_source_models(&AppInstanceId::new(instance_id), &source_id)
        .await
        .map_err(AppConfigError::from)
}

#[tauri::command]
pub async fn astrbot_list_subagent_tools(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppConfigError> {
    state
        .app_manager
        .astrbot_list_subagent_tools(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppConfigError::from)
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
    let id = AppInstanceId::new(instance_id);
    let instance = state.app_manager.get_instance(&id).await?;
    resolve_host_with_autoconnect(&instance.host_id, &state)
        .await
        .map_err(|e| AppConfigError::from(AppFrameworkError::Host(e)))?;
    Ok(state.app_manager.reset_webui_password(&id, password).await?)
}

#[tauri::command]
pub async fn list_karin_plugin_market(
    state: State<'_, AppState>,
) -> Result<Vec<KarinPluginMarketEntry>, String> {
    state
        .app_manager
        .list_karin_plugin_market()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_app_store(
    framework_id: String,
    resource: AppStoreResource,
    state: State<'_, AppState>,
) -> Result<Vec<AppStoreMarketEntry>, String> {
    state
        .app_manager
        .list_store(&AppFrameworkId::new(framework_id), resource)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_app_store_installed(
    instance_id: String,
    resource: AppStoreResource,
    state: State<'_, AppState>,
) -> Result<Vec<AppStoreInstalled>, String> {
    let id = AppInstanceId::new(instance_id);
    let instance = state
        .app_manager
        .get_instance(&id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = resolve_host_with_autoconnect(&instance.host_id, &state).await?;
    state
        .app_manager
        .list_store_installed(&id, resource)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_app_plugin_config_docs(
    instance_id: String,
    plugin_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<AppConfigDocument>, String> {
    let id = AppInstanceId::new(instance_id);
    let instance = state
        .app_manager
        .get_instance(&id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = resolve_host_with_autoconnect(&instance.host_id, &state).await?;
    state
        .app_manager
        .list_plugin_config_docs(&id, &plugin_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_app_plugin_config_schema(
    instance_id: String,
    plugin_name: String,
    state: State<'_, AppState>,
) -> Result<Option<AppPluginConfigSchema>, String> {
    let id = AppInstanceId::new(instance_id);
    let instance = state
        .app_manager
        .get_instance(&id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = resolve_host_with_autoconnect(&instance.host_id, &state).await?;
    state
        .app_manager
        .plugin_config_schema(&id, &plugin_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_app_instance_plugins(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<KarinPluginInstalled>, String> {
    let id = AppInstanceId::new(instance_id);
    let instance = state
        .app_manager
        .get_instance(&id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = resolve_host_with_autoconnect(&instance.host_id, &state).await?;
    state
        .app_manager
        .list_plugins(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn submit_app_plugin_op(
    instance_id: String,
    plugin_name: String,
    action: AppPluginAction,
    resource: Option<AppStoreResource>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let id = AppInstanceId::new(instance_id);
    let instance = state
        .app_manager
        .get_instance(&id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = resolve_host_with_autoconnect(&instance.host_id, &state).await?;
    let resource = resource.unwrap_or(AppStoreResource::Plugin);
    let verb = match action {
        AppPluginAction::Install => "安装",
        AppPluginAction::Update => "更新",
        AppPluginAction::Uninstall => "卸载",
    };
    let action_key = match action {
        AppPluginAction::Install => "install",
        AppPluginAction::Update => "update",
        AppPluginAction::Uninstall => "uninstall",
    };
    let kind_label = match resource {
        AppStoreResource::Adapter => "适配器",
        AppStoreResource::Plugin => "插件",
    };
    let fw = match instance.framework_id.as_str() {
        "karin" => "Karin",
        "nonebot2" => "NoneBot2",
        "astrbot" => "AstrBot",
        other => other,
    };
    let app_manager = std::sync::Arc::clone(&state.app_manager);
    let run_id = instance.id.clone();
    let run_name = plugin_name.clone();
    let submitted = state
        .deployment_tasks
        .submit(DeploymentTaskRequest {
            task_id: uuid::Uuid::new_v4().to_string(),
            kind: DeploymentTaskKind::AppPlugin {
                instance_id: instance.id.as_str().to_string(),
                plugin_name: plugin_name.clone(),
                action,
                resource,
            },
            host_id: instance.host_id.clone(),
            title: format!("{fw} · {verb}{kind_label} {plugin_name}"),
            resources: vec![DeploymentTaskResource::InstallTarget {
                host_id: instance.host_id.clone(),
                target: instance.install_dir.clone(),
            }],
            depends_on: vec![],
            dedupe_key: Some(format!(
                "app-plugin:{}:{}:{}:{action_key}",
                instance.id.as_str(),
                match resource {
                    AppStoreResource::Adapter => "adapter",
                    AppStoreResource::Plugin => "plugin",
                },
                plugin_name
            )),
            cancellable: true,
            runner: Box::new(move |ctx| {
                Box::pin(async move {
                    run_app_plugin_task(app_manager, run_id, run_name, action, resource, ctx).await
                })
            }),
        })
        .await;
    Ok(submitted)
}

#[tauri::command]
pub async fn set_app_plugin_enabled(
    instance_id: String,
    plugin_name: String,
    enabled: bool,
    overwrite: Option<bool>,
    resource: Option<AppStoreResource>,
    state: State<'_, AppState>,
) -> Result<AppConfigWriteResult, AppConfigError> {
    state
        .app_manager
        .set_store_enabled(
            &AppInstanceId::new(instance_id),
            &plugin_name,
            resource.unwrap_or(AppStoreResource::Plugin),
            enabled,
            overwrite.unwrap_or(false),
        )
        .await
        .map_err(AppConfigError::from)
}
