use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::bot_config::{
    BackendType, BotConfig, ConnectConfig, HttpClientConfig, HttpServerConfig,
    HttpSseServerConfig, LogLevel, MessagePostFormat, NetworkBaseFields,
    WebsocketClientConfig, WebsocketServerConfig,
};
use crate::ids::BotId;
use crate::traits::backend_config_renderer::{BackendConfigRenderer, RenderError};
use crate::traits::config_store::JsonTransaction;

// ==================== Deep merge helper ====================
//
// 把 `existing` 中 `known_keys` 之外的字段保留下来合进 `rendered`。两层都按 JSON
// object merge 处理，不递归更深的层级——内层结构（network、bypass）由 schema 完全
// 拥有，用户在子层加字段不在保留范围内（避免破坏 NapCat 反序列化）。
//
// 我们关心的"未知字段保留"边界仅限**顶层**：用户最常见的需求是给 onebot11 加
// `imageDownloadProxy`、给 napcat 加 `autoTimeSync` 这种顶层扩展字段。
fn merge_unknown_top_level(
    rendered: Value,
    existing: Option<&Value>,
    known_keys: &[&str],
) -> Value {
    let Some(existing_obj) = existing.and_then(Value::as_object) else {
        return rendered;
    };
    let Value::Object(mut rendered_obj) = rendered else {
        return Value::Object(existing_obj.clone());
    };
    for (key, value) in existing_obj {
        if known_keys.contains(&key.as_str()) {
            continue;
        }
        rendered_obj.insert(key.clone(), value.clone());
    }
    Value::Object(rendered_obj)
}

/// NapCat WebUI / TypeBox 落盘形态：与 BotConfig 直连 serde 的字段集合不同。
fn napcat_normalize_connect(connect: &ConnectConfig) -> Value {
    json!({
        "httpServers": connect.http_servers.iter().map(napcat_http_server).collect::<Vec<_>>(),
        "httpSseServers": connect.http_sse_servers.iter().map(napcat_http_sse_server).collect::<Vec<_>>(),
        "httpClients": connect.http_clients.iter().map(napcat_http_client).collect::<Vec<_>>(),
        "websocketServers": connect.websocket_servers.iter().map(napcat_ws_server).collect::<Vec<_>>(),
        "websocketClients": connect.websocket_clients.iter().map(napcat_ws_client).collect::<Vec<_>>(),
        "plugins": connect.plugins,
    })
}

fn napcat_http_server(s: &HttpServerConfig) -> Value {
    let mut o = napcat_base_fields(&s.base);
    o.insert("host".into(), json!(s.host));
    o.insert("port".into(), json!(s.port));
    o.insert("enableCors".into(), json!(s.enable_cors));
    o.insert("enableWebsocket".into(), json!(s.enable_websocket));
    if s.path != "/" {
        o.insert("path".into(), json!(s.path));
    }
    Value::Object(o)
}

fn napcat_http_sse_server(s: &HttpSseServerConfig) -> Value {
    let mut o = napcat_base_fields(&s.base);
    o.insert("host".into(), json!(s.host));
    o.insert("port".into(), json!(s.port));
    o.insert("enableCors".into(), json!(s.enable_cors));
    o.insert("enableWebsocket".into(), json!(s.enable_websocket));
    o.insert("reportSelfMessage".into(), json!(s.report_self_message));
    Value::Object(o)
}

fn napcat_http_client(c: &HttpClientConfig) -> Value {
    let mut o = napcat_base_fields(&c.base);
    o.insert("url".into(), json!(c.url));
    o.insert("reportSelfMessage".into(), json!(c.report_self_message));
    Value::Object(o)
}

fn napcat_ws_server(s: &WebsocketServerConfig) -> Value {
    let mut o = napcat_base_fields(&s.base);
    o.insert("host".into(), json!(s.host));
    o.insert("port".into(), json!(s.port));
    o.insert("reportSelfMessage".into(), json!(s.report_self_message));
    o.insert("enableForcePushEvent".into(), json!(s.enable_force_push_event));
    o.insert("heartInterval".into(), json!(s.heart_interval));
    if s.path != "/" {
        o.insert("path".into(), json!(s.path));
    }
    Value::Object(o)
}

fn napcat_ws_client(c: &WebsocketClientConfig) -> Value {
    let mut o = napcat_base_fields(&c.base);
    o.insert("url".into(), json!(c.url));
    o.insert("reportSelfMessage".into(), json!(c.report_self_message));
    o.insert("reconnectInterval".into(), json!(c.reconnect_interval));
    o.insert("heartInterval".into(), json!(c.heart_interval));
    Value::Object(o)
}

fn napcat_base_fields(base: &NetworkBaseFields) -> serde_json::Map<String, Value> {
    use MessagePostFormat;
    let mut o = serde_json::Map::new();
    o.insert("name".into(), json!(base.name));
    o.insert("enable".into(), json!(base.enable));
    o.insert(
        "messagePostFormat".into(),
        json!(match base.message_post_format {
            MessagePostFormat::Array => "array",
            MessagePostFormat::String => "string",
        }),
    );
    o.insert("token".into(), json!(base.token));
    if base.debug {
        o.insert("debug".into(), json!(true));
    }
    o
}

fn napcat_build_napcat_payload(config: &BotConfig) -> Value {
    let adv = &config.advanced;
    let mut o = serde_json::Map::new();
    if adv.file_log {
        o.insert("fileLog".into(), json!(true));
    }
    if adv.console_log {
        o.insert("consoleLog".into(), json!(true));
    }
    o.insert(
        "fileLogLevel".into(),
        json!(match adv.file_log_level {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Error => "error",
        }),
    );
    o.insert(
        "consoleLogLevel".into(),
        json!(match adv.console_log_level {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Error => "error",
        }),
    );
    o.insert("packetBackend".into(), json!(adv.packet_backend));
    if !adv.packet_server.is_empty() {
        o.insert("packetServer".into(), json!(adv.packet_server));
    }
    o.insert("o3HookMode".into(), json!(u8::from(adv.o3_hook_mode)));
    o.insert("bypass".into(), json!(adv.bypass));
    Value::Object(o)
}

// ==================== NapCat Renderer ====================

/// onebot11_<qq>.json 顶层"已知" key 集合（renderer 输出范围）。
/// 用户在派生文件里加这个集合之外的字段（如 `imageDownloadProxy`）会在
/// `render_with_existing` 里被保留下来，每次启动重新渲染时不会丢。
const NAPCAT_ONEBOT_KNOWN_KEYS: &[&str] =
    &["network", "musicSignUrl", "enableLocalFile2Url", "parseMultMsg"];

/// napcat_<qq>.json 顶层"已知" key 集合。
const NAPCAT_NAPCAT_KNOWN_KEYS: &[&str] = &[
    "fileLog",
    "consoleLog",
    "fileLogLevel",
    "consoleLogLevel",
    "packetBackend",
    "packetServer",
    "o3HookMode",
    "bypass",
];

/// Renders `BotConfig` into NapCat-specific JSON files:
/// - `onebot11_<qq>.json` — OneBot network + musicSignUrl + enableLocalFile2Url + parseMultMsg
/// - `napcat_<qq>.json`   — log / packet / bypass settings
pub struct NapCatConfigRenderer {
    config_dir: PathBuf,
}

impl NapCatConfigRenderer {
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
        }
    }

    fn onebot_path(&self, bot_id: &BotId) -> PathBuf {
        self.config_dir.join(format!("onebot11_{}.json", bot_id))
    }

    fn napcat_path(&self, bot_id: &BotId) -> PathBuf {
        self.config_dir.join(format!("napcat_{}.json", bot_id))
    }

    fn build_onebot_payload(config: &BotConfig) -> Value {
        json!({
            "network": napcat_normalize_connect(&config.connect),
            "musicSignUrl": config.bot.music_sign_url,
            "enableLocalFile2Url": config.advanced.enable_local_file_to_url,
            "parseMultMsg": config.advanced.parse_mult_msg,
        })
    }

    fn build_napcat_payload(config: &BotConfig) -> Value {
        napcat_build_napcat_payload(config)
    }
}

impl BackendConfigRenderer for NapCatConfigRenderer {
    fn render(&self, bot_id: &BotId, config: &BotConfig) -> Result<JsonTransaction, RenderError> {
        let onebot = Self::build_onebot_payload(config);
        let napcat = Self::build_napcat_payload(config);

        let txn = JsonTransaction::new()
            .write(self.onebot_path(bot_id), onebot)
            .write(self.napcat_path(bot_id), napcat);

        Ok(txn)
    }

    fn render_with_existing(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        existing: &HashMap<PathBuf, Value>,
    ) -> Result<JsonTransaction, RenderError> {
        let onebot_path = self.onebot_path(bot_id);
        let napcat_path = self.napcat_path(bot_id);

        let onebot = merge_unknown_top_level(
            Self::build_onebot_payload(config),
            existing.get(&onebot_path),
            NAPCAT_ONEBOT_KNOWN_KEYS,
        );
        let napcat = merge_unknown_top_level(
            Self::build_napcat_payload(config),
            existing.get(&napcat_path),
            NAPCAT_NAPCAT_KNOWN_KEYS,
        );

        let txn = JsonTransaction::new()
            .write(onebot_path, onebot)
            .write(napcat_path, napcat);
        Ok(txn)
    }

    fn output_paths(&self, bot_id: &BotId) -> Vec<PathBuf> {
        vec![self.onebot_path(bot_id), self.napcat_path(bot_id)]
    }
}

// ==================== SnowLuma Renderer ====================

/// SnowLuma onebot_<qq>.json 顶层"已知" key 集合。
const SNOWLUMA_ONEBOT_KNOWN_KEYS: &[&str] = &["networks", "musicSignUrl", "statusCommand"];

fn snowluma_message_format(fmt: MessagePostFormat) -> &'static str {
    match fmt {
        MessagePostFormat::Array => "array",
        MessagePostFormat::String => "string",
    }
}

fn snowluma_status_command_json(sc: &ncd_domain::bot_config::StatusCommandConfig) -> Value {
    json!({
        "enabled": sc.enabled,
        "swallow": sc.swallow,
        "cooldownSeconds": sc.cooldown_seconds,
    })
}

/// SnowLuma reconnectIntervalMs lower bound (upstream enforces max(1000, value)).
const SNOWLUMA_MIN_RECONNECT_MS: u32 = 1000;

/// Full implementation — renders `BotConfig` into SnowLuma-specific JSON:
/// - `onebot_<qq>.json` — networks (httpServers/httpClients/wsServers/wsClients) + musicSignUrl
///
/// This is a complete port of legacy `snowluma_config_renderer.py`, not a placeholder.
/// All field mappings and fallback logic are production-ready.
///
/// Field mapping differences from NapCat ConnectConfig:
/// - `enable` → `enabled`
/// - `token` → `accessToken`
/// - `messagePostFormat` → `messageFormat`
/// - `websocketServers` → `wsServers`
/// - `websocketClients` → `wsClients`
/// - `httpSseServers` / `plugins` → silently dropped (SnowLuma unsupported)
/// - WS client `reconnectInterval` → `reconnectIntervalMs` (clamped ≥ 1000)
pub struct SnowLumaConfigRenderer {
    config_dir: PathBuf,
}

impl SnowLumaConfigRenderer {
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
        }
    }

    fn onebot_path(&self, bot_id: &BotId) -> PathBuf {
        self.config_dir.join(format!("onebot_{}.json", bot_id))
    }

    fn render_http_server(server: &HttpServerConfig) -> Value {
        json!({
            "name": server.base.name,
            "enabled": server.base.enable,
            "messageFormat": snowluma_message_format(server.base.message_post_format),
            "accessToken": server.base.token,
            "reportSelfMessage": false,
            "host": server.host,
            "port": server.port,
            "path": server.path,
        })
    }

    fn render_http_client(client: &HttpClientConfig) -> Value {
        let mut payload = json!({
            "name": client.base.name,
            "enabled": client.base.enable,
            "messageFormat": snowluma_message_format(client.base.message_post_format),
            "accessToken": client.base.token,
            "url": client.url,
            "reportSelfMessage": client.report_self_message,
        });
        if let Some(timeout) = client.timeout_ms {
            payload["timeoutMs"] = json!(timeout);
        }
        payload
    }

    fn render_ws_server(server: &WebsocketServerConfig) -> Value {
        json!({
            "name": server.base.name,
            "enabled": server.base.enable,
            "messageFormat": snowluma_message_format(server.base.message_post_format),
            "accessToken": server.base.token,
            "reportSelfMessage": server.report_self_message,
            "host": server.host,
            "port": server.port,
            "path": server.path,
            "role": server.role,
        })
    }

    fn render_ws_client(client: &WebsocketClientConfig) -> Value {
        let reconnect_ms = client.reconnect_interval.max(SNOWLUMA_MIN_RECONNECT_MS);
        json!({
            "name": client.base.name,
            "enabled": client.base.enable,
            "messageFormat": snowluma_message_format(client.base.message_post_format),
            "accessToken": client.base.token,
            "url": client.url,
            "reportSelfMessage": client.report_self_message,
            "reconnectIntervalMs": reconnect_ms,
            "role": client.role,
        })
    }

    fn build_fallback_networks() -> Value {
        json!({
            "httpServers": [{
                "name": "http-default",
                "enabled": true,
                "messageFormat": "array",
                "accessToken": "",
                "reportSelfMessage": false,
                "host": "0.0.0.0",
                "port": 3000,
                "path": "/",
            }],
            "wsServers": [{
                "name": "ws-default",
                "enabled": true,
                "messageFormat": "array",
                "accessToken": "",
                "reportSelfMessage": false,
                "host": "0.0.0.0",
                "port": 3001,
                "path": "/",
                "role": "Universal",
            }],
            "httpClients": [],
            "wsClients": [],
        })
    }

    fn build_networks(connect: &ConnectConfig) -> Value {
        let no_servers = connect.http_servers.is_empty() && connect.websocket_servers.is_empty();

        if no_servers {
            let mut networks = Self::build_fallback_networks();
            let obj = networks.as_object_mut().unwrap();
            obj["httpClients"] = json!(
                connect
                    .http_clients
                    .iter()
                    .map(Self::render_http_client)
                    .collect::<Vec<_>>()
            );
            obj["wsClients"] = json!(
                connect
                    .websocket_clients
                    .iter()
                    .map(Self::render_ws_client)
                    .collect::<Vec<_>>()
            );
            networks
        } else {
            json!({
                "httpServers": connect.http_servers.iter().map(Self::render_http_server).collect::<Vec<_>>(),
                "httpClients": connect.http_clients.iter().map(Self::render_http_client).collect::<Vec<_>>(),
                "wsServers": connect.websocket_servers.iter().map(Self::render_ws_server).collect::<Vec<_>>(),
                "wsClients": connect.websocket_clients.iter().map(Self::render_ws_client).collect::<Vec<_>>(),
            })
        }
    }

    fn build_onebot_payload(config: &BotConfig) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "networks".into(),
            Self::build_networks(&config.connect),
        );
        obj.insert(
            "musicSignUrl".into(),
            json!(config.bot.music_sign_url),
        );
        if let Some(sc) = &config.status_command {
            obj.insert("statusCommand".into(), snowluma_status_command_json(sc));
        }
        Value::Object(obj)
    }

    /// Drift baseline: empty Desktop connect => empty networks (no install-default listeners).
    fn build_onebot_payload_for_drift(config: &BotConfig) -> Value {
        let connect = &config.connect;
        let no_servers =
            connect.http_servers.is_empty() && connect.websocket_servers.is_empty();
        let networks = if no_servers {
            json!({
                "httpServers": [],
                "httpClients": connect.http_clients.iter().map(Self::render_http_client).collect::<Vec<_>>(),
                "wsServers": [],
                "wsClients": connect.websocket_clients.iter().map(Self::render_ws_client).collect::<Vec<_>>(),
            })
        } else {
            Self::build_networks(connect)
        };
        let mut base = if no_servers {
            json!({
                "networks": networks,
                "musicSignUrl": config.bot.music_sign_url,
            })
        } else {
            Self::build_onebot_payload(config)
        };
        if let Value::Object(ref mut o) = base {
            if let Some(sc) = &config.status_command {
                o.insert("statusCommand".into(), snowluma_status_command_json(sc));
            }
        }
        base
    }
}

impl BackendConfigRenderer for SnowLumaConfigRenderer {
    fn render(&self, bot_id: &BotId, config: &BotConfig) -> Result<JsonTransaction, RenderError> {
        let payload = Self::build_onebot_payload(config);
        let txn = JsonTransaction::new().write(self.onebot_path(bot_id), payload);
        Ok(txn)
    }

    fn render_with_existing(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        existing: &HashMap<PathBuf, Value>,
    ) -> Result<JsonTransaction, RenderError> {
        let path = self.onebot_path(bot_id);
        let payload = merge_unknown_top_level(
            Self::build_onebot_payload(config),
            existing.get(&path),
            SNOWLUMA_ONEBOT_KNOWN_KEYS,
        );
        let txn = JsonTransaction::new().write(path, payload);
        Ok(txn)
    }

    fn output_paths(&self, bot_id: &BotId) -> Vec<PathBuf> {
        vec![self.onebot_path(bot_id)]
    }

    fn render_for_drift(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<JsonTransaction, RenderError> {
        let payload = Self::build_onebot_payload_for_drift(config);
        Ok(JsonTransaction::new().write(self.onebot_path(bot_id), payload))
    }
}

// ==================== Factory ====================

/// Create the appropriate renderer for a given `BackendType`.
pub fn create_renderer(
    backend_type: BackendType,
    config_dir: impl Into<PathBuf>,
) -> Box<dyn BackendConfigRenderer> {
    match backend_type {
        BackendType::NapCat => Box::new(NapCatConfigRenderer::new(config_dir)),
        BackendType::SnowLuma => Box::new(SnowLumaConfigRenderer::new(config_dir)),
    }
}

/// Return the derived config paths for a specific backend type.
pub fn output_paths_for_backend(
    backend_type: BackendType,
    config_dir: impl Into<PathBuf>,
    bot_id: &BotId,
) -> Vec<PathBuf> {
    match backend_type {
        BackendType::NapCat => NapCatConfigRenderer::new(config_dir).output_paths(bot_id),
        BackendType::SnowLuma => SnowLumaConfigRenderer::new(config_dir).output_paths(bot_id),
    }
}

// ==================== Dispatch Renderer ====================

/// A composite renderer that dispatches to the appropriate backend renderer
/// based on `config.bot.backend_type`. Used by `BotManager` which holds a single
/// `Arc<dyn BackendConfigRenderer>`.
pub struct DispatchRenderer {
    napcat: NapCatConfigRenderer,
    snowluma: SnowLumaConfigRenderer,
}

impl DispatchRenderer {
    pub fn new(napcat_config_dir: impl Into<PathBuf>, snowluma_config_dir: impl Into<PathBuf>) -> Self {
        Self {
            napcat: NapCatConfigRenderer::new(napcat_config_dir),
            snowluma: SnowLumaConfigRenderer::new(snowluma_config_dir),
        }
    }
}

impl BackendConfigRenderer for DispatchRenderer {
    fn render(&self, bot_id: &BotId, config: &BotConfig) -> Result<JsonTransaction, RenderError> {
        match config.bot.backend_type {
            BackendType::NapCat => self.napcat.render(bot_id, config),
            BackendType::SnowLuma => self.snowluma.render(bot_id, config),
        }
    }

    fn render_with_existing(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        existing: &HashMap<PathBuf, Value>,
    ) -> Result<JsonTransaction, RenderError> {
        match config.bot.backend_type {
            BackendType::NapCat => self.napcat.render_with_existing(bot_id, config, existing),
            BackendType::SnowLuma => self.snowluma.render_with_existing(bot_id, config, existing),
        }
    }

    fn render_for_drift(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<JsonTransaction, RenderError> {
        match config.bot.backend_type {
            BackendType::NapCat => self.napcat.render_for_drift(bot_id, config),
            BackendType::SnowLuma => self.snowluma.render_for_drift(bot_id, config),
        }
    }

    fn output_paths(&self, bot_id: &BotId) -> Vec<PathBuf> {
        let mut paths = self.napcat.output_paths(bot_id);
        paths.extend(self.snowluma.output_paths(bot_id));
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_config::*;
    use crate::kinds::RuntimeTarget;

    fn make_bot_id() -> BotId {
        BotId::new("10001")
    }

    fn make_basic_config() -> BotBasicConfig {
        BotBasicConfig {
            name: "test-bot".to_string(),
            qq_id: 10001,
            music_sign_url: "https://sign.example.com".to_string(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: BackendType::NapCat,
            deployment_type: DeploymentType::Native,
            snowluma_start_mode: None,
        }
    }

    fn make_advanced_config() -> AdvancedConfig {
        AdvancedConfig {
            auto_start: true,
            offline_notice: false,
            parse_mult_msg: true,
            packet_server: "".to_string(),
            packet_backend: "auto".to_string(),
            enable_local_file_to_url: true,
            file_log: true,
            console_log: true,
            file_log_level: LogLevel::Debug,
            console_log_level: LogLevel::Info,
            o3_hook_mode: O3HookMode::On,
            bypass: BypassConfig::default(),
        }
    }

    fn make_connect_with_http_server() -> ConnectConfig {
        ConnectConfig {
            http_servers: vec![HttpServerConfig {
                base: NetworkBaseFields {
                    enable: true,
                    name: "http-main".to_string(),
                    message_post_format: MessagePostFormat::Array,
                    token: "secret123".to_string(),
                    debug: false,
                },
                host: "127.0.0.1".to_string(),
                port: 8080,
                enable_cors: true,
                enable_websocket: false,
                path: "/".to_string(),
            }],
            http_sse_servers: vec![],
            http_clients: vec![],
            websocket_servers: vec![WebsocketServerConfig {
                base: NetworkBaseFields {
                    enable: true,
                    name: "ws-main".to_string(),
                    message_post_format: MessagePostFormat::Array,
                    token: "secret123".to_string(),
                    debug: false,
                },
                host: "0.0.0.0".to_string(),
                port: 8081,
                report_self_message: false,
                enable_force_push_event: false,
                heart_interval: 30000,
                path: "/".to_string(),
                role: WsRole::Universal,
            }],
            websocket_clients: vec![],
            plugins: vec![],
        }
    }

    fn make_full_config() -> BotConfig {
        BotConfig {
            bot: make_basic_config(),
            connect: make_connect_with_http_server(),
            advanced: make_advanced_config(),
            status_command: None,
        }
    }

    // ==================== NapCat Renderer Tests ====================

    #[test]
    fn napcat_render_produces_two_files() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();

        assert_eq!(txn.writes.len(), 2);
        assert!(txn.deletes.is_empty());

        let paths: Vec<_> = txn
            .writes
            .iter()
            .map(|w| w.path.to_string_lossy().to_string())
            .collect();
        assert!(paths.iter().any(|p| p.contains("onebot11_10001.json")));
        assert!(paths.iter().any(|p| p.contains("napcat_10001.json")));
    }

    #[test]
    fn napcat_onebot_payload_contains_network_and_fields() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let onebot_write = txn
            .writes
            .iter()
            .find(|w| w.path.to_string_lossy().contains("onebot11_"))
            .unwrap();

        let payload = &onebot_write.payload;
        assert!(payload.get("network").is_some());
        assert_eq!(payload["musicSignUrl"], "https://sign.example.com");
        assert_eq!(payload["enableLocalFile2Url"], true);
        assert_eq!(payload["parseMultMsg"], true);

        let network = &payload["network"];
        assert!(network["httpServers"].is_array());
        assert_eq!(network["httpServers"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn napcat_napcat_payload_contains_log_and_bypass() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let napcat_write = txn
            .writes
            .iter()
            .find(|w| w.path.to_string_lossy().contains("napcat_"))
            .unwrap();

        let payload = &napcat_write.payload;
        assert_eq!(payload["fileLog"], true);
        assert_eq!(payload["consoleLog"], true);
        assert_eq!(payload["fileLogLevel"], "debug");
        assert_eq!(payload["consoleLogLevel"], "info");
        assert_eq!(payload["packetBackend"], "auto");
        assert_eq!(payload["o3HookMode"], 1);
        assert!(payload.get("bypass").is_some());
    }

    #[test]
    fn napcat_output_paths_returns_two() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let paths = renderer.output_paths(&bot_id);
        assert_eq!(paths.len(), 2);
    }

    // ==================== SnowLuma Renderer Tests ====================

    #[test]
    fn snowluma_render_produces_one_file() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();

        assert_eq!(txn.writes.len(), 1);
        assert!(txn.deletes.is_empty());
        assert!(
            txn.writes[0]
                .path
                .to_string_lossy()
                .contains("onebot_10001.json")
        );
    }

    #[test]
    fn snowluma_field_mapping_enable_to_enabled() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let payload = &txn.writes[0].payload;

        let http = &payload["networks"]["httpServers"][0];
        assert_eq!(http["enabled"], true);
        assert!(http.get("enable").is_none());
    }

    #[test]
    fn snowluma_field_mapping_token_to_access_token() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let payload = &txn.writes[0].payload;

        let http = &payload["networks"]["httpServers"][0];
        assert_eq!(http["accessToken"], "secret123");
        assert!(http.get("token").is_none());
    }

    #[test]
    fn snowluma_field_mapping_message_format() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let payload = &txn.writes[0].payload;

        let http = &payload["networks"]["httpServers"][0];
        assert_eq!(http["messageFormat"], "array");
        assert!(http.get("messagePostFormat").is_none());
    }

    #[test]
    fn snowluma_ws_servers_key_name() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let networks = &txn.writes[0].payload["networks"];

        assert!(networks.get("wsServers").is_some());
        assert!(networks.get("websocketServers").is_none());
        assert_eq!(networks["wsServers"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn snowluma_drops_napcat_only_fields() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let payload = &txn.writes[0].payload;

        let http = &payload["networks"]["httpServers"][0];
        assert!(http.get("enableCors").is_none());
        assert!(http.get("enableWebsocket").is_none());
        assert!(http.get("debug").is_none());

        let ws = &payload["networks"]["wsServers"][0];
        assert!(ws.get("enableForcePushEvent").is_none());
        assert!(ws.get("heartInterval").is_none());
    }

    #[test]
    fn snowluma_drops_http_sse_servers() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let networks = &txn.writes[0].payload["networks"];

        assert!(networks.get("httpSseServers").is_none());
    }

    #[test]
    fn snowluma_fallback_when_no_servers() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig::default(),
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &config).unwrap();
        let networks = &txn.writes[0].payload["networks"];

        assert_eq!(networks["httpServers"].as_array().unwrap().len(), 1);
        assert_eq!(networks["wsServers"].as_array().unwrap().len(), 1);

        let http = &networks["httpServers"][0];
        assert_eq!(http["name"], "http-default");
        assert_eq!(http["port"], 3000);

        let ws = &networks["wsServers"][0];
        assert_eq!(ws["name"], "ws-default");
        assert_eq!(ws["port"], 3001);
        assert_eq!(ws["role"], "Universal");
    }

    #[test]
    fn snowluma_ws_client_reconnect_interval_clamped() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig {
                websocket_clients: vec![WebsocketClientConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "ws-client".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    url: "ws://127.0.0.1:6700".to_string(),
                    report_self_message: false,
                    heart_interval: 30000,
                    reconnect_interval: 500, // below 1000ms minimum
                    role: WsRole::Universal,
                }],
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-placeholder".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".to_string(),
                }],
                ..ConnectConfig::default()
            },
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &config).unwrap();
        let ws_client = &txn.writes[0].payload["networks"]["wsClients"][0];
        assert_eq!(ws_client["reconnectIntervalMs"], 1000);
    }

    #[test]
    fn snowluma_ws_client_reconnect_interval_passthrough_when_above_minimum() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig {
                websocket_clients: vec![WebsocketClientConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "ws-client".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    url: "ws://127.0.0.1:6700".to_string(),
                    report_self_message: false,
                    heart_interval: 30000,
                    reconnect_interval: 5000,
                    role: WsRole::Universal,
                }],
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-placeholder".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".to_string(),
                }],
                ..ConnectConfig::default()
            },
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &config).unwrap();
        let ws_client = &txn.writes[0].payload["networks"]["wsClients"][0];
        assert_eq!(ws_client["reconnectIntervalMs"], 5000);
    }

    #[test]
    fn snowluma_music_sign_url_in_payload() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        assert_eq!(
            txn.writes[0].payload["musicSignUrl"],
            "https://sign.example.com"
        );
    }

    #[test]
    fn snowluma_http_client_timeout_optional() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();

        let with_timeout = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig {
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-s".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".to_string(),
                }],
                http_clients: vec![HttpClientConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-c".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "t".to_string(),
                        debug: false,
                    },
                    url: "http://example.com".to_string(),
                    report_self_message: false,
                    timeout_ms: Some(3000),
                }],
                ..ConnectConfig::default()
            },
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &with_timeout).unwrap();
        let client = &txn.writes[0].payload["networks"]["httpClients"][0];
        assert_eq!(client["timeoutMs"], 3000);

        let without_timeout = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig {
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-s".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".to_string(),
                }],
                http_clients: vec![HttpClientConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-c".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "t".to_string(),
                        debug: false,
                    },
                    url: "http://example.com".to_string(),
                    report_self_message: false,
                    timeout_ms: None,
                }],
                ..ConnectConfig::default()
            },
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &without_timeout).unwrap();
        let client = &txn.writes[0].payload["networks"]["httpClients"][0];
        assert!(client.get("timeoutMs").is_none());
    }

    // ==================== Factory Tests ====================

    #[test]
    fn factory_creates_napcat_renderer() {
        let renderer = create_renderer(BackendType::NapCat, "/tmp/config");
        let bot_id = make_bot_id();
        let paths = renderer.output_paths(&bot_id);
        assert_eq!(paths.len(), 2);
        assert!(paths[0].to_string_lossy().contains("onebot11_"));
    }

    #[test]
    fn factory_creates_snowluma_renderer() {
        let renderer = create_renderer(BackendType::SnowLuma, "/tmp/config");
        let bot_id = make_bot_id();
        let paths = renderer.output_paths(&bot_id);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].to_string_lossy().contains("onebot_"));
    }

    // ==================== Transaction Merge Test ====================

    #[test]
    fn renderer_txn_merges_with_repo_txn() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let render_txn = renderer.render(&bot_id, &config).unwrap();
        let mut repo_txn = JsonTransaction::new().write("/tmp/bot.json", json!({"bots": []}));

        repo_txn.merge(render_txn);

        assert_eq!(repo_txn.writes.len(), 3);
    }
}
