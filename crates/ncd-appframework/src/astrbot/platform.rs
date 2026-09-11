//! 认领 / 预置 `cmd_config.json` 的 `platform[]` 里那条 `aiocqhttp`。
//! 其它 type（qq_official / lark / telegram …）只读，不改、不删、不禁用。

use ncd_domain::APP_LINK_CONNECTION_PREFIX;
use ncd_traits::AppFrameworkError;
use serde_json::{Map, Value};

use super::manifest::{
    ASTRBOT_DEFAULT_DASHBOARD_PORT, KEY_WS_REVERSE_HOST, KEY_WS_REVERSE_PORT,
    KEY_WS_REVERSE_TOKEN, PLATFORM_TYPE_AIOCQHTTP,
};
use crate::config_doc::IssueSink;

pub const AMBIGUOUS_AIOCQHTTP: &str =
    "有多条 OneBot v11（aiocqhttp），无法唯一认领。到 AstrBot WebUI 或原文指定要对接的那条";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    Index(usize),
    Append,
}

pub fn ncd_platform_id(instance_id: &str) -> String {
    format!("{APP_LINK_CONNECTION_PREFIX}{instance_id}")
}

pub fn platforms(root: &Value) -> &[Value] {
    root.get("platform")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub fn platforms_mut(root: &mut Value) -> Result<&mut Vec<Value>, AppFrameworkError> {
    let obj = root
        .as_object_mut()
        .ok_or_else(|| AppFrameworkError::Integration("cmd_config.json 根必须是对象".into()))?;
    if !obj.contains_key("platform") {
        obj.insert("platform".into(), Value::Array(Vec::new()));
    }
    obj.get_mut("platform")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| AppFrameworkError::Integration("platform 必须是数组".into()))
}

pub fn is_aiocqhttp(row: &Value) -> bool {
    row.get("type")
        .and_then(Value::as_str)
        .is_some_and(|t| t == PLATFORM_TYPE_AIOCQHTTP)
}

pub fn is_enabled(row: &Value) -> bool {
    row.get("enable").and_then(Value::as_bool).unwrap_or(true)
}

pub fn row_id(row: &Value) -> Option<&str> {
    row.get("id").and_then(Value::as_str)
}

pub fn read_aiocqhttp_port(row: &Value) -> Option<u16> {
    let v = row.get(KEY_WS_REVERSE_PORT)?;
    if let Some(n) = v.as_u64() {
        return u16::try_from(n).ok().filter(|p| *p > 0);
    }
    v.as_str()?.parse().ok().filter(|p| *p > 0)
}

pub fn read_aiocqhttp_token(row: &Value) -> Option<String> {
    row.get(KEY_WS_REVERSE_TOKEN)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

pub fn read_aiocqhttp_host(row: &Value) -> String {
    row.get(KEY_WS_REVERSE_HOST)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("0.0.0.0")
        .to_string()
}

pub fn dashboard_port(root: &Value) -> u16 {
    root.get("dashboard")
        .and_then(|d| d.get("port"))
        .and_then(|v| {
            v.as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .or_else(|| v.as_str()?.parse().ok())
        })
        .filter(|p| *p > 0)
        .unwrap_or(ASTRBOT_DEFAULT_DASHBOARD_PORT)
}

pub fn set_dashboard_port(root: &mut Value, port: u16) -> Result<(), AppFrameworkError> {
    let obj = root
        .as_object_mut()
        .ok_or_else(|| AppFrameworkError::Integration("cmd_config.json 根必须是对象".into()))?;
    let dash = obj.entry("dashboard".to_string()).or_insert_with(|| {
        let mut m = Map::new();
        m.insert("enable".into(), Value::Bool(true));
        m.insert("host".into(), Value::String("0.0.0.0".into()));
        Value::Object(m)
    });
    let dash = dash
        .as_object_mut()
        .ok_or_else(|| AppFrameworkError::Integration("dashboard 必须是对象".into()))?;
    dash.insert("port".into(), Value::from(port));
    Ok(())
}

pub fn aiocqhttp_indices(platforms: &[Value]) -> Vec<usize> {
    platforms
        .iter()
        .enumerate()
        .filter(|(_, row)| is_aiocqhttp(row))
        .map(|(i, _)| i)
        .collect()
}

/// 导入探测：还没有 instance_id。恰好一条（或恰好一条已启用）才认。
pub fn claim_for_probe(rows: &[Value]) -> Option<usize> {
    let all = aiocqhttp_indices(rows);
    match all.as_slice() {
        [] => None,
        [i] => Some(*i),
        _ => {
            let enabled: Vec<usize> = all
                .into_iter()
                .filter(|i| is_enabled(&rows[*i]))
                .collect();
            match enabled.as_slice() {
                [i] => Some(*i),
                _ => None,
            }
        }
    }
}

/// 对接 / 写配置：按 id → 同口唯一 → 恰好一条已启用 → 零条追加；多条歧义报错。
pub fn claim_for_upsert(
    rows: &[Value],
    instance_id: &str,
    port: u16,
) -> Result<Claim, AppFrameworkError> {
    let want_id = ncd_platform_id(instance_id);
    if let Some(i) = rows.iter().position(|row| row_id(row) == Some(want_id.as_str())) {
        if !is_aiocqhttp(&rows[i]) {
            return Err(AppFrameworkError::Validation(format!(
                "id={want_id} 不是 aiocqhttp，拒绝改写成 OneBot"
            )));
        }
        return Ok(Claim::Index(i));
    }

    let all = aiocqhttp_indices(rows);
    if all.is_empty() {
        return Ok(Claim::Append);
    }

    if port > 0 {
        let same_port: Vec<usize> = all
            .iter()
            .copied()
            .filter(|i| read_aiocqhttp_port(&rows[*i]) == Some(port))
            .collect();
        match same_port.as_slice() {
            [i] => return Ok(Claim::Index(*i)),
            [] => {}
            _ => {
                return Err(AppFrameworkError::Validation(AMBIGUOUS_AIOCQHTTP.into()));
            }
        }
    }

    let enabled: Vec<usize> = all
        .iter()
        .copied()
        .filter(|i| is_enabled(&rows[*i]))
        .collect();
    match (all.as_slice(), enabled.as_slice()) {
        ([i], _) => Ok(Claim::Index(*i)),
        (_, [i]) => Ok(Claim::Index(*i)),
        ([], _) => Ok(Claim::Append),
        _ => Err(AppFrameworkError::Validation(AMBIGUOUS_AIOCQHTTP.into())),
    }
}

pub fn seed_row(instance_id: &str, port: u16, token: &str) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), Value::String(ncd_platform_id(instance_id)));
    m.insert("type".into(), Value::String(PLATFORM_TYPE_AIOCQHTTP.into()));
    m.insert("enable".into(), Value::Bool(true));
    m.insert(
        KEY_WS_REVERSE_HOST.into(),
        Value::String("0.0.0.0".into()),
    );
    m.insert(KEY_WS_REVERSE_PORT.into(), Value::from(port));
    m.insert(
        KEY_WS_REVERSE_TOKEN.into(),
        Value::String(token.to_string()),
    );
    Value::Object(m)
}

pub fn apply_claimed_fields(row: &mut Value, instance_id: &str, port: u16, token: &str) {
    let obj = match row.as_object_mut() {
        Some(o) => o,
        None => return,
    };
    obj.insert("id".into(), Value::String(ncd_platform_id(instance_id)));
    obj.insert("type".into(), Value::String(PLATFORM_TYPE_AIOCQHTTP.into()));
    obj.insert("enable".into(), Value::Bool(true));
    obj.insert(
        KEY_WS_REVERSE_HOST.into(),
        Value::String("0.0.0.0".into()),
    );
    obj.insert(KEY_WS_REVERSE_PORT.into(), Value::from(port));
    obj.insert(
        KEY_WS_REVERSE_TOKEN.into(),
        Value::String(token.to_string()),
    );
}

pub fn upsert_claimed_row(
    root: &mut Value,
    instance_id: &str,
    port: u16,
    token: &str,
) -> Result<usize, AppFrameworkError> {
    validate_proposed_port(root, instance_id, port)?;
    let claim = {
        let rows = platforms(root);
        claim_for_upsert(rows, instance_id, port)?
    };
    let rows = platforms_mut(root)?;
    match claim {
        Claim::Index(i) => {
            apply_claimed_fields(&mut rows[i], instance_id, port, token);
            Ok(i)
        }
        Claim::Append => {
            rows.push(seed_row(instance_id, port, token));
            Ok(rows.len() - 1)
        }
    }
}

pub fn other_platform_types(rows: &[Value], claimed: Option<usize>) -> Vec<String> {
    rows.iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != claimed)
        .filter_map(|(_, row)| row.get("type").and_then(Value::as_str))
        .filter(|t| *t != PLATFORM_TYPE_AIOCQHTTP)
        .map(ToOwned::to_owned)
        .collect()
}

pub fn validate_proposed_port(
    root: &Value,
    instance_id: &str,
    port: u16,
) -> Result<(), AppFrameworkError> {
    let mut sink = IssueSink::default();
    collect_port_issues(root, instance_id, port, &mut sink);
    let issues = sink.into_vec();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(AppFrameworkError::ConfigInvalid(issues))
    }
}

pub fn collect_port_issues(root: &Value, instance_id: &str, port: u16, sink: &mut IssueSink) {
    if port == 0 {
        sink.push("onebot/ws_reverse_port", "端口不能为 0");
        return;
    }
    let dash = dashboard_port(root);
    if port == dash {
        sink.push(
            "onebot/ws_reverse_port",
            format!("OneBot 口不能与 WebUI 口相同（{dash}）"),
        );
    }
    let want_id = ncd_platform_id(instance_id);
    let rows = platforms(root);
    let claimed = claim_for_upsert(rows, instance_id, port).ok();
    for (i, row) in rows.iter().enumerate() {
        if !is_aiocqhttp(row) {
            continue;
        }
        let skip = match claimed {
            Some(Claim::Index(ci)) if ci == i => true,
            _ => row_id(row) == Some(want_id.as_str()),
        };
        if skip {
            continue;
        }
        if read_aiocqhttp_port(row) == Some(port) {
            sink.push(
                "onebot/ws_reverse_port",
                "OneBot 口已被其它 aiocqhttp 行占用",
            );
            break;
        }
    }
}

/// 从 6185 起找一个不与 OneBot 口、也不在 `taken` 里的 WebUI 口。
pub fn next_dashboard_port(ws_port: u16, taken: &[u16]) -> u16 {
    let mut p = ASTRBOT_DEFAULT_DASHBOARD_PORT;
    for _ in 0..256 {
        if p != ws_port && !taken.contains(&p) {
            return p;
        }
        p = p.saturating_add(1);
        if p == 0 {
            break;
        }
    }
    ASTRBOT_DEFAULT_DASHBOARD_PORT.saturating_add(1)
}

pub fn default_empty_root() -> Value {
    let mut dash = Map::new();
    dash.insert("enable".into(), Value::Bool(true));
    dash.insert("host".into(), Value::String("0.0.0.0".into()));
    dash.insert(
        "port".into(),
        Value::from(ASTRBOT_DEFAULT_DASHBOARD_PORT),
    );
    let mut root = Map::new();
    root.insert("config_version".into(), Value::from(3));
    root.insert("dashboard".into(), Value::Object(dash));
    root.insert("platform".into(), Value::Array(Vec::new()));
    Value::Object(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    const EMPTY: &str = r#"{
  "config_version": 3,
  "dashboard": { "host": "0.0.0.0", "port": 6185 },
  "platform": []
}"#;

    const SINGLE_DEFAULT: &str = r#"{
  "dashboard": { "port": 6185 },
  "platform": [
    {
      "id": "default",
      "type": "aiocqhttp",
      "enable": true,
      "ws_reverse_host": "0.0.0.0",
      "ws_reverse_port": 6199,
      "ws_reverse_token": ""
    }
  ]
}"#;

    const NCD_PLUS_OFFICIAL: &str = r#"{
  "dashboard": { "port": 6185 },
  "platform": [
    {
      "id": "ncd-app:a1",
      "type": "aiocqhttp",
      "enable": true,
      "ws_reverse_host": "0.0.0.0",
      "ws_reverse_port": 6201,
      "ws_reverse_token": "tok"
    },
    {
      "id": "default",
      "type": "qq_official",
      "enable": true,
      "appid": "x",
      "secret": "y"
    }
  ]
}"#;

    const TWO_AIOCQHTTP: &str = r#"{
  "dashboard": { "port": 6185 },
  "platform": [
    {
      "id": "one",
      "type": "aiocqhttp",
      "enable": true,
      "ws_reverse_port": 6199
    },
    {
      "id": "two",
      "type": "aiocqhttp",
      "enable": true,
      "ws_reverse_port": 6200
    }
  ]
}"#;

    const DASH_EQ_WS: &str = r#"{
  "dashboard": { "port": 6199 },
  "platform": [
    {
      "id": "ncd-app:a1",
      "type": "aiocqhttp",
      "enable": true,
      "ws_reverse_port": 6199
    }
  ]
}"#;

    #[test]
    fn probe_empty_is_none() {
        let root = parse(EMPTY);
        assert_eq!(claim_for_probe(platforms(&root)), None);
        assert_eq!(dashboard_port(&root), 6185);
    }

    #[test]
    fn probe_single_default_aiocqhttp() {
        let root = parse(SINGLE_DEFAULT);
        let i = claim_for_probe(platforms(&root)).unwrap();
        assert_eq!(read_aiocqhttp_port(&platforms(&root)[i]), Some(6199));
    }

    #[test]
    fn upsert_by_ncd_id_leaves_qq_official() {
        let mut root = parse(NCD_PLUS_OFFICIAL);
        let idx = upsert_claimed_row(&mut root, "a1", 6201, "tok2").unwrap();
        assert_eq!(idx, 0);
        let rows = platforms(&root);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["type"], "qq_official");
        assert_eq!(rows[1]["enable"], true);
        assert_eq!(rows[0]["ws_reverse_token"], "tok2");
        assert_eq!(rows[0]["id"], "ncd-app:a1");
    }

    #[test]
    fn two_aiocqhttp_without_unique_hit_is_error() {
        let root = parse(TWO_AIOCQHTTP);
        let err = claim_for_upsert(platforms(&root), "a1", 0).unwrap_err();
        assert!(err.to_string().contains("多条"));
        assert_eq!(claim_for_probe(platforms(&root)), None);
    }

    #[test]
    fn two_aiocqhttp_same_port_claims_that_row() {
        let root = parse(TWO_AIOCQHTTP);
        assert_eq!(
            claim_for_upsert(platforms(&root), "a1", 6200).unwrap(),
            Claim::Index(1)
        );
    }

    #[test]
    fn empty_upsert_appends_ncd_row() {
        let mut root = parse(EMPTY);
        upsert_claimed_row(&mut root, "a1", 6199, "t").unwrap();
        let rows = platforms(&root);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "ncd-app:a1");
        assert_eq!(rows[0]["type"], "aiocqhttp");
        assert_eq!(rows[0]["enable"], true);
        assert_eq!(rows[0]["ws_reverse_host"], "0.0.0.0");
        assert_eq!(rows[0]["ws_reverse_port"], 6199);
    }

    #[test]
    fn dashboard_equals_ws_is_rejected() {
        let root = parse(DASH_EQ_WS);
        let err = validate_proposed_port(&root, "a1", 6199).unwrap_err();
        assert!(err.to_string().contains("WebUI"));
    }

    #[test]
    fn refuse_to_rewrite_other_type_with_ncd_id() {
        let root = parse(
            r#"{
  "platform": [{ "id": "ncd-app:a1", "type": "lark", "enable": true }]
}"#,
        );
        let err = claim_for_upsert(platforms(&root), "a1", 6199).unwrap_err();
        assert!(err.to_string().contains("不是 aiocqhttp"));
    }

    #[test]
    fn pick_dashboard_avoids_ws_port() {
        assert_eq!(next_dashboard_port(6199, &[]), 6185);
        assert_eq!(next_dashboard_port(6185, &[]), 6186);
        assert_eq!(next_dashboard_port(6199, &[6185]), 6186);
        assert_eq!(next_dashboard_port(6199, &[6185, 6186]), 6187);
    }
}
