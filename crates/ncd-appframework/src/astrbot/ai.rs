//! AstrBot AI 投影：从 `cmd_config.json` 抽出提供商 / 对话设置 / 平台门控 / 子代理。
//! 未知键走 `extra` 回写，不进 IPC。

use ncd_domain::AppConfigIssue;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::config_doc::IssueSink;

type Extra = Map<String, Value>;

fn extra_new() -> Extra {
    Extra::new()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotProviderSource {
    pub id: String,
    pub provider: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub provider_type: String,
    pub enable: bool,
    pub key: Vec<String>,
    pub api_base: String,
    #[ts(type = "number")]
    pub timeout: i64,
    pub proxy: String,
    #[serde(skip, default = "extra_new")]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for AstrBotProviderSource {
    fn default() -> Self {
        Self {
            id: String::new(),
            provider: "openai".into(),
            type_name: "openai_chat_completion".into(),
            provider_type: "chat_completion".into(),
            enable: true,
            key: Vec::new(),
            api_base: String::new(),
            timeout: 120,
            proxy: String::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotProviderModel {
    pub id: String,
    pub enable: bool,
    pub provider_source_id: String,
    pub model: String,
    pub modalities: Vec<String>,
    #[ts(type = "number")]
    pub max_context_tokens: i64,
    #[serde(skip, default = "extra_new")]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for AstrBotProviderModel {
    fn default() -> Self {
        Self {
            id: String::new(),
            enable: false,
            provider_source_id: String::new(),
            model: String::new(),
            modalities: vec!["text".into()],
            max_context_tokens: 0,
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotAiSettings {
    pub enable: bool,
    pub default_provider_id: String,
    pub fallback_chat_models: Vec<String>,
    pub default_image_caption_provider_id: String,
    pub image_caption_prompt: String,
    pub default_personality: String,
    pub wake_prefix: String,
    pub prompt_prefix: String,
    pub agent_runner_type: String,
    pub dify_agent_runner_provider_id: String,
    pub coze_agent_runner_provider_id: String,
    pub dashscope_agent_runner_provider_id: String,
    pub deerflow_agent_runner_provider_id: String,
    #[ts(type = "number")]
    pub max_context_length: i64,
    #[ts(type = "number")]
    pub dequeue_context_length: i64,
    pub context_limit_reached_strategy: String,
    pub llm_compress_instruction: String,
    #[ts(type = "number")]
    pub llm_compress_keep_recent: i64,
    pub llm_compress_provider_id: String,
    pub streaming_response: bool,
    pub display_reasoning_text: bool,
    pub llm_safety_mode: bool,
    pub identifier: bool,
    pub group_name_display: bool,
    pub datetime_system_prompt: bool,
    pub show_tool_use_status: bool,
    pub show_tool_call_result: bool,
    #[ts(type = "number")]
    pub max_agent_step: i64,
    #[ts(type = "number")]
    pub tool_call_timeout: i64,
    pub tool_schema_mode: String,
}

impl Default for AstrBotAiSettings {
    fn default() -> Self {
        Self {
            enable: true,
            default_provider_id: String::new(),
            fallback_chat_models: Vec::new(),
            default_image_caption_provider_id: String::new(),
            image_caption_prompt: "Please describe the image using Chinese.".into(),
            default_personality: "default".into(),
            wake_prefix: String::new(),
            prompt_prefix: "{{prompt}}".into(),
            agent_runner_type: "local".into(),
            dify_agent_runner_provider_id: String::new(),
            coze_agent_runner_provider_id: String::new(),
            dashscope_agent_runner_provider_id: String::new(),
            deerflow_agent_runner_provider_id: String::new(),
            max_context_length: -1,
            dequeue_context_length: 1,
            context_limit_reached_strategy: "truncate_by_turns".into(),
            llm_compress_instruction: String::new(),
            llm_compress_keep_recent: 6,
            llm_compress_provider_id: String::new(),
            streaming_response: false,
            display_reasoning_text: false,
            llm_safety_mode: true,
            identifier: false,
            group_name_display: false,
            datetime_system_prompt: true,
            show_tool_use_status: false,
            show_tool_call_result: false,
            max_agent_step: 30,
            tool_call_timeout: 60,
            tool_schema_mode: "full".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotSttSettings {
    pub enable: bool,
    pub provider_id: String,
}

impl Default for AstrBotSttSettings {
    fn default() -> Self {
        Self {
            enable: false,
            provider_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotTtsSettings {
    pub enable: bool,
    pub provider_id: String,
    pub dual_output: bool,
    #[ts(type = "number")]
    pub trigger_probability: f64,
}

impl Default for AstrBotTtsSettings {
    fn default() -> Self {
        Self {
            enable: false,
            provider_id: String::new(),
            dual_output: false,
            trigger_probability: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotWebSearchSettings {
    pub enable: bool,
    pub provider: String,
    pub tavily_key: Vec<String>,
    pub bocha_key: Vec<String>,
    pub baidu_app_builder_key: String,
    pub show_link: bool,
}

impl Default for AstrBotWebSearchSettings {
    fn default() -> Self {
        Self {
            enable: false,
            provider: "default".into(),
            tavily_key: Vec::new(),
            bocha_key: Vec::new(),
            baidu_app_builder_key: String::new(),
            show_link: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotKbBind {
    pub names: Vec<String>,
    #[ts(type = "number")]
    pub fusion_top_k: i64,
    #[ts(type = "number")]
    pub final_top_k: i64,
    pub agentic_mode: bool,
}

impl Default for AstrBotKbBind {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            fusion_top_k: 20,
            final_top_k: 5,
            agentic_mode: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotPlatformGates {
    pub wake_prefix: Vec<String>,
    pub unique_session: bool,
    pub friend_message_needs_wake_prefix: bool,
    pub enable_id_white_list: bool,
    pub id_whitelist: Vec<String>,
    pub id_whitelist_log: bool,
    pub wl_ignore_admin_on_group: bool,
    pub wl_ignore_admin_on_friend: bool,
    pub admins_id: Vec<String>,
}

impl Default for AstrBotPlatformGates {
    fn default() -> Self {
        Self {
            wake_prefix: vec!["/".into()],
            unique_session: false,
            friend_message_needs_wake_prefix: false,
            enable_id_white_list: true,
            id_whitelist: Vec::new(),
            id_whitelist_log: true,
            wl_ignore_admin_on_group: true,
            wl_ignore_admin_on_friend: true,
            admins_id: vec!["astrbot".into()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotSubagentRow {
    pub provider_id: String,
    pub persona_id: String,
}

impl Default for AstrBotSubagentRow {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            persona_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotSubagentConfig {
    pub main_enable: bool,
    pub remove_main_duplicate_tools: bool,
    pub router_system_prompt: String,
    pub agents: Vec<AstrBotSubagentRow>,
}

impl Default for AstrBotSubagentConfig {
    fn default() -> Self {
        Self {
            main_enable: false,
            remove_main_duplicate_tools: false,
            router_system_prompt: String::new(),
            agents: Vec::new(),
        }
    }
}

pub fn sources_from_root(root: &Value) -> Vec<AstrBotProviderSource> {
    as_array(root, "provider_sources")
        .iter()
        .filter_map(|row| parse_source(row))
        .collect()
}

pub fn models_from_root(root: &Value) -> Vec<AstrBotProviderModel> {
    as_array(root, "provider")
        .iter()
        .filter_map(|row| parse_model(row))
        .collect()
}

pub fn ai_from_root(root: &Value) -> AstrBotAiSettings {
    let mut out = AstrBotAiSettings::default();
    let ps = root.get("provider_settings").and_then(Value::as_object);
    let Some(ps) = ps else {
        return out;
    };
    out.enable = bool_of(ps, "enable", out.enable);
    out.default_provider_id = str_of(ps, "default_provider_id");
    out.fallback_chat_models = str_list_of(ps, "fallback_chat_models");
    out.default_image_caption_provider_id = str_of(ps, "default_image_caption_provider_id");
    if let Some(s) = ps.get("image_caption_prompt").and_then(Value::as_str) {
        out.image_caption_prompt = s.to_string();
    }
    if let Some(s) = ps.get("default_personality").and_then(Value::as_str) {
        out.default_personality = s.to_string();
    }
    out.wake_prefix = str_of(ps, "wake_prefix");
    if let Some(s) = ps.get("prompt_prefix").and_then(Value::as_str) {
        out.prompt_prefix = s.to_string();
    }
    if let Some(s) = ps.get("agent_runner_type").and_then(Value::as_str) {
        out.agent_runner_type = s.to_string();
    }
    out.dify_agent_runner_provider_id = str_of(ps, "dify_agent_runner_provider_id");
    out.coze_agent_runner_provider_id = str_of(ps, "coze_agent_runner_provider_id");
    out.dashscope_agent_runner_provider_id = str_of(ps, "dashscope_agent_runner_provider_id");
    out.deerflow_agent_runner_provider_id = str_of(ps, "deerflow_agent_runner_provider_id");
    out.max_context_length = i64_of(ps, "max_context_length", out.max_context_length);
    out.dequeue_context_length = i64_of(ps, "dequeue_context_length", out.dequeue_context_length);
    if let Some(s) = ps
        .get("context_limit_reached_strategy")
        .and_then(Value::as_str)
    {
        out.context_limit_reached_strategy = s.to_string();
    }
    out.llm_compress_instruction = str_of(ps, "llm_compress_instruction");
    out.llm_compress_keep_recent = i64_of(ps, "llm_compress_keep_recent", out.llm_compress_keep_recent);
    out.llm_compress_provider_id = str_of(ps, "llm_compress_provider_id");
    out.streaming_response = bool_of(ps, "streaming_response", out.streaming_response);
    out.display_reasoning_text = bool_of(ps, "display_reasoning_text", out.display_reasoning_text);
    out.llm_safety_mode = bool_of(ps, "llm_safety_mode", out.llm_safety_mode);
    out.identifier = bool_of(ps, "identifier", out.identifier);
    out.group_name_display = bool_of(ps, "group_name_display", out.group_name_display);
    out.datetime_system_prompt = bool_of(ps, "datetime_system_prompt", out.datetime_system_prompt);
    out.show_tool_use_status = bool_of(ps, "show_tool_use_status", out.show_tool_use_status);
    out.show_tool_call_result = bool_of(ps, "show_tool_call_result", out.show_tool_call_result);
    out.max_agent_step = i64_of(ps, "max_agent_step", out.max_agent_step);
    out.tool_call_timeout = i64_of(ps, "tool_call_timeout", out.tool_call_timeout);
    if let Some(s) = ps.get("tool_schema_mode").and_then(Value::as_str) {
        out.tool_schema_mode = s.to_string();
    }
    out
}

pub fn stt_from_root(root: &Value) -> AstrBotSttSettings {
    let mut out = AstrBotSttSettings::default();
    let Some(o) = root.get("provider_stt_settings").and_then(Value::as_object) else {
        return out;
    };
    out.enable = bool_of(o, "enable", out.enable);
    out.provider_id = str_of(o, "provider_id");
    out
}

pub fn tts_from_root(root: &Value) -> AstrBotTtsSettings {
    let mut out = AstrBotTtsSettings::default();
    let Some(o) = root.get("provider_tts_settings").and_then(Value::as_object) else {
        return out;
    };
    out.enable = bool_of(o, "enable", out.enable);
    out.provider_id = str_of(o, "provider_id");
    out.dual_output = bool_of(o, "dual_output", out.dual_output);
    if let Some(n) = o.get("trigger_probability").and_then(Value::as_f64) {
        out.trigger_probability = n;
    }
    out
}

pub fn websearch_from_root(root: &Value) -> AstrBotWebSearchSettings {
    let mut out = AstrBotWebSearchSettings::default();
    let Some(ps) = root.get("provider_settings").and_then(Value::as_object) else {
        return out;
    };
    out.enable = bool_of(ps, "web_search", out.enable);
    if let Some(s) = ps.get("websearch_provider").and_then(Value::as_str) {
        out.provider = s.to_string();
    }
    out.tavily_key = str_list_of(ps, "websearch_tavily_key");
    out.bocha_key = str_list_of(ps, "websearch_bocha_key");
    out.baidu_app_builder_key = str_of(ps, "websearch_baidu_app_builder_key");
    out.show_link = bool_of(ps, "web_search_link", out.show_link);
    out
}

pub fn kb_from_root(root: &Value) -> AstrBotKbBind {
    let mut out = AstrBotKbBind::default();
    if let Some(arr) = root.get("kb_names").and_then(Value::as_array) {
        out.names = arr
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
    }
    out.fusion_top_k = root
        .get("kb_fusion_top_k")
        .and_then(Value::as_i64)
        .unwrap_or(out.fusion_top_k);
    out.final_top_k = root
        .get("kb_final_top_k")
        .and_then(Value::as_i64)
        .unwrap_or(out.final_top_k);
    out.agentic_mode = root
        .get("kb_agentic_mode")
        .and_then(Value::as_bool)
        .unwrap_or(out.agentic_mode);
    out
}

pub fn gates_from_root(root: &Value) -> AstrBotPlatformGates {
    let mut out = AstrBotPlatformGates::default();
    if let Some(arr) = root.get("wake_prefix").and_then(Value::as_array) {
        out.wake_prefix = arr
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
    }
    if let Some(arr) = root.get("admins_id").and_then(Value::as_array) {
        out.admins_id = arr
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect();
    }
    let Some(ps) = root.get("platform_settings").and_then(Value::as_object) else {
        return out;
    };
    out.unique_session = bool_of(ps, "unique_session", out.unique_session);
    out.friend_message_needs_wake_prefix =
        bool_of(ps, "friend_message_needs_wake_prefix", out.friend_message_needs_wake_prefix);
    out.enable_id_white_list = bool_of(ps, "enable_id_white_list", out.enable_id_white_list);
    out.id_whitelist = str_list_of(ps, "id_whitelist");
    out.id_whitelist_log = bool_of(ps, "id_whitelist_log", out.id_whitelist_log);
    out.wl_ignore_admin_on_group = bool_of(ps, "wl_ignore_admin_on_group", out.wl_ignore_admin_on_group);
    out.wl_ignore_admin_on_friend =
        bool_of(ps, "wl_ignore_admin_on_friend", out.wl_ignore_admin_on_friend);
    out
}

pub fn subagent_from_root(root: &Value) -> AstrBotSubagentConfig {
    let mut out = AstrBotSubagentConfig::default();
    let Some(o) = root.get("subagent_orchestrator").and_then(Value::as_object) else {
        return out;
    };
    out.main_enable = bool_of(o, "main_enable", out.main_enable);
    out.remove_main_duplicate_tools =
        bool_of(o, "remove_main_duplicate_tools", out.remove_main_duplicate_tools);
    if let Some(s) = o.get("router_system_prompt").and_then(Value::as_str) {
        out.router_system_prompt = s.to_string();
    }
    if let Some(arr) = o.get("agents").and_then(Value::as_array) {
        out.agents = arr
            .iter()
            .filter_map(|row| {
                let obj = row.as_object()?;
                Some(AstrBotSubagentRow {
                    provider_id: str_of(obj, "provider_id"),
                    persona_id: str_of(obj, "persona_id"),
                })
            })
            .collect();
    }
    out
}

/// 只改已知指针，其它键原样留着（含 `provider_settings` 兄弟字段）。
pub fn apply_ai_patch(
    root: &mut Value,
    sources: &[AstrBotProviderSource],
    models: &[AstrBotProviderModel],
    ai: &AstrBotAiSettings,
    stt: &AstrBotSttSettings,
    tts: &AstrBotTtsSettings,
    websearch: &AstrBotWebSearchSettings,
    kb: &AstrBotKbBind,
    gates: &AstrBotPlatformGates,
    subagent: &AstrBotSubagentConfig,
) -> Result<(), String> {
    let obj = root
        .as_object_mut()
        .ok_or_else(|| "cmd_config.json 根必须是对象".to_string())?;
    obj.insert(
        "provider_sources".into(),
        Value::Array(sources.iter().map(source_to_value).collect()),
    );
    obj.insert(
        "provider".into(),
        Value::Array(models.iter().map(model_to_value).collect()),
    );
    patch_object(obj, "provider_settings", |ps| {
        ps.insert("enable".into(), Value::Bool(ai.enable));
        ps.insert(
            "default_provider_id".into(),
            Value::String(ai.default_provider_id.clone()),
        );
        ps.insert(
            "fallback_chat_models".into(),
            str_list_value(&ai.fallback_chat_models),
        );
        ps.insert(
            "default_image_caption_provider_id".into(),
            Value::String(ai.default_image_caption_provider_id.clone()),
        );
        ps.insert(
            "image_caption_prompt".into(),
            Value::String(ai.image_caption_prompt.clone()),
        );
        ps.insert(
            "default_personality".into(),
            Value::String(ai.default_personality.clone()),
        );
        ps.insert("wake_prefix".into(), Value::String(ai.wake_prefix.clone()));
        ps.insert(
            "prompt_prefix".into(),
            Value::String(ai.prompt_prefix.clone()),
        );
        ps.insert(
            "agent_runner_type".into(),
            Value::String(ai.agent_runner_type.clone()),
        );
        ps.insert(
            "dify_agent_runner_provider_id".into(),
            Value::String(ai.dify_agent_runner_provider_id.clone()),
        );
        ps.insert(
            "coze_agent_runner_provider_id".into(),
            Value::String(ai.coze_agent_runner_provider_id.clone()),
        );
        ps.insert(
            "dashscope_agent_runner_provider_id".into(),
            Value::String(ai.dashscope_agent_runner_provider_id.clone()),
        );
        ps.insert(
            "deerflow_agent_runner_provider_id".into(),
            Value::String(ai.deerflow_agent_runner_provider_id.clone()),
        );
        ps.insert("max_context_length".into(), Value::from(ai.max_context_length));
        ps.insert(
            "dequeue_context_length".into(),
            Value::from(ai.dequeue_context_length),
        );
        ps.insert(
            "context_limit_reached_strategy".into(),
            Value::String(ai.context_limit_reached_strategy.clone()),
        );
        ps.insert(
            "llm_compress_instruction".into(),
            Value::String(ai.llm_compress_instruction.clone()),
        );
        ps.insert(
            "llm_compress_keep_recent".into(),
            Value::from(ai.llm_compress_keep_recent),
        );
        ps.insert(
            "llm_compress_provider_id".into(),
            Value::String(ai.llm_compress_provider_id.clone()),
        );
        ps.insert("streaming_response".into(), Value::Bool(ai.streaming_response));
        ps.insert(
            "display_reasoning_text".into(),
            Value::Bool(ai.display_reasoning_text),
        );
        ps.insert("llm_safety_mode".into(), Value::Bool(ai.llm_safety_mode));
        ps.insert("identifier".into(), Value::Bool(ai.identifier));
        ps.insert(
            "group_name_display".into(),
            Value::Bool(ai.group_name_display),
        );
        ps.insert(
            "datetime_system_prompt".into(),
            Value::Bool(ai.datetime_system_prompt),
        );
        ps.insert(
            "show_tool_use_status".into(),
            Value::Bool(ai.show_tool_use_status),
        );
        ps.insert(
            "show_tool_call_result".into(),
            Value::Bool(ai.show_tool_call_result),
        );
        ps.insert("max_agent_step".into(), Value::from(ai.max_agent_step));
        ps.insert("tool_call_timeout".into(), Value::from(ai.tool_call_timeout));
        ps.insert(
            "tool_schema_mode".into(),
            Value::String(ai.tool_schema_mode.clone()),
        );
        ps.insert("web_search".into(), Value::Bool(websearch.enable));
        ps.insert(
            "websearch_provider".into(),
            Value::String(websearch.provider.clone()),
        );
        ps.insert(
            "websearch_tavily_key".into(),
            str_list_value(&websearch.tavily_key),
        );
        ps.insert(
            "websearch_bocha_key".into(),
            str_list_value(&websearch.bocha_key),
        );
        ps.insert(
            "websearch_baidu_app_builder_key".into(),
            Value::String(websearch.baidu_app_builder_key.clone()),
        );
        ps.insert("web_search_link".into(), Value::Bool(websearch.show_link));
    });
    patch_object(obj, "provider_stt_settings", |st| {
        st.insert("enable".into(), Value::Bool(stt.enable));
        st.insert("provider_id".into(), Value::String(stt.provider_id.clone()));
    });
    patch_object(obj, "provider_tts_settings", |tt| {
        tt.insert("enable".into(), Value::Bool(tts.enable));
        tt.insert("provider_id".into(), Value::String(tts.provider_id.clone()));
        tt.insert("dual_output".into(), Value::Bool(tts.dual_output));
        tt.insert(
            "trigger_probability".into(),
            json_number(tts.trigger_probability),
        );
    });
    patch_object(obj, "platform_settings", |ps| {
        ps.insert("unique_session".into(), Value::Bool(gates.unique_session));
        ps.insert(
            "friend_message_needs_wake_prefix".into(),
            Value::Bool(gates.friend_message_needs_wake_prefix),
        );
        ps.insert(
            "enable_id_white_list".into(),
            Value::Bool(gates.enable_id_white_list),
        );
        ps.insert("id_whitelist".into(), str_list_value(&gates.id_whitelist));
        ps.insert("id_whitelist_log".into(), Value::Bool(gates.id_whitelist_log));
        ps.insert(
            "wl_ignore_admin_on_group".into(),
            Value::Bool(gates.wl_ignore_admin_on_group),
        );
        ps.insert(
            "wl_ignore_admin_on_friend".into(),
            Value::Bool(gates.wl_ignore_admin_on_friend),
        );
    });
    obj.insert("wake_prefix".into(), str_list_value(&gates.wake_prefix));
    obj.insert("admins_id".into(), str_list_value(&gates.admins_id));
    obj.insert("kb_names".into(), str_list_value(&kb.names));
    obj.insert("kb_fusion_top_k".into(), Value::from(kb.fusion_top_k));
    obj.insert("kb_final_top_k".into(), Value::from(kb.final_top_k));
    obj.insert("kb_agentic_mode".into(), Value::Bool(kb.agentic_mode));
    patch_object(obj, "subagent_orchestrator", |so| {
        so.insert("main_enable".into(), Value::Bool(subagent.main_enable));
        so.insert(
            "remove_main_duplicate_tools".into(),
            Value::Bool(subagent.remove_main_duplicate_tools),
        );
        so.insert(
            "router_system_prompt".into(),
            Value::String(subagent.router_system_prompt.clone()),
        );
        let agents: Vec<Value> = subagent
            .agents
            .iter()
            .map(|a| {
                let mut m = Map::new();
                m.insert("provider_id".into(), Value::String(a.provider_id.clone()));
                m.insert("persona_id".into(), Value::String(a.persona_id.clone()));
                Value::Object(m)
            })
            .collect();
        so.insert("agents".into(), Value::Array(agents));
    });
    Ok(())
}

pub fn restore_extras(root: &Value, sources: &mut [AstrBotProviderSource], models: &mut [AstrBotProviderModel]) {
    let old_sources = sources_from_root(root);
    for src in sources.iter_mut() {
        if src.extra.is_empty()
            && let Some(old) = old_sources.iter().find(|e| e.id == src.id)
        {
            src.extra = old.extra.clone();
        }
    }
    let old_models = models_from_root(root);
    for row in models.iter_mut() {
        if row.extra.is_empty()
            && let Some(old) = old_models.iter().find(|e| e.id == row.id)
        {
            row.extra = old.extra.clone();
        }
    }
}

pub fn validate_ai(
    sources: &[AstrBotProviderSource],
    models: &[AstrBotProviderModel],
) -> Vec<AppConfigIssue> {
    let mut sink = IssueSink::default();
    let mut seen = std::collections::HashSet::new();
    for (i, s) in sources.iter().enumerate() {
        if s.id.trim().is_empty() {
            sink.push(format!("sources/{i}/id"), "提供商 id 不能为空");
            continue;
        }
        if !seen.insert(s.id.as_str()) {
            sink.push(format!("sources/{i}/id"), "提供商 id 重复");
        }
    }
    let source_ids: std::collections::HashSet<&str> =
        sources.iter().map(|s| s.id.as_str()).collect();
    let mut model_ids = std::collections::HashSet::new();
    for (i, m) in models.iter().enumerate() {
        if m.id.trim().is_empty() {
            sink.push(format!("models/{i}/id"), "模型 id 不能为空");
        } else if !model_ids.insert(m.id.as_str()) {
            sink.push(format!("models/{i}/id"), "模型 id 重复");
        }
        if !m.provider_source_id.is_empty() && !source_ids.contains(m.provider_source_id.as_str()) {
            sink.push(
                format!("models/{i}/provider_source_id"),
                "模型引用了不存在的提供商",
            );
        }
    }
    sink.into_vec()
}

pub fn source_to_value(src: &AstrBotProviderSource) -> Value {
    let mut m = src.extra.clone();
    m.insert("id".into(), Value::String(src.id.clone()));
    m.insert("provider".into(), Value::String(src.provider.clone()));
    m.insert("type".into(), Value::String(src.type_name.clone()));
    m.insert(
        "provider_type".into(),
        Value::String(src.provider_type.clone()),
    );
    m.insert("enable".into(), Value::Bool(src.enable));
    m.insert("key".into(), str_list_value(&src.key));
    m.insert("api_base".into(), Value::String(src.api_base.clone()));
    m.insert("timeout".into(), Value::from(src.timeout));
    m.insert("proxy".into(), Value::String(src.proxy.clone()));
    Value::Object(m)
}

pub fn model_to_value(row: &AstrBotProviderModel) -> Value {
    let mut m = row.extra.clone();
    m.insert("id".into(), Value::String(row.id.clone()));
    m.insert("enable".into(), Value::Bool(row.enable));
    m.insert(
        "provider_source_id".into(),
        Value::String(row.provider_source_id.clone()),
    );
    m.insert("model".into(), Value::String(row.model.clone()));
    m.insert("modalities".into(), str_list_value(&row.modalities));
    m.insert(
        "max_context_tokens".into(),
        Value::from(row.max_context_tokens),
    );
    Value::Object(m)
}

fn parse_source(row: &Value) -> Option<AstrBotProviderSource> {
    let obj = row.as_object()?;
    let mut extra = obj.clone();
    for k in [
        "id",
        "provider",
        "type",
        "provider_type",
        "enable",
        "key",
        "api_base",
        "timeout",
        "proxy",
    ] {
        extra.remove(k);
    }
    Some(AstrBotProviderSource {
        id: str_of(obj, "id"),
        provider: nonempty(str_of(obj, "provider"), "openai"),
        type_name: nonempty(str_of(obj, "type"), "openai_chat_completion"),
        provider_type: nonempty(str_of(obj, "provider_type"), "chat_completion"),
        enable: bool_of(obj, "enable", true),
        key: str_or_list(obj, "key"),
        api_base: str_of(obj, "api_base"),
        timeout: i64_of(obj, "timeout", 120),
        proxy: str_of(obj, "proxy"),
        extra,
    })
}

fn parse_model(row: &Value) -> Option<AstrBotProviderModel> {
    let obj = row.as_object()?;
    let mut extra = obj.clone();
    for k in [
        "id",
        "enable",
        "provider_source_id",
        "model",
        "modalities",
        "max_context_tokens",
    ] {
        extra.remove(k);
    }
    Some(AstrBotProviderModel {
        id: str_of(obj, "id"),
        enable: bool_of(obj, "enable", false),
        provider_source_id: str_of(obj, "provider_source_id"),
        model: str_of(obj, "model"),
        modalities: {
            let v = str_list_of(obj, "modalities");
            if v.is_empty() {
                vec!["text".into()]
            } else {
                v
            }
        },
        max_context_tokens: i64_of(obj, "max_context_tokens", 0),
        extra,
    })
}

fn patch_object(root: &mut Map<String, Value>, key: &str, f: impl FnOnce(&mut Map<String, Value>)) {
    let entry = root.entry(key.to_string()).or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(obj) = entry.as_object_mut() {
        f(obj);
    }
}

fn as_array<'a>(root: &'a Value, key: &str) -> &'a [Value] {
    root.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn str_of(obj: &Map<String, Value>, key: &str) -> String {
    obj.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn nonempty(s: String, fallback: &str) -> String {
    if s.is_empty() {
        fallback.to_string()
    } else {
        s
    }
}

fn bool_of(obj: &Map<String, Value>, key: &str, default: bool) -> bool {
    obj.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn i64_of(obj: &Map<String, Value>, key: &str, default: i64) -> i64 {
    obj.get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok())))
        .unwrap_or(default)
}

fn str_list_of(obj: &Map<String, Value>, key: &str) -> Vec<String> {
    obj.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn str_or_list(obj: &Map<String, Value>, key: &str) -> Vec<String> {
    if let Some(s) = obj.get(key).and_then(Value::as_str) {
        if s.is_empty() {
            return Vec::new();
        }
        return vec![s.to_string()];
    }
    str_list_of(obj, key)
}

fn str_list_value(items: &[String]) -> Value {
    Value::Array(items.iter().cloned().map(Value::String).collect())
}

fn json_number(v: f64) -> Value {
    serde_json::Number::from_f64(v)
        .map(Value::Number)
        .unwrap_or(Value::from(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apply_ai_patch_keeps_provider_settings_siblings() {
        let mut root = json!({
            "provider_settings": {
                "enable": true,
                "wake_prefix": "chat",
                "mystery": 1
            },
            "keep": true
        });
        let mut ai = AstrBotAiSettings::default();
        ai.enable = false;
        apply_ai_patch(
            &mut root,
            &[],
            &[],
            &ai,
            &AstrBotSttSettings::default(),
            &AstrBotTtsSettings::default(),
            &AstrBotWebSearchSettings::default(),
            &AstrBotKbBind::default(),
            &AstrBotPlatformGates::default(),
            &AstrBotSubagentConfig::default(),
        )
        .unwrap();
        assert_eq!(root["provider_settings"]["enable"], false);
        assert_eq!(root["provider_settings"]["mystery"], 1);
        assert_eq!(root["keep"], true);
        assert_eq!(root["provider_settings"]["wake_prefix"], "");
    }

    #[test]
    fn extra_keys_round_trip_on_source() {
        let row = json!({
            "id": "ds",
            "provider": "deepseek",
            "type": "openai_chat_completion",
            "provider_type": "chat_completion",
            "enable": true,
            "key": ["sk"],
            "api_base": "https://api.deepseek.com/v1",
            "gm_native_search": true
        });
        let src = parse_source(&row).unwrap();
        assert!(src.extra.get("gm_native_search").is_some());
        let back = source_to_value(&src);
        assert_eq!(back["gm_native_search"], true);
        assert_eq!(back["id"], "ds");
    }

    #[test]
    fn validate_rejects_orphan_model() {
        let issues = validate_ai(
            &[AstrBotProviderSource {
                id: "a".into(),
                ..AstrBotProviderSource::default()
            }],
            &[AstrBotProviderModel {
                id: "a/m".into(),
                provider_source_id: "missing".into(),
                ..AstrBotProviderModel::default()
            }],
        );
        assert!(!issues.is_empty());
    }

    #[test]
    fn empty_provider_array_is_valid() {
        assert!(validate_ai(&[], &[]).is_empty());
    }

    #[test]
    fn duplicate_source_id_is_rejected() {
        let issues = validate_ai(
            &[
                AstrBotProviderSource {
                    id: "a".into(),
                    ..AstrBotProviderSource::default()
                },
                AstrBotProviderSource {
                    id: "a".into(),
                    ..AstrBotProviderSource::default()
                },
            ],
            &[],
        );
        assert!(issues.iter().any(|i| i.path.contains("sources")));
    }
}
