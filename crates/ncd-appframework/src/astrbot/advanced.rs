//! 非专页配置的差量投影。表单只携带未被专页拥有的字段；保存时对读取基线做差量，
//! 不用过期的整份高级配置覆盖上游在表单打开后写入的新值。
use ncd_traits::AppFrameworkError;
use serde_json::{Map, Value};

/// 这些值由已有页面和连接协调器负责；高级表单不能抢写。
pub const MANAGED_PATHS: &[&str] = &[
    "config_version", "provider_sources", "provider", "platform",
    "dashboard.port", "dashboard.username", "dashboard.password",
    "dashboard.pbkdf2_password", "dashboard.password_storage_upgraded",
    "dashboard.password_change_required", "dashboard.jwt_secret",
    "wake_prefix", "admins_id", "kb_names", "kb_fusion_top_k", "kb_final_top_k", "kb_agentic_mode",
    "subagent_orchestrator",
    "provider_settings.enable", "provider_settings.default_provider_id",
    "provider_settings.fallback_chat_models", "provider_settings.default_image_caption_provider_id",
    "provider_settings.image_caption_prompt", "provider_settings.default_personality",
    "provider_settings.wake_prefix", "provider_settings.prompt_prefix",
    "provider_settings.streaming_response", "provider_settings.llm_safety_mode",
    "provider_settings.max_context_length", "provider_settings.context_limit_reached_strategy",
    "provider_settings.max_agent_step", "provider_settings.tool_call_timeout",
    "provider_settings.web_search",
    "provider_stt_settings.enable", "provider_stt_settings.provider_id",
    "provider_tts_settings.enable", "provider_tts_settings.provider_id",
    "platform_settings.friend_message_needs_wake_prefix", "platform_settings.unique_session",
    "platform_settings.enable_id_white_list", "platform_settings.id_whitelist",
    "agent_runner.config.model.provider_id", "agent_runner.config.model.fallback_provider_ids",
    "agent_runner.config.persona.persona_id", "agent_runner.config.persona.safety_mode",
    "agent_runner.config.compression.max_turns", "agent_runner.config.compression.overflow_strategy",
    "agent_runner.config.misc.max_steps", "agent_runner.config.misc.tool_call_timeout",
];

pub fn project(root: &Value) -> Value {
    let mut value = root.clone();
    if !value.is_object() { return Value::Object(Map::new()); }
    for path in MANAGED_PATHS {
        remove_path(&mut value, &path.split('.').collect::<Vec<_>>());
    }
    value
}

fn remove_path(value: &mut Value, parts: &[&str]) {
    let Some((head, tail)) = parts.split_first() else { return; };
    let Some(obj) = value.as_object_mut() else { return; };
    if tail.is_empty() { obj.remove(*head); }
    else if let Some(child) = obj.get_mut(*head) {
        // 保护叶的祖先也属于保留对象容器；标量/数组不能借整节替换绕过保护。
        if !child.is_object() { obj.remove(*head); return; }
        remove_path(child, tail);
        if child.as_object().is_some_and(Map::is_empty) { obj.remove(*head); }
    }
}

pub fn encode(root: &Value) -> String {
    serde_json::to_string_pretty(&project(root)).unwrap_or_else(|_| "{}".into())
}

pub fn parse(text: &str) -> Result<Value, AppFrameworkError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| AppFrameworkError::Validation(format!("高级设置不是有效 JSON: {e}")))?;
    if !value.is_object() {
        return Err(AppFrameworkError::Validation("高级设置必须是 JSON 对象".into()));
    }
    Ok(value)
}

/// 旧版前端没有这两个字段时保持当前行为；同时过滤专页字段，不能通过 JSON 兜底改端口或认领行。
pub fn apply(root: &mut Value, base: &str, edited: &str) -> Result<(), AppFrameworkError> {
    if edited.trim().is_empty() { return Ok(()); }
    let after = project(&parse(edited)?);
    let before = if base.trim().is_empty() { project(root) } else { project(&parse(base)?) };
    apply_delta(root, &before, &after);
    Ok(())
}

/// 切换执行器先重置其参数，再让专页把本次模型/人格/上下文草稿投影到最终执行器。
pub fn prepare_runner_change(root: &mut Value, base: &str, edited: &str) -> Result<(), AppFrameworkError> {
    if edited.trim().is_empty() { return Ok(()); }
    let after = project(&parse(edited)?);
    let before = if base.trim().is_empty() { project(root) } else { project(&parse(base)?) };
    let old_runner = before.pointer("/agent_runner/runner_type");
    let new_runner = after.pointer("/agent_runner/runner_type");
    if old_runner != new_runner && let Some(value) = new_runner {
        if !value.as_str().is_some_and(|kind| !kind.trim().is_empty()) {
            return Err(AppFrameworkError::Validation("Agent 执行器必须是非空字符串".into()));
        }
        if let (Some(obj), Some(runner)) = (root.as_object_mut(), after.get("agent_runner")) {
            obj.insert("agent_runner".into(), runner.clone());
        }
    }
    Ok(())
}

/// 类型化旧投影里还有只读兼容字段；只有专页拥有的路径能从该投影写回。
/// 从最新文档开始逐个拷贝专页路径，避免旧兼容值覆盖高级页未修改的并发更新。
pub fn retain_owned_changes(target: &mut Value, latest: &Value) {
    let typed = target.clone();
    *target = latest.clone();
    for path in MANAGED_PATHS {
        let parts = path.split('.').collect::<Vec<_>>();
        if let Some(value) = get_path(&typed, &parts) { set_path(target, &parts, value.clone()); }
    }
}

fn get_path<'a>(root: &'a Value, parts: &[&str]) -> Option<&'a Value> {
    let mut value = root;
    for part in parts { value = value.as_object()?.get(*part)?; }
    Some(value)
}
fn set_path(root: &mut Value, parts: &[&str], value: Value) {
    let Some((head, tail)) = parts.split_first() else { return; };
    if !root.is_object() { *root = Value::Object(Map::new()); }
    let obj = root.as_object_mut().expect("object just initialized");
    if tail.is_empty() { obj.insert((*head).to_owned(), value); }
    else {
        let child = obj.entry((*head).to_owned()).or_insert_with(|| Value::Object(Map::new()));
        set_path(child, tail, value);
    }
}

fn apply_delta(target: &mut Value, before: &Value, after: &Value) {
    if before == after { return; }
    if let (Some(old), Some(new)) = (before.as_object(), after.as_object()) {
        if !target.is_object() { *target = Value::Object(Map::new()); }
        let target = target.as_object_mut().expect("object just initialized");
        for key in old.keys().filter(|key| !new.contains_key(*key)) {
            if let Some(old_value) = old.get(key).filter(|v| v.is_object()) {
                if let Some(slot) = target.get_mut(key) {
                    apply_delta(slot, old_value, &Value::Object(Map::new()));
                    if slot.as_object().is_some_and(Map::is_empty) { target.remove(key); }
                }
            } else { target.remove(key); }
        }
        for (key, value) in new {
            match old.get(key) {
                Some(old_value) if old_value == value => {}
                Some(old_value) => {
                    let slot = target.entry(key.clone()).or_insert(Value::Null);
                    apply_delta(slot, old_value, value);
                }
                None if value.is_object() => {
                    let slot = target.entry(key.clone()).or_insert_with(|| Value::Object(Map::new()));
                    apply_delta(slot, &Value::Object(Map::new()), value);
                }
                None => { target.insert(key.clone(), value.clone()); }
            }
        }
    } else { *target = after.clone(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn changed_leaf_preserves_new_siblings_and_dedicated_fields() {
        let original = json!({
            "platform": [{"id":"ncd-app:a"}],
            "provider_settings": {"enable":true, "websearch_brave_key":["old"], "future":1},
            "dashboard":{"port":6185,"password":"private","ssl":{"enable":false}}
        });
        let base = encode(&original);
        let mut edited = parse(&base).unwrap();
        edited["provider_settings"]["websearch_brave_key"] = json!(["new"]);
        edited["platform"] = json!([]);
        edited["dashboard"]["port"] = json!(9);
        let mut current = original.clone();
        current["provider_settings"]["new_from_upstream"] = json!(2);
        current["provider_settings"]["future"] = json!(3);
        apply(&mut current, &base, &edited.to_string()).unwrap();
        assert_eq!(current["provider_settings"]["websearch_brave_key"], json!(["new"]));
        assert_eq!(current["provider_settings"]["future"], 3);
        assert_eq!(current["provider_settings"]["new_from_upstream"], 2);
        assert_eq!(current["platform"], original["platform"]);
        assert_eq!(current["dashboard"]["port"], 6185);
        assert_eq!(current["dashboard"]["password"], "private");
    }

    #[test]
    fn removing_a_leaf_keeps_unknown_siblings() {
        let old = json!({"group":{"remove":1,"keep":2}});
        let base = encode(&old);
        let after = json!({"group":{"keep":2}});
        let mut current = json!({"group":{"remove":1,"keep":4,"new":5}});
        apply(&mut current, &base, &after.to_string()).unwrap();
        assert_eq!(current, json!({"group":{"keep":4,"new":5}}));
    }

    #[test]
    fn non_object_ancestors_cannot_replace_dedicated_values() {
        let original = json!({
            "dashboard":{"port":6185,"password":"keep","ssl":{"enable":true}},
            "provider_settings":{"enable":true,"default_provider_id":"old","future":1},
            "agent_runner":{"runner_type":"local","config":{"model":{"provider_id":"owned","request_max_retries":5}}}
        });
        let base = encode(&original);
        for edited in [
            json!({"dashboard":null}),
            json!({"provider_settings":[]}),
            json!({"agent_runner":{"config":{"model":null}}}),
        ] {
            let mut root = original.clone();
            apply(&mut root, &base, &edited.to_string()).unwrap();
            assert_eq!(root.pointer("/dashboard/port").unwrap(), 6185);
            assert_eq!(root.pointer("/dashboard/password").unwrap(), "keep");
            assert_eq!(root.pointer("/provider_settings/enable").unwrap(), true);
            assert_eq!(root.pointer("/provider_settings/default_provider_id").unwrap(), "old");
            assert_eq!(root.pointer("/agent_runner/config/model/provider_id").unwrap(), "owned");
        }
    }

    #[test]
    fn deleting_or_adding_an_object_does_not_drop_new_unknown_children() {
        let mut current = json!({"group":{"old":1,"upstream":2},"new_group":{"upstream":3}});
        apply(&mut current, r#"{"group":{"old":1}}"#, r#"{"new_group":{"edited":4}}"#).unwrap();
        assert_eq!(current, json!({"group":{"upstream":2},"new_group":{"upstream":3,"edited":4}}));
    }

    #[test]
    fn switching_runners_clears_the_previous_runner_config() {
        let mut root = json!({"agent_runner":{"runner_type":"dify","config":{"dify_api_key":"old"}}});
        let base = encode(&root);
        let next = json!({"agent_runner":{"runner_type":"coze","config":{"coze_api_key":"new"}}});
        prepare_runner_change(&mut root, &base, &next.to_string()).unwrap();
        apply(&mut root, &base, &next.to_string()).unwrap();
        assert_eq!(root, next);
    }

    #[test]
    fn newly_added_empty_object_is_an_object() {
        let mut root = json!({});
        apply(&mut root, "{}", r#"{"custom":{}}"#).unwrap();
        assert_eq!(root, json!({"custom":{}}));
    }

    #[test]
    fn absent_legacy_projection_does_not_touch_document() {
        let mut root = json!({"future":{"value":1}});
        apply(&mut root, "", "").unwrap();
        assert_eq!(root, json!({"future":{"value":1}}));
        assert!(parse("[]").is_err());
        assert!(parse("{").is_err());
    }
}
