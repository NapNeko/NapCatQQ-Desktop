//! 同名动作在 NapCat 和 SnowLuma 上的参数差异。
//!
//! 只比顶层 `properties` 的键、类型和 `required`，不深入嵌套结构：
//! 界面上要的是「两边接口大致哪里不一样」，不是完整的 schema diff。
//!
//! 两个层次：[`param_diff`] 列出全部出入，给文档页的对照表；
//! [`params_incompatible`] 只回答「照一边的写法发到另一边会不会直接失败」，给目录行上的徽章。

use std::collections::{BTreeMap, BTreeSet};

use ncd_domain::onebot_debug::{DebugParamDiff, DebugParamDiffKind};
use serde_json::Value;

/// 一个参数在某一侧的样子
struct ParamInfo {
    /// 类型集合；空表示这一侧没写类型，不参与比较
    types: BTreeSet<String>,
    /// 必须由调用方给出：列在 `required` 里且没有 `default`。
    /// NapCat 会把带默认值的字段也列进 `required`，不给也照样能调，不算必填
    required: bool,
}

impl ParamInfo {
    /// 类型的展示形式：排序后用 `|` 连接，如 `boolean|string`
    fn type_label(&self) -> String {
        self.types.iter().cloned().collect::<Vec<_>>().join("|")
    }
}

/// 比较两份参数 schema，`here` 是当前后端的，`other` 是另一个后端的。
/// 输出按参数名排序，同一个参数的类型差异排在必填差异前面。
///
/// 类型差异的判定刻意放得很松，因为两个后端的写法习惯本来就不同：
/// NapCat 的 ID 一律是字符串（`string`，或 `number|string`、`boolean|string`），
/// SnowLuma 用整数和布尔。所以任意一侧接受 `string` 就不算差异；
/// `integer` 与 `number` 也视为同一族。剩下两侧都不含 `string` 且类型集合不同时才报，
/// 例如一边是 `array` 另一边是 `object`。任何一侧没写类型的也不报。
pub fn param_diff(here: &Value, other: &Value) -> Vec<DebugParamDiff> {
    let here = collect_params(here);
    let other = collect_params(other);

    let names: BTreeSet<&String> = here.keys().chain(other.keys()).collect();
    let mut diffs = Vec::new();
    for name in names {
        match (here.get(name), other.get(name)) {
            (Some(_), None) => diffs.push(diff(name, DebugParamDiffKind::OnlyHere)),
            (None, Some(_)) => diffs.push(diff(name, DebugParamDiffKind::OnlyOther)),
            (Some(a), Some(b)) => {
                if types_differ(&a.types, &b.types) {
                    diffs.push(diff(
                        name,
                        DebugParamDiffKind::TypeDiffers {
                            here: a.type_label(),
                            other: b.type_label(),
                        },
                    ));
                }
                if a.required != b.required {
                    diffs.push(diff(
                        name,
                        DebugParamDiffKind::RequiredDiffers {
                            here: a.required,
                            other: b.required,
                        },
                    ));
                }
            }
            (None, None) => {}
        }
    }
    diffs
}

/// 两边参数是否真的不兼容：同名参数类型大类不同（判法同 [`param_diff`]），
/// 或者某一侧有必填参数、另一侧根本没有这个参数（多半是改了名）。
/// 只是必填与否不同、或只有一侧有的可选参数，照一边的写法发到另一边通常还能调通，不算
pub fn params_incompatible(here: &Value, other: &Value) -> bool {
    let here = collect_params(here);
    let other = collect_params(other);
    let lacks_required = |a: &BTreeMap<String, ParamInfo>, b: &BTreeMap<String, ParamInfo>| {
        a.iter()
            .any(|(name, info)| info.required && !b.contains_key(name))
    };
    let type_clash = here.iter().any(|(name, a)| {
        other
            .get(name)
            .is_some_and(|b| types_differ(&a.types, &b.types))
    });
    type_clash || lacks_required(&here, &other) || lacks_required(&other, &here)
}

fn diff(name: &str, kind: DebugParamDiffKind) -> DebugParamDiff {
    DebugParamDiff {
        name: name.to_owned(),
        diff: kind,
    }
}

fn types_differ(a: &BTreeSet<String>, b: &BTreeSet<String>) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.contains("string") || b.contains("string") {
        return false;
    }
    normalize_numeric(a) != normalize_numeric(b)
}

/// `integer` 是 `number` 的子集，比较前并成同一个
fn normalize_numeric(types: &BTreeSet<String>) -> BTreeSet<&str> {
    types
        .iter()
        .map(|t| if t == "integer" { "number" } else { t.as_str() })
        .collect()
}

/// 取顶层参数。`allOf` 拆出来的子 schema 也算：有的动作把公共参数放在 `allOf` 里
fn collect_params(schema: &Value) -> BTreeMap<String, ParamInfo> {
    let mut out = BTreeMap::new();
    collect_into(schema, &mut out);
    out
}

fn collect_into(schema: &Value, out: &mut BTreeMap<String, ParamInfo>) {
    let Some(obj) = schema.as_object() else {
        return;
    };
    let required: BTreeSet<&str> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    if let Some(props) = obj.get("properties").and_then(Value::as_object) {
        for (name, prop) in props {
            let mut types = BTreeSet::new();
            collect_types(prop, &mut types);
            let has_default = prop.get("default").is_some();
            out.insert(
                name.clone(),
                ParamInfo {
                    types,
                    required: required.contains(name.as_str()) && !has_default,
                },
            );
        }
    }
    if let Some(parts) = obj.get("allOf").and_then(Value::as_array) {
        for part in parts {
            collect_into(part, out);
        }
    }
}

/// 收集一个属性声明的类型：`type`（字符串或数组）、`const` / `enum` 里值的类型、
/// `anyOf` / `oneOf` 各分支的并集
fn collect_types(prop: &Value, types: &mut BTreeSet<String>) {
    let Some(obj) = prop.as_object() else {
        return;
    };
    match obj.get("type") {
        Some(Value::String(t)) => {
            types.insert(t.clone());
        }
        Some(Value::Array(list)) => {
            types.extend(list.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        _ => {}
    }
    if let Some(value) = obj.get("const") {
        types.insert(json_type_of(value).to_owned());
    }
    if let Some(values) = obj.get("enum").and_then(Value::as_array) {
        types.extend(values.iter().map(|v| json_type_of(v).to_owned()));
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(branches) = obj.get(key).and_then(Value::as_array) {
            for branch in branches {
                collect_types(branch, types);
            }
        }
    }
}

fn json_type_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn kinds(diffs: &[DebugParamDiff]) -> Vec<(&str, &DebugParamDiffKind)> {
        diffs.iter().map(|d| (d.name.as_str(), &d.diff)).collect()
    }

    #[test]
    fn reports_only_here_and_only_other_sorted_by_name() {
        let here = json!({"properties": {"b": {"type": "string"}, "a": {"type": "string"}}});
        let other = json!({"properties": {"c": {"type": "string"}}});
        let out = param_diff(&here, &other);
        assert_eq!(
            kinds(&out),
            [
                ("a", &DebugParamDiffKind::OnlyHere),
                ("b", &DebugParamDiffKind::OnlyHere),
                ("c", &DebugParamDiffKind::OnlyOther),
            ]
        );
    }

    #[test]
    fn string_vs_integer_id_is_not_a_type_diff() {
        let here = json!({"properties": {
            "group_id": {"type": "string"},
            "message_id": {"anyOf": [{"type": "number"}, {"type": "string"}]},
            "no_cache": {"anyOf": [{"type": "boolean"}, {"type": "string"}]},
        }});
        let other = json!({"properties": {
            "group_id": {"type": "integer"},
            "message_id": {"type": "integer"},
            "no_cache": {"type": "boolean"},
        }});
        assert!(param_diff(&here, &other).is_empty());
    }

    #[test]
    fn integer_and_number_are_one_family() {
        let here = json!({"properties": {"n": {"type": "number"}}});
        let other = json!({"properties": {"n": {"type": "integer"}}});
        assert!(param_diff(&here, &other).is_empty());
    }

    #[test]
    fn reports_type_diff_when_families_differ_and_no_string_involved() {
        let here = json!({"properties": {"x": {"type": "array"}}});
        let other = json!({"properties": {"x": {"type": "object"}}});
        let out = param_diff(&here, &other);
        assert_eq!(
            kinds(&out),
            [(
                "x",
                &DebugParamDiffKind::TypeDiffers {
                    here: "array".to_owned(),
                    other: "object".to_owned()
                }
            )]
        );
    }

    #[test]
    fn unknown_type_on_either_side_is_skipped() {
        let here =
            json!({"properties": {"message": {"anyOf": [{"type": "array"}, {"type": "boolean"}]}}});
        let other = json!({"properties": {"message": {"description": "any"}}});
        assert!(param_diff(&here, &other).is_empty());
        assert!(param_diff(&other, &here).is_empty());
    }

    #[test]
    fn any_of_types_are_joined_sorted_and_const_uses_value_type() {
        let here =
            json!({"properties": {"x": {"anyOf": [{"type": "number"}, {"type": "boolean"}]}}});
        let other = json!({"properties": {"x": {"const": "a"}, "y": {"type": "boolean"}}});
        let here_y = json!({"properties": {"y": {"anyOf": [{"const": 1}, {"const": true}]}}});
        // x：一侧含 string（const "a"），放过
        assert!(param_diff(&here, &other).iter().all(|d| d.name != "x"));
        // y：`integer|boolean` 与 `boolean` 不同，且都不含 string
        let out = param_diff(&here_y, &other);
        let y = out.iter().find(|d| d.name == "y").unwrap();
        assert_eq!(
            y.diff,
            DebugParamDiffKind::TypeDiffers {
                here: "boolean|integer".to_owned(),
                other: "boolean".to_owned()
            }
        );
    }

    #[test]
    fn required_difference_is_reported_after_type_difference() {
        let here = json!({"properties": {"x": {"type": "array"}}, "required": ["x"]});
        let other = json!({"properties": {"x": {"type": "object"}}});
        let out = param_diff(&here, &other);
        assert_eq!(out.len(), 2);
        assert!(matches!(
            out[0].diff,
            DebugParamDiffKind::TypeDiffers { .. }
        ));
        assert_eq!(
            out[1].diff,
            DebugParamDiffKind::RequiredDiffers {
                here: true,
                other: false
            }
        );
    }

    #[test]
    fn a_required_field_with_a_default_counts_as_optional() {
        let here = json!({
            "properties": {"no_cache": {"type": "boolean", "default": false}},
            "required": ["no_cache"],
        });
        let other = json!({"properties": {"no_cache": {"type": "boolean"}}});
        assert!(param_diff(&here, &other).is_empty());
        let only_here = json!({"properties": {}});
        assert!(!params_incompatible(&here, &only_here));
    }

    #[test]
    fn incompatible_only_for_type_clash_or_a_missing_required_param() {
        let renamed_here =
            json!({"properties": {"file_id": {"type": "string"}}, "required": ["file_id"]});
        let renamed_other =
            json!({"properties": {"file": {"type": "string"}}, "required": ["file"]});
        assert!(params_incompatible(&renamed_here, &renamed_other));
        assert!(params_incompatible(&renamed_other, &renamed_here));

        let clash_here = json!({"properties": {"x": {"type": "array"}}});
        let clash_other = json!({"properties": {"x": {"type": "object"}}});
        assert!(params_incompatible(&clash_here, &clash_other));

        // 只是必填与否不同、只有一侧有的可选参数：列进对照表，但不算不兼容
        let strict = json!({
            "properties": {"g": {"type": "integer"}, "extra": {"type": "string"}},
            "required": ["g"],
        });
        let lenient = json!({"properties": {"g": {"type": "string"}}});
        assert!(!param_diff(&strict, &lenient).is_empty());
        assert!(!params_incompatible(&strict, &lenient));
    }

    #[test]
    fn missing_or_non_object_schemas_yield_no_diff() {
        assert!(param_diff(&json!(null), &json!({"properties": {}})).is_empty());
        assert!(param_diff(&json!({}), &json!({})).is_empty());
        let some = json!({"properties": {"a": {"type": "string"}}});
        assert_eq!(param_diff(&some, &json!("nope")).len(), 1);
    }

    #[test]
    fn all_of_members_contribute_properties() {
        let here = json!({"allOf": [
            {"properties": {"a": {"type": "string"}}, "required": ["a"]},
            {"properties": {"b": {"type": "string"}}},
        ]});
        let other = json!({"properties": {"a": {"type": "string"}}, "required": ["a"]});
        let out = param_diff(&here, &other);
        assert_eq!(kinds(&out), [("b", &DebugParamDiffKind::OnlyHere)]);
    }
}
