//! 应用端框架 Tauri commands：薄壳，只做参数转换 + 错误转 String。
//! 业务在 `ncd_runtime::AppManager`，安装、排插件任务、开 WebUI 隧道都在那边；
//! 远端主机没连上时由它的主机解析器现连，这里不用预热。

use ncd_domain::{
    AppConfigDocument, AppConfigError, AppConfigText, AppFrameworkId, AppFrameworkManifest,
    AppInstance, AppInstanceId, AppInstanceWebUi, AppPendingTerms, AppPluginAction,
    AppPluginConfigSchema,
    AppProjectProbe, AppStoreResource, AppWebUiAccount, BotId, CreateAppInstanceRequest,
    ImportAppInstanceRequest, OneBotLinkPlan,
};
use ncd_runtime::{
    AppConfigWriteResult, AppInstanceConfig, AppInstanceConfigEnvelope, AppStoreInstalled,
    AppStoreMarketEntry, AstrBotAbconfInfo, AstrBotDashboardStatus, AstrBotKbCreate,
    AstrBotKnowledgeBase, AstrBotPersona, AstrBotSessionRule,
    KarinPluginInstalled, KarinPluginMarketEntry, MaiBotAPIProvider,
    MaiBotBehaviorDetail, MaiBotBehaviorOverview, MaiBotBehaviorPage, MaiBotBehaviorQuery,
    MaiBotChatSession, MaiBotChatTicket, MaiBotEmojiAction, MaiBotEmojiImage, MaiBotEmojiOverview, MaiBotEmojiPage,
    MaiBotEmojiQuery, MaiBotEmojiUpload, MaiBotEmojiUploadDone, MaiBotExpressionAction,
    MaiBotExpressionOverview, MaiBotExpressionPage, MaiBotExpressionQuery, MaiBotJargonAction,
    MaiBotJargonOverview, MaiBotJargonPage, MaiBotJargonQuery, MaiBotLocalImage,
    MaiBotMCPServerItemConfig, MaiBotMcpStatus, MaiBotMcpTest, MaiBotPersonAction,
    MaiBotPersonOverview, MaiBotPersonPage, MaiBotPersonQuery, MaiBotLocalTextFile,
    MaiBotMemoryDeleteAction, MaiBotMemoryDeleteOp, MaiBotMemoryDeleteResult, MaiBotMemoryGraph,
    MaiBotMemoryGraphHit, MaiBotMemoryImport, MaiBotMemoryImportSetup, MaiBotMemoryNodeDetail,
    MaiBotMemoryQuery, MaiBotMemoryRecordDetail, MaiBotMemoryRecordKind, MaiBotMemoryRecordPage,
    MaiBotMemorySource, MaiBotMemoryStatus, MaiBotMemoryTask, MaiBotMemoryTaskAction,
    MaiBotMemoryTaskDetail,
    MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile, MaiBotProviderCheck,
    MaiBotProviderModel, MaiBotResourceDone, MaiBotRuntimeStatus, MaiBotStatsSummary,
};
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

/// 安装 / 重装：按实例目录提交应用端组件任务（缺的依赖会一起排），返回 task id
#[tauri::command]
pub async fn install_app_instance(
    instance_id: String,
    task_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .app_manager
        .install_instance(
            &AppInstanceId::new(instance_id),
            task_id,
            &state.components,
            state.deployment_tasks.clone(),
        )
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

#[tauri::command]
pub async fn read_app_instance_config(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<AppInstanceConfigEnvelope, AppConfigError> {
    state
        .app_manager
        .read_config(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .write_config_text(&AppInstanceId::new(instance_id), &doc_id, &text, base_revision)
        .await
        .map_err(AppFrameworkError::into_config_error)
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
    state
        .app_manager
        .open_webui(&AppInstanceId::new(instance_id), path.as_deref())
        .await
        .map_err(|e| e.to_string())
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
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
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_status(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotRuntimeStatus, AppConfigError> {
    state
        .app_manager
        .maibot_status(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_restart(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppConfigError> {
    state
        .app_manager
        .maibot_restart(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_stats(
    instance_id: String,
    hours: u32,
    state: State<'_, AppState>,
) -> Result<MaiBotStatsSummary, AppConfigError> {
    state
        .app_manager
        .maibot_stats(&AppInstanceId::new(instance_id), hours)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_chat_sessions(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<MaiBotChatSession>, AppConfigError> {
    state
        .app_manager
        .maibot_chat_sessions(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_provider_models(
    instance_id: String,
    provider: MaiBotAPIProvider,
    state: State<'_, AppState>,
) -> Result<Vec<MaiBotProviderModel>, AppConfigError> {
    state
        .app_manager
        .maibot_provider_models(&AppInstanceId::new(instance_id), provider)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_test_provider(
    instance_id: String,
    provider: MaiBotAPIProvider,
    state: State<'_, AppState>,
) -> Result<MaiBotProviderCheck, AppConfigError> {
    state
        .app_manager
        .maibot_test_provider(&AppInstanceId::new(instance_id), provider)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_mcp_status(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotMcpStatus, AppConfigError> {
    state
        .app_manager
        .maibot_mcp_status(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_test_mcp(
    instance_id: String,
    server: MaiBotMCPServerItemConfig,
    state: State<'_, AppState>,
) -> Result<MaiBotMcpTest, AppConfigError> {
    state
        .app_manager
        .maibot_test_mcp(&AppInstanceId::new(instance_id), server)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_prompt_catalog(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotPromptCatalog, AppConfigError> {
    state
        .app_manager
        .maibot_prompt_catalog(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_prompt_file(
    instance_id: String,
    language: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<MaiBotPromptFile, AppConfigError> {
    state
        .app_manager
        .maibot_prompt_file(&AppInstanceId::new(instance_id), &language, &name)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_prompt_version(
    instance_id: String,
    language: String,
    name: String,
    version_id: String,
    state: State<'_, AppState>,
) -> Result<String, AppConfigError> {
    state
        .app_manager
        .maibot_prompt_version(&AppInstanceId::new(instance_id), &language, &name, &version_id)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_prompt_action(
    instance_id: String,
    action: MaiBotPromptAction,
    state: State<'_, AppState>,
) -> Result<MaiBotPromptFile, AppConfigError> {
    state
        .app_manager
        .maibot_prompt_action(&AppInstanceId::new(instance_id), action)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_expressions(
    instance_id: String,
    query: MaiBotExpressionQuery,
    state: State<'_, AppState>,
) -> Result<MaiBotExpressionPage, AppConfigError> {
    state
        .app_manager
        .maibot_expressions(&AppInstanceId::new(instance_id), query)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_expression_overview(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotExpressionOverview, AppConfigError> {
    state
        .app_manager
        .maibot_expression_overview(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_expression_action(
    instance_id: String,
    action: MaiBotExpressionAction,
    state: State<'_, AppState>,
) -> Result<MaiBotResourceDone, AppConfigError> {
    state
        .app_manager
        .maibot_expression_action(&AppInstanceId::new(instance_id), action)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_jargons(
    instance_id: String,
    query: MaiBotJargonQuery,
    state: State<'_, AppState>,
) -> Result<MaiBotJargonPage, AppConfigError> {
    state
        .app_manager
        .maibot_jargons(&AppInstanceId::new(instance_id), query)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_jargon_overview(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotJargonOverview, AppConfigError> {
    state
        .app_manager
        .maibot_jargon_overview(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_jargon_action(
    instance_id: String,
    action: MaiBotJargonAction,
    state: State<'_, AppState>,
) -> Result<MaiBotResourceDone, AppConfigError> {
    state
        .app_manager
        .maibot_jargon_action(&AppInstanceId::new(instance_id), action)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_behaviors(
    instance_id: String,
    query: MaiBotBehaviorQuery,
    state: State<'_, AppState>,
) -> Result<MaiBotBehaviorPage, AppConfigError> {
    state
        .app_manager
        .maibot_behaviors(&AppInstanceId::new(instance_id), query)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_behavior_overview(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotBehaviorOverview, AppConfigError> {
    state
        .app_manager
        .maibot_behavior_overview(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_behavior_detail(
    instance_id: String,
    id: i64,
    state: State<'_, AppState>,
) -> Result<MaiBotBehaviorDetail, AppConfigError> {
    state
        .app_manager
        .maibot_behavior(&AppInstanceId::new(instance_id), id)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_chat_ticket(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotChatTicket, AppConfigError> {
    state
        .app_manager
        .maibot_chat_ticket(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_chat_clear(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotResourceDone, AppConfigError> {
    state
        .app_manager
        .maibot_chat_clear(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_persons(
    instance_id: String,
    query: MaiBotPersonQuery,
    state: State<'_, AppState>,
) -> Result<MaiBotPersonPage, AppConfigError> {
    state
        .app_manager
        .maibot_persons(&AppInstanceId::new(instance_id), query)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_person_overview(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotPersonOverview, AppConfigError> {
    state
        .app_manager
        .maibot_person_overview(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_person_action(
    instance_id: String,
    action: MaiBotPersonAction,
    state: State<'_, AppState>,
) -> Result<MaiBotResourceDone, AppConfigError> {
    state
        .app_manager
        .maibot_person_action(&AppInstanceId::new(instance_id), action)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_emojis(
    instance_id: String,
    query: MaiBotEmojiQuery,
    state: State<'_, AppState>,
) -> Result<MaiBotEmojiPage, AppConfigError> {
    state
        .app_manager
        .maibot_emojis(&AppInstanceId::new(instance_id), query)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_emoji_overview(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotEmojiOverview, AppConfigError> {
    state
        .app_manager
        .maibot_emoji_overview(&AppInstanceId::new(instance_id))
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_emoji_action(
    instance_id: String,
    action: MaiBotEmojiAction,
    state: State<'_, AppState>,
) -> Result<MaiBotResourceDone, AppConfigError> {
    state
        .app_manager
        .maibot_emoji_action(&AppInstanceId::new(instance_id), action)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_emoji_image(
    instance_id: String,
    emoji_id: i64,
    original: bool,
    state: State<'_, AppState>,
) -> Result<MaiBotEmojiImage, AppConfigError> {
    state
        .app_manager
        .maibot_emoji_image(&AppInstanceId::new(instance_id), emoji_id, original)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_emoji_upload(
    instance_id: String,
    upload: MaiBotEmojiUpload,
    state: State<'_, AppState>,
) -> Result<MaiBotEmojiUploadDone, AppConfigError> {
    state
        .app_manager
        .maibot_emoji_upload(&AppInstanceId::new(instance_id), upload)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_local_images(
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<MaiBotLocalImage>, AppConfigError> {
    Ok(state.app_manager.maibot_local_images(paths).await)
}

#[tauri::command]
pub async fn maibot_local_texts(
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<MaiBotLocalTextFile>, AppConfigError> {
    Ok(state.app_manager.maibot_local_texts(paths).await)
}

#[tauri::command]
pub async fn maibot_memory_status(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryStatus, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_status(&id).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_import_setup(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryImportSetup, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_import_setup(&id).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_import(
    instance_id: String,
    req: MaiBotMemoryImport,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryTask, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_import(&id, req).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_tasks(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<MaiBotMemoryTask>, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_tasks(&id).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_task(
    instance_id: String,
    task_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryTaskDetail, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_task(&id, &task_id).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_task_action(
    instance_id: String,
    action: MaiBotMemoryTaskAction,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryTask, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_task_action(&id, action).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_records(
    instance_id: String,
    query: MaiBotMemoryQuery,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryRecordPage, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_records(&id, query).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_record(
    instance_id: String,
    kind: MaiBotMemoryRecordKind,
    record_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryRecordDetail, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state
        .app_manager
        .maibot_memory_record(&id, kind, &record_id)
        .await
        .map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_sources(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<MaiBotMemorySource>, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_sources(&id).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_delete(
    instance_id: String,
    action: MaiBotMemoryDeleteAction,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryDeleteResult, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_delete(&id, action).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_delete_ops(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<MaiBotMemoryDeleteOp>, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_delete_ops(&id).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_graph(
    instance_id: String,
    max_nodes: u32,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryGraph, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_graph(&id, max_nodes).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_graph_node(
    instance_id: String,
    node_id: String,
    state: State<'_, AppState>,
) -> Result<MaiBotMemoryNodeDetail, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_graph_node(&id, &node_id).await.map_err(AppFrameworkError::into_config_error)
}

#[tauri::command]
pub async fn maibot_memory_graph_search(
    instance_id: String,
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<MaiBotMemoryGraphHit>, AppConfigError> {
    let id = AppInstanceId::new(instance_id);
    state.app_manager.maibot_memory_graph_search(&id, &query).await.map_err(AppFrameworkError::into_config_error)
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
    state
        .app_manager
        .list_store_installed(&AppInstanceId::new(instance_id), resource)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_app_plugin_config_docs(
    instance_id: String,
    plugin_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<AppConfigDocument>, String> {
    state
        .app_manager
        .list_plugin_config_docs(&AppInstanceId::new(instance_id), &plugin_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_app_plugin_config_schema(
    instance_id: String,
    plugin_name: String,
    state: State<'_, AppState>,
) -> Result<Option<AppPluginConfigSchema>, String> {
    state
        .app_manager
        .plugin_config_schema(&AppInstanceId::new(instance_id), &plugin_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_app_instance_plugins(
    instance_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<KarinPluginInstalled>, String> {
    state
        .app_manager
        .list_plugins(&AppInstanceId::new(instance_id))
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
    state
        .app_manager
        .submit_store_op(
            &AppInstanceId::new(instance_id),
            &plugin_name,
            action,
            resource.unwrap_or(AppStoreResource::Plugin),
            &state.deployment_tasks,
        )
        .await
        .map_err(|e| e.to_string())
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
        .map_err(AppFrameworkError::into_config_error)
}
