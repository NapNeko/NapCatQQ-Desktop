use super::*;
use std::collections::BTreeMap;
use std::sync::Mutex;

const VALID_CONFIG: &str = r#"{"Info":{"ConfigVersion":"v2.0"}}"#;

#[derive(Default)]
struct RecordingSecrets(Mutex<BTreeMap<String, String>>);

impl ncd_traits::SecretStore for RecordingSecrets {
    fn get(&self, key: &str) -> Result<Option<String>, ncd_domain::errors::SecretError> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }

    fn put(&self, key: &str, value: &str) -> Result<(), ncd_domain::errors::SecretError> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), ncd_domain::errors::SecretError> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

fn write_json(dir: &Path, relative: &str, value: serde_json::Value) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

fn make_zip(path: &Path, entries: &[(&str, &str)]) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    for (name, contents) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn new_chat_preferences_alone_are_recognized() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_json(
        staging.path(),
        "chat-desktop.json",
        serde_json::json!({"accounts": []}),
    );
    let prepared =
        build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default());
    assert!(
        prepared.is_ok(),
        "new configuration must be recognized: {prepared:?}"
    );
}

#[test]
fn zip_extraction_preserves_registered_nested_paths() {
    let temp = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let zip_path = temp.path().join("nested.zip");
    make_zip(
        &zip_path,
        &[("onebot-debug/workspace.json", r#"{"version":1,"tabs":[]}"#)],
    );
    extract_zip_to_dir(&zip_path, staging.path()).unwrap();
    assert!(staging.path().join("onebot-debug/workspace.json").is_file());
}

#[test]
fn unrelated_nested_basenames_do_not_become_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let zip_path = temp.path().join("unrelated.zip");
    make_zip(
        &zip_path,
        &[("components/some-app/config.json", VALID_CONFIG)],
    );
    extract_zip_to_dir(&zip_path, staging.path()).unwrap();
    assert!(!staging.path().join("config.json").exists());
}

#[test]
fn zip_extraction_rejects_path_traversal() {
    let temp = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let zip_path = temp.path().join("unsafe.zip");
    make_zip(&zip_path, &[("../config.json", VALID_CONFIG)]);
    assert!(extract_zip_to_dir(&zip_path, staging.path()).is_err());
}

#[test]
fn flat_and_config_layout_aliases_cannot_overwrite_same_target() {
    let temp = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let zip_path = temp.path().join("duplicate.zip");
    make_zip(
        &zip_path,
        &[
            ("config.json", VALID_CONFIG),
            ("config/config.json", VALID_CONFIG),
        ],
    );
    assert!(extract_zip_to_dir(&zip_path, staging.path()).is_err());
}

#[test]
fn future_archive_version_is_rejected() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_json(
        staging.path(),
        "config.json",
        serde_json::from_str(VALID_CONFIG).unwrap(),
    );
    write_json(
        staging.path(),
        "export_meta.json",
        serde_json::json!({"exportFormatVersion":"v99"}),
    );
    assert!(
        build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
            .is_err()
    );
}

#[test]
fn explicit_settings_win_over_legacy_notification_seed() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_json(
        staging.path(),
        "config.json",
        serde_json::json!({"Info":{"ConfigVersion":"v2.0"},"WebHook":{"WebHookUrl":"https://legacy.example"}}),
    );
    let explicit = serde_json::to_value(ncd_domain::AppSettings::default()).unwrap();
    write_json(staging.path(), "app-settings.json", explicit);
    let (txn, _, _) =
        build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
            .unwrap();
    assert_eq!(
        txn.writes
            .iter()
            .filter(|w| w.path == target.path().join("config/app-settings.json"))
            .count(),
        1
    );
}

#[test]
fn invalid_import_has_no_real_secret_store_side_effects() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let secrets = RecordingSecrets::default();
    write_json(
        staging.path(),
        "bot.json",
        serde_json::json!({"bots":[{"bot":{"QQID":"10001","name":"X","snowluma_webui_password_override":"legacy-password"},"connect":{},"advanced":{}}]}),
    );
    write_json(
        staging.path(),
        "servers.json",
        serde_json::json!({"invalid":true}),
    );
    assert!(build_import_transaction(staging.path(), target.path(), &secrets).is_err());
    assert!(
        secrets.0.lock().unwrap().is_empty(),
        "all files must validate before real secret migration"
    );
}

#[tokio::test]
async fn invalid_configuration_preview_disables_import() {
    let staging = tempfile::tempdir().unwrap();
    write_json(staging.path(), "config.json", serde_json::json!([1, 2, 3]));
    let preview = preview_config_import(staging.path().to_string_lossy().into())
        .await
        .unwrap();
    assert!(
        !preview.can_import,
        "invalid configuration must not be importable: {preview:?}"
    );
}

#[test]
fn recognized_new_files_cannot_use_empty_defaulted_objects() {
    for name in [
        "app-instances.json",
        "chat-desktop.json",
        "onebot-debug/workspace.json",
        "onebot-debug/collections.json",
        "state/snowluma/app-config.json",
        "frontend-preferences.json",
    ] {
        let staging = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_json(
            staging.path(),
            "config.json",
            serde_json::from_str(VALID_CONFIG).unwrap(),
        );
        write_json(staging.path(), name, serde_json::json!({}));
        assert!(
            build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
                .is_err(),
            "{name} must reject an empty object"
        );
    }
}

#[test]
fn new_versioned_files_reject_future_schemas() {
    for (name, value) in [
        (
            "app-instances.json",
            serde_json::json!({"version":2,"instances":[]}),
        ),
        (
            "onebot-debug/workspace.json",
            serde_json::json!({"version":3,"tabs":[]}),
        ),
        (
            "onebot-debug/collections.json",
            serde_json::json!({"version":2,"folders":[],"requests":[]}),
        ),
        (
            "frontend-preferences.json",
            serde_json::json!({"version":2,"storage":{}}),
        ),
    ] {
        let staging = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_json(
            staging.path(),
            "config.json",
            serde_json::from_str(VALID_CONFIG).unwrap(),
        );
        write_json(staging.path(), name, value);
        assert!(
            build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
                .is_err(),
            "{name} must reject a future schema"
        );
    }
}

#[test]
fn frontend_preferences_reject_unknown_keys_and_wrong_json_shapes() {
    for storage in [
        serde_json::json!({"ssh_private_key":"private"}),
        serde_json::json!({"ncd.terminal.prefs.v1":"[]"}),
        serde_json::json!({"ncd.terminal.layout.v1":"null"}),
        serde_json::json!({"ncd.chat.ui.v1":"42"}),
        serde_json::json!({"ncd:bot_custom_order:v1":"[1]"}),
        serde_json::json!({"ncd.maibot.chat.name../outside":"name"}),
    ] {
        let staging = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_json(
            staging.path(),
            "config.json",
            serde_json::from_str(VALID_CONFIG).unwrap(),
        );
        write_json(
            staging.path(),
            "frontend-preferences.json",
            serde_json::json!({"version":1,"storage":storage}),
        );
        assert!(
            build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
                .is_err(),
            "invalid browser preferences must reject import"
        );
    }
}

#[test]
fn chat_background_account_limit_is_enforced() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_json(
        staging.path(),
        "config.json",
        serde_json::from_str(VALID_CONFIG).unwrap(),
    );
    let accounts: Vec<_> = (1..=9).map(|id| serde_json::json!({"botId":format!("bot-{id}"),"selfId":format!("{id}"),"enabled":true,"background":true,"tray":false})).collect();
    write_json(
        staging.path(),
        "chat-desktop.json",
        serde_json::json!({"accounts":accounts}),
    );
    assert!(
        build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
            .is_err()
    );
}

#[test]
fn adopt_snapshot_instance_id_must_match_filename() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_json(
        staging.path(),
        "config.json",
        serde_json::from_str(VALID_CONFIG).unwrap(),
    );
    write_json(
        staging.path(),
        "config/app-adopts/instance-a.json",
        serde_json::json!({"version":1,"instance_id":"instance-b","host_id":"local","install_dir":"/apps/bot","captured_at_ms":1,"files":[],"supervisors":[]}),
    );
    assert!(
        build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
            .is_err()
    );
}

#[test]
fn directory_aliases_cannot_duplicate_one_target() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let value: serde_json::Value = serde_json::from_str(VALID_CONFIG).unwrap();
    write_json(staging.path(), "config.json", value.clone());
    write_json(staging.path(), "config/config.json", value);
    assert!(
        build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
            .is_err()
    );
}

fn sample_instance(id: &str) -> serde_json::Value {
    serde_json::json!({"id":id,"framework_id":"karin","display_name":"Karin","placement":"local_native","host_id":"local","install_dir":"/apps/karin","port":7777,"state":"installed","created_at_ms":1})
}

fn framework_backup_fixture(id: &str) -> FrameworkConfigBackup {
    FrameworkConfigBackup {
        version: 1,
        instance_id: id.into(),
        framework_id: "karin".into(),
        files: vec![ncd_runtime::app_framework::FrameworkConfigFile {
            relative_path: ".env".into(),
            text: "# preserve comments\r\nHTTP_PORT=7777\r\nCUSTOM=keep\r\n".into(),
        }],
    }
}

fn write_framework_fixture(dir: &Path) {
    write_json(
        dir,
        "app-instances.json",
        serde_json::json!({"version":1,"instances":[sample_instance("instance-a")]}),
    );
    write_json(
        dir,
        "config/framework-configs/instance-a.json",
        serde_json::to_value(framework_backup_fixture("instance-a")).unwrap(),
    );
}

#[test]
fn framework_config_zip_round_trip_keeps_raw_text_and_instance_binding() {
    let data = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_json(
        data.path(),
        "config/app-instances.json",
        serde_json::json!({"version":1,"instances":[sample_instance("instance-a")]}),
    );
    let zip = data.path().join("frameworks.zip");
    let result = export_config_with_frameworks(
        data.path(),
        &zip,
        None,
        vec![framework_backup_fixture("instance-a")],
        None,
        None,
    )
    .unwrap();
    assert!(result.files.iter().any(|f| f.starts_with("框架配置")));
    extract_zip_to_dir(&zip, staging.path()).unwrap();
    assert!(
        staging
            .path()
            .join("config/framework-configs/instance-a.json")
            .is_file()
    );
    let prepared =
        prepare_import_transaction(staging.path(), target.path(), &MemorySecretStore::default())
            .unwrap();
    assert_eq!(
        prepared.framework_backups,
        [framework_backup_fixture("instance-a")]
    );
}

#[tokio::test]
async fn preview_rejects_invalid_framework_syntax_before_any_production_write() {
    let source = tempfile::tempdir().unwrap();
    write_framework_fixture(source.path());
    let mut backup = framework_backup_fixture("instance-a");
    backup.files[0].relative_path = "@karinjs/config/adapter.json".into();
    backup.files[0].text = "{".into();
    write_json(
        source.path(),
        "config/framework-configs/instance-a.json",
        serde_json::to_value(backup).unwrap(),
    );
    let preview = preview_config_import(source.path().to_string_lossy().into())
        .await
        .unwrap();
    assert!(!preview.can_import);
    assert!(preview.warnings.iter().any(|w| w.contains("JSON")));
}

#[test]
fn framework_backups_require_matching_instance_metadata() {
    for case in ["missing", "wrong-id", "wrong-framework", "wrong-filename"] {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_framework_fixture(source.path());
        let mut inst = sample_instance("instance-a");
        match case {
            "missing" => fs::remove_file(source.path().join("app-instances.json")).unwrap(),
            "wrong-id" => {
                inst["id"] = "other".into();
                write_json(
                    source.path(),
                    "app-instances.json",
                    serde_json::json!({"version":1,"instances":[inst]}),
                );
            }
            "wrong-framework" => {
                inst["framework_id"] = "koishi".into();
                write_json(
                    source.path(),
                    "app-instances.json",
                    serde_json::json!({"version":1,"instances":[inst]}),
                );
            }
            _ => {
                write_json(
                    source.path(),
                    "config/framework-configs/instance-a.json",
                    serde_json::to_value(framework_backup_fixture("other")).unwrap(),
                );
            }
        }
        assert!(
            prepare_import_transaction(source.path(), target.path(), &MemorySecretStore::default())
                .is_err(),
            "{case}"
        );
        assert!(!target.path().join("config").exists());
    }
}

#[test]
fn framework_backup_paths_cannot_restore_programs_history_or_traversal() {
    for path in [
        "bot.py",
        "data/history.json",
        "node_modules/core/config.json",
        "../.env",
        "@karinjs/config/../../secret.json",
    ] {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_framework_fixture(source.path());
        let mut backup = framework_backup_fixture("instance-a");
        backup.files[0].relative_path = path.into();
        write_json(
            source.path(),
            "config/framework-configs/instance-a.json",
            serde_json::to_value(backup).unwrap(),
        );
        assert!(
            prepare_import_transaction(source.path(), target.path(), &MemorySecretStore::default())
                .is_err(),
            "{path}"
        );
    }
}

#[test]
fn duplicate_framework_snapshot_aliases_are_rejected() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_framework_fixture(source.path());
    write_json(
        source.path(),
        "framework-configs/instance-a.json",
        serde_json::to_value(framework_backup_fixture("instance-a")).unwrap(),
    );
    assert!(
        prepare_import_transaction(source.path(), target.path(), &MemorySecretStore::default())
            .unwrap_err()
            .contains("重复")
    );
}

#[test]
fn remote_framework_backups_require_the_corresponding_server_profile() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_framework_fixture(source.path());
    let mut inst = sample_instance("instance-a");
    inst["host_id"] = "remote:source".into();
    inst["placement"] = "remote_native".into();
    write_json(
        source.path(),
        "app-instances.json",
        serde_json::json!({"version":1,"instances":[inst]}),
    );
    assert!(
        prepare_import_transaction(source.path(), target.path(), &MemorySecretStore::default())
            .unwrap_err()
            .contains("servers.json")
    );
    write_json(source.path(), "servers.json", serde_json::json!([]));
    assert!(
        prepare_import_transaction(source.path(), target.path(), &MemorySecretStore::default())
            .is_err()
    );
    write_json(
        source.path(),
        "servers.json",
        serde_json::json!([{"id":"source","name":"Remote","host":"203.0.113.5","port":22,"username":"bot","authMethod":"key"}]),
    );
    assert!(
        prepare_import_transaction(source.path(), target.path(), &MemorySecretStore::default())
            .is_ok()
    );
}

#[test]
fn pending_framework_recovery_is_persisted_reexported_and_deleted_only_after_success() {
    use ncd_traits::ConfigStore;
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    write_framework_fixture(source.path());
    let mut prepared =
        prepare_import_transaction(source.path(), target.path(), &MemorySecretStore::default())
            .unwrap();
    stage_framework_recoveries(
        &mut prepared.txn,
        target.path(),
        &prepared.framework_backups,
        &[],
    )
    .unwrap();
    let store = ncd_runtime::LocalConfigStore::new(target.path());
    store.apply_transaction(prepared.txn).unwrap();
    let path = target
        .path()
        .join("config/framework-configs/instance-a.json");
    let recovery: FrameworkConfigRecovery =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(recovery.pending);
    assert_eq!(recovery.backup, framework_backup_fixture("instance-a"));
    let zip = target.path().join("pending.zip");
    export_config_to_path(target.path(), &zip, None).unwrap();
    extract_zip_to_dir(&zip, staging.path()).unwrap();
    let snapshot: FrameworkConfigBackup = serde_json::from_slice(
        &fs::read(
            staging
                .path()
                .join("config/framework-configs/instance-a.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot, framework_backup_fixture("instance-a"));
    let mut txn = ncd_traits::JsonTransaction::new();
    stage_framework_recoveries(&mut txn, target.path(), &[snapshot], &["instance-a".into()])
        .unwrap();
    assert!(txn.writes.is_empty());
    assert_eq!(txn.deletes, [path.clone()]);
    store.apply_transaction(txn).unwrap();
    assert!(!path.exists());
}

#[test]
fn active_instance_paths_are_preserved_and_imported_operational_states_are_not_replayed() {
    let mut old: ncd_domain::AppInstance =
        serde_json::from_value(sample_instance("instance-a")).unwrap();
    old.state = ncd_domain::AppInstanceState::Running;
    let mut imported = vec![old.clone()];
    normalize_imported_instance_states(&mut imported, &[]).unwrap();
    assert_eq!(imported[0].state, ncd_domain::AppInstanceState::Stopped);
    normalize_imported_instance_states(&mut imported, &[old.clone()]).unwrap();
    assert_eq!(imported[0].state, ncd_domain::AppInstanceState::Running);
    imported[0].install_dir = "/different".into();
    assert!(normalize_imported_instance_states(&mut imported, &[old.clone()]).is_err());
    assert!(normalize_imported_instance_states(&mut [], &[old]).is_err());
}

fn new_configuration_fixture(dir: &Path) {
    write_json(
        dir,
        "app-instances.json",
        serde_json::json!({"version":1,"instances":[sample_instance("instance-a")]}),
    );
    write_json(
        dir,
        "chat-desktop.json",
        serde_json::json!({"accounts":[{"botId":"10001","selfId":"10001","enabled":true,"background":true,"tray":true,"ignoredGroups":["123"],"hiddenGroups":["456"]}]}),
    );
    write_json(
        dir,
        "onebot-debug/workspace.json",
        serde_json::json!({"version":1,"tabs":[{"id":"draft-a","action":"send_msg","params_text":"{unfinished","timeout_ms":3000,"channel":null}],"active_tab":"draft-a"}),
    );
    write_json(
        dir,
        "onebot-debug/collections.json",
        serde_json::json!({"version":1,"folders":[{"id":"folder-a","name":"Saved","order":0}],"requests":[{"id":"request-a","name":"Message","folder_id":"folder-a","action":"send_msg","params":{"message":"hi"},"channel":null,"note":null,"order":0,"created_at_ms":1,"updated_at_ms":2}]}),
    );
    write_json(
        dir,
        "state/snowluma/app-config.json",
        serde_json::json!({"snowlumaWebuiPort":6099,"snowlumaWebuiPasswordOverride":"existing-inline-password"}),
    );
    write_json(
        dir,
        "config/app-adopts/instance-a.json",
        serde_json::json!({"version":1,"instance_id":"instance-a","host_id":"local","install_dir":"/apps/karin","captured_at_ms":1,"files":[{"rel_path":".env","existed":true,"text":"PORT=7777\nTOKEN=existing-inline-token\n","skip_reason":null}],"supervisors":["karin-a"]}),
    );
    write_json(
        dir,
        "frontend-preferences.json",
        serde_json::json!({"version":1,"storage":{"ncd.terminal.prefs.v1":"{\"fontSize\":15}","ncd.terminal.layout.v1":"{}","ncd.chat.ui.v1":"{}","ncd:bot_custom_order:v1":"[\"10001\"]","ncd.maibot.chat.name.instance-a":"小麦"}}),
    );
}

#[test]
fn all_new_configuration_types_join_one_transaction() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    new_configuration_fixture(staging.path());
    let (txn, _, _) =
        build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
            .unwrap();
    let paths: std::collections::BTreeSet<_> = txn
        .writes
        .iter()
        .map(|w| {
            w.path
                .strip_prefix(target.path())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(
        paths,
        [
            "config/app-instances.json",
            "config/chat-desktop.json",
            "onebot-debug/workspace.json",
            "onebot-debug/collections.json",
            "state/snowluma/app-config.json",
            "config/app-adopts/instance-a.json",
            "config/frontend-preferences.json"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
}

#[test]
fn instance_identity_and_host_placement_are_validated() {
    let mut remote_mismatch = sample_instance("instance-a");
    remote_mismatch["host_id"] = "remote:server-a".into();
    for instances in [
        serde_json::json!([sample_instance("instance-a"), sample_instance("instance-a")]),
        serde_json::json!([remote_mismatch]),
        serde_json::json!([sample_instance("../outside")]),
    ] {
        let staging = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        write_json(
            staging.path(),
            "config.json",
            serde_json::from_str(VALID_CONFIG).unwrap(),
        );
        write_json(
            staging.path(),
            "app-instances.json",
            serde_json::json!({"version":1,"instances":instances}),
        );
        assert!(
            build_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
                .is_err()
        );
    }
}

#[test]
fn export_failure_preserves_existing_zip() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let dest = output.path().join("backup.zip");
    fs::write(&dest, b"previous-complete-backup").unwrap();
    fs::create_dir_all(root.path().join("config")).unwrap();
    fs::write(root.path().join("config/bot.json"), b"invalid-json").unwrap();
    assert!(export_config_to_path(root.path(), &dest, None).is_err());
    assert_eq!(fs::read(&dest).unwrap(), b"previous-complete-backup");
}

#[test]
fn empty_export_preserves_existing_zip() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let dest = output.path().join("backup.zip");
    fs::write(&dest, b"previous-complete-backup").unwrap();
    assert!(export_config_to_path(root.path(), &dest, None).is_err());
    assert_eq!(fs::read(&dest).unwrap(), b"previous-complete-backup");
}

#[test]
fn export_accepts_uppercase_zip_extension() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let preferences = ConfigFrontendPreferences {
        version: 1,
        storage: BTreeMap::new(),
    };
    assert!(
        export_config_to_path(
            root.path(),
            &output.path().join("backup.ZIP"),
            Some(preferences)
        )
        .is_ok()
    );
}

#[test]
fn frontend_known_fields_cannot_have_incompatible_shapes() {
    for (key, raw) in [
        ("ncd.terminal.prefs.v1", r#"{"fontSize":"huge"}"#),
        ("ncd.terminal.prefs.v1", r#"{"gpu":1}"#),
        ("ncd.terminal.prefs.v1", r#"{"cursorStyle":"unknown"}"#),
        (
            "ncd.terminal.prefs.v1",
            r#"{"snippets":[{"label":"sample","command":false}]}"#,
        ),
        ("ncd.terminal.layout.v1", r#"{"filesOpen":"yes"}"#),
        (
            "ncd.chat.ui.v1",
            r#"{"hiddenConversations":{"10001":[false]}}"#,
        ),
        ("ncd:bot_custom_order:v1", r#"[""]"#),
        ("ncd.maibot.chat.name.instance-a", "name\0suffix"),
    ] {
        let preferences = ConfigFrontendPreferences {
            version: 1,
            storage: BTreeMap::from([(key.into(), raw.into())]),
        };
        assert!(
            validation::validate_frontend(&preferences).is_err(),
            "malformed preference must fail: {key} {raw}"
        );
    }
}

#[test]
fn workspace_v2_preserves_explicit_zero_widths() {
    let staging = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    write_json(
        staging.path(),
        "onebot-debug/workspace.json",
        serde_json::json!({"version":2,"tabs":[],"layout":{"left_width":0,"right_width":0}}),
    );
    let prepared =
        prepare_import_transaction(staging.path(), target.path(), &RecordingSecrets::default())
            .unwrap();
    assert_eq!(prepared.txn.writes[0].payload["version"], 2);
    assert_eq!(prepared.txn.writes[0].payload["layout"]["left_width"], 0);
    assert_eq!(prepared.txn.writes[0].payload["layout"]["right_width"], 0);
}

#[test]
fn exports_live_preferences_instead_of_stale_recovery_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    write_json(
        root.path(),
        "config/frontend-preferences.json",
        serde_json::json!({"version":1,"storage":{"ncd.maibot.chat.name.instance-a":"stale"}}),
    );
    let preferences = ConfigFrontendPreferences {
        version: 1,
        storage: BTreeMap::from([("ncd.maibot.chat.name.instance-a".into(), "current".into())]),
    };
    let dest = output.path().join("backup.zip");
    export_config_to_path(root.path(), &dest, Some(preferences.clone())).unwrap();
    let (staging, _, _guard) = resolve_import_staging(&dest).unwrap();
    let target = tempfile::tempdir().unwrap();
    let prepared =
        prepare_import_transaction(&staging, target.path(), &RecordingSecrets::default()).unwrap();
    assert_eq!(prepared.frontend_preferences, Some(preferences));
}

#[test]
fn zip_extraction_limits_uncompressed_file_size() {
    let temp = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let zip_path = temp.path().join("too-large.zip");
    let payload = "x".repeat((source::MAX_FILE_BYTES + 1) as usize);
    make_zip(&zip_path, &[("config.json", &payload)]);
    assert!(extract_zip_to_dir(&zip_path, staging.path()).is_err());
}

#[test]
fn legacy_zip_without_metadata_and_v1_remain_importable() {
    for metadata in [
        None,
        Some(r#"{"exportFormatVersion":"v1","files":["应用配置"]}"#),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let zip_path = temp.path().join("legacy.zip");
        let mut entries = vec![("config.json", VALID_CONFIG)];
        if let Some(meta) = metadata {
            entries.push(("export_meta.json", meta));
        }
        make_zip(&zip_path, &entries);
        let (staging, _, _guard) = resolve_import_staging(&zip_path).unwrap();
        let prepared =
            prepare_import_transaction(&staging, target.path(), &RecordingSecrets::default())
                .unwrap();
        assert_eq!(prepared.txn.writes.len(), 1);
    }
}

#[test]
fn all_configuration_types_roundtrip_through_v2_zip_and_transaction() {
    use ncd_traits::ConfigStore;
    let staging = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let restored = tempfile::tempdir().unwrap();
    new_configuration_fixture(staging.path());
    write_json(
        staging.path(),
        "config.json",
        serde_json::from_str(VALID_CONFIG).unwrap(),
    );
    write_json(
        staging.path(),
        "bot.json",
        serde_json::json!({"bots":[{"bot":{"QQID":"10001","name":"X"},"connect":{"websocketServers":[{"name":"inline","enable":true,"token":"existing-inline-token","host":"127.0.0.1","port":3001}]},"advanced":{}}]}),
    );
    write_json(
        staging.path(),
        "app-settings.json",
        serde_json::to_value(ncd_domain::AppSettings::default()).unwrap(),
    );
    write_json(staging.path(), "servers.json", serde_json::json!([]));
    let prepared =
        prepare_import_transaction(staging.path(), root.path(), &RecordingSecrets::default())
            .unwrap();
    let expected = prepared.txn.writes.clone();
    ncd_runtime::LocalConfigStore::new(root.path())
        .apply_transaction(prepared.txn)
        .unwrap();
    let dest = output.path().join("complete.zip");
    export_config_to_path(root.path(), &dest, prepared.frontend_preferences).unwrap();
    let (zip_staging, _, _guard) = resolve_import_staging(&dest).unwrap();
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(zip_staging.join("export_meta.json")).unwrap()).unwrap();
    assert_eq!(meta["exportFormatVersion"], "v2");
    assert_eq!(meta["entries"].as_array().unwrap().len(), 11);
    let imported =
        prepare_import_transaction(&zip_staging, restored.path(), &RecordingSecrets::default())
            .unwrap();
    assert_eq!(imported.txn.writes.len(), 11);
    ncd_runtime::LocalConfigStore::new(restored.path())
        .apply_transaction(imported.txn)
        .unwrap();
    for write in expected {
        let path = restored
            .path()
            .join(write.path.strip_prefix(root.path()).unwrap());
        let actual: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            actual,
            write.payload,
            "roundtrip mismatch: {}",
            path.display()
        );
    }
}

#[test]
fn frontend_preferences_enforce_key_value_and_total_budgets() {
    let too_many = ConfigFrontendPreferences {
        version: 1,
        storage: (0..513)
            .map(|index| {
                (
                    format!("ncd.maibot.chat.name.instance-{index}"),
                    "name".into(),
                )
            })
            .collect(),
    };
    assert!(validation::validate_frontend(&too_many).is_err());
    let oversized = ConfigFrontendPreferences {
        version: 1,
        storage: BTreeMap::from([("ncd.terminal.prefs.v1".into(), " ".repeat(512 * 1024 + 1))]),
    };
    assert!(validation::validate_frontend(&oversized).is_err());
    let storage = [
        "ncd.terminal.prefs.v1",
        "ncd.terminal.layout.v1",
        "ncd.chat.ui.v1",
        "ncd:bot_custom_order:v1",
    ]
    .into_iter()
    .map(|key| {
        (
            key.into(),
            format!(
                "{}{}",
                " ".repeat(512 * 1024 - 2),
                if key == "ncd:bot_custom_order:v1" {
                    "[]"
                } else {
                    "{}"
                }
            ),
        )
    })
    .collect();
    assert!(
        validation::validate_frontend(&ConfigFrontendPreferences {
            version: 1,
            storage
        })
        .is_err()
    );
}

#[test]
fn export_registry_excludes_secrets_installs_and_history() {
    use zip::read::ZipArchive;
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    write_json(
        root.path(),
        "config/config.json",
        serde_json::from_str(VALID_CONFIG).unwrap(),
    );
    for path in [
        "secrets/password.json",
        "ssh_keys/private.key",
        "onebot-debug/history.jsonl",
        "chat/history/archive.json",
        "components/app/config.json",
        "state/snowluma/session.json",
    ] {
        write_json(
            root.path(),
            path,
            serde_json::json!({"private":"must-stay-local"}),
        );
    }
    let dest = output.path().join("backup.zip");
    export_config_to_path(root.path(), &dest, None).unwrap();
    let mut zip = ZipArchive::new(File::open(&dest).unwrap()).unwrap();
    let names: Vec<_> = (0..zip.len())
        .map(|index| zip.by_index(index).unwrap().name().to_string())
        .collect();
    assert_eq!(names, vec!["config.json", "export_meta.json"]);
}
