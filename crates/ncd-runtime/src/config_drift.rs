use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use crate::bot_config::{BackendType, BotConfig};
use crate::ids::BotId;
use crate::traits::backend_config_renderer::{BackendConfigRenderer, RenderError};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct DriftEntry {
    pub file: String,
    pub path: String,
    #[ts(type = "unknown")]
    pub external: Value,
    #[ts(type = "unknown")]
    pub internal: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct ConfigDrift {
    pub bot_id: String,
    pub backend_type: BackendType,
    pub added: Vec<DriftEntry>,
    pub modified: Vec<DriftEntry>,
}

impl ConfigDrift {
    pub fn is_clean(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DriftDecision {
    KeepAdded { file: String, path: String },
    DropAdded { file: String, path: String },
    AcceptExternal {
        file: String,
        path: String,
        #[ts(type = "unknown")]
        value: Value,
    },
    UseInternal { file: String, path: String },
}

#[derive(Debug, thiserror::Error)]
pub enum DriftError {
    #[error("io: {0}")]
    Io(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("render: {0}")]
    Render(#[from] RenderError),
}

pub async fn detect_drift(
    bot_id: &BotId,
    config: &BotConfig,
    renderer: &dyn BackendConfigRenderer,
) -> Result<ConfigDrift, DriftError> {
    let expected_txn = renderer.render_for_drift(bot_id, config)?;
    let expected: HashMap<PathBuf, Value> = expected_txn
        .writes.into_iter().map(|w| (w.path, w.payload)).collect();
    let mut added = Vec::new();
    let mut modified = Vec::new();
    for (path, expected_value) in &expected {
        let bytes = match tokio::fs::read(path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(DriftError::Io(format!("{}: {e}", path.display()))),
        };
        let actual: Value = serde_json::from_slice(&bytes)
            .map_err(|e| DriftError::Parse(format!("{}: {e}", path.display())))?;
        let file_name = path.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut expected_norm = expected_value.clone();
        let mut actual_norm = actual.clone();
        normalize_values_for_drift(&mut expected_norm);
        normalize_values_for_drift(&mut actual_norm);
        diff_json(&file_name, "", &expected_norm, &actual_norm, &mut added, &mut modified);
    }
    Ok(ConfigDrift {
        bot_id: bot_id.as_str().to_string(),
        backend_type: config.bot.backend_type,
        added,
        modified,
    })
}

fn diff_json(
    file: &str,
    prefix: &str,
    expected: &Value,
    actual: &Value,
    added: &mut Vec<DriftEntry>,
    modified: &mut Vec<DriftEntry>,
) {
    match (expected, actual) {
        (Value::Object(exp), Value::Object(act)) => {
            for (k, v) in act {
                if !exp.contains_key(k) {
                    added.push(DriftEntry {
                        file: file.into(),
                        path: jp(prefix, k),
                        external: v.clone(),
                        internal: Value::Null,
                    });
                }
            }
            for (k, ev) in exp {
                let p = jp(prefix, k);
                match act.get(k) {
                    Some(av) => diff_json(file, &p, ev, av, added, modified),
                    None => modified.push(DriftEntry {
                        file: file.into(),
                        path: p,
                        external: Value::Null,
                        internal: ev.clone(),
                    }),
                }
            }
        }
        (a, b) if a == b => {}
        // 一方是空(null / 空数组 / 空字符串)另一方有内容:不值得让用户确认——
        // 典型场景是 daemon 第一次启动后生成了默认配置,我们这边还是 []；
        // 或者我们渲染了一个字段但 daemon 根本不认不写(null)。直接跳过。
        (exp, act) if is_trivially_empty(exp) || is_trivially_empty(act) => {
            let _ = (exp, act);
        }
        (exp, act) => {
            modified.push(DriftEntry {
                file: file.into(),
                path: prefix.into(),
                external: act.clone(),
                internal: exp.clone(),
            });
        }
    }
}

fn jp(prefix: &str, key: &str) -> String {
    if prefix.is_empty() { key.into() } else { format!("{prefix}.{key}") }
}

/// 判断一个 Value 是否"空"到不值得 diff。
fn is_trivially_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(arr) => arr.is_empty(),
        Value::String(s) => s.is_empty(),
        Value::Object(obj) => obj.is_empty(),
        _ => false,
    }
}

/// Align JSON shapes before diff so WebUI round-trips do not look like user edits.
fn normalize_values_for_drift(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for val in map.values_mut() {
                normalize_values_for_drift(val);
            }
            sort_network_adapter_arrays(map);
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                normalize_values_for_drift(item);
            }
        }
        _ => {}
    }
}

fn sort_network_adapter_arrays(map: &mut serde_json::Map<String, Value>) {
    const NETWORK_KEYS: &[&str] = &[
        "network",
        "networks",
        "httpServers",
        "httpSseServers",
        "httpClients",
        "websocketServers",
        "websocketClients",
        "wsServers",
        "wsClients",
        "plugins",
    ];
    for key in NETWORK_KEYS {
        if let Some(Value::Array(arr)) = map.get_mut(*key) {
            sort_adapter_array_by_name(arr);
        }
    }
    if let Some(Value::Object(net)) = map.get_mut("network") {
        sort_network_adapter_arrays(net);
    }
    if let Some(Value::Object(net)) = map.get_mut("networks") {
        sort_network_adapter_arrays(net);
    }
}

fn sort_adapter_array_by_name(arr: &mut Vec<Value>) {
    for item in arr.iter_mut() {
        normalize_values_for_drift(item);
    }
    arr.sort_by(|a, b| adapter_sort_key(a).cmp(&adapter_sort_key(b)));
}

fn adapter_sort_key(v: &Value) -> String {
    v.as_object()
        .and_then(|o| o.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diff_ignores_array_order_by_name() {
        let mut expected = json!({
            "network": {
                "httpServers": [
                    {"name": "B", "port": 2},
                    {"name": "A", "port": 1}
                ]
            }
        });
        let mut actual = json!({
            "network": {
                "httpServers": [
                    {"name": "A", "port": 1},
                    {"name": "B", "port": 2}
                ]
            }
        });
        let mut added = Vec::new();
        let mut modified = Vec::new();
        normalize_values_for_drift(&mut expected);
        normalize_values_for_drift(&mut actual);
        diff_json("f.json", "", &expected, &actual, &mut added, &mut modified);
        assert!(added.is_empty() && modified.is_empty());
    }

    #[test]
    fn diff_treats_null_and_empty_array_as_equivalent_at_leaf() {
        let expected = json!([]);
        let actual = json!(null);
        let mut added = Vec::new();
        let mut modified = Vec::new();
        diff_json("f.json", "httpClients", &expected, &actual, &mut added, &mut modified);
        assert!(added.is_empty() && modified.is_empty());
    }
}
