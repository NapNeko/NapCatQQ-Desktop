use std::sync::Arc;

use ncd_config::{LocalBotConfigRepo, LocalConfigStore};
use ncd_domain::BotConfig;
use ncd_test_support::MockSecretStore;
use serde_json::json;

#[tokio::test]
async fn cached_queries_do_not_load_or_migrate_secrets() {
    let root = tempfile::tempdir().unwrap();
    let config_dir = root.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("bot.json");
    let payload = json!({"bots": [{
        "bot": {"name": "test", "QQID": 10001, "snowluma_webui_password_override": "must-not-migrate"},
        "connect": {}, "advanced": {},
    }]}).to_string();
    std::fs::write(&path, &payload).unwrap();
    let secrets = Arc::new(MockSecretStore::new());
    let store = Arc::new(LocalConfigStore::new(root.path()));
    let repo = LocalBotConfigRepo::new(store, secrets.clone());

    assert!(repo.get_cached(10001).await.is_err());
    assert_eq!(secrets.len(), 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);

    let config: BotConfig = serde_json::from_value(json!({
        "bot": {"name": "cached", "QQID": 10001}, "connect": {}, "advanced": {},
    }))
    .unwrap();
    repo.replace_with(Some(vec![config]), async { Ok::<(), String>(()) })
        .await
        .unwrap();
    assert_eq!(
        repo.get_cached(10001).await.unwrap().unwrap().bot.name,
        "cached"
    );
    assert!(repo.get_cached(10002).await.unwrap().is_none());
    assert_eq!(secrets.len(), 0);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);
}
