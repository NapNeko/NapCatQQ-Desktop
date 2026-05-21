use std::path::{Path, PathBuf};

use ncd_core::{
    ConfigStore, JsonTransaction, LocalConfigStore, MigrationOrchestrator, PathProbe,
    SchemaVersion, SecretError, SecretStore,
};
use serde_json::Value;
use tempfile::tempdir;

#[derive(Debug)]
struct StaticPathProbe {
    roots: Vec<PathBuf>,
}

impl PathProbe for StaticPathProbe {
    fn probe(&self) -> Result<Vec<PathBuf>, ncd_core::PathError> {
        Ok(self.roots.clone())
    }

    fn is_allowed(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
    }
}

#[derive(Debug, Default)]
struct MemorySecretStore {
    values: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl MemorySecretStore {
    fn contains_key(&self, key: &str) -> bool {
        self.values.lock().unwrap().contains_key(key)
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self.values.lock().unwrap().get(key).cloned())
    }

    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.values
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.values.lock().unwrap().remove(key);
        Ok(())
    }
}

#[test]
fn transaction_failure_restores_previous_files() {
    let temp = tempdir().unwrap();
    let store = LocalConfigStore::new(temp.path());
    let config_dir = store.config_dir();
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.json"),
        serde_json::to_vec(&serde_json::json!({"a": 1})).unwrap(),
    )
    .unwrap();
    std::fs::write(config_dir.join("runtime-summary"), b"block").unwrap();

    let tx = JsonTransaction::new()
        .write(config_dir.join("config.json"), serde_json::json!({"a": 2}))
        .write(
            config_dir.join("runtime-summary/10001.json"),
            serde_json::json!({"bot_id": "10001"}),
        );

    assert!(store.apply_transaction(tx).is_err());
    let payload = store.read_json(&config_dir.join("config.json")).unwrap();
    assert_eq!(payload["a"], 1);
    assert!(config_dir.join("runtime-summary").is_file());
}

#[test]
fn secret_store_failure_keeps_legacy_field() {
    #[derive(Debug)]
    struct FailingSecretStore;

    impl SecretStore for FailingSecretStore {
        fn get(&self, _: &str) -> Result<Option<String>, SecretError> {
            Ok(None)
        }

        fn put(&self, _: &str, _: &str) -> Result<(), SecretError> {
            Err(SecretError::Unavailable)
        }

        fn delete(&self, _: &str) -> Result<(), SecretError> {
            Ok(())
        }
    }

    let result = ncd_core::migration::migrate_payload_for_tests(
        serde_json::json!([{
            "bot": {
                "QQID": "10001",
                "snowluma_webui_password_override": "secret-password"
            },
            "connect": {},
            "advanced": {}
        }]),
        &FailingSecretStore,
    )
    .unwrap();

    assert!(result["bots"][0]["bot"]["snowluma_webui_password_override"].is_string());
}

#[test]
fn orchestrator_prefers_best_candidate_root() {
    let legacy_root = tempdir().unwrap();
    let legacy_root2 = tempdir().unwrap();
    let target_root = tempdir().unwrap();

    let best_config = legacy_root.path().join("runtime/config");
    std::fs::create_dir_all(&best_config).unwrap();
    std::fs::write(
        best_config.join("config.json"),
        serde_json::to_vec(&serde_json::json!({
            "Info": {"main_window": true},
            "Personalized": {"CloseBtnAction": "close"}
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        best_config.join("bot.json"),
        serde_json::to_vec(&serde_json::json!([
            {
                "bot": {"QQID": "10001", "name": "Preferred"},
                "connect": {},
                "advanced": {}
            }
        ]))
        .unwrap(),
    )
    .unwrap();

    let weaker_config = legacy_root2.path().join("runtime/config");
    std::fs::create_dir_all(&weaker_config).unwrap();
    std::fs::write(
        weaker_config.join("bot.json"),
        serde_json::to_vec(&serde_json::json!([
            {
                "bot": {"QQID": "10002"},
                "connect": {},
                "advanced": {}
            }
        ]))
        .unwrap(),
    )
    .unwrap();

    let store = LocalConfigStore::new(target_root.path());
    let probe = StaticPathProbe {
        roots: vec![
            legacy_root2.path().to_path_buf(),
            legacy_root.path().to_path_buf(),
        ],
    };
    let secrets = MemorySecretStore::default();
    let orchestrator = MigrationOrchestrator::new(&store, &probe, &secrets);

    let snapshot = orchestrator.bootstrap();
    assert_eq!(snapshot.schema_version, SchemaVersion::CURRENT);
    assert_eq!(snapshot.report.outcome, ncd_core::MigrationOutcome::Updated);
    assert_eq!(
        snapshot.report.source.as_ref().unwrap().root,
        legacy_root.path()
    );

    let app_payload = store
        .read_json(&store.config_dir().join("config.json"))
        .unwrap();
    assert_eq!(app_payload["Info"]["ConfigVersion"], "v2.0");
}

#[test]
fn orchestrator_migrates_legacy_tree_and_is_idempotent() {
    let legacy_root = tempdir().unwrap();
    let target_root = tempdir().unwrap();
    let legacy_config = legacy_root.path().join("runtime/config");
    std::fs::create_dir_all(&legacy_config).unwrap();
    std::fs::write(
        legacy_config.join("config.json"),
        serde_json::to_vec(&serde_json::json!({
            "Info": {"main_window": true},
            "Personalized": {"CloseBtnAction": "close"},
            "Personalize": {"BgHomePage": "legacy", "ThemeMode": "Dark"},
            "Remote": {"Password": "do-not-persist"}
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        legacy_config.join("bot.json"),
        serde_json::to_vec(&serde_json::json!([
            {
                "bot": {
                    "QQID": "10001",
                    "name": "Local Bot",
                    "snowluma_webui_password_override": "secret-password"
                },
                "connect": {
                    "http": {"enable": true, "host": "127.0.0.1", "port": 3000},
                    "reverseWs": {"enable": true, "urls": ["ws://localhost:8080"]}
                },
                "advanced": {}
            }
        ]))
        .unwrap(),
    )
    .unwrap();

    let store = LocalConfigStore::new(target_root.path());
    let probe = StaticPathProbe {
        roots: vec![legacy_root.path().to_path_buf()],
    };
    let secrets = MemorySecretStore::default();
    let orchestrator = MigrationOrchestrator::new(&store, &probe, &secrets);

    let first = orchestrator.bootstrap();
    assert_eq!(first.schema_version, SchemaVersion::CURRENT);
    assert_eq!(first.report.outcome, ncd_core::MigrationOutcome::Updated);
    assert!(secrets.contains_key("bot:10001:snowluma_webui_password_override"));

    let app_payload = store
        .read_json(&store.config_dir().join("config.json"))
        .unwrap();
    assert_eq!(app_payload["Info"]["ConfigVersion"], "v2.0");
    assert!(app_payload["Remote"].get("Password").is_none());

    let bot_payload = store
        .read_json(&store.config_dir().join("bot.json"))
        .unwrap();
    assert_eq!(bot_payload["info"]["configVersion"], "v2.1");
    assert!(
        bot_payload["bots"][0]["bot"]
            .get("snowluma_webui_password_override")
            .is_none()
    );
    assert_eq!(
        bot_payload["bots"][0]["connect"]["httpServers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        bot_payload["bots"][0]["connect"]["websocketClients"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let summary_path = store.config_dir().join("runtime-summary/10001.json");
    let summary: Value = store.read_json(&summary_path).unwrap();
    assert_eq!(summary["bot_id"], "10001");

    let second = orchestrator.bootstrap();
    assert_eq!(second.report.outcome, ncd_core::MigrationOutcome::NoChange);
}
