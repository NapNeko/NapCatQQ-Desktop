use serde_json::{Map, Value};

use ncd_domain::errors::MigrationError;
use ncd_domain::kinds::{BackendKind, BotFlavor, RuntimeTarget, SchemaVersion};
use ncd_domain::migration::{BotRuntimeSummary, MigrationWarning};
use ncd_traits::SecretStore;

pub const BOT_CONFIG_COMPAT_VERSION: &str = "v2.1";
const BOT_CONFIG_LEGACY_VERSION: &str = "v1.7.28";

#[derive(Debug, Clone, PartialEq)]
pub struct BotConfigMigrationResult {
    pub payload: Value,
    pub source_version: String,
    pub target_version: String,
    pub rules_applied: Vec<String>,
    pub summaries: Vec<BotRuntimeSummary>,
    pub warnings: Vec<MigrationWarning>,
}

pub fn migrate_bot_config(
    payload: Value,
    secrets: &dyn SecretStore,
) -> Result<BotConfigMigrationResult, MigrationError> {
    let source_version = read_bot_config_version(&payload);
    let (raw_bots, mut rules_applied) = normalize_root(payload)?;
    let mut migrated_bots = Vec::new();
    let mut summaries = Vec::new();
    let mut warnings = Vec::new();

    for (index, raw_bot) in raw_bots.into_iter().enumerate() {
        let mut bot = ensure_object(raw_bot, "single bot config must be an object")?;
        let entry_rules = migrate_bot_entry(&mut bot, index, secrets, &mut warnings)?;
        rules_applied.extend(
            entry_rules
                .into_iter()
                .map(|rule| format!("bots[{}]: {}", index, rule)),
        );
        if let Some(summary) = runtime_summary(&bot) {
            summaries.push(summary);
        }
        migrated_bots.push(Value::Object(bot));
    }

    Ok(BotConfigMigrationResult {
        payload: serde_json::json!({
            "info": {"configVersion": BOT_CONFIG_COMPAT_VERSION},
            "bots": migrated_bots,
        }),
        source_version,
        target_version: BOT_CONFIG_COMPAT_VERSION.to_string(),
        rules_applied,
        summaries,
        warnings,
    })
}

fn normalize_root(payload: Value) -> Result<(Vec<Value>, Vec<String>), MigrationError> {
    let mut rules = Vec::new();
    match payload {
        Value::Array(items) => {
            rules.push("root list -> object with info/bots".to_string());
            Ok((items, rules))
        }
        Value::Object(mut map) => {
            if let Some(bots) = map.remove("bots") {
                let Value::Array(items) = bots else {
                    return Err(MigrationError::InvalidPayload(
                        "bot.json 的 bots 字段必须为列表".to_string(),
                    ));
                };
                Ok((items, rules))
            } else if map.contains_key("bot")
                && map.contains_key("connect")
                && map.contains_key("advanced")
            {
                rules.push("single bot object -> object with info/bots".to_string());
                Ok((vec![Value::Object(map)], rules))
            } else {
                Err(MigrationError::InvalidPayload(
                    "bot.json 根节点必须为列表、包含 bots 的对象或单个 Bot 对象".to_string(),
                ))
            }
        }
        _ => Err(MigrationError::InvalidPayload(
            "bot.json 根节点必须为列表或对象".to_string(),
        )),
    }
}

fn migrate_bot_entry(
    payload: &mut Map<String, Value>,
    index: usize,
    secrets: &dyn SecretStore,
    warnings: &mut Vec<MigrationWarning>,
) -> Result<Vec<String>, MigrationError> {
    let mut rules = Vec::new();
    ensure_section(payload, "bot");
    ensure_section(payload, "connect");
    ensure_section(payload, "advanced");

    rules.extend(ensure_connect_shape(payload)?);
    rules.extend(normalize_advanced(payload)?);
    rules.extend(normalize_bot_fields(payload, index, secrets, warnings)?);
    rules.extend(normalize_qqid(payload));
    rules.extend(normalize_urls(payload));

    Ok(rules)
}

fn require_object_mut<'a>(
    payload: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>, MigrationError> {
    payload
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| MigrationError::InvalidPayload(format!("bot 配置缺少对象字段: {key}")))
}

fn require_array_mut<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Vec<Value>, MigrationError> {
    object
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| MigrationError::InvalidPayload(format!("bot 配置缺少数组字段: {key}")))
}

fn ensure_connect_shape(payload: &mut Map<String, Value>) -> Result<Vec<String>, MigrationError> {
    let mut rules = Vec::new();
    let connect = require_object_mut(payload, "connect")?;

    for key in [
        "httpServers",
        "httpSseServers",
        "httpClients",
        "websocketServers",
        "websocketClients",
    ] {
        if !connect.get(key).is_some_and(Value::is_array) {
            connect.insert(key.to_string(), Value::Array(Vec::new()));
            rules.push(format!("connect.{} default", key));
        }
    }

    if let Some(http) = connect.remove("http") {
        if let Some(server) = legacy_http_server(&http) {
            require_array_mut(connect, "httpServers")?.push(server);
            rules.push("connect.http -> connect.httpServers".to_string());
        }
        let clients = legacy_http_clients(&http);
        if !clients.is_empty() {
            require_array_mut(connect, "httpClients")?.extend(clients);
            rules.push("connect.http.postUrls -> connect.httpClients".to_string());
        }
    }

    if let Some(ws) = connect.remove("ws") {
        if let Some(server) = legacy_websocket_server(&ws) {
            require_array_mut(connect, "websocketServers")?.push(server);
            rules.push("connect.ws -> connect.websocketServers".to_string());
        }
    }

    if let Some(reverse_ws) = connect.remove("reverseWs") {
        let clients = legacy_websocket_clients(&reverse_ws);
        if !clients.is_empty() {
            require_array_mut(connect, "websocketClients")?.extend(clients);
            rules.push("connect.reverseWs -> connect.websocketClients".to_string());
        }
    }

    Ok(rules)
}

fn normalize_advanced(payload: &mut Map<String, Value>) -> Result<Vec<String>, MigrationError> {
    let mut rules = Vec::new();
    {
        let advanced = require_object_mut(payload, "advanced")?;
        if !advanced.get("bypass").is_some_and(Value::is_object) {
            advanced.insert("bypass".to_string(), serde_json::json!({}));
            rules.push("advanced.bypass default".to_string());
        }
        let bypass = advanced
            .get_mut("bypass")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                MigrationError::InvalidPayload("bot 配置 advanced.bypass 必须是对象".to_string())
            })?;
        for (key, default) in [
            ("hook", false),
            ("window", false),
            ("module", false),
            ("process", false),
            ("container", false),
            ("js", false),
        ] {
            if !bypass.contains_key(key) {
                bypass.insert(key.to_string(), Value::Bool(default));
                rules.push(format!("advanced.bypass.{} default", key));
            }
        }
    }
    let advanced = require_object_mut(payload, "advanced")?;
    for (key, default) in [
        ("fileLog", Value::Bool(true)),
        ("consoleLog", Value::Bool(true)),
        ("fileLogLevel", Value::from("info")),
        ("consoleLogLevel", Value::from("info")),
        ("packetBackend", Value::from("auto")),
        ("packetServer", Value::from("")),
        ("o3HookMode", Value::from(1)),
    ] {
        if !advanced.contains_key(key) {
            advanced.insert(key.to_string(), default);
            rules.push(format!("advanced.{} default", key));
        }
    }
    Ok(rules)
}

fn normalize_bot_fields(
    payload: &mut Map<String, Value>,
    index: usize,
    secrets: &dyn SecretStore,
    warnings: &mut Vec<MigrationWarning>,
) -> Result<Vec<String>, MigrationError> {
    let mut rules = Vec::new();
    let bot = require_object_mut(payload, "bot")?;

    if !bot.contains_key("name")
        || bot
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(str::is_empty)
    {
        bot.insert(
            "name".to_string(),
            Value::from(format!("bot-{}", index + 1)),
        );
        rules.push("bot.name default".to_string());
    }
    if !bot.contains_key("offlineAutoRestart") {
        bot.insert("offlineAutoRestart".to_string(), Value::Bool(false));
        rules.push("bot.offlineAutoRestart default".to_string());
    }
    if !bot.contains_key("runtime_target") {
        bot.insert("runtime_target".to_string(), Value::from("local"));
        rules.push("bot.runtime_target default".to_string());
    } else if bot
        .get("runtime_target")
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim().is_empty())
    {
        bot.insert("runtime_target".to_string(), Value::from("local"));
        rules.push("bot.runtime_target normalized".to_string());
    }
    if !bot.contains_key("backend_type") {
        bot.insert("backend_type".to_string(), Value::from("napcat"));
        rules.push("bot.backend_type default".to_string());
    }
    if !bot.contains_key("autoRestartSchedule") {
        bot.insert(
            "autoRestartSchedule".to_string(),
            serde_json::json!({
                "enable": false,
                "time_unit": "h",
                "duration": 6,
            }),
        );
        rules.push("bot.autoRestartSchedule default".to_string());
    }

    if let Some(secret) = bot
        .get("snowluma_webui_password_override")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let key = format!("bot:{}:snowluma_webui_password_override", bot_id(bot));
        if secrets.put(&key, secret).is_ok() {
            bot.remove("snowluma_webui_password_override");
            rules.push("bot.snowluma_webui_password_override migrated to secret store".to_string());
        } else {
            warnings.push(MigrationWarning::new(
                "secret_store_unavailable",
                format!(
                    "SnowLuma WebUI 密码保留在 Bot 配置中，因为安全存储写入失败: {}",
                    key
                ),
            ));
        }
    } else if bot.contains_key("snowluma_webui_password_override") {
        bot.remove("snowluma_webui_password_override");
        rules.push(
            "bot.snowluma_webui_password_override removed (empty legacy default)".to_string(),
        );
    }

    Ok(rules)
}

fn runtime_summary(payload: &Map<String, Value>) -> Option<BotRuntimeSummary> {
    let bot = payload.get("bot")?.as_object()?;
    let bot_id = bot_id(bot);
    let runtime_target = bot
        .get("runtime_target")
        .and_then(Value::as_str)
        .map(RuntimeTarget::from)
        .unwrap_or(RuntimeTarget::Local);
    let flavor = match bot
        .get("backend_type")
        .and_then(Value::as_str)
        .unwrap_or("napcat")
    {
        "snowluma" => BotFlavor::SnowLuma,
        _ => BotFlavor::NapCat,
    };
    let backend_kind = if runtime_target.is_local() {
        BackendKind::Local
    } else {
        BackendKind::RemoteSsh
    };
    let backend_id = runtime_target.server_id().unwrap_or("local").to_string();

    Some(BotRuntimeSummary::new(
        bot_id,
        backend_id,
        backend_kind,
        flavor,
        runtime_target,
        SchemaVersion::CURRENT,
    ))
}

fn read_bot_config_version(payload: &Value) -> String {
    payload
        .get("info")
        .and_then(|info| info.get("configVersion"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .unwrap_or(BOT_CONFIG_LEGACY_VERSION)
        .to_string()
}

fn ensure_section(payload: &mut Map<String, Value>, key: &str) {
    if !payload.get(key).is_some_and(Value::is_object) {
        payload.insert(key.to_string(), Value::Object(Map::new()));
    }
}

fn ensure_object(value: Value, message: &str) -> Result<Map<String, Value>, MigrationError> {
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(MigrationError::InvalidPayload(message.to_string())),
    }
}

fn bot_id(bot: &Map<String, Value>) -> String {
    bot.get("QQID")
        .and_then(|value| match value {
            Value::String(value) => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn bool_value(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn str_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn port_value(value: &Value, key: &str, default: u16) -> u16 {
    value
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0 && *value <= u16::MAX as u64)
        .unwrap_or(default as u64) as u16
}

fn legacy_http_server(http: &Value) -> Option<Value> {
    let meaningful = bool_value(http, "enable", false)
        || http
            .get("host")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
        || http.get("port").is_some();
    meaningful.then(|| {
        serde_json::json!({
            "enable": bool_value(http, "enable", false),
            "name": "legacy-http-server",
            "messagePostFormat": "array",
            "token": "",
            "debug": false,
            "host": str_value(http, "host", ""),
            "port": port_value(http, "port", 3000),
            "enableCors": false,
            "enableWebsocket": false,
        })
    })
}

fn legacy_http_clients(http: &Value) -> Vec<Value> {
    http.get("postUrls")
        .and_then(Value::as_array)
        .map(|urls| {
            urls.iter()
                .enumerate()
                .filter_map(|(index, url)| {
                    let url = url.as_str()?.trim();
                    (!url.is_empty()).then(|| {
                        serde_json::json!({
                            "enable": bool_value(http, "enablePost", true),
                            "name": format!("legacy-http-client-{}", index + 1),
                            "messagePostFormat": "array",
                            "token": "",
                            "debug": false,
                            "url": url,
                            "reportSelfMessage": false,
                        })
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn legacy_websocket_server(ws: &Value) -> Option<Value> {
    let meaningful = bool_value(ws, "enable", false)
        || ws
            .get("host")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
        || ws.get("port").is_some();
    meaningful.then(|| {
        serde_json::json!({
            "enable": bool_value(ws, "enable", false),
            "name": "legacy-websocket-server",
            "messagePostFormat": "array",
            "token": "",
            "debug": false,
            "host": str_value(ws, "host", ""),
            "port": port_value(ws, "port", 3001),
            "reportSelfMessage": false,
            "enableForcePushEvent": false,
            "heartInterval": 30000,
        })
    })
}

fn legacy_websocket_clients(reverse_ws: &Value) -> Vec<Value> {
    let enabled = bool_value(reverse_ws, "enable", false);
    reverse_ws
        .get("urls")
        .and_then(Value::as_array)
        .map(|urls| {
            urls.iter()
                .enumerate()
                .filter_map(|(index, url)| {
                    let url = url.as_str()?.trim();
                    (!url.is_empty()).then(|| {
                        serde_json::json!({
                            "enable": enabled,
                            "name": format!("legacy-websocket-client-{}", index + 1),
                            "messagePostFormat": "array",
                            "token": "",
                            "debug": false,
                            "url": url,
                            "reportSelfMessage": false,
                            "heartInterval": 30000,
                            "reconnectInterval": 30000,
                        })
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Convert string QQID to number in the raw JSON so that BotConfig can
/// deserialize it with plain u64 (no deserialize_with needed).
fn normalize_qqid(payload: &mut Map<String, Value>) -> Vec<String> {
    let mut rules = Vec::new();
    let bot = match payload.get_mut("bot").and_then(Value::as_object_mut) {
        Some(bot) => bot,
        None => return rules,
    };
    if let Some(value) = bot.get("QQID") {
        match value {
            Value::String(s) => {
                if let Ok(num) = s.trim().parse::<u64>() {
                    bot.insert("QQID".to_string(), Value::from(num));
                    rules.push("bot.QQID string -> number".to_string());
                }
            }
            Value::Number(n) if n.as_u64().is_none() => {
                if let Some(i) = n.as_i64() {
                    if let Ok(u) = u64::try_from(i) {
                        bot.insert("QQID".to_string(), Value::from(u));
                        rules.push("bot.QQID i64 -> u64".to_string());
                    }
                }
            }
            _ => {}
        }
    }
    rules
}

/// Validate that URL fields in connect entries are non-empty.
/// Replaces the deserialize_url serde helper — validation now happens at the Value level.
fn normalize_urls(payload: &mut Map<String, Value>) -> Vec<String> {
    let rules = Vec::new();
    let connect = match payload.get_mut("connect").and_then(Value::as_object_mut) {
        Some(c) => c,
        None => return rules,
    };
    for key in ["httpClients", "websocketClients"] {
        if let Some(entries) = connect.get_mut(key).and_then(Value::as_array_mut) {
            for entry in entries.iter_mut() {
                if let Some(obj) = entry.as_object_mut() {
                    if let Some(url) = obj.get("url").and_then(Value::as_str) {
                        if url.trim().is_empty() {
                            obj.insert("url".to_string(), Value::from("ws://127.0.0.1:6700"));
                        }
                    }
                }
            }
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_test_support::MockSecretStore;

    #[test]
    fn migrates_root_list_to_collection() {
        let result = migrate_bot_config(
            serde_json::json!([{ "bot": {"QQID": "10001", "name": "A"}, "connect": {}, "advanced": {} }]),
            &MockSecretStore::new(),
        )
        .unwrap();

        assert_eq!(result.payload["info"]["configVersion"], "v2.1");
        assert_eq!(result.payload["bots"].as_array().unwrap().len(), 1);
        assert_eq!(result.summaries[0].bot_id.as_str(), "10001");
    }
}
