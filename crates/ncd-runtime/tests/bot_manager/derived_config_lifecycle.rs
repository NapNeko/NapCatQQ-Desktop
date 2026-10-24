use super::*;
use ncd_domain::bot_config::{NetworkBaseFields, WebsocketClientConfig};

fn component_manager(root: &std::path::Path) -> ManagerFixture {
    let store = Arc::new(LocalConfigStore::new(root));
    let secrets: Arc<dyn SecretStore + Send + Sync> = Arc::new(
        SecretStoreImpl::new_with_force_fallback(root.join("secrets"), true),
    );
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let paths = ncd_runtime::DataPaths::new(root);
    let renderer = Arc::new(DispatchRenderer::new(
        paths.napcat_config_dir(),
        paths.snowluma_config_dir(),
    ));
    let backend = Arc::new(FakeBackend::default());
    let manager = BotManager::new(
        Arc::clone(&repo),
        Arc::clone(&store),
        renderer,
        backend.clone(),
        Arc::new(TestLaunchPlanner),
        Arc::new(BroadcastEventBus::default()),
        default_webui_client(),
        default_offline_notifier(),
        default_poller_settings(),
        default_desktop_notify(),
    );
    (store, repo, backend, manager)
}

fn snowluma_config(qq_id: u64, url: &str) -> BotConfig {
    let mut config = bot_config(qq_id, "snowluma");
    config.bot.backend_type = BackendType::SnowLuma;
    config
        .connect
        .websocket_clients
        .push(WebsocketClientConfig {
            base: NetworkBaseFields {
                enable: true,
                name: "app".into(),
                message_post_format: Default::default(),
                token: String::new(),
                debug: false,
            },
            url: url.into(),
            report_self_message: false,
            heart_interval: 30_000,
            reconnect_interval: 5_000,
            role: Default::default(),
        });
    config
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[tokio::test]
async fn component_config_survives_creation_and_save() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = component_manager(temp.path());
    let paths = ncd_runtime::DataPaths::new(temp.path());
    let path = paths.snowluma_config_dir().join("onebot_10001.json");
    let mut config = snowluma_config(10001, "ws://127.0.0.1:8081/old");

    manager.upsert_bot_config(config.clone()).await.unwrap();
    assert_eq!(
        read_json(&path)["networks"]["wsClients"][0]["url"],
        config.connect.websocket_clients[0].url
    );

    for (field, value) in [
        ("mode", serde_json::Value::Null),
        ("mode", serde_json::json!("overlay")),
        ("mode.inherited", serde_json::json!(true)),
    ] {
        let overrides = std::collections::HashMap::from([(
            "onebot_10001.json".to_owned(),
            vec![(field.to_owned(), value)],
        )]);
        manager
            .upsert_bot_config_with_overrides(config.clone(), &overrides)
            .await
            .unwrap();
        assert_eq!(read_json(&path)["mode"], "snapshot");
    }

    config.connect.websocket_clients[0].url = "ws://127.0.0.1:8082/new".into();
    manager.upsert_bot_config(config.clone()).await.unwrap();
    assert_eq!(
        read_json(&path)["networks"]["wsClients"][0]["url"],
        config.connect.websocket_clients[0].url
    );
}

#[tokio::test]
async fn switching_backends_removes_only_previous_bot_files() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, _, manager) = component_manager(temp.path());
    let paths = ncd_runtime::DataPaths::new(temp.path());
    let napcat = paths.napcat_config_dir();
    let onebot11 = napcat.join("onebot11_10001.json");
    let napcat_bot = napcat.join("napcat_10001.json");
    let shared = napcat.join("napcat.json");
    let snowluma = paths.snowluma_config_dir().join("onebot_10001.json");
    let mut config = bot_config(10001, "switch");

    manager.upsert_bot_config(config.clone()).await.unwrap();
    assert!(onebot11.exists());
    assert!(napcat_bot.exists());
    let shared_payload = std::fs::read(&shared).unwrap();

    config.bot.backend_type = BackendType::SnowLuma;
    manager.upsert_bot_config(config.clone()).await.unwrap();
    assert!(snowluma.exists());
    assert!(!onebot11.exists());
    assert!(!napcat_bot.exists());
    assert_eq!(std::fs::read(&shared).unwrap(), shared_payload);

    config.bot.backend_type = BackendType::NapCat;
    manager.upsert_bot_config(config).await.unwrap();
    assert!(onebot11.exists());
    assert!(napcat_bot.exists());
    assert!(!snowluma.exists());
    assert_eq!(std::fs::read(&shared).unwrap(), shared_payload);
}

#[tokio::test]
async fn deleting_then_recreating_same_qq_cleans_current_and_legacy_config() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (store, _, _, manager) = component_manager(temp.path());
    let paths = ncd_runtime::DataPaths::new(temp.path());
    let actual = paths.snowluma_config_dir().join("onebot_10001.json");
    let other = paths.snowluma_config_dir().join("onebot_10002.json");
    let shared = paths.napcat_config_dir().join("napcat.json");
    let legacy_shared = store.config_dir().join("napcat.json");
    let legacy_paths = [
        "onebot_10001.json",
        "onebot11_10001.json",
        "napcat_10001.json",
    ]
    .map(|file| store.config_dir().join(file));
    manager
        .upsert_bot_config(snowluma_config(10001, "ws://127.0.0.1:8081/old"))
        .await
        .unwrap();
    manager
        .upsert_bot_config(snowluma_config(10002, "ws://127.0.0.1:8082/other"))
        .await
        .unwrap();
    let other_payload = std::fs::read(&other).unwrap();
    let shared_payload = serde_json::json!({"shared": true});
    let mut previous = read_json(&actual);
    previous["legacyAccountMarker"] = serde_json::json!("old-account");
    let mut txn = JsonTransaction::new()
        .write(actual.clone(), previous)
        .write(shared.clone(), shared_payload.clone())
        .write(legacy_shared.clone(), shared_payload.clone());
    for path in &legacy_paths {
        txn = txn.write(path.clone(), serde_json::json!({"legacy": true}));
    }
    store.apply_transaction(txn).unwrap();

    manager
        .delete_bot_config(&BotId::new("10001"))
        .await
        .unwrap();
    assert!(!actual.exists());
    assert!(legacy_paths.iter().all(|path| !path.exists()));
    assert_eq!(std::fs::read(&other).unwrap(), other_payload);
    assert_eq!(read_json(&shared), shared_payload);
    assert_eq!(read_json(&legacy_shared), shared_payload);

    manager
        .upsert_bot_config(snowluma_config(10001, "ws://127.0.0.1:8083/new"))
        .await
        .unwrap();
    let recreated = read_json(&actual);
    let clients = recreated["networks"]["wsClients"].as_array().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0]["url"], "ws://127.0.0.1:8083/new");
    assert!(recreated.get("legacyAccountMarker").is_none());
    assert_eq!(std::fs::read(&other).unwrap(), other_payload);
}

#[tokio::test]
async fn legacy_invalid_url_is_rejected_before_runtime_start() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (store, repo, backend, manager) = component_manager(temp.path());
    let bot = BotId::new("10001");
    let mut config = snowluma_config(10001, "ws://127.0.0.1:8081/app");
    config.bot.backend_type = BackendType::NapCat;
    manager.upsert_bot_config(config).await.unwrap();
    let path = ncd_runtime::DataPaths::new(temp.path())
        .napcat_config_dir()
        .join("onebot11_10001.json");
    let original = std::fs::read(&path).unwrap();
    let before = manager.get_snapshot(&bot).await.unwrap();
    let mut persisted = read_json(&store.bot_config_path());
    persisted["bots"][0]["connect"]["websocketClients"][0]["url"] = serde_json::json!("ws://");
    store
        .apply_transaction(JsonTransaction::new().write(store.bot_config_path(), persisted))
        .unwrap();
    repo.invalidate_cache().await;
    assert_eq!(
        repo.get(10001)
            .await
            .unwrap()
            .unwrap()
            .connect
            .websocket_clients[0]
            .url,
        "ws://"
    );

    let error = manager.start_bot(&bot).await.unwrap_err();
    assert!(matches!(error, BotManagerError::Config(_)));
    let after = manager.get_snapshot(&bot).await.unwrap();
    assert_eq!(after.state, before.state);
    assert_eq!(after.revision, before.revision);
    assert_eq!(backend.start_count(bot).await, 0);
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
