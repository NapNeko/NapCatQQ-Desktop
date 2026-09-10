//! 插件配置：`data/plugins/<dir>/_conf_schema.json` 是表单，`data/config/<dir>_config.json` 是值。
//! 上游只在插件载入时按 schema 补默认、删未知键、按 schema 排序（`check_config_integrity`）；
//! 桌面端读原文时做同样的归一化，缺文件就整份物化但不落盘，保证用户看到的就是 AstrBot 下次载入后的样子。
//! 直接改文件不会热生效：运行中的插件持有内存字典，`save_config()` 还会把我们的改动覆盖回去，所以 `hot_reload = false`。

use ncd_domain::{
    AppConfigDocument, AppConfigFormat, AppConfigText, AppInstance, AppPluginConfigField,
    AppPluginConfigFieldKind, AppPluginConfigSchema,
};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde_json::{Map, Value};

use super::store::{parse_metadata_yaml, plugins_root, read_text};
use crate::config_doc::{read_document, render_json_pretty};

pub const PLUGIN_DOC_PREFIX: &str = "plugin:";
pub const ASTRBOT_CONF_SCHEMA: &str = "_conf_schema.json";
pub const ASTRBOT_PLUGIN_CONFIG_DIR: &str = "data/config";

pub fn plugin_doc_id(dir: &str) -> String {
    format!("{PLUGIN_DOC_PREFIX}{dir}")
}

pub fn parse_plugin_doc_id(doc_id: &str) -> Option<&str> {
    let dir = doc_id.strip_prefix(PLUGIN_DOC_PREFIX)?;
    if dir.is_empty() || dir.contains('/') || dir.contains('\\') || dir.contains("..") {
        return None;
    }
    Some(dir)
}

pub fn plugin_config_document(dir: &str) -> AppConfigDocument {
    AppConfigDocument {
        id: plugin_doc_id(dir),
        label: format!("{dir}_config.json"),
        rel_path: format!("{ASTRBOT_PLUGIN_CONFIG_DIR}/{dir}_config.json"),
        format: AppConfigFormat::Json,
        hot_reload: false,
    }
}

fn schema_path(instance: &AppInstance, dir: &str) -> HostPath {
    plugins_root(instance).join(dir).join(ASTRBOT_CONF_SCHEMA)
}

fn host_err(e: ncd_host::HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

/// 商店行的 id 是 `author/name`，配置文件却按目录名落；先按目录名对，再按 metadata 对。
pub async fn resolve_plugin_dir(
    host: &dyn Host,
    instance: &AppInstance,
    plugin_name: &str,
) -> Result<Option<String>, AppFrameworkError> {
    let wanted = plugin_name.trim();
    if wanted.is_empty() {
        return Ok(None);
    }
    let tail = wanted.rsplit('/').next().unwrap_or(wanted);
    let root = plugins_root(instance);
    if !host.exists(&root).await.map_err(host_err)? {
        return Ok(None);
    }
    let dirs: Vec<String> = host
        .list_dir(&root)
        .await
        .map_err(host_err)?
        .into_iter()
        .filter(|e| e.is_dir && !e.name.starts_with('.'))
        .map(|e| e.name)
        .collect();
    if let Some(d) = dirs.iter().find(|d| d.as_str() == tail || d.as_str() == wanted) {
        return Ok(Some(d.clone()));
    }
    if let Some(d) = dirs.iter().find(|d| d.eq_ignore_ascii_case(tail)) {
        return Ok(Some(d.clone()));
    }
    for d in &dirs {
        let Some(text) = read_text(host, &root.join(d).join("metadata.yaml")).await? else {
            continue;
        };
        let meta = parse_metadata_yaml(&text);
        if meta.plugin_id().eq_ignore_ascii_case(wanted) || meta.name.eq_ignore_ascii_case(tail) {
            return Ok(Some(d.clone()));
        }
    }
    Ok(None)
}

async fn read_schema(
    host: &dyn Host,
    instance: &AppInstance,
    dir: &str,
) -> Result<Option<Value>, AppFrameworkError> {
    let Some(text) = read_text(host, &schema_path(instance, dir)).await? else {
        return Ok(None);
    };
    let trimmed = text.trim_start_matches('\u{feff}');
    let v: Value = serde_json::from_str(trimmed).map_err(|e| {
        AppFrameworkError::Integration(format!("{dir}/{ASTRBOT_CONF_SCHEMA} 无法解析: {e}"))
    })?;
    if !v.is_object() {
        return Err(AppFrameworkError::Integration(format!(
            "{dir}/{ASTRBOT_CONF_SCHEMA} 根必须是对象"
        )));
    }
    Ok(Some(v))
}

pub async fn list_plugin_config_docs(
    host: &dyn Host,
    instance: &AppInstance,
    plugin_name: &str,
) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
    let Some(dir) = resolve_plugin_dir(host, instance, plugin_name).await? else {
        return Ok(Vec::new());
    };
    let doc = plugin_config_document(&dir);
    let has_schema = host
        .exists(&schema_path(instance, &dir))
        .await
        .map_err(host_err)?;
    let has_file = host
        .exists(&HostPath::from_posix(&instance.install_dir).join(&doc.rel_path))
        .await
        .map_err(host_err)?;
    Ok(if has_schema || has_file {
        vec![doc]
    } else {
        Vec::new()
    })
}

pub async fn plugin_config_schema(
    host: &dyn Host,
    instance: &AppInstance,
    plugin_name: &str,
) -> Result<Option<AppPluginConfigSchema>, AppFrameworkError> {
    let Some(dir) = resolve_plugin_dir(host, instance, plugin_name).await? else {
        return Ok(None);
    };
    let Some(schema) = read_schema(host, instance, &dir).await? else {
        return Ok(None);
    };
    Ok(Some(AppPluginConfigSchema {
        doc_id: plugin_doc_id(&dir),
        fields: translate_schema(&schema),
    }))
}

/// 读插件配置原文：有文件按 schema 归一化，没文件按 schema 物化；都没有就是空文本。
pub async fn read_plugin_config_text(
    host: &dyn Host,
    instance: &AppInstance,
    dir: &str,
) -> Result<AppConfigText, AppFrameworkError> {
    let doc = plugin_config_document(dir);
    let snap = read_document(host, &HostPath::from_posix(&instance.install_dir), &doc).await?;
    let schema = read_schema(host, instance, dir).await?;
    let text = match (snap.text, schema) {
        (Some(text), Some(schema)) => match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
            Ok(Value::Object(conf)) => render_json_pretty(&normalize_to_schema(&schema, &conf))?,
            _ => text,
        },
        (Some(text), None) => text,
        (None, Some(schema)) => render_json_pretty(&schema_defaults(&schema))?,
        (None, None) => String::new(),
    };
    Ok(AppConfigText {
        doc_id: doc.id,
        text,
        revision: snap.revision,
    })
}

fn schema_type(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or("")
}

fn schema_flag(node: &Value, key: &str) -> bool {
    node.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn schema_str(node: &Value, key: &str) -> String {
    node.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn field_kind(node: &Value) -> AppPluginConfigFieldKind {
    // editor_mode 是给 string 开代码编辑器；_special 是 WebUI 内的 provider 选择器，桌面端只能当字符串
    match schema_type(node) {
        "string" if schema_flag(node, "editor_mode") => AppPluginConfigFieldKind::Text,
        "string" => AppPluginConfigFieldKind::String,
        "text" => AppPluginConfigFieldKind::Text,
        "int" => AppPluginConfigFieldKind::Int,
        "float" => AppPluginConfigFieldKind::Float,
        "bool" => AppPluginConfigFieldKind::Bool,
        "list" => AppPluginConfigFieldKind::List,
        "object" => AppPluginConfigFieldKind::Object,
        _ => AppPluginConfigFieldKind::Json,
    }
}

fn option_strings(node: &Value) -> Vec<String> {
    node.get("options")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn translate_schema(schema: &Value) -> Vec<AppPluginConfigField> {
    let Some(obj) = schema.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter(|(_, node)| node.is_object() && !schema_flag(node, "invisible"))
        .map(|(key, node)| {
            let kind = field_kind(node);
            let description = schema_str(node, "description");
            AppPluginConfigField {
                key: key.clone(),
                kind,
                label: if description.is_empty() {
                    key.clone()
                } else {
                    description
                },
                hint: schema_str(node, "hint"),
                obvious_hint: schema_flag(node, "obvious_hint"),
                secret: schema_flag(node, "secret"),
                options: option_strings(node),
                items: if kind == AppPluginConfigFieldKind::Object {
                    node.get("items").map(translate_schema).unwrap_or_default()
                } else {
                    Vec::new()
                },
            }
        })
        .collect()
}

fn type_default(ty: &str) -> Value {
    match ty {
        "int" => Value::from(0),
        "float" => Value::from(0.0),
        "bool" => Value::Bool(false),
        "string" | "text" => Value::String(String::new()),
        "list" | "file" | "template_list" => Value::Array(Vec::new()),
        "object" | "dict" => Value::Object(Map::new()),
        _ => Value::Null,
    }
}

/// 上游 `_config_schema_to_default_config`
pub fn schema_defaults(schema: &Value) -> Value {
    let mut out = Map::new();
    if let Some(obj) = schema.as_object() {
        for (key, node) in obj {
            if !node.is_object() {
                continue;
            }
            let ty = schema_type(node);
            if ty == "object" {
                out.insert(
                    key.clone(),
                    node.get("items").map(schema_defaults).unwrap_or_else(|| Value::Object(Map::new())),
                );
            } else {
                out.insert(
                    key.clone(),
                    node.get("default").cloned().unwrap_or_else(|| type_default(ty)),
                );
            }
        }
    }
    Value::Object(out)
}

/// 上游 `check_config_integrity`：按 schema 顺序重建；缺键 / null / 类型不合用默认；`dict` 类型保留用户键；
/// schema 里没有的键丢掉。
pub fn normalize_to_schema(schema: &Value, conf: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    let Some(obj) = schema.as_object() else {
        return Value::Object(conf.clone());
    };
    for (key, node) in obj {
        if !node.is_object() {
            continue;
        }
        let ty = schema_type(node);
        let default = if ty == "object" {
            node.get("items").map(schema_defaults).unwrap_or_else(|| Value::Object(Map::new()))
        } else {
            node.get("default").cloned().unwrap_or_else(|| type_default(ty))
        };
        let value = match conf.get(key) {
            None | Some(Value::Null) => default,
            Some(existing) => {
                if default.is_object() {
                    match existing.as_object() {
                        None => default,
                        Some(_) if ty == "dict" => existing.clone(),
                        Some(child) => match node.get("items") {
                            Some(items) if ty == "object" => normalize_to_schema(items, child),
                            _ => existing.clone(),
                        },
                    }
                } else {
                    existing.clone()
                }
            }
        };
        out.insert(key.clone(), value);
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r#"{
  "token": { "description": "Bot Token", "type": "string", "secret": true, "hint": "醒目", "obvious_hint": true },
  "mode": { "description": "模式", "type": "string", "options": ["chat", "agent"], "default": "chat" },
  "prompt": { "type": "string", "editor_mode": true },
  "hidden": { "type": "bool", "invisible": true, "default": true },
  "sub": {
    "description": "嵌套",
    "type": "object",
    "items": {
      "name": { "type": "string" },
      "time": { "type": "int", "default": 123 }
    }
  },
  "extra": { "type": "dict" },
  "files": { "type": "file" }
}"#;

    fn schema() -> Value {
        serde_json::from_str(SCHEMA).unwrap()
    }

    #[test]
    fn doc_id_round_trips_and_rejects_paths() {
        assert_eq!(parse_plugin_doc_id(&plugin_doc_id("helloworld")), Some("helloworld"));
        assert_eq!(parse_plugin_doc_id("plugin:../x"), None);
        assert_eq!(parse_plugin_doc_id("plugin:a/b"), None);
        assert_eq!(parse_plugin_doc_id("cmd_config"), None);
        let doc = plugin_config_document("helloworld");
        assert_eq!(doc.rel_path, "data/config/helloworld_config.json");
        assert!(!doc.hot_reload);
    }

    #[test]
    fn translate_skips_invisible_and_maps_kinds() {
        let fields = translate_schema(&schema());
        let keys: Vec<&str> = fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["token", "mode", "prompt", "sub", "extra", "files"]);
        assert_eq!(fields[0].kind, AppPluginConfigFieldKind::String);
        assert!(fields[0].secret && fields[0].obvious_hint);
        assert_eq!(fields[0].label, "Bot Token");
        assert_eq!(fields[1].options, vec!["chat", "agent"]);
        assert_eq!(fields[2].kind, AppPluginConfigFieldKind::Text);
        assert_eq!(fields[2].label, "prompt");
        assert_eq!(fields[3].kind, AppPluginConfigFieldKind::Object);
        assert_eq!(fields[3].items.len(), 2);
        assert_eq!(fields[3].items[1].kind, AppPluginConfigFieldKind::Int);
        assert_eq!(fields[4].kind, AppPluginConfigFieldKind::Json);
        assert_eq!(fields[5].kind, AppPluginConfigFieldKind::Json);
    }

    #[test]
    fn defaults_follow_upstream_value_map() {
        let d = schema_defaults(&schema());
        assert_eq!(d["token"], "");
        assert_eq!(d["mode"], "chat");
        assert_eq!(d["hidden"], true);
        assert_eq!(d["sub"]["name"], "");
        assert_eq!(d["sub"]["time"], 123);
        assert_eq!(d["extra"], serde_json::json!({}));
        assert_eq!(d["files"], serde_json::json!([]));
    }

    #[test]
    fn normalize_fills_missing_drops_unknown_keeps_dict_and_order() {
        let conf: Map<String, Value> = serde_json::from_str(
            r#"{"stale": 1, "sub": {"time": 7, "junk": true}, "extra": {"user": "kept"}, "mode": null, "token": "t"}"#,
        )
        .unwrap();
        let out = normalize_to_schema(&schema(), &conf);
        let keys: Vec<&String> = out.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["token", "mode", "prompt", "hidden", "sub", "extra", "files"]);
        assert_eq!(out["token"], "t");
        assert_eq!(out["mode"], "chat");
        assert_eq!(out["sub"]["time"], 7);
        assert_eq!(out["sub"]["name"], "");
        assert!(out["sub"].get("junk").is_none());
        assert_eq!(out["extra"]["user"], "kept");
        assert!(out.get("stale").is_none());
    }

    #[test]
    fn normalize_replaces_type_mismatch_on_object() {
        let conf: Map<String, Value> = serde_json::from_str(r#"{"sub": "oops"}"#).unwrap();
        let out = normalize_to_schema(&schema(), &conf);
        assert_eq!(out["sub"]["time"], 123);
    }
}
