//! SnowLuma 目录：把 `/api/debug/actions`（或随包快照，形状相同）转成统一的 [`Catalog`]。

use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{DebugActionCategory, DebugActionSpec, DebugCatalogSource};
use serde_json::{Map, Value};

use super::model::{Catalog, CatalogError};
use super::overrides::{annotate_roles, category_from_name, safety_for};
use super::{default_params_schema, non_empty_str, string_list};

/// 转换 SnowLuma 的目录。输入是 `{ "actions": [ActionDoc…], "categories": […] }`，
/// `categories` 只是各分类的计数，用不上。保持上游给的动作顺序（它本来就按分类排好了）
pub fn parse_snowluma(
    body: &Value,
    version: &str,
    source: DebugCatalogSource,
) -> Result<Catalog, CatalogError> {
    let list = body
        .get("actions")
        .and_then(Value::as_array)
        .ok_or_else(|| CatalogError::Shape("SnowLuma 目录缺少 actions 数组".to_owned()))?;

    let mut actions = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let entry = item.as_object();
        let name = entry
            .and_then(|e| e.get("name"))
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty());
        match (entry, name) {
            (Some(entry), Some(name)) => actions.push(convert_action(name, entry, &source)),
            _ => tracing::warn!(index = i, "SnowLuma 目录里这一项缺少 name，已跳过"),
        }
    }

    Ok(Catalog::new(
        BackendType::SnowLuma,
        source,
        version,
        actions,
    ))
}

fn convert_action(
    name: &str,
    entry: &Map<String, Value>,
    source: &DebugCatalogSource,
) -> DebugActionSpec {
    // `x-role` 原样留着，另外统一写一份 `x-ncd-role` 给前端；上游没标的再按名字猜
    let mut params_schema = entry
        .get("inputSchema")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(default_params_schema);
    annotate_roles(name, &mut params_schema);

    let returns_text = non_empty_str(entry.get("returns"));
    // 12 个动作没写 summary，用返回说明的第一句顶上，列表里至少有句话
    let summary = non_empty_str(entry.get("summary"))
        .or_else(|| returns_text.as_deref().map(first_sentence))
        .unwrap_or_default();

    DebugActionSpec {
        name: name.to_owned(),
        aliases: string_list(entry.get("aliases")),
        summary,
        description: None,
        category: snowluma_category(name, entry.get("category").and_then(Value::as_str)),
        safety: safety_for(name, entry.get("readOnly").and_then(Value::as_bool)),
        stream: entry
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        supported: true,
        params_schema,
        returns_schema: entry
            .get("returnsSchema")
            .filter(|v| v.is_object())
            .cloned(),
        returns_text,
        return_example: None,
        examples: Vec::new(),
        error_examples: Vec::new(),
        invariants: string_list(entry.get("invariants")),
        other_backend: None,
        source: source.clone(),
    }
}

/// SnowLuma 的分类里，「扩展 / 群相册 / 空间」太杂（扩展一类就占了一半以上的动作），
/// 认不出的分类一律按名字猜
fn snowluma_category(name: &str, category: Option<&str>) -> DebugActionCategory {
    match category {
        Some("信息") => DebugActionCategory::Account,
        Some("消息") => DebugActionCategory::Message,
        Some("好友") => DebugActionCategory::Friend,
        Some("群信息") => DebugActionCategory::GroupInfo,
        Some("群管理") => DebugActionCategory::GroupAdmin,
        Some("群文件") => DebugActionCategory::File,
        Some("请求") => DebugActionCategory::Request,
        Some("系统表情") => DebugActionCategory::Face,
        Some("流式接口") => DebugActionCategory::Stream,
        _ => category_from_name(name),
    }
}

/// 取第一句：到第一个句号 / 换行为止，去掉句末标点。整段都没有断句标记就整段用
fn first_sentence(text: &str) -> String {
    let text = text.trim();
    let end = text
        .char_indices()
        .find(|&(i, c)| c == '。' || c == '\n' || (c == '.' && text[i + 1..].starts_with(' ')))
        .map_or(text.len(), |(i, _)| i);
    text[..end].trim().to_owned()
}

#[cfg(test)]
mod tests {
    use ncd_domain::onebot_debug::DebugActionSafety;
    use serde_json::json;

    use super::*;

    fn parse(value: &Value) -> Catalog {
        parse_snowluma(value, "test", DebugCatalogSource::Live).unwrap()
    }

    #[test]
    fn rejects_body_without_actions_array() {
        let err = parse_snowluma(&json!({}), "v", DebugCatalogSource::Live).unwrap_err();
        assert!(err.to_string().contains("目录格式不对"), "{err}");
        assert!(parse_snowluma(&json!({"actions": {}}), "v", DebugCatalogSource::Live).is_err());
    }

    #[test]
    fn entries_without_name_are_skipped() {
        let cat = parse(&json!({"actions": [{"summary": "no name"}, 7, {"name": "ok"}]}));
        assert_eq!(cat.actions().len(), 1);
        assert!(cat.get("ok").is_some());
    }

    #[test]
    fn first_sentence_cases() {
        assert_eq!(
            first_sentence("当前登录账号的 QQ 号与昵称。第二句"),
            "当前登录账号的 QQ 号与昵称"
        );
        assert_eq!(
            first_sentence("{ message_id: number }"),
            "{ message_id: number }"
        );
        assert_eq!(first_sentence("Returns id. Then more"), "Returns id");
        assert_eq!(first_sentence("v1.2 是版本号"), "v1.2 是版本号");
        assert_eq!(first_sentence("第一行\n第二行"), "第一行");
        assert_eq!(first_sentence("  "), "");
    }

    #[test]
    fn summary_prefers_own_text_over_returns() {
        let cat = parse(&json!({"actions": [
            {"name": "a", "summary": "自己的", "returns": "返回说明。"},
            {"name": "b", "returns": "返回说明。后面还有"},
            {"name": "c"},
            {"name": "d", "summary": ""},
        ]}));
        assert_eq!(cat.get("a").unwrap().summary, "自己的");
        assert_eq!(cat.get("b").unwrap().summary, "返回说明");
        assert_eq!(
            cat.get("b").unwrap().returns_text.as_deref(),
            Some("返回说明。后面还有")
        );
        assert_eq!(cat.get("c").unwrap().summary, "");
        assert_eq!(cat.get("d").unwrap().summary, "");
    }

    #[test]
    fn category_by_upstream_label_then_name() {
        use DebugActionCategory as C;
        let cases: &[(&str, Option<&str>, DebugActionCategory)] = &[
            ("x", Some("信息"), C::Account),
            ("x", Some("消息"), C::Message),
            ("x", Some("好友"), C::Friend),
            ("x", Some("群信息"), C::GroupInfo),
            ("x", Some("群管理"), C::GroupAdmin),
            ("x", Some("群文件"), C::File),
            ("x", Some("请求"), C::Request),
            ("x", Some("系统表情"), C::Face),
            ("x", Some("流式接口"), C::Stream),
            ("fetch_custom_face", Some("扩展"), C::Face),
            ("get_group_album_list", Some("群相册"), C::GroupInfo),
            ("comment_qzone", Some("空间"), C::Extension),
            ("send_like", Some("没见过的分类"), C::Friend),
            ("send_like", None, C::Friend),
        ];
        for (name, category, want) in cases {
            assert_eq!(
                snowluma_category(name, *category),
                *want,
                "{name} {category:?}"
            );
        }
    }

    #[test]
    fn keeps_upstream_order_aliases_and_invariants() {
        let cat = parse(&json!({"actions": [
            {"name": "z", "aliases": ["z2", 5, "z3"], "invariants": ["必须在群里"], "stream": true, "readOnly": true},
            {"name": "a"},
        ]}));
        let names: Vec<&str> = cat.actions().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["z", "a"]);
        let z = cat.get("z").unwrap();
        assert_eq!(z.aliases, ["z2", "z3"]);
        assert_eq!(z.invariants, ["必须在群里"]);
        assert!(z.stream);
        assert_eq!(z.safety, DebugActionSafety::ReadOnly);
        assert!(cat.get("z3").is_some());
        assert!(!cat.get("a").unwrap().stream);
        assert_eq!(
            cat.get("a").unwrap().params_schema,
            json!({"type": "object", "properties": {}})
        );
    }

    #[test]
    fn x_role_is_mirrored_into_x_ncd_role_and_missing_ones_inferred() {
        let cat = parse(&json!({"actions": [{
            "name": "send_group_msg",
            "inputSchema": {"type": "object", "properties": {
                "group_id": {"type": "integer", "x-role": "group_id"},
                "message": {"description": "任意"},
                "count": {"type": "integer"}
            }}
        }]}));
        let props = &cat.get("send_group_msg").unwrap().params_schema["properties"];
        assert_eq!(props["group_id"]["x-role"], "group_id");
        assert_eq!(props["group_id"]["x-ncd-role"], "group_id");
        assert_eq!(props["message"]["x-ncd-role"], "message");
        assert!(props["count"].get("x-ncd-role").is_none());
    }
}
