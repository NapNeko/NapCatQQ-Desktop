//! 本机诊断助手的会话、交互与设置契约。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AssistantProvider {
    Builtin,
    Dsh,
    Qwen,
    Kimi,
    MiniMax,
    CodeBuddy,
    Qoder,
    Trae,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantProviderConfig {
    pub provider: AssistantProvider,
    pub enabled: bool,
    pub executable_path: Option<String>,
    pub extra_args: Vec<String>,
    pub allow_native_tools: bool,
}

impl Default for AssistantProviderConfig {
    fn default() -> Self {
        Self {
            provider: AssistantProvider::Dsh,
            enabled: true,
            executable_path: None,
            extra_args: Vec::new(),
            allow_native_tools: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantSettings {
    pub default_provider: Option<AssistantProvider>,
    pub builtin: AssistantBuiltinSettings,
    pub providers: Vec<AssistantProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantBuiltinSettings {
    pub provider_preset: AssistantModelPreset,
    pub api_format: AssistantApiFormat,
    pub base_url: String,
    pub model: String,
    pub max_steps: u32,
    pub max_output_tokens: u32,
    pub request_timeout_secs: u32,
}

impl Default for AssistantBuiltinSettings {
    fn default() -> Self {
        Self {
            provider_preset: AssistantModelPreset::Custom,
            api_format: AssistantApiFormat::OpenAiChatCompletions,
            base_url: String::new(),
            model: String::new(),
            max_steps: 6,
            max_output_tokens: 2048,
            request_timeout_secs: 90,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AssistantModelPreset {
    #[default]
    Custom,
    OpenAi,
    Anthropic,
    Gemini,
    DeepSeek,
    MiniMax,
    Moonshot,
    DashScope,
    OpenRouter,
    Ollama,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AssistantApiFormat {
    #[default]
    OpenAiChatCompletions,
    OpenAiResponses,
    AnthropicMessages,
    GeminiGenerateContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantModelStatus {
    pub has_api_key: bool,
    pub configured: bool,
    pub error: Option<String>,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            default_provider: Some(AssistantProvider::Builtin),
            builtin: AssistantBuiltinSettings::default(),
            providers: [
                AssistantProvider::Dsh,
                AssistantProvider::Qwen,
                AssistantProvider::Kimi,
                AssistantProvider::MiniMax,
                AssistantProvider::CodeBuddy,
                AssistantProvider::Qoder,
                AssistantProvider::Trae,
                AssistantProvider::Custom,
            ]
            .into_iter()
            .map(|provider| AssistantProviderConfig {
                provider,
                ..Default::default()
            })
            .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantProviderStatus {
    pub provider: AssistantProvider,
    pub label: String,
    pub enabled: bool,
    pub installed: bool,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub error: Option<String>,
    pub documentation_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantCapabilities {
    pub mcp_http: bool,
    pub load_session: bool,
    pub resume_session: bool,
    pub images: bool,
    pub elicitation: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AssistantSessionState {
    Starting,
    Ready,
    Running,
    WaitingForInput,
    Disconnected,
    Closed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantSessionInfo {
    pub id: String,
    pub provider: AssistantProvider,
    pub bot_id: String,
    pub bot_name: String,
    pub title: String,
    pub state: AssistantSessionState,
    pub capabilities: AssistantCapabilities,
    pub created_at_ms: f64,
    pub updated_at_ms: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AssistantMessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantMessage {
    pub id: String,
    pub role: AssistantMessageRole,
    pub text: String,
    pub created_at_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantToolCall {
    pub id: String,
    pub title: String,
    pub status: String,
    pub details: String,
    pub created_at_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AssistantPermissionSource {
    Agent,
    NcdOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantPermissionOption {
    pub id: String,
    pub label: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantPermissionRequest {
    pub id: String,
    pub source: AssistantPermissionSource,
    pub title: String,
    pub details: String,
    pub options: Vec<AssistantPermissionOption>,
    pub created_at_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantQuestionField {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub kind: String,
    pub required: bool,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantQuestion {
    pub id: String,
    pub title: String,
    pub fields: Vec<AssistantQuestionField>,
    pub created_at_ms: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantQuestionAnswer {
    pub cancelled: bool,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantSession {
    #[serde(default)]
    pub sequence: u32,
    pub info: AssistantSessionInfo,
    pub messages: Vec<AssistantMessage>,
    pub tools: Vec<AssistantToolCall>,
    pub permissions: Vec<AssistantPermissionRequest>,
    pub questions: Vec<AssistantQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantSessionList {
    pub sessions: Vec<AssistantSessionInfo>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantLoginPlan {
    pub command: String,
    pub shell: crate::terminal::LocalShellKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AssistantUpdate {
    Session { info: AssistantSessionInfo },
    Message { message: AssistantMessage },
    Tool { tool: AssistantToolCall },
    Permission { request: AssistantPermissionRequest },
    Question { question: AssistantQuestion },
    InteractionResolved { id: String },
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AssistantEvent {
    pub v: u32,
    pub session_id: String,
    pub sequence: u32,
    pub update: AssistantUpdate,
}

#[cfg(test)]
mod model_settings_tests {
    use super::*;

    #[test]
    fn legacy_model_configuration_keeps_its_endpoint_and_chat_protocol() {
        let settings: AssistantBuiltinSettings = serde_json::from_str(
            r#"{"baseUrl":"https://gateway.invalid/custom/chat","model":"custom-model","maxSteps":4}"#,
        ).unwrap();
        assert_eq!(settings.provider_preset, AssistantModelPreset::Custom);
        assert_eq!(
            settings.api_format,
            AssistantApiFormat::OpenAiChatCompletions
        );
        assert_eq!(settings.base_url, "https://gateway.invalid/custom/chat");
        assert_eq!(settings.model, "custom-model");
        assert_eq!(settings.max_steps, 4);
    }

    #[test]
    fn model_api_formats_round_trip_with_stable_ipc_names() {
        for (format, expected) in [
            (
                AssistantApiFormat::OpenAiChatCompletions,
                "openAiChatCompletions",
            ),
            (AssistantApiFormat::OpenAiResponses, "openAiResponses"),
            (AssistantApiFormat::AnthropicMessages, "anthropicMessages"),
            (
                AssistantApiFormat::GeminiGenerateContent,
                "geminiGenerateContent",
            ),
        ] {
            let value = serde_json::to_value(format).unwrap();
            assert_eq!(value, expected);
            assert_eq!(
                serde_json::from_value::<AssistantApiFormat>(value).unwrap(),
                format
            );
        }
    }

    #[test]
    fn unknown_model_api_format_cannot_silently_fall_back() {
        assert!(
            serde_json::from_str::<AssistantBuiltinSettings>(r#"{"apiFormat":"automatic"}"#,)
                .is_err()
        );
    }
}
