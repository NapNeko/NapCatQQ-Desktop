//! 主窗口诊断助手命令与流式事件接线。

use std::sync::Arc;

use ncd_domain::assistant::*;
use ncd_traits::assistant::AssistantEventSink;
use tauri::{State, WebviewWindow, ipc::Channel};

use crate::AppState;

struct ChannelSink(Channel<AssistantEvent>);

impl AssistantEventSink for ChannelSink {
    fn send(&self, event: &AssistantEvent) -> bool {
        self.0.send(event.clone()).is_ok()
    }
}

fn require_main(window: &WebviewWindow, state: &AppState) -> Result<(), String> {
    if window.label() != "main" {
        return Err("诊断助手只能从主窗口使用".into());
    }
    state.migrate_gate.ensure_idle()
}

#[tauri::command]
pub async fn assistant_settings(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<AssistantSettings, String> {
    require_main(&window, &state)?;
    state.assistant.settings().await
}

#[tauri::command]
pub async fn assistant_save_settings(
    window: WebviewWindow,
    state: State<'_, AppState>,
    settings: AssistantSettings,
) -> Result<(), String> {
    require_main(&window, &state)?;
    state.assistant.save_settings(settings).await
}

#[tauri::command]
pub async fn assistant_model_status(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<AssistantModelStatus, String> {
    require_main(&window, &state)?;
    state.assistant.model_status().await
}

#[tauri::command]
pub async fn assistant_set_api_key(
    window: WebviewWindow,
    state: State<'_, AppState>,
    api_key: String,
) -> Result<AssistantModelStatus, String> {
    require_main(&window, &state)?;
    super::bot::ensure_desktop_consent(&state)?;
    state.assistant.set_api_key(api_key).await
}

#[tauri::command]
pub async fn assistant_clear_api_key(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<AssistantModelStatus, String> {
    require_main(&window, &state)?;
    super::bot::ensure_desktop_consent(&state)?;
    state.assistant.clear_api_key().await
}

#[tauri::command]
pub async fn assistant_providers(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<AssistantProviderStatus>, String> {
    require_main(&window, &state)?;
    state.assistant.providers().await
}

#[tauri::command]
pub async fn assistant_list_sessions(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<AssistantSessionList, String> {
    require_main(&window, &state)?;
    state.assistant.list_sessions().await
}

#[tauri::command]
pub async fn assistant_get_session(
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<AssistantSession, String> {
    require_main(&window, &state)?;
    state.assistant.get_session(&session_id).await
}

#[tauri::command]
pub async fn assistant_create_session(
    window: WebviewWindow,
    state: State<'_, AppState>,
    provider: AssistantProvider,
    bot_id: String,
) -> Result<AssistantSession, String> {
    require_main(&window, &state)?;
    super::bot::ensure_desktop_consent(&state)?;
    state.assistant.create_session(provider, bot_id).await
}

#[tauri::command]
pub async fn assistant_send_message(
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
    text: String,
) -> Result<(), String> {
    require_main(&window, &state)?;
    super::bot::ensure_desktop_consent(&state)?;
    state.assistant.send_message(&session_id, text).await
}

#[tauri::command]
pub async fn assistant_cancel_session(
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    require_main(&window, &state)?;
    state.assistant.cancel_session(&session_id).await
}

#[tauri::command]
pub async fn assistant_close_session(
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    require_main(&window, &state)?;
    state.assistant.close_session(&session_id).await
}

#[tauri::command]
pub async fn assistant_delete_session(
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<(), String> {
    require_main(&window, &state)?;
    state.assistant.delete_session(&session_id).await
}

#[tauri::command]
pub async fn assistant_respond_permission(
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    option_id: Option<String>,
) -> Result<(), String> {
    require_main(&window, &state)?;
    super::bot::ensure_desktop_consent(&state)?;
    state
        .assistant
        .respond_permission(&session_id, &request_id, option_id)
        .await
}

#[tauri::command]
pub async fn assistant_respond_question(
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    answer: AssistantQuestionAnswer,
) -> Result<(), String> {
    require_main(&window, &state)?;
    state
        .assistant
        .respond_question(&session_id, &request_id, answer)
        .await
}

#[tauri::command]
pub async fn assistant_subscribe(
    window: WebviewWindow,
    state: State<'_, AppState>,
    on_event: Channel<AssistantEvent>,
) -> Result<(), String> {
    require_main(&window, &state)?;
    state.assistant.subscribe(Arc::new(ChannelSink(on_event)));
    Ok(())
}

#[tauri::command]
pub async fn assistant_login_plan(
    window: WebviewWindow,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<AssistantLoginPlan, String> {
    require_main(&window, &state)?;
    super::bot::ensure_desktop_consent(&state)?;
    state.assistant.login_plan(&session_id).await
}

#[cfg(test)]
pub(crate) fn test_assistant(
    root: &std::path::Path,
) -> Arc<ncd_runtime::assistant::AssistantManager> {
    struct EmptyHost;
    #[async_trait::async_trait]
    impl ncd_traits::assistant::AssistantToolHost for EmptyHost {
        async fn context(
            &self,
            _: &str,
        ) -> Result<ncd_traits::assistant::AssistantContext, String> {
            Err("测试未配置诊断目标".into())
        }
        async fn open(
            &self,
            _: &str,
            _: &str,
            _: Arc<dyn ncd_traits::assistant::AssistantInteraction>,
            _: tokio_util::sync::CancellationToken,
        ) -> Result<ncd_traits::assistant::AssistantToolConnection, String> {
            Err("测试不启动诊断服务".into())
        }
        async fn close(&self, _: &str) {}
        fn redact(&self, text: &str) -> String {
            text.to_owned()
        }
    }
    Arc::new(ncd_runtime::assistant::AssistantManager::new(
        root.to_owned(),
        Arc::new(EmptyHost),
    ))
}
