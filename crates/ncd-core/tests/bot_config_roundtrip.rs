use ncd_core::bot_config::NetworkBaseFields;
use ncd_core::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, BotConfigError,
    ConnectConfig, HttpServerConfig, MessagePostFormat, O3HookMode, RuntimeTarget,
    WebsocketServerConfig, WsRole,
};

fn bot_config(qq_id: u64, name: &str) -> BotConfig {
    BotConfig {
        bot: BotBasicConfig {
            name: name.to_string(),
            qq_id,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: BackendType::NapCat,
            snowluma_start_mode: None,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
    }
}

fn network_base(name: &str) -> NetworkBaseFields {
    NetworkBaseFields {
        enable: true,
        name: name.to_string(),
        message_post_format: MessagePostFormat::Array,
        token: String::new(),
        debug: false,
    }
}

#[test]
fn test_default_bot_config_round_trip() {
    let config = bot_config(10001, "bot");

    let json = serde_json::to_string(&config).unwrap();
    let decoded: BotConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, config);
}

#[test]
fn test_legacy_field_names_preserved() {
    let mut config = bot_config(10001, "bot");
    config.connect.http_servers.push(HttpServerConfig {
        base: network_base("http"),
        host: "127.0.0.1".to_string(),
        port: 3000,
        enable_cors: true,
        enable_websocket: false,
        path: "/".to_string(),
    });
    config
        .connect
        .websocket_servers
        .push(WebsocketServerConfig {
            base: network_base("ws"),
            host: "127.0.0.1".to_string(),
            port: 3001,
            report_self_message: false,
            enable_force_push_event: false,
            heart_interval: 30000,
            path: "/".to_string(),
            role: WsRole::Universal,
        });

    let json = serde_json::to_string(&config).unwrap();

    assert!(json.contains("\"QQID\""));
    assert!(json.contains("\"autoRestartSchedule\""));
    assert!(json.contains("\"messagePostFormat\""));
    assert!(json.contains("\"httpServers\""));
    assert!(json.contains("\"enableCors\""));
    assert!(json.contains("\"heartInterval\""));
    assert!(json.contains("\"o3HookMode\""));
    assert!(json.contains("\"fileLogLevel\""));
    assert!(json.contains("\"packetBackend\""));
    assert!(json.contains("\"enableLocalFile2Url\""));
}

#[test]
fn test_validate_rejects_qq_id_zero() {
    let config = bot_config(0, "bot");

    let error = config.validate().unwrap_err();

    assert!(matches!(error, BotConfigError::InvalidQqId(0)));
}

#[test]
fn test_deserialize_preserves_empty_name() {
    // Empty name normalization is handled by the migration layer (normalize_bot_fields),
    // not by serde deserialization. At the serde level, empty name is preserved as-is.
    let config = bot_config(10001, "");

    let json = serde_json::to_string(&config).unwrap();
    let decoded: BotConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.bot.name, "");
}

#[test]
fn test_validate_rejects_duplicate_connect_names_case_insensitive() {
    let mut config = bot_config(10001, "bot");
    config.connect.http_servers.push(HttpServerConfig {
        base: network_base("main"),
        host: "127.0.0.1".to_string(),
        port: 3000,
        enable_cors: false,
        enable_websocket: false,
        path: "/".to_string(),
    });
    config
        .connect
        .websocket_servers
        .push(WebsocketServerConfig {
            base: network_base("MAIN"),
            host: "127.0.0.1".to_string(),
            port: 3001,
            report_self_message: false,
            enable_force_push_event: false,
            heart_interval: 30000,
            path: "/".to_string(),
            role: WsRole::Universal,
        });

    let error = config.validate().unwrap_err();

    assert!(matches!(
        error,
        BotConfigError::DuplicateConnectName(name) if name == "main"
    ));
}

#[test]
fn test_message_post_format_serializes_lowercase() {
    assert_eq!(
        serde_json::to_string(&MessagePostFormat::Array).unwrap(),
        "\"array\""
    );
    assert_eq!(
        serde_json::to_string(&MessagePostFormat::String).unwrap(),
        "\"string\""
    );
}

#[test]
fn test_o3_hook_mode_serializes_as_integer() {
    let off: O3HookMode = serde_json::from_str("0").unwrap();
    let on: O3HookMode = serde_json::from_str("1").unwrap();

    assert_eq!(off, O3HookMode::Off);
    assert_eq!(on, O3HookMode::On);
    assert_eq!(serde_json::to_value(O3HookMode::Off).unwrap(), 0);
    assert_eq!(serde_json::to_value(O3HookMode::On).unwrap(), 1);
}

#[test]
fn test_runtime_target_default_is_local() {
    let config: BotConfig = serde_json::from_value(serde_json::json!({
        "bot": {
            "name": "bot",
            "QQID": 10001
        },
        "connect": {},
        "advanced": {}
    }))
    .unwrap();

    assert_eq!(config.bot.runtime_target, RuntimeTarget::Local);
}
