//! 把 schema 里的 `$ref` 展开成被引用定义的副本。
//!
//! 随包快照为了体积把公共定义放在 `defs` 里、用 `$ref` 引用；前端的表单和类型展示
//! 不想自己解引用，所以在目录解析阶段一次性展开。

use serde_json::{Map, Value};

/// 引用链最多展开这么深。超出的原样保留 `$ref`，宁可少展开也不能让体积失控
const MAX_REF_DEPTH: usize = 16;

/// `$ref` 值里能识别的前缀：OpenAPI 原始形式和快照里改写过的形式
const REF_PREFIXES: &[&str] = &["#/components/schemas/", "#/defs/"];

/// 展开 `schema` 里所有能在 `defs` 里找到的 `$ref`。
///
/// 三种情况保持 `{"$ref": …}` 原样不动，界面上显示为一个引用名：
/// 前缀不认识、`defs` 里没有、或者会造成环（正在展开的定义又引用了自己）/ 嵌套超过 16 层。
/// `$ref` 旁边若还有 `description` 之类的同级字段，覆盖到展开结果上
pub fn inline_refs(schema: &Value, defs: &Map<String, Value>) -> Value {
    let mut stack = Vec::new();
    walk(schema, defs, &mut stack)
}

/// `stack` 是当前正在展开的定义 id（祖先链），用来判环，同时它的长度就是嵌套深度
fn walk<'a>(value: &Value, defs: &'a Map<String, Value>, stack: &mut Vec<&'a str>) -> Value {
    match value {
        Value::Object(map) => {
            if let Some(expanded) = expand_ref(map, defs, stack) {
                return expanded;
            }
            Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), walk(v, defs, stack)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| walk(v, defs, stack)).collect()),
        other => other.clone(),
    }
}

/// 该对象是 `$ref` 且允许展开时返回展开结果；否则返回 `None`，让调用方按普通对象处理
fn expand_ref<'a>(
    map: &Map<String, Value>,
    defs: &'a Map<String, Value>,
    stack: &mut Vec<&'a str>,
) -> Option<Value> {
    let reference = map.get("$ref")?.as_str()?;
    let id = REF_PREFIXES
        .iter()
        .find_map(|prefix| reference.strip_prefix(prefix))?;
    let (key, target) = defs.get_key_value(id)?;
    if stack.len() >= MAX_REF_DEPTH || stack.contains(&key.as_str()) {
        return None;
    }

    stack.push(key.as_str());
    let mut expanded = walk(target, defs, stack);
    // 同级字段（如 description）是引用处自己的说明，比被引用的定义更具体
    if let Value::Object(out) = &mut expanded {
        for (k, v) in map.iter().filter(|(k, _)| k.as_str() != "$ref") {
            out.insert(k.clone(), walk(v, defs, stack));
        }
    }
    stack.pop();
    Some(expanded)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn defs(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn inlines_nested_refs_with_both_prefixes() {
        let defs = defs(json!({
            "User": {"type": "object", "properties": {"id": {"$ref": "#/defs/Id"}}},
            "Id": {"type": "string"},
        }));
        let schema = json!({
            "type": "object",
            "properties": {
                "user": {"$ref": "#/components/schemas/User"},
                "list": {"type": "array", "items": {"$ref": "#/defs/User"}},
            }
        });
        let out = inline_refs(&schema, &defs);
        assert_eq!(
            out["properties"]["user"]["properties"]["id"],
            json!({"type": "string"})
        );
        assert_eq!(
            out["properties"]["list"]["items"]["properties"]["id"],
            json!({"type": "string"})
        );
    }

    #[test]
    fn cycle_leaves_ref_in_place() {
        let defs = defs(json!({
            "Node": {
                "type": "object",
                "properties": {"child": {"$ref": "#/defs/Node"}, "name": {"type": "string"}}
            }
        }));
        let out = inline_refs(&json!({"$ref": "#/defs/Node"}), &defs);
        // 第一层展开，第二层再遇到 Node 就停下保留引用
        assert_eq!(out["properties"]["name"], json!({"type": "string"}));
        assert_eq!(out["properties"]["child"], json!({"$ref": "#/defs/Node"}));
    }

    #[test]
    fn mutual_cycle_terminates() {
        let defs = defs(json!({
            "A": {"properties": {"b": {"$ref": "#/defs/B"}}},
            "B": {"properties": {"a": {"$ref": "#/defs/A"}}},
        }));
        let out = inline_refs(&json!({"$ref": "#/defs/A"}), &defs);
        assert_eq!(
            out["properties"]["b"]["properties"]["a"],
            json!({"$ref": "#/defs/A"})
        );
    }

    #[test]
    fn same_definition_twice_side_by_side_is_not_a_cycle() {
        let defs = defs(json!({"S": {"type": "string"}}));
        let schema = json!({"anyOf": [{"$ref": "#/defs/S"}, {"$ref": "#/defs/S"}]});
        let out = inline_refs(&schema, &defs);
        assert_eq!(
            out,
            json!({"anyOf": [{"type": "string"}, {"type": "string"}]})
        );
    }

    #[test]
    fn depth_guard_stops_long_chains() {
        // D0 → D1 → … → D19，没有环，但超过 16 层的部分不再展开
        let mut map = Map::new();
        for i in 0..20 {
            let next = json!({"$ref": format!("#/defs/D{}", i + 1)});
            map.insert(format!("D{i}"), json!({"next": next}));
        }
        map.insert("D20".to_owned(), json!({"type": "string"}));
        let out = inline_refs(&json!({"$ref": "#/defs/D0"}), &map);

        let mut cursor = &out;
        let mut levels = 0;
        while let Some(next) = cursor.get("next") {
            cursor = next;
            levels += 1;
        }
        assert_eq!(levels, MAX_REF_DEPTH);
        assert!(
            cursor.get("$ref").is_some(),
            "到深度上限时应留下 $ref: {cursor}"
        );
    }

    #[test]
    fn unknown_or_foreign_refs_are_left_alone() {
        let defs = defs(json!({}));
        let schema = json!({"a": {"$ref": "#/defs/Missing"}, "b": {"$ref": "http://x/y"}});
        assert_eq!(inline_refs(&schema, &defs), schema);
    }

    #[test]
    fn sibling_keys_override_the_inlined_definition() {
        let defs = defs(json!({"S": {"type": "string", "description": "generic"}}));
        let out = inline_refs(
            &json!({"$ref": "#/defs/S", "description": "specific"}),
            &defs,
        );
        assert_eq!(out, json!({"type": "string", "description": "specific"}));
    }

    #[test]
    fn definition_itself_is_not_mutated() {
        let defs = defs(json!({"S": {"type": "string"}}));
        let before = defs.clone();
        let _ = inline_refs(&json!({"$ref": "#/defs/S", "x": 1}), &defs);
        assert_eq!(defs, before);
    }
}
