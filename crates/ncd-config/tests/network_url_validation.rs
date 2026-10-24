use std::sync::Arc;

use ncd_config::{LocalBotConfigRepo, LocalConfigStore};
use ncd_domain::{BotConfig, bot_config::BotConfigError};
use ncd_test_support::MockSecretStore;
use ncd_traits::BotConfigRepo;
use serde_json::json;

fn bot_with_client(group: &str, url: &str, enabled: bool) -> BotConfig {
    let mut payload = json!({
        "bot": {"name": "test", "QQID": 10001},
        "connect": {},
        "advanced": {},
    });
    payload["connect"][group] = json!([{
        "name": "client",
        "enable": enabled,
        "url": url,
    }]);
    serde_json::from_value(payload).unwrap()
}

#[tokio::test]
async fn invalid_client_urls_leave_disk_and_cached_bot_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalConfigStore::new(root.path()));
    let repo = LocalBotConfigRepo::new(store, Arc::new(MockSecretStore::new()));
    let original = bot_with_client("websocketClients", "ws://localhost:8080/onebot", true);
    repo.upsert(original.clone()).await.unwrap();
    let path = root.path().join("config/bot.json");
    let before = std::fs::read(&path).unwrap();

    for (group, url) in [
        ("httpClients", "http://"),
        ("httpClients", "https://bad host/path"),
        ("httpClients", "https://localhost:bad/path"),
        ("httpClients", "ws://localhost:8080/path"),
        ("websocketClients", "ws://"),
        ("websocketClients", "wss://:bad/path"),
        ("websocketClients", "ws://[::1/path"),
        ("websocketClients", "wss://localhost:65536/path"),
        ("websocketClients", "http://localhost:8080/path"),
    ] {
        let error = repo
            .upsert(bot_with_client(group, url, true))
            .await
            .unwrap_err();
        assert!(matches!(error, BotConfigError::InvalidUrl(_)), "{url}");
        assert_eq!(std::fs::read(&path).unwrap(), before, "{url}");
        assert_eq!(
            repo.get_cached(10001).await.unwrap(),
            Some(original.clone())
        );
        assert_eq!(repo.get(10001).await.unwrap(), Some(original.clone()));
    }
}

#[tokio::test]
async fn legacy_invalid_url_can_be_loaded_and_corrected() {
    let root = tempfile::tempdir().unwrap();
    let config_dir = root.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("bot.json");
    let legacy = bot_with_client("websocketClients", "ws://", true);
    let payload = json!({"bots": [legacy.clone()]}).to_string();
    std::fs::write(&path, &payload).unwrap();
    let store = Arc::new(LocalConfigStore::new(root.path()));
    let repo = LocalBotConfigRepo::new(store.clone(), Arc::new(MockSecretStore::new()));

    assert_eq!(repo.list().await.unwrap(), vec![legacy.clone()]);
    assert_eq!(repo.get(10001).await.unwrap(), Some(legacy.clone()));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);

    let mut corrected = legacy;
    corrected.connect.websocket_clients[0].url = "ws://localhost:8080/onebot".into();
    repo.upsert(corrected.clone()).await.unwrap();
    assert_eq!(
        repo.get_cached(10001).await.unwrap(),
        Some(corrected.clone())
    );
    let reloaded = LocalBotConfigRepo::new(store, Arc::new(MockSecretStore::new()));
    assert_eq!(reloaded.get(10001).await.unwrap(), Some(corrected));
}

#[tokio::test]
async fn valid_and_disabled_nonempty_client_urls_can_be_saved() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalConfigStore::new(root.path()));
    let repo = LocalBotConfigRepo::new(store.clone(), Arc::new(MockSecretStore::new()));

    for (group, url, enabled) in [
        ("httpClients", "http://[::1]:8080/webhook", true),
        (
            "httpClients",
            " https://localhost:8080/path?type=message ",
            true,
        ),
        ("websocketClients", "ws://[2001:db8::1]:8080/onebot", true),
        (
            "websocketClients",
            " wss://localhost/onebot?token=value ",
            true,
        ),
        ("httpClients", "https://:bad/path", false),
        ("websocketClients", "ws://", false),
    ] {
        let config = bot_with_client(group, url, enabled);
        repo.upsert(config.clone()).await.unwrap();
        assert_eq!(repo.get_cached(10001).await.unwrap(), Some(config.clone()));
        let reloaded = LocalBotConfigRepo::new(store.clone(), Arc::new(MockSecretStore::new()));
        assert_eq!(reloaded.get(10001).await.unwrap(), Some(config), "{url}");
    }
}
