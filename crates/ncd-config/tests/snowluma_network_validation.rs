use std::sync::Arc;

use ncd_config::{LocalBotConfigRepo, LocalConfigStore};
use ncd_domain::{BotConfig, bot_config::BotConfigError};
use ncd_test_support::MockSecretStore;
use ncd_traits::BotConfigRepo;
use serde_json::{Value, json};

fn bot_with_networks(connect: Value) -> BotConfig {
    serde_json::from_value(json!({
        "bot": {"name": "test", "QQID": 10001, "backend_type": "snowluma"},
        "connect": connect,
        "advanced": {},
    }))
    .unwrap()
}

fn server(name: &str, host: &str, port: u16, path: &str, enabled: bool) -> Value {
    json!({"name": name, "host": host, "port": port, "path": path, "enable": enabled})
}

fn assert_invalid_field(config: &BotConfig, expected: &str) {
    config.validate().unwrap();
    match config.validate_network_urls().unwrap_err() {
        BotConfigError::InvalidSnowLumaNetwork { field, .. } => assert_eq!(field, expected),
        other => panic!("expected field {expected}, got {other:?}"),
    }
}

#[test]
fn snowluma_rejects_invalid_bind_hosts_even_when_the_server_is_disabled() {
    for host in [
        "",
        " localhost",
        "localhost ",
        "127.0.0.999",
        "01.2.3.4",
        "[::1]",
        "http://localhost",
        "host/path",
        "host?query",
        "user@host",
        "-host",
        "host_1",
        "host..local",
        "fe80::1%",
        "fe80::1%a_b",
    ] {
        for enabled in [true, false] {
            let config = bot_with_networks(json!({
                "httpServers": [server("http", host, 3000, "/", enabled)],
            }));
            assert_invalid_field(&config, "connect.httpServers[0].host");
        }
    }
}

#[test]
fn snowluma_accepts_bind_hosts_supported_by_upstream() {
    for host in [
        "0.0.0.0",
        "127.0.0.1",
        "::",
        "::1",
        "2001:db8::1",
        "fe80::1%eth0",
        "localhost",
        "example.local",
        "example.local.",
        "qq-host-1",
    ] {
        let config = bot_with_networks(json!({
            "websocketServers": [server("ws", host, 3001, "/onebot", true)],
        }));
        config.validate_network_urls().unwrap();
    }
}

#[test]
fn snowluma_rejects_paths_that_upstream_normalizes_or_interprets_as_a_url_suffix() {
    for path in [
        "onebot",
        " /onebot",
        "/onebot ",
        "/onebot?token=value",
        "/onebot#part",
        "/a/../onebot",
        "/a/./onebot",
        "/a\\onebot",
        "/hello world",
        "/中文",
        "/a/%2e%2e/onebot",
        "/onebot\n",
    ] {
        let config = bot_with_networks(json!({
            "websocketServers": [server("ws", "127.0.0.1", 3001, path, false)],
        }));
        assert_invalid_field(&config, "connect.websocketServers[0].path");
    }
    for path in ["", "/", "/onebot", "/onebot/", "/hello%20world", "//onebot"] {
        let config = bot_with_networks(json!({
            "httpServers": [server("http", "127.0.0.1", 3000, path, true)],
        }));
        config.validate_network_urls().unwrap();
    }
}

#[test]
fn snowluma_rejects_exact_and_wildcard_server_binding_conflicts_across_transports() {
    for (http_host, ws_host) in [
        ("127.0.0.1", "127.0.0.1"),
        ("LOCALHOST", "localhost"),
        ("0.0.0.0", "127.0.0.1"),
        ("127.0.0.1", "0.0.0.0"),
        ("::", "::1"),
        ("::1", "::"),
        ("0.0.0.0", "::1"),
    ] {
        let config = bot_with_networks(json!({
            "httpServers": [server("http", http_host, 3000, "/http", true)],
            "websocketServers": [server("ws", ws_host, 3000, "/ws", true)],
        }));
        assert_invalid_field(&config, "connect.websocketServers[0]");
        assert!(
            config
                .validate_network_urls()
                .unwrap_err()
                .to_string()
                .contains("connect.httpServers[0]")
        );
    }
}

#[test]
fn snowluma_keeps_disabled_or_distinct_server_bindings_available() {
    for (http_host, http_port, ws_host, ws_port, ws_enabled) in [
        ("0.0.0.0", 3000, "127.0.0.1", 3000, false),
        ("127.0.0.1", 3000, "127.0.0.2", 3000, true),
        ("0.0.0.0", 3000, "0.0.0.0", 3001, true),
    ] {
        let config = bot_with_networks(json!({
            "httpServers": [server("http", http_host, http_port, "/", true)],
            "websocketServers": [server("ws", ws_host, ws_port, "/", ws_enabled)],
        }));
        config.validate_network_urls().unwrap();
    }
}

#[test]
fn snowluma_checks_node_timer_bounds_including_disabled_clients() {
    for enabled in [true, false] {
        for timeout in [0, 2_147_483_648_u32, u32::MAX] {
            let config = bot_with_networks(json!({
                "httpClients": [{"name": "http", "url": "http://localhost/hook", "enable": enabled, "timeoutMs": timeout}],
            }));
            assert_invalid_field(&config, "connect.httpClients[0].timeoutMs");
        }
        for reconnect in [2_147_483_648_u32, u32::MAX] {
            let config = bot_with_networks(json!({
                "websocketClients": [{"name": "ws", "url": "ws://localhost/onebot", "enable": enabled, "reconnectInterval": reconnect}],
            }));
            assert_invalid_field(&config, "connect.websocketClients[0].reconnectInterval");
        }
    }
    for timeout in [1, 2_147_483_647_u32] {
        for reconnect in [0, 999, 1000, 2_147_483_647_u32] {
            let config = bot_with_networks(json!({
                "httpClients": [{"name": "http", "url": "http://localhost/hook", "timeoutMs": timeout}],
                "websocketClients": [{"name": "ws", "url": "ws://localhost/onebot", "reconnectInterval": reconnect}],
            }));
            config.validate_network_urls().unwrap();
        }
    }
}

#[test]
fn napcat_keeps_its_existing_server_and_timer_contract() {
    let mut config = bot_with_networks(json!({
        "httpServers": [server("http", " invalid host ", 3000, "relative?query", true)],
        "websocketServers": [server("ws", " invalid host ", 3000, "relative?query", true)],
        "httpClients": [{"name": "post", "url": "http://localhost/hook", "timeoutMs": u32::MAX}],
        "websocketClients": [{"name": "reverse", "url": "ws://localhost/onebot", "reconnectInterval": u32::MAX}],
    }));
    config.bot.backend_type = ncd_domain::bot_config::BackendType::NapCat;
    config.validate().unwrap();
    config.validate_network_urls().unwrap();
}

#[tokio::test]
async fn legacy_snowluma_network_can_be_loaded_and_corrected_without_rewriting_on_read() {
    let root = tempfile::tempdir().unwrap();
    let config_dir = root.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("bot.json");
    let legacy = bot_with_networks(json!({
        "websocketServers": [server("ws", "[::1]", 3001, "relative", true)],
    }));
    let original = json!({"bots": [legacy.clone()]}).to_string();
    std::fs::write(&path, &original).unwrap();
    let store = Arc::new(LocalConfigStore::new(root.path()));
    let repo = LocalBotConfigRepo::new(store.clone(), Arc::new(MockSecretStore::new()));

    assert_eq!(repo.get(10001).await.unwrap(), Some(legacy.clone()));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    assert_invalid_field(&legacy, "connect.websocketServers[0].host");

    let mut corrected = legacy;
    corrected.connect.websocket_servers[0].host = "::1".into();
    corrected.connect.websocket_servers[0].path = "/onebot".into();
    repo.upsert(corrected.clone()).await.unwrap();
    let reloaded = LocalBotConfigRepo::new(store, Arc::new(MockSecretStore::new()));
    assert_eq!(reloaded.get(10001).await.unwrap(), Some(corrected));
}

#[tokio::test]
async fn rejected_snowluma_network_leaves_disk_and_cache_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalConfigStore::new(root.path()));
    let repo = LocalBotConfigRepo::new(store, Arc::new(MockSecretStore::new()));
    let original = bot_with_networks(json!({
        "httpServers": [server("http", "127.0.0.1", 3000, "/", true)],
    }));
    repo.upsert(original.clone()).await.unwrap();
    let path = root.path().join("config/bot.json");
    let before = std::fs::read(&path).unwrap();
    let mut invalid = original.clone();
    invalid.connect.http_servers[0].path = "/onebot?query".into();

    assert!(matches!(
        repo.upsert(invalid).await,
        Err(BotConfigError::InvalidSnowLumaNetwork { .. })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(repo.get_cached(10001).await.unwrap(), Some(original));
}
