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
    let expected_txn = renderer.render(bot_id, config)?;
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
        diff_json(&file_name, "", expected_value, &actual, &mut added, &mut modified);
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
