use ncd_domain::bot_config::{BackendType, BotConfig};
use ncd_domain::ids::BotId;
use ncd_traits::backend_config_renderer::{BackendConfigRenderer, RenderError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use ts_rs::TS;

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
    KeepAdded {
        file: String,
        path: String,
    },
    DropAdded {
        file: String,
        path: String,
    },
    AcceptExternal {
        file: String,
        path: String,
        #[ts(type = "unknown")]
        value: Value,
    },
    UseInternal {
        file: String,
        path: String,
    },
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
        .writes
        .into_iter()
        .map(|w| (w.path, w.payload))
        .collect();
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
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut expected_norm = expected_value.clone();
        let mut actual_norm = actual.clone();
        normalize_values_for_drift(&mut expected_norm);
        normalize_values_for_drift(&mut actual_norm);
        diff_json(
            config.bot.backend_type,
            &file_name,
            "",
            &expected_norm,
            &actual_norm,
            &mut added,
            &mut modified,
        );
    }
    Ok(ConfigDrift {
        bot_id: bot_id.as_str().to_string(),
        backend_type: config.bot.backend_type,
        added,
        modified,
    })
}

fn diff_json(
    backend_type: BackendType,
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
                    Some(av) => diff_json(backend_type, file, &p, ev, av, added, modified),
                    None => modified.push(DriftEntry {
                        file: file.into(),
                        path: p,
                        external: Value::Null,
                        internal: ev.clone(),
                    }),
                }
            }
        }
        (Value::Array(exp), Value::Array(act)) => {
            let shared_len = exp.len().min(act.len());
            for idx in 0..shared_len {
                let p = jp(prefix, &idx.to_string());
                diff_json(
                    backend_type,
                    file,
                    &p,
                    &exp[idx],
                    &act[idx],
                    added,
                    modified,
                );
            }
            for (idx, v) in act.iter().enumerate().skip(shared_len) {
                added.push(DriftEntry {
                    file: file.into(),
                    path: jp(prefix, &idx.to_string()),
                    external: v.clone(),
                    internal: Value::Null,
                });
            }
            for (idx, v) in exp.iter().enumerate().skip(shared_len) {
                modified.push(DriftEntry {
                    file: file.into(),
                    path: jp(prefix, &idx.to_string()),
                    external: Value::Null,
                    internal: v.clone(),
                });
            }
        }
        (a, b) if a == b => {}
        (exp, act) if empty_equivalence_allowed(backend_type, file, prefix, exp, act) => {}
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
    if prefix.is_empty() {
        key.into()
    } else {
        format!("{prefix}.{key}")
    }
}

#[derive(Debug, Clone, Copy)]
struct EmptyEquivalenceRule {
    backend_type: BackendType,
    file: &'static str,
    path: &'static str,
}

const EMPTY_EQUIVALENCE_RULES: &[EmptyEquivalenceRule] = &[
    EmptyEquivalenceRule {
        backend_type: BackendType::NapCat,
        file: "onebot11_*.json",
        path: "musicSignUrl",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::NapCat,
        file: "onebot11_*.json",
        path: "network.httpServers.*.token",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::NapCat,
        file: "onebot11_*.json",
        path: "network.httpSseServers.*.token",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::NapCat,
        file: "onebot11_*.json",
        path: "network.httpClients.*.token",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::NapCat,
        file: "onebot11_*.json",
        path: "network.httpClients.*.url",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::NapCat,
        file: "onebot11_*.json",
        path: "network.websocketServers.*.token",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::NapCat,
        file: "onebot11_*.json",
        path: "network.websocketClients.*.token",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::NapCat,
        file: "onebot11_*.json",
        path: "network.websocketClients.*.url",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::SnowLuma,
        file: "onebot_*.json",
        path: "musicSignUrl",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::SnowLuma,
        file: "onebot_*.json",
        path: "statusCommand",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::SnowLuma,
        file: "onebot_*.json",
        path: "networks.httpServers.*.accessToken",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::SnowLuma,
        file: "onebot_*.json",
        path: "networks.httpClients.*.accessToken",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::SnowLuma,
        file: "onebot_*.json",
        path: "networks.httpClients.*.url",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::SnowLuma,
        file: "onebot_*.json",
        path: "networks.wsServers.*.accessToken",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::SnowLuma,
        file: "onebot_*.json",
        path: "networks.wsClients.*.accessToken",
    },
    EmptyEquivalenceRule {
        backend_type: BackendType::SnowLuma,
        file: "onebot_*.json",
        path: "networks.wsClients.*.url",
    },
];

fn empty_equivalence_allowed(
    backend_type: BackendType,
    file: &str,
    path: &str,
    expected: &Value,
    actual: &Value,
) -> bool {
    is_trivially_empty(expected)
        && is_trivially_empty(actual)
        && EMPTY_EQUIVALENCE_RULES.iter().any(|rule| {
            rule.backend_type == backend_type
                && wildcard_match(rule.file, file)
                && path_match(rule.path, path)
        })
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        value.starts_with(prefix) && value.ends_with(suffix)
    } else {
        pattern == value
    }
}

fn path_match(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split('.').collect();
    let path_segments: Vec<&str> = path.split('.').collect();
    pattern_segments.len() == path_segments.len()
        && pattern_segments
            .iter()
            .zip(path_segments.iter())
            .all(|(p, s)| *p == "*" || p == s)
}

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

fn sort_adapter_array_by_name(arr: &mut [Value]) {
    for item in arr.iter_mut() {
        normalize_values_for_drift(item);
    }
    arr.sort_by_key(adapter_sort_key);
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
        diff_json(
            BackendType::NapCat,
            "onebot11_10001.json",
            "",
            &expected,
            &actual,
            &mut added,
            &mut modified,
        );
        assert!(added.is_empty() && modified.is_empty());
    }

    #[test]
    fn diff_treats_allowlisted_empty_values_as_equivalent() {
        let expected = json!("");
        let actual = json!(null);
        let mut added = Vec::new();
        let mut modified = Vec::new();
        diff_json(
            BackendType::NapCat,
            "onebot11_10001.json",
            "network.httpServers.0.token",
            &expected,
            &actual,
            &mut added,
            &mut modified,
        );
        assert!(added.is_empty() && modified.is_empty());
    }

    #[test]
    fn diff_reports_empty_values_outside_allowlist() {
        let expected = json!("");
        let actual = json!(null);
        let mut added = Vec::new();
        let mut modified = Vec::new();
        diff_json(
            BackendType::NapCat,
            "napcat_10001.json",
            "packetServer",
            &expected,
            &actual,
            &mut added,
            &mut modified,
        );
        assert!(added.is_empty());
        assert_eq!(modified.len(), 1);
        assert_eq!(modified[0].path, "packetServer");
    }

    #[test]
    fn diff_reports_napcat_user_cleared_token_and_url() {
        let expected = json!({
            "network": {
                "httpClients": [{
                    "name": "webhook",
                    "token": "secret123",
                    "url": "https://example.com/hook"
                }]
            }
        });
        let actual = json!({
            "network": {
                "httpClients": [{
                    "name": "webhook",
                    "token": "",
                    "url": ""
                }]
            }
        });
        let mut added = Vec::new();
        let mut modified = Vec::new();
        diff_json(
            BackendType::NapCat,
            "onebot11_10001.json",
            "",
            &expected,
            &actual,
            &mut added,
            &mut modified,
        );
        assert!(added.is_empty());
        assert_eq!(modified.len(), 2);
        assert!(
            modified
                .iter()
                .any(|entry| entry.path == "network.httpClients.0.token")
        );
        assert!(
            modified
                .iter()
                .any(|entry| entry.path == "network.httpClients.0.url")
        );
    }

    #[test]
    fn diff_reports_snowluma_user_cleared_access_token_and_url() {
        let expected = json!({
            "networks": {
                "httpClients": [{
                    "name": "webhook",
                    "accessToken": "secret123",
                    "url": "https://example.com/hook"
                }]
            }
        });
        let actual = json!({
            "networks": {
                "httpClients": [{
                    "name": "webhook",
                    "accessToken": "",
                    "url": ""
                }]
            }
        });
        let mut added = Vec::new();
        let mut modified = Vec::new();
        diff_json(
            BackendType::SnowLuma,
            "onebot_10001.json",
            "",
            &expected,
            &actual,
            &mut added,
            &mut modified,
        );
        assert!(added.is_empty());
        assert_eq!(modified.len(), 2);
        assert!(
            modified
                .iter()
                .any(|entry| entry.path == "networks.httpClients.0.accessToken")
        );
        assert!(
            modified
                .iter()
                .any(|entry| entry.path == "networks.httpClients.0.url")
        );
    }
}
