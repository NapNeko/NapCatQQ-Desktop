//! NapCat 目录：把 `/api/Debug/schemas` 的 `data`（上游现取）或随包快照转成统一的 [`Catalog`]。

use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{
    DebugActionCategory, DebugActionSpec, DebugCatalogSource, DebugErrorExample,
};
use serde_json::{Map, Value};

use super::model::{Catalog, CatalogError};
use super::overrides::{annotate_roles, category_from_name, group_category, safety_for};
use super::refs::inline_refs;
use super::{default_params_schema, non_empty_str, non_null};

/// 转换 NapCat 的目录。
///
/// 两种输入：
/// - 上游现取的：`data` 本身就是 `{ 动作名: {description, payload, response, payloadExample, tags} }`，
///   字段都可能缺，schema 里没有 `$ref`；
/// - 随包快照：`{ "defs": {…}, "actions": { 动作名: {…} } }`，schema 里用 `$ref` 引用 `defs`，
///   这里统一展开，前端拿到的永远是自包含的 schema。
pub fn parse_napcat(
    data: &Value,
    version: &str,
    source: DebugCatalogSource,
) -> Result<Catalog, CatalogError> {
    let root = data
        .as_object()
        .ok_or_else(|| CatalogError::Shape("NapCat 目录应是一个以动作名为键的对象".to_owned()))?;

    let empty_defs = Map::new();
    let (entries, defs) = match root.get("actions") {
        Some(Value::Object(actions)) => {
            let defs = root
                .get("defs")
                .and_then(Value::as_object)
                .unwrap_or(&empty_defs);
            (actions, defs)
        }
        Some(_) => {
            return Err(CatalogError::Shape(
                "NapCat 快照里的 actions 应是对象".to_owned(),
            ));
        }
        None => (root, &empty_defs),
    };

    let mut actions = Vec::with_capacity(entries.len());
    for (name, entry) in entries {
        let Some(entry) = entry.as_object() else {
            tracing::warn!(action = %name, "NapCat 目录里这一项不是对象，已跳过");
            continue;
        };
        actions.push(convert_action(name, entry, defs, &source));
    }
    // JSON 对象没有顺序可言，排一下让目录顺序稳定
    actions.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Catalog::new(BackendType::NapCat, source, version, actions))
}

fn convert_action(
    name: &str,
    entry: &Map<String, Value>,
    defs: &Map<String, Value>,
    source: &DebugCatalogSource,
) -> DebugActionSpec {
    let tags: Vec<&str> = entry
        .get("tags")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut params_schema = entry
        .get("payload")
        .filter(|v| v.is_object())
        .map(|v| inline_refs(v, defs))
        .unwrap_or_else(default_params_schema);
    annotate_roles(name, &mut params_schema);

    // 没有返回值 schema 的动作，快照里可能记成 `{}`，对界面等于没有
    let returns_schema = entry
        .get("response")
        .and_then(Value::as_object)
        .filter(|obj| !obj.is_empty())
        .map(|obj| inline_refs(&Value::Object(obj.clone()), defs));

    let summary = entry
        .get("description")
        .and_then(Value::as_str)
        .and_then(|text| text.lines().map(str::trim).find(|line| !line.is_empty()))
        .unwrap_or_default()
        .to_owned();

    DebugActionSpec {
        name: name.to_owned(),
        aliases: Vec::new(),
        summary,
        description: non_empty_str(entry.get("longDescription")),
        category: napcat_category(name, &tags),
        safety: safety_for(name, None),
        stream: is_stream(name, &tags),
        supported: true,
        params_schema,
        returns_schema,
        returns_text: None,
        return_example: non_null(entry.get("returnExample")),
        examples: non_null(entry.get("payloadExample")).into_iter().collect(),
        error_examples: parse_error_examples(entry.get("errorExamples")),
        invariants: Vec::new(),
        other_backend: None,
        source: source.clone(),
    }
}

fn is_stream(name: &str, tags: &[&str]) -> bool {
    name.contains("_stream")
        || tags
            .iter()
            .any(|t| matches!(*t, "流式接口" | "流式传输扩展"))
}

/// 先看 NapCat 自己给的标签，标签没有信息量（Go-CQHTTP / 扩展接口 / AI 扩展 / 频道接口 / 没标签）时才看名字。
/// 多个标签时取第一个有信息量的
fn napcat_category(name: &str, tags: &[&str]) -> DebugActionCategory {
    for tag in tags {
        match *tag {
            "消息接口" | "消息扩展" => return DebugActionCategory::Message,
            "群组接口" | "群组扩展" => return group_tag_category(name),
            "用户接口" | "用户扩展" => return DebugActionCategory::Friend,
            "文件接口" | "文件扩展" => return DebugActionCategory::File,
            "流式接口" | "流式传输扩展" => return DebugActionCategory::Stream,
            "系统接口" | "系统扩展" | "核心接口" => {
                return DebugActionCategory::Account;
            }
            _ => {}
        }
    }
    category_from_name(name)
}

/// 「群组接口 / 群组扩展」标签的分类规则。
///
/// 有意比「get_ 开头 → GroupInfo，否则 GroupAdmin」多走一步：这个标签太粗，
/// `send_group_msg`、`set_group_add_request`、`upload_group_file` 都挂在它下面
/// （上游 openapi 里 `send_group_msg` 的标签就是「群组接口」），
/// 按字面规则会把最常用的发消息动作归进「群管理」。
/// 所以名字能明确认出 Message / File / Request / Face / Stream 的，听名字的
/// （SnowLuma 那边也是按用途这么分的，两个后端的分组能对上）；
/// 认不出才按读前缀拆成 GroupInfo（查）/ GroupAdmin（管）。
fn group_tag_category(name: &str) -> DebugActionCategory {
    match category_from_name(name) {
        specific @ (DebugActionCategory::Message
        | DebugActionCategory::File
        | DebugActionCategory::Request
        | DebugActionCategory::Face
        | DebugActionCategory::Stream) => specific,
        _ => group_category(name),
    }
}

/// `errorExamples`：`[{retcode, message}]`；retcode 也容忍写成数字字符串，读不出来的项丢掉
fn parse_error_examples(value: Option<&Value>) -> Vec<DebugErrorExample> {
    let Some(list) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|item| {
            let retcode = match item.get("retcode")? {
                Value::Number(n) => n.as_i64()?,
                Value::String(s) => s.trim().parse().ok()?,
                _ => return None,
            };
            let message = item
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Some(DebugErrorExample { retcode, message })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ncd_domain::onebot_debug::DebugActionSafety;
    use serde_json::json;

    use super::*;

    fn parse(value: &Value) -> Catalog {
        parse_napcat(value, "test", DebugCatalogSource::Live).unwrap()
    }

    #[test]
    fn rejects_non_object_input() {
        let err = parse_napcat(&json!([1, 2]), "v", DebugCatalogSource::Live).unwrap_err();
        assert!(err.to_string().contains("目录格式不对"), "{err}");
        assert!(parse_napcat(&json!({"actions": []}), "v", DebugCatalogSource::Live).is_err());
    }

    #[test]
    fn skips_entries_that_are_not_objects() {
        let cat = parse(&json!({"good": {"description": "x"}, "bad": 3}));
        assert_eq!(cat.actions().len(), 1);
        assert!(cat.get("good").is_some());
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let cat = parse(&json!({"get_status": {"tags": ["系统接口"]}}));
        let spec = cat.get("get_status").unwrap();
        assert_eq!(
            spec.params_schema,
            json!({"type": "object", "properties": {}})
        );
        assert_eq!(spec.summary, "");
        assert!(spec.returns_schema.is_none());
        assert!(spec.examples.is_empty());
        assert!(spec.return_example.is_none());
    }

    #[test]
    fn summary_is_first_non_empty_line_of_description() {
        let cat = parse(&json!({"a": {"description": "\n  第一行 \n第二行"}}));
        assert_eq!(cat.get("a").unwrap().summary, "第一行");
    }

    #[test]
    fn empty_response_schema_becomes_none() {
        let cat = parse(&json!({"a": {"response": {}}, "b": {"response": {"type": "object"}}}));
        assert!(cat.get("a").unwrap().returns_schema.is_none());
        assert!(cat.get("b").unwrap().returns_schema.is_some());
    }

    #[test]
    fn snapshot_inlines_refs_and_reads_extra_docs() {
        let snapshot = json!({
            "defs": {"Msg": {"anyOf": [{"type": "string"}, {"type": "array"}]}},
            "actions": {
                "send_msg": {
                    "description": "发消息",
                    "longDescription": "更长的说明",
                    "tags": ["消息接口"],
                    "payload": {"type": "object", "properties": {"message": {"$ref": "#/components/schemas/Msg"}}},
                    "response": {"type": "object", "properties": {"message_id": {"type": "number"}}},
                    "payloadExample": {"message": "hi"},
                    "returnExample": {"message_id": 1},
                    "errorExamples": [{"retcode": 1400, "message": "参数错误"}, {"retcode": "1404", "message": "x"}, {"message": "no code"}]
                }
            }
        });
        let cat = parse_napcat(&snapshot, "4.15.18", DebugCatalogSource::Snapshot).unwrap();
        assert_eq!(cat.version(), "4.15.18");
        assert_eq!(cat.source(), DebugCatalogSource::Snapshot);
        let spec = cat.get("send_msg").unwrap();
        assert_eq!(
            spec.params_schema["properties"]["message"]["anyOf"][0],
            json!({"type": "string"})
        );
        assert_eq!(
            spec.params_schema["properties"]["message"]["x-ncd-role"],
            "message"
        );
        assert_eq!(spec.description.as_deref(), Some("更长的说明"));
        assert_eq!(spec.return_example, Some(json!({"message_id": 1})));
        assert_eq!(spec.examples, vec![json!({"message": "hi"})]);
        assert_eq!(spec.error_examples.len(), 2);
        assert_eq!(spec.error_examples[1].retcode, 1404);
        assert_eq!(spec.source, DebugCatalogSource::Snapshot);
    }

    #[test]
    fn category_by_tag_with_group_refinement() {
        use DebugActionCategory as C;
        let cases: &[(&str, &[&str], DebugActionCategory)] = &[
            ("send_group_msg", &["群组接口"], C::Message),
            ("get_group_list", &["群组接口"], C::GroupInfo),
            ("set_group_ban", &["群组接口"], C::GroupAdmin),
            ("set_group_add_request", &["群组接口"], C::Request),
            ("upload_group_file", &["群组扩展"], C::File),
            ("delete_msg", &["消息接口"], C::Message),
            ("get_stranger_info", &["用户接口"], C::Friend),
            ("get_file", &["文件接口"], C::File),
            ("upload_file_stream", &["流式传输扩展"], C::Stream),
            ("get_status", &["系统接口"], C::Account),
            ("get_version_info", &["核心接口"], C::Account),
            // 标签没信息量：看名字
            ("send_like", &["Go-CQHTTP"], C::Friend),
            ("get_mini_app_ark", &["扩展接口"], C::Extension),
            ("get_something_odd", &[], C::Extension),
            // 第一个没信息量的标签不挡后面有信息量的
            ("x", &["扩展接口", "文件接口"], C::File),
        ];
        for (name, tags, want) in cases {
            assert_eq!(napcat_category(name, tags), *want, "{name} {tags:?}");
        }
    }

    #[test]
    fn stream_flag_from_tag_or_name() {
        assert!(is_stream("x", &["流式接口"]));
        assert!(is_stream("x", &["流式传输扩展"]));
        assert!(is_stream("download_file_stream", &[]));
        assert!(!is_stream("get_file", &["文件接口"]));
    }

    #[test]
    fn safety_is_name_based() {
        let cat = parse(&json!({"delete_msg": {}, "get_msg": {}, "send_msg": {}}));
        assert_eq!(
            cat.get("delete_msg").unwrap().safety,
            DebugActionSafety::Dangerous
        );
        assert_eq!(
            cat.get("get_msg").unwrap().safety,
            DebugActionSafety::ReadOnly
        );
        assert_eq!(
            cat.get("send_msg").unwrap().safety,
            DebugActionSafety::SideEffect
        );
    }

    #[test]
    fn actions_are_sorted_by_name() {
        let cat = parse(&json!({"b": {}, "a": {}, "c": {}}));
        let names: Vec<&str> = cat.actions().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
    }
}
