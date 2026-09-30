//! OneBot 动作目录：把 NapCat 与 SnowLuma 两种格式的文档统一成
//! `ncd_domain::onebot_debug` 里的 [`DebugActionSpec`](ncd_domain::onebot_debug::DebugActionSpec)，
//! 并补上两边文档都没给的安全等级、分类和参数角色。

mod diff;
mod model;
mod napcat;
pub mod overrides;
mod refs;
mod snapshot;
mod snowluma;

pub use diff::param_diff;
pub use model::{Catalog, CatalogError};
pub use napcat::parse_napcat;
pub use overrides::{
    DANGEROUS, READ_ONLY_EXTRA, annotate_roles, category_from_name, infer_role, safety_for,
};
pub use refs::inline_refs;
pub use snapshot::snapshot;
pub use snowluma::parse_snowluma;

use serde_json::Value;

/// 没有参数（或上游没给参数 schema）的动作用的空 schema
fn default_params_schema() -> Value {
    serde_json::json!({"type": "object", "properties": {}})
}

/// 取非空字符串；缺失、非字符串、空白串都当没有
fn non_empty_str(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// 取非 null 的值。`null` 在文档里等于「没有示例」，不当作示例展示
fn non_null(value: Option<&Value>) -> Option<Value> {
    value.filter(|v| !v.is_null()).cloned()
}

/// 字符串数组；不是数组返回空，数组里非字符串的项丢掉
fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
