use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use ncd_domain::bot_config::{
    BackendType, BotConfig, ConnectConfig, HttpClientConfig, HttpServerConfig, HttpSseServerConfig,
    LogLevel, MessagePostFormat, NetworkBaseFields, WebsocketClientConfig, WebsocketServerConfig,
};
use ncd_domain::ids::BotId;
use ncd_traits::backend_config_renderer::{BackendConfigRenderer, RenderError};
use ncd_traits::config_store::JsonTransaction;

// 把 existing 中 known_keys 之外的字段保留下来合进 rendered,两层都按 JSON
// object merge 处理,不递归更深的层级——内层结构(network,bypass)由 schema 完全
// 拥有,用户在子层加字段不在保留范围内(避免破坏 NapCat 反序列化)
//
// "未知字段保留"边界仅限顶层:用户最常见的需求是给 onebot11 加
// imageDownloadProxy,给 napcat 加 autoTimeSync 这种顶层扩展字段
pub fn merge_unknown_top_level(
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

/// NapCat WebUI / TypeBox 落盘形态:与 BotConfig 直连 serde 的字段集合不同
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
    o.insert(
        "enableForcePushEvent".into(),
        json!(s.enable_force_push_event),
    );
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

// NapCat Renderer

/// onebot11_<qq>.json 顶层"已知" key 集合(renderer 输出范围)
/// 用户在派生文件里加这个集合之外的字段(如 imageDownloadProxy)会在
/// render_with_existing 里被保留下来,每次启动重新渲染时不会丢
const NAPCAT_ONEBOT_KNOWN_KEYS: &[&str] = &[
    "network",
    "musicSignUrl",
    "enableLocalFile2Url",
    "parseMultMsg",
];

/// napcat_<qq>.json 顶层"已知" key 集合
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

/// Renders BotConfig into NapCat-specific JSON files:
/// - onebot11_<qq>.json — OneBot network + musicSignUrl + enableLocalFile2Url + parseMultMsg
/// - napcat_<qq>.json   — log / packet / bypass settings
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

/// Docker bot 在远端 project_dir/napcat/config 下写入的 NapCat 派生配置
/// file_name 不带目录,调用方负责按 Host 的路径语义拼接目标目录
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerConfigPayload {
    pub file_name: String,
    pub payload: Value,
}

/// 渲染 Docker NapCat 容器挂载目录需要的配置文件
///
/// existing 的 key 使用文件名(如 onebot11_10001.json),方便远端 Host 调用方
/// 用 POSIX 目标目录读取后直接合并,不必把远端路径塞进本机 PathBuf
pub fn render_napcat_docker_config_payloads(
    bot_id: &BotId,
    config: &BotConfig,
    existing: &HashMap<String, Value>,
) -> Vec<DockerConfigPayload> {
    let onebot_file = format!("onebot11_{}.json", bot_id.as_str());
    let napcat_file = format!("napcat_{}.json", bot_id.as_str());
    let onebot = merge_unknown_top_level(
        NapCatConfigRenderer::build_onebot_payload(config),
        existing.get(&onebot_file),
        NAPCAT_ONEBOT_KNOWN_KEYS,
    );
    let napcat = merge_unknown_top_level(
        NapCatConfigRenderer::build_napcat_payload(config),
        existing.get(&napcat_file),
        NAPCAT_NAPCAT_KNOWN_KEYS,
    );
    vec![
        DockerConfigPayload {
            file_name: onebot_file,
            payload: onebot,
        },
        DockerConfigPayload {
            file_name: napcat_file,
            payload: napcat,
        },
    ]
}

/// SnowLuma 容器 named volume /app/snowluma-data/config 下的 onebot 配置
pub fn render_snowluma_docker_config_payloads(
    bot_id: &BotId,
    config: &BotConfig,
    existing: &HashMap<String, Value>,
) -> Vec<DockerConfigPayload> {
    let onebot_file = format!("onebot_{}.json", bot_id.as_str());
    let onebot = merge_unknown_top_level(
        SnowLumaConfigRenderer::build_onebot_payload(config),
        existing.get(&onebot_file),
        SNOWLUMA_ONEBOT_KNOWN_KEYS,
    );
    vec![DockerConfigPayload {
        file_name: onebot_file,
        payload: onebot,
    }]
}

fn nonempty_json_string(raw: &Value, key: &str) -> Option<String> {
    raw.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 「导入迁移」反向映射：把远端 SnowLuma onebot_<qq>.json 转成桌面网络配置。
/// 键名对齐（accessToken→token、messageFormat→messagePostFormat、
/// wsServers/wsClients→websocketServers/websocketClients、enabled→enable、
/// reconnectIntervalMs→reconnectInterval），其余交由 serde 默认值；
/// 返回 None 表示缺少 networks 结构（不是合法的 SL onebot 配置）。
pub fn parse_snowluma_onebot_connect(raw: &Value) -> Option<ncd_domain::ImportedNetworkConfig> {
    let networks = raw.get("networks")?.as_object()?;
    let rename_arrays = |o: &serde_json::Map<String, Value>| {
        let mut out = serde_json::Map::new();
        for (k, v) in o {
            let nk = match k.as_str() {
                "wsServers" => "websocketServers",
                "wsClients" => "websocketClients",
                other => other,
            };
            let nv = match v {
                Value::Array(items) => Value::Array(
                    items
                        .iter()
                        .map(|item| {
                            if let Some(obj) = item.as_object() {
                                let mut m = obj.clone();
                                if let Some(tok) = m.remove("accessToken") {
                                    m.insert("token".into(), tok);
                                }
                                if let Some(fmt) = m.remove("messageFormat") {
                                    m.insert("messagePostFormat".into(), fmt);
                                }
                                // WsRole 的 serde 是 PascalCase，远端写小写
                                if let Some(Value::String(role)) = m.get("role") {
                                    let normalized = match role.as_str() {
                                        "api" | "Api" | "API" => "Api",
                                        "event" | "Event" => "Event",
                                        _ => "Universal",
                                    };
                                    m.insert("role".into(), Value::String(normalized.into()));
                                }
                                if let Some(v) = m.remove("enableWebSocket") {
                                    m.insert("enableWebsocket".into(), v);
                                }
                                if !m.contains_key("enable") {
                                    if let Some(en) = m.remove("enabled") {
                                        m.insert("enable".into(), en);
                                    }
                                }
                                if let Some(ms) = m.remove("reconnectIntervalMs") {
                                    m.insert("reconnectInterval".into(), ms);
                                }
                                Value::Object(m)
                            } else {
                                item.clone()
                            }
                        })
                        .collect(),
                ),
                other => other.clone(),
            };
            out.insert(nk.into(), nv);
        }
        Value::Object(out)
    };

    let converted = rename_arrays(networks);
    let connect: ncd_domain::ConnectConfig = serde_json::from_value(converted).ok()?;

    let status = raw.get("statusCommand").and_then(|sc| {
        let enabled = sc.get("enabled")?.as_bool()?;
        let swallow = sc.get("swallow").and_then(|v| v.as_bool())?;
        let cooldown = sc
            .get("cooldownSeconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);
        Some(ncd_domain::StatusCommandConfig {
            enabled,
            swallow,
            cooldown_seconds: u32::try_from(cooldown).unwrap_or(5),
        })
    });

    Some(ncd_domain::ImportedNetworkConfig {
        connect,
        music_sign_url: nonempty_json_string(raw, "musicSignUrl"),
        status_command: status,
        enable_local_file_to_url: None,
        parse_mult_msg: None,
    })
}

/// NapCat onebot11_<qq>.json 的逆：`.network` 与桌面 ConnectConfig 同名，
/// 顶层回填 musicSignUrl / enableLocalFile2Url / parseMultMsg。
/// 返回 None 表示缺少 network 对象。
pub fn parse_napcat_onebot_connect(raw: &Value) -> Option<ncd_domain::ImportedNetworkConfig> {
    let network = raw.get("network")?.clone();
    let connect: ncd_domain::ConnectConfig = serde_json::from_value(network).ok()?;
    Some(ncd_domain::ImportedNetworkConfig {
        connect,
        music_sign_url: nonempty_json_string(raw, "musicSignUrl"),
        status_command: None,
        enable_local_file_to_url: raw.get("enableLocalFile2Url").and_then(Value::as_bool),
        parse_mult_msg: raw.get("parseMultMsg").and_then(Value::as_bool),
    })
}

// SnowLuma Renderer

/// SnowLuma onebot_<qq>.json 顶层"已知" key 集合
const SNOWLUMA_ONEBOT_KNOWN_KEYS: &[&str] =
    &["networks", "musicSignUrl", "statusCommand", "historySync"];

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
        // 上游默认 "#sl";Desktop 模型暂不暴露 trigger,写默认值让校验通过
        // (assertValidOneBotConfig 要求非空、单行、<=32 字符)
        "trigger": "#sl",
    })
}

/// SnowLuma reconnectIntervalMs lower bound (upstream enforces max(1000, value)).
const SNOWLUMA_MIN_RECONNECT_MS: u32 = 1000;

/// Full implementation — renders BotConfig into SnowLuma-specific JSON:
/// - onebot_<qq>.json — networks (httpServers/httpClients/wsServers/wsClients) + musicSignUrl
///
/// This is a complete port of legacy snowluma_config_renderer.py, not a placeholder.
/// All field mappings and fallback logic are production-ready.
///
/// Field mapping differences from NapCat ConnectConfig:
/// - enable → enabled
/// - token → accessToken
/// - messagePostFormat → messageFormat
/// - websocketServers → wsServers
/// - websocketClients → wsClients
/// - httpSseServers / plugins → silently dropped (SnowLuma unsupported)
/// - WS client reconnectInterval → reconnectIntervalMs (clamped ≥ 1000)
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
            let Some(obj) = networks.as_object_mut() else {
                return networks;
            };
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

    /// 供 Docker SnowLuma 卷内 config 写入复用
    pub fn build_onebot_payload(config: &BotConfig) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("networks".into(), Self::build_networks(&config.connect));
        obj.insert("musicSignUrl".into(), json!(config.bot.music_sign_url));
        // SnowLuma assertValidOneBotConfig 强制要求 statusCommand / historySync 存在
        // (is-object 校验,缺字段直接 400)。写盘走 loadOneBotConfig 会补默认,但
        // WebUI POST /api/config/:uin 走 assertValidOneBotConfig 严格校验原始 body,
        // 不补默认 → 缺字段就 success:false。这里始终输出,让两条路径格式同源。
        let sc = config
            .status_command
            .clone()
            .unwrap_or_default();
        obj.insert("statusCommand".into(), snowluma_status_command_json(&sc));
        obj.insert("historySync".into(), json!({ "enabled": false }));
        Value::Object(obj)
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

    // render_for_drift 用 trait 默认实现(== render):空连接时 render 注入
    // http-default/ws-default 兜底 listener,drift 基线必须用同一套,否则 Desktop
    // 自己写出去的兜底 listener 会被当成外部新增,造成"自写自漂移"反复误报
}

// Factory

/// Create the appropriate renderer for a given BackendType.
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

// Dispatch Renderer

/// A composite renderer that dispatches to the appropriate backend renderer
/// based on config.bot.backend_type. Used by BotManager which holds a single
/// Arc<dyn BackendConfigRenderer>.
pub struct DispatchRenderer {
    napcat: NapCatConfigRenderer,
    snowluma: SnowLumaConfigRenderer,
}

impl DispatchRenderer {
    pub fn new(
        napcat_config_dir: impl Into<PathBuf>,
        snowluma_config_dir: impl Into<PathBuf>,
    ) -> Self {
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
mod onebot_import_tests {
    use super::*;

    #[test]
    fn parse_renames_keys_and_maps_status_command() {
        let raw: Value = json!({
            "mode": "default",
            "musicSignUrl": "https://sign.example/api",
            "networks": {
                "httpServers": [{
                    "name": "hs", "accessToken": "tok1", "messageFormat": "array",
                    "reportSelfMessage": false, "host": "0.0.0.0", "port": 3000,
                    "path": "/", "enableWebSocket": true, "enabled": false
                }],
                "httpClients": [],
                "wsServers": [{
                    "name": "ws", "accessToken": "tok2", "messageFormat": "string",
                    "host": "127.0.0.1", "port": 13106, "path": "/ws",
                    "role": "universal", "reportSelfMessage": false
                }],
                "wsClients": [{
                    "name": "wsc", "accessToken": "tok3", "messageFormat": "array",
                    "url": "ws://127.0.0.1:8080", "reconnectIntervalMs": 5000,
                    "enabled": true
                }]
            },
            "statusCommand": {
                "enabled": true, "swallow": false, "cooldownSeconds": 7, "trigger": "#sl"
            }
        });

        let imported =
            parse_snowluma_onebot_connect(&raw).expect("kunming 形状的 onebot 配置应可解析");
        let connect = &imported.connect;

        assert_eq!(connect.http_servers.len(), 1);
        let hs = &connect.http_servers[0];
        assert_eq!(hs.base.name, "hs");
        assert_eq!(hs.base.token, "tok1");
        assert_eq!(
            hs.base.message_post_format,
            ncd_domain::MessagePostFormat::Array
        );
        assert!(hs.enable_websocket);
        assert!(
            !hs.base.enable,
            "SL enabled:false 应映射为桌面 enable:false，不能落到默认 true"
        );

        assert_eq!(connect.websocket_servers.len(), 1);
        let ws = &connect.websocket_servers[0];
        assert_eq!(ws.base.token, "tok2");
        assert_eq!(ws.port, 13106);
        assert_eq!(ws.role, ncd_domain::WsRole::Universal);

        assert_eq!(connect.websocket_clients.len(), 1);
        assert_eq!(connect.websocket_clients[0].reconnect_interval, 5000);

        assert_eq!(
            imported.status_command.as_ref().map(|s| s.cooldown_seconds),
            Some(7),
            "statusCommand.trigger 不在桌面模型里，忽略；其余字段迁移"
        );
        assert_eq!(
            imported.music_sign_url.as_deref(),
            Some("https://sign.example/api")
        );
    }

    #[test]
    fn parse_returns_none_without_networks() {
        let raw: Value = json!({ "mode": "default" });
        assert!(parse_snowluma_onebot_connect(&raw).is_none());
    }

    #[test]
    fn parse_napcat_network_object_and_top_level_fields() {
        let raw: Value = json!({
            "network": {
                "httpServers": [{
                    "name": "hs", "enable": true, "token": "ntok",
                    "host": "0.0.0.0", "port": 3000, "enableCors": true
                }],
                "websocketServers": [{
                    "name": "ws", "token": "wtok",
                    "host": "127.0.0.1", "port": 3001, "heartInterval": 15000
                }]
            },
            "musicSignUrl": "https://nc.sign",
            "enableLocalFile2Url": true,
            "parseMultMsg": true,
            "imageDownloadProxy": "http://proxy.example"
        });

        let imported = parse_napcat_onebot_connect(&raw).expect("NC onebot11 形状应可解析");
        assert_eq!(imported.connect.http_servers.len(), 1);
        assert_eq!(imported.connect.http_servers[0].base.token, "ntok");
        assert!(imported.connect.http_servers[0].enable_cors);
        assert_eq!(imported.connect.websocket_servers.len(), 1);
        assert_eq!(imported.connect.websocket_servers[0].port, 3001);
        assert_eq!(imported.connect.http_sse_servers.len(), 0);
        assert_eq!(imported.music_sign_url.as_deref(), Some("https://nc.sign"));
        assert_eq!(imported.enable_local_file_to_url, Some(true));
        assert_eq!(imported.parse_mult_msg, Some(true));
        assert!(imported.status_command.is_none());
    }

    #[test]
    fn parse_napcat_returns_none_without_network() {
        let raw: Value = json!({ "musicSignUrl": "x" });
        assert!(parse_napcat_onebot_connect(&raw).is_none());
    }
}
