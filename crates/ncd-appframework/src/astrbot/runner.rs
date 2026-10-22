//! AstrBot v4.29 的 agent_runner 根节与现有模型 / 人格 / 回复控件之间的映射。
//! 老实例仍使用 provider_settings；只在文件已采用新格式时读写新位置。
use serde_json::{Map, Value};
use super::ai::AstrBotAiSettings;

fn text(value: &Value, pointer: &str, default: &str) -> String {
    value.pointer(pointer).and_then(Value::as_str).unwrap_or(default).to_owned()
}
fn number(value: &Value, pointer: &str, default: i64) -> i64 {
    value.pointer(pointer).and_then(Value::as_i64).unwrap_or(default)
}

pub fn read(root: &Value, out: &mut AstrBotAiSettings) {
    let Some(runner) = root.get("agent_runner").filter(|v| v.is_object()) else { return; };
    out.agent_runner_type = text(runner, "/runner_type", "local");
    if out.agent_runner_type != "local" { return; }
    let config = runner.get("config").unwrap_or(&Value::Null);
    out.default_provider_id = text(config, "/model/provider_id", "");
    out.fallback_chat_models = config.pointer("/model/fallback_provider_ids")
        .and_then(Value::as_array).map(|values| values.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();
    out.default_personality = text(config, "/persona/persona_id", "default");
    out.llm_safety_mode = config.pointer("/persona/safety_mode").and_then(Value::as_bool).unwrap_or(true);
    out.max_context_length = number(config, "/compression/max_turns", -1);
    out.dequeue_context_length = number(config, "/compression/trim_turns", 1);
    out.context_limit_reached_strategy = text(config, "/compression/overflow_strategy", "llm_compress");
    out.llm_compress_instruction = text(config, "/compression/instruction", "");
    out.llm_compress_provider_id = text(config, "/compression/provider_id", "");
    out.max_agent_step = number(config, "/misc/max_steps", 128);
    out.tool_call_timeout = number(config, "/misc/tool_call_timeout", 120);
    out.tool_schema_mode = text(config, "/misc/tool_schema_mode", "full");
}

/// 已采用新版格式的实例只更新对应叶；压缩比例、重试、第三方执行器配置和未来字段均保留。
pub fn apply(root: &mut Value, ai: &AstrBotAiSettings) {
    let Some(runner) = root.get_mut("agent_runner").filter(|v| v.is_object()) else { return; };
    if runner.get("runner_type").and_then(Value::as_str).unwrap_or("local") != "local" { return; }
    let Some(obj) = runner.as_object_mut() else { return; };
    let config = obj.entry("config").or_insert_with(|| Value::Object(Map::new()));
    set(config, &["model", "provider_id"], Value::String(ai.default_provider_id.clone()));
    set(config, &["model", "fallback_provider_ids"], serde_json::json!(ai.fallback_chat_models));
    set(config, &["persona", "persona_id"], Value::String(ai.default_personality.clone()));
    set(config, &["persona", "safety_mode"], Value::Bool(ai.llm_safety_mode));
    set(config, &["compression", "max_turns"], Value::from(ai.max_context_length));
    set(config, &["compression", "trim_turns"], Value::from(ai.dequeue_context_length));
    set(config, &["compression", "overflow_strategy"], Value::String(ai.context_limit_reached_strategy.clone()));
    set(config, &["compression", "instruction"], Value::String(ai.llm_compress_instruction.clone()));
    set(config, &["compression", "provider_id"], Value::String(ai.llm_compress_provider_id.clone()));
    set(config, &["misc", "max_steps"], Value::from(ai.max_agent_step));
    set(config, &["misc", "tool_call_timeout"], Value::from(ai.tool_call_timeout));
    set(config, &["misc", "tool_schema_mode"], Value::String(ai.tool_schema_mode.clone()));
}

fn set(root: &mut Value, path: &[&str], value: Value) {
    let Some((head, tail)) = path.split_first() else { return; };
    if !root.is_object() { *root = Value::Object(Map::new()); }
    let obj = root.as_object_mut().expect("object just initialized");
    if tail.is_empty() { obj.insert((*head).to_owned(), value); }
    else {
        let child = obj.entry((*head).to_owned()).or_insert_with(|| Value::Object(Map::new()));
        set(child, tail, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn modern_root_is_authoritative_and_keeps_unmapped_fields() {
        let mut root = json!({
            "provider_settings":{"default_provider_id":"stale"},
            "agent_runner":{"runner_type":"local","config":{
                "model":{"provider_id":"openai/new","request_max_retries":7},
                "persona":{"persona_id":"kind","safety_mode":false,"future":1},
                "compression":{"max_turns":25,"keep_recent_ratio":0.2},
                "misc":{"max_steps":64,"tool_call_timeout":90,"future":2}
            }}
        });
        let mut ai = AstrBotAiSettings::default();
        read(&root, &mut ai);
        assert_eq!(ai.default_provider_id, "openai/new");
        assert_eq!(ai.default_personality, "kind");
        assert!(!ai.llm_safety_mode);
        assert_eq!(ai.max_context_length, 25);
        ai.default_provider_id = "openai/changed".into();
        apply(&mut root, &ai);
        assert_eq!(root.pointer("/agent_runner/config/model/provider_id").unwrap(), "openai/changed");
        assert_eq!(root.pointer("/agent_runner/config/model/request_max_retries").unwrap(), 7);
        assert_eq!(root.pointer("/agent_runner/config/compression/keep_recent_ratio").unwrap(), 0.2);
        assert_eq!(root.pointer("/agent_runner/config/misc/future").unwrap(), 2);
    }

    #[test]
    fn legacy_and_third_party_runners_remain_unchanged() {
        for mut root in [
            json!({"provider_settings":{"default_provider_id":"old"}}),
            json!({"agent_runner":{"runner_type":"dify","config":{"dify_api_key":"key","future":1}}}),
        ] {
            let old = root.clone();
            apply(&mut root, &AstrBotAiSettings::default());
            assert_eq!(root, old);
        }
    }
}
