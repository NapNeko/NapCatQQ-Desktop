use super::*;
use ncd_appframework::config_backup::FrameworkConfigFile;

struct EmptyBots;
#[async_trait::async_trait]
impl BotConfigPort for EmptyBots {
    async fn bot_config(&self, _: &BotId) -> Result<Option<BotConfig>, String> {
        Ok(None)
    }
    async fn upsert_bot_config(&self, _: BotConfig) -> Result<(), String> {
        Ok(())
    }
}

struct Resolver;
#[async_trait::async_trait]
impl HostResolver for Resolver {
    async fn resolve(
        &self,
        target: &RuntimeTarget,
    ) -> Result<Arc<dyn Host>, ncd_server::HostResolveError> {
        if target.server_id().is_some() {
            Err(ncd_server::HostResolveError::message("SSH unavailable"))
        } else {
            Ok(Arc::new(ncd_host::local::LocalWindowsHost::new()))
        }
    }
}

fn fixture(root: &Path) -> AppManager {
    let store = Arc::new(AppInstanceStore::empty(root));
    let bus = Arc::new(BroadcastEventBus::default());
    AppManager::new(
        Arc::new(AppFrameworkRegistry::with_builtin()),
        store.clone(),
        Arc::new(NativeAppRuntime::new(bus.clone(), store)),
        Arc::new(Resolver),
        Arc::new(EmptyBots),
        bus,
        root,
    )
}

fn instance(project: &Path, state: AppInstanceState) -> AppInstance {
    serde_json::from_value(serde_json::json!({"id":"instance-a","framework_id":"karin","display_name":"Karin",
        "placement":"local_native","host_id":"local","install_dir":HostPath::from_windows(project.to_str().unwrap()).as_posix(),
        "port":7777,"state":state,"created_at_ms":1})).unwrap()
}

fn backup() -> FrameworkConfigBackup {
    FrameworkConfigBackup {
        version: 1,
        instance_id: "instance-a".into(),
        framework_id: "karin".into(),
        files: vec![FrameworkConfigFile {
            relative_path: ".env".into(),
            text: "# restored\r\nHTTP_PORT=7777\r\n".into(),
        }],
    }
}

fn project_files(project: &Path) {
    std::fs::create_dir_all(project.join("node_modules/node-karin")).unwrap();
    std::fs::write(
        project.join("package.json"),
        r#"{"name":"karin","dependencies":{"node-karin":"1"}}"#,
    )
    .unwrap();
    std::fs::write(
        project.join("node_modules/node-karin/package.json"),
        r#"{"version":"1"}"#,
    )
    .unwrap();
    std::fs::write(project.join(".env"), "HTTP_PORT=1111\n").unwrap();
}

#[test]
fn remote_connection_must_match_host_port_and_username_of_restored_profile() {
    let mut profile: ServerProfile = serde_json::from_value(serde_json::json!({"id":"example","name":"Remote","host":"old.example","port":22,"username":"bot"})).unwrap();
    let dial = SshDialTarget {
        host: "OLD.EXAMPLE".into(),
        port: 22,
        username: "bot".into(),
    };
    assert!(server_matches_connection(&profile, &dial));
    profile.host = "new.example".into();
    assert!(!server_matches_connection(&profile, &dial));
    profile.host = "old.example".into();
    profile.port = 2222;
    assert!(!server_matches_connection(&profile, &dial));
    profile.port = 22;
    profile.username = "other".into();
    assert!(!server_matches_connection(&profile, &dial));
}

#[tokio::test]
async fn available_project_restores_without_creating_pid_sidecars() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("karin");
    project_files(&project);
    let manager = fixture(root.path());
    let inst = instance(&project, AppInstanceState::Stopped);
    manager.store.upsert(inst.clone()).await.unwrap();
    let _guard = manager.framework_config_transfer_guard().await;
    let prepared = manager
        .prepare_framework_config_restore(&[backup()], &[inst.clone()], None)
        .await
        .unwrap();
    assert!(prepared.pending.is_empty());
    assert_eq!(prepared.restored_ids, ["instance-a"]);
    manager
        .replace_instances_with(
            Some(vec![inst]),
            prepared.plan.commit_with(async { Ok(()) }),
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(project.join(".env")).unwrap(),
        backup().files[0].text
    );
    assert!(!project.join(".ncd-app.pid").exists());
}

#[tokio::test]
async fn running_missing_and_disconnected_instances_are_deferred_without_file_writes() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("karin");
    project_files(&project);
    let manager = fixture(root.path());
    let active = instance(&project, AppInstanceState::Running);
    manager.store.upsert(active.clone()).await.unwrap();
    let mut imported = active.clone();
    imported.state = AppInstanceState::Stopped;
    let result = manager
        .prepare_framework_config_restore(&[backup()], &[imported.clone()], None)
        .await
        .unwrap();
    assert!(result.pending[0].contains("正在运行"));
    assert!(result.restored_ids.is_empty());
    result.plan.commit_with(async { Ok(()) }).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(project.join(".env")).unwrap(),
        "HTTP_PORT=1111\n"
    );
    manager
        .store
        .update(&active.id, |i| i.state = AppInstanceState::Stopped)
        .await
        .unwrap();
    imported.install_dir = HostPath::from_windows(root.path().join("missing").to_str().unwrap())
        .as_posix()
        .into();
    let result = manager
        .prepare_framework_config_restore(&[backup()], &[imported.clone()], None)
        .await
        .unwrap();
    assert!(result.pending[0].contains("安装目录不存在"));
    assert!(!root.path().join("missing").exists());
    imported.host_id = "remote:example".into();
    imported.placement = AppPlacement::RemoteNative;
    let result = manager
        .prepare_framework_config_restore(&[backup()], &[imported], None)
        .await
        .unwrap();
    assert!(result.pending[0].contains("主机暂不可用"));
}

#[tokio::test]
async fn pending_backup_survives_restart_and_is_exported_before_live_config() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("not-installed");
    let inst = instance(&project, AppInstanceState::NotInstalled);
    let manager = fixture(root.path());
    manager.store.upsert(inst.clone()).await.unwrap();
    let folder = root.path().join("config/framework-configs");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("instance-a.json"),
        serde_json::to_vec(&FrameworkConfigRecovery {
            backup: backup(),
            pending: true,
        })
        .unwrap(),
    )
    .unwrap();
    let restarted = fixture(root.path());
    restarted.store.upsert(inst.clone()).await.unwrap();
    assert_eq!(
        restarted.pending_framework_config_restores().await.unwrap(),
        ["Karin"]
    );
    assert_eq!(
        restarted
            .export_framework_config_backups(&[inst.clone()], None)
            .await
            .unwrap(),
        [backup()]
    );
    assert!(
        restarted
            .start_instance(&inst.id)
            .await
            .unwrap_err()
            .to_string()
            .contains("待恢复")
    );
}

#[tokio::test]
async fn backup_gate_excludes_raw_configuration_edits() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("karin");
    project_files(&project);
    let manager = fixture(root.path());
    let inst = instance(&project, AppInstanceState::Stopped);
    manager.store.upsert(inst.clone()).await.unwrap();
    let guard = manager.framework_config_transfer_guard().await;
    let blocked = tokio::time::timeout(
        Duration::from_millis(20),
        manager.write_config_text(&inst.id, "env", "HTTP_PORT=7777\n", None),
    )
    .await;
    assert!(blocked.is_err());
    assert_eq!(
        std::fs::read_to_string(project.join(".env")).unwrap(),
        "HTTP_PORT=1111\n"
    );
    drop(guard);
    manager
        .write_config_text(&inst.id, "env", "HTTP_PORT=7777\n", None)
        .await
        .unwrap();
}
