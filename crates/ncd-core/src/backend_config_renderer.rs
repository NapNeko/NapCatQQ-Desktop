use std::path::PathBuf;

use serde_json::{Value, json};

use crate::bot_config::{
    BackendType, BotConfig, ConnectConfig, HttpClientConfig, HttpServerConfig,
    WebsocketClientConfig, WebsocketServerConfig,
};
use crate::ids::BotId;
use crate::traits::backend_config_renderer::{BackendConfigRenderer, RenderError};
use crate::traits::config_store::JsonTransaction;

// ==================== NapCat Renderer ====================

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
            "network": config.connect,
            "musicSignUrl": config.bot.music_sign_url,
            "enableLocalFile2Url": config.advanced.enable_local_file_to_url,
            "parseMultMsg": config.advanced.parse_mult_msg,
        })
    }

    fn build_napcat_payload(config: &BotConfig) -> Value {
        json!({
            "fileLog": config.advanced.file_log,
            "consoleLog": config.advanced.console_log,
            "fileLogLevel": config.advanced.file_log_level,
            "consoleLogLevel": config.advanced.console_log_level,
            "packetBackend": config.advanced.packet_backend,
            "packetServer": config.advanced.packet_server,
            "o3HookMode": config.advanced.o3_hook_mode,
            "bypass": config.advanced.bypass,
        })
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

    fn output_paths(&self, bot_id: &BotId) -> Vec<PathBuf> {
        vec![self.onebot_path(bot_id), self.napcat_path(bot_id)]
    }
}

// ==================== SnowLuma Renderer ====================

/// SnowLuma reconnectIntervalMs lower bound (upstream enforces max(1000, value)).
const SNOWLUMA_MIN_RECONNECT_MS: u32 = 1000;

/// **Full implementation** — renders `BotConfig` into SnowLuma-specific JSON:
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
            "messageFormat": server.base.message_post_format,
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
            "messageFormat": client.base.message_post_format,
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
            "messageFormat": server.base.message_post_format,
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
            "messageFormat": client.base.message_post_format,
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
        let no_servers = connect.http_servers.is_empty()
            && connect.websocket_servers.is_empty();

        if no_servers {
            let mut networks = Self::build_fallback_networks();
            let obj = networks.as_object_mut().unwrap();
            obj["httpClients"] = json!(
                connect.http_clients.iter().map(Self::render_http_client).collect::<Vec<_>>()
            );
            obj["wsClients"] = json!(
                connect.websocket_clients.iter().map(Self::render_ws_client).collect::<Vec<_>>()
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
        json!({
            "networks": Self::build_networks(&config.connect),
            "musicSignUrl": config.bot.music_sign_url,
        })
    }
}

impl BackendConfigRenderer for SnowLumaConfigRenderer {
    fn render(&self, bot_id: &BotId, config: &BotConfig) -> Result<JsonTransaction, RenderError> {
        let payload = Self::build_onebot_payload(config);
        let txn = JsonTransaction::new().write(self.onebot_path(bot_id), payload);
        Ok(txn)
    }

    fn output_paths(&self, bot_id: &BotId) -> Vec<PathBuf> {
        vec![self.onebot_path(bot_id)]
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

        let paths: Vec<_> = txn.writes.iter().map(|w| w.path.to_string_lossy().to_string()).collect();
        assert!(paths.iter().any(|p| p.contains("onebot11_10001.json")));
        assert!(paths.iter().any(|p| p.contains("napcat_10001.json")));
    }

    #[test]
    fn napcat_onebot_payload_contains_network_and_fields() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let onebot_write = txn.writes.iter().find(|w| {
            w.path.to_string_lossy().contains("onebot11_")
        }).unwrap();

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
        let napcat_write = txn.writes.iter().find(|w| {
            w.path.to_string_lossy().contains("napcat_")
        }).unwrap();

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
        assert!(txn.writes[0].path.to_string_lossy().contains("onebot_10001.json"));
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
        assert_eq!(txn.writes[0].payload["musicSignUrl"], "https://sign.example.com");
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
        let mut repo_txn = JsonTransaction::new()
            .write("/tmp/bot.json", json!({"bots": []}));

        repo_txn.merge(render_txn);

        assert_eq!(repo_txn.writes.len(), 3);
    }
}
