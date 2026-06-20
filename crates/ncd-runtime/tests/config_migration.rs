#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::path::{Path, PathBuf};

use ncd_runtime::{
    ConfigStore, JsonTransaction, LocalConfigStore, MigrationOrchestrator, PathProbe, SchemaVersion,
};
use ncd_test_support::{MockSecretStore, TempWorkspace};
use serde_json::Value;

#[derive(Debug)]
struct StaticPathProbe {
    roots: Vec<PathBuf>,
}

impl PathProbe for StaticPathProbe {
    fn probe(&self) -> Result<Vec<PathBuf>, ncd_runtime::PathError> {
        Ok(self.roots.clone())
    }

    fn is_allowed(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| path.starts_with(root))
    }
}

#[test]
fn transaction_failure_restores_previous_files() {
    let temp = TempWorkspace::new().unwrap();
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
    let secrets = MockSecretStore::new();
    // 注入一次 put 失败,模拟存储不可用
    secrets.fail_next_put("simulated unavailable");

    let result = ncd_runtime::migration::migrate_payload_for_tests(
        serde_json::json!([{
            "bot": {
                "QQID": "10001",
                "snowluma_webui_password_override": "secret-password"
            },
            "connect": {},
            "advanced": {}
        }]),
        &secrets,
    )
    .unwrap();

    assert!(result["bots"][0]["bot"]["snowluma_webui_password_override"].is_string());
}

#[test]
fn orchestrator_prefers_best_candidate_root() {
    let legacy_root = TempWorkspace::new().unwrap();
    let legacy_root2 = TempWorkspace::new().unwrap();
    let target_root = TempWorkspace::new().unwrap();

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
    let secrets = MockSecretStore::new();
    let orchestrator = MigrationOrchestrator::new(&store, &probe, &secrets);

    let snapshot = orchestrator.bootstrap();
    assert_eq!(snapshot.schema_version, SchemaVersion::CURRENT);
    assert_eq!(
        snapshot.report.outcome,
        ncd_runtime::MigrationOutcome::Updated
    );
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
    let legacy_root = TempWorkspace::new().unwrap();
    let target_root = TempWorkspace::new().unwrap();
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
    let secrets = MockSecretStore::new();
    let orchestrator = MigrationOrchestrator::new(&store, &probe, &secrets);

    let first = orchestrator.bootstrap();
    assert_eq!(first.schema_version, SchemaVersion::CURRENT);
    assert_eq!(first.report.outcome, ncd_runtime::MigrationOutcome::Updated);
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
    assert!(bot_payload["bots"][0]["bot"]
        .get("snowluma_webui_password_override")
        .is_none());
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
    assert_eq!(
        second.report.outcome,
        ncd_runtime::MigrationOutcome::NoChange
    );
}

fn write_legacy_bot_config(root: &Path, qq: &str, name: &str) {
    let config_dir = root.join("runtime/config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("bot.json"),
        serde_json::to_vec(&serde_json::json!([
            {
                "bot": {"QQID": qq, "name": name},
                "connect": {},
                "advanced": {}
            }
        ]))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn failed_migration_report_does_not_mark_current_and_skip_next_run() {
    let legacy_root = TempWorkspace::new().unwrap();
    let target_root = TempWorkspace::new().unwrap();
    write_legacy_bot_config(legacy_root.path(), "10003", "Recoverable");

    let blocked_root = target_root.path().join("blocked-root");
    std::fs::write(&blocked_root, b"not a directory").unwrap();

    let blocked_store = LocalConfigStore::new(&blocked_root);
    let probe = StaticPathProbe {
        roots: vec![legacy_root.path().to_path_buf()],
    };
    let secrets = MockSecretStore::new();
    let first = MigrationOrchestrator::new(&blocked_store, &probe, &secrets).bootstrap();

    assert_eq!(first.report.stage, ncd_runtime::MigrationStage::Failed);
    assert!(!blocked_store.migration_report_path().exists());

    std::fs::remove_file(&blocked_root).unwrap();
    let recovered_store = LocalConfigStore::new(&blocked_root);
    let second = MigrationOrchestrator::new(&recovered_store, &probe, &secrets).bootstrap();

    assert_eq!(
        second.report.outcome,
        ncd_runtime::MigrationOutcome::Updated
    );
    assert_eq!(second.schema_version, SchemaVersion::CURRENT);
    let bot_payload = recovered_store
        .read_json(&recovered_store.config_dir().join("bot.json"))
        .unwrap();
    assert_eq!(bot_payload["bots"][0]["bot"]["name"], "Recoverable");
}

#[test]
fn primary_empty_target_migrates_from_legacy_source() {
    let workspace = TempWorkspace::new().unwrap();
    let program_data = workspace.path().join("ProgramData");
    let primary = program_data.join("NapCatQQ Desktop");
    let legacy = program_data.join("NapCatQQ-Desktop");
    std::fs::create_dir_all(&primary).unwrap();
    write_legacy_bot_config(&legacy, "10004", "Legacy Source");

    let store = LocalConfigStore::new(&primary);
    let probe = StaticPathProbe {
        roots: vec![legacy.clone()],
    };
    let secrets = MockSecretStore::new();
    let snapshot = MigrationOrchestrator::new(&store, &probe, &secrets).bootstrap();

    assert_eq!(
        snapshot.report.outcome,
        ncd_runtime::MigrationOutcome::Updated
    );
    assert_eq!(snapshot.report.source.as_ref().unwrap().root, legacy);
    let bot_payload = store
        .read_json(&store.config_dir().join("bot.json"))
        .unwrap();
    assert_eq!(bot_payload["bots"][0]["bot"]["name"], "Legacy Source");
}

#[test]
fn target_write_failure_reports_failed_without_localappdata_fork() {
    let legacy_root = TempWorkspace::new().unwrap();
    let target_root = TempWorkspace::new().unwrap();
    write_legacy_bot_config(legacy_root.path(), "10005", "Write Denied");

    let blocked_root = target_root.path().join("ProgramDataTarget");
    std::fs::write(&blocked_root, b"not a directory").unwrap();
    let store = LocalConfigStore::new(&blocked_root);
    let probe = StaticPathProbe {
        roots: vec![legacy_root.path().to_path_buf()],
    };
    let secrets = MockSecretStore::new();
    let snapshot = MigrationOrchestrator::new(&store, &probe, &secrets).bootstrap();

    assert_eq!(snapshot.report.stage, ncd_runtime::MigrationStage::Failed);
    assert_eq!(
        snapshot.report.outcome,
        ncd_runtime::MigrationOutcome::NeedsRepair
    );
    assert!(!target_root.path().join("LocalAppData").exists());
}

#[test]
fn bootstrap_persists_failure_report_to_writable_store() {
    // run() 在读取坏掉的 bot.json 时报错,但 target store 可写:bootstrap 的 Err
    // 分支应尽力把失败报告落盘,保留首次失败证据,重启后仍可读。
    let legacy_root = TempWorkspace::new().unwrap();
    let target_root = TempWorkspace::new().unwrap();

    let legacy_config = legacy_root.path().join("runtime/config");
    std::fs::create_dir_all(&legacy_config).unwrap();
    std::fs::write(
        legacy_config.join("config.json"),
        serde_json::to_vec(&serde_json::json!({"Info": {"main_window": true}})).unwrap(),
    )
    .unwrap();
    // 坏 JSON:迁移在 read_source_json 解析阶段失败。
    std::fs::write(legacy_config.join("bot.json"), b"{ not valid json :::").unwrap();

    let store = LocalConfigStore::new(target_root.path());
    let probe = StaticPathProbe {
        roots: vec![legacy_root.path().to_path_buf()],
    };
    let secrets = MockSecretStore::new();
    let snapshot = MigrationOrchestrator::new(&store, &probe, &secrets).bootstrap();

    assert_eq!(snapshot.report.stage, ncd_runtime::MigrationStage::Failed);

    let report_path = store.migration_report_path();
    assert!(report_path.exists(), "失败迁移报告应落盘保留首次失败证据");
    let saved = std::fs::read_to_string(&report_path).unwrap();
    assert!(saved.contains("migration_failed"));
}
