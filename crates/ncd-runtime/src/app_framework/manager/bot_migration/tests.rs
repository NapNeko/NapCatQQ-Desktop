use super::*;
use ncd_appframework::AdoptedFile;
use ncd_domain::{AdvancedConfig, AutoRestartSchedule, BotBasicConfig, ConnectConfig};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const JOB_ID: &str = "9163c4e5-36b5-4c6e-9ff8-ae3aaab13452";

struct NoRemoteResolver {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl HostResolver for NoRemoteResolver {
    async fn resolve(
        &self,
        _: &RuntimeTarget,
    ) -> Result<Arc<dyn Host>, ncd_server::HostResolveError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(ncd_server::HostResolveError::message(
            "migration tests never resolve a real host",
        ))
    }
}

struct MemoryBots {
    config: tokio::sync::Mutex<BotConfig>,
    reservations: std::sync::Mutex<Vec<(String, bool)>>,
    writes: AtomicUsize,
    running: AtomicBool,
    online: AtomicBool,
}

#[async_trait::async_trait]
impl BotConfigPort for MemoryBots {
    async fn bot_config(&self, id: &BotId) -> Result<Option<BotConfig>, String> {
        let config = self.config.lock().await;
        Ok((config.bot.qq_id.to_string() == id.as_str()).then(|| config.clone()))
    }

    async fn upsert_bot_config(&self, config: BotConfig) -> Result<(), String> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        *self.config.lock().await = config;
        Ok(())
    }

    async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
        Ok(vec![self.config.lock().await.clone()])
    }

    async fn migration_reserve_protocol(&self, id: &BotId, reserve: bool) -> Result<(), String> {
        self.reservations
            .lock()
            .unwrap()
            .push((id.as_str().to_string(), reserve));
        Ok(())
    }

    async fn migration_upsert_protocol(&self, config: BotConfig) -> Result<(), String> {
        self.upsert_bot_config(config).await
    }

    async fn migration_protocol_running(&self, _: &BotId) -> Result<bool, String> {
        Ok(self.running.load(Ordering::SeqCst))
    }

    async fn migration_protocol_online(&self, _: &BotId) -> Result<bool, String> {
        Ok(self.online.load(Ordering::SeqCst))
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    manager: Arc<AppManager>,
    resolver: Arc<NoRemoteResolver>,
    bots: Arc<MemoryBots>,
}

fn config() -> BotConfig {
    let mut config = BotConfig {
        bot: BotBasicConfig {
            name: "Migration test Bot".into(),
            qq_id: 10001,
            music_sign_url: "https://fixture.invalid/private-config-snapshot".into(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: true,
            runtime_target: RuntimeTarget::server("source"),
            backend_type: BackendType::NapCat,
            deployment_type: DeploymentType::Native,
            snowluma_start_mode: None,
            webui_password_takeover: false,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
        docker: None,
        status_command: None,
    };
    config.advanced.auto_start = true;
    config.bot.auto_restart_schedule.enable = true;
    config.bot.auto_restart_schedule.duration = 6;
    config
}

fn app() -> AppInstance {
    let id = AppInstanceId::new("migration-app-a");
    AppInstance {
        id: id.clone(),
        framework_id: AppFrameworkId::new("nonebot2"),
        display_name: "Migration test app".into(),
        placement: AppPlacement::RemoteNative,
        host_id: "remote:source".into(),
        install_dir: "/home/bot/ncd/apps/nonebot2/source-app".into(),
        port: 18080,
        state: AppInstanceState::Stopped,
        link: Some(AppLinkRecord {
            bot_id: BotId::new("10001"),
            mode: OneBotLinkMode::ReverseWs,
            connection_name: app_link_connection_name(&id),
            linked_at_ms: 1,
            resident_forward_port: None,
        }),
        installed_version: Some("2.4.0".into()),
        last_error: None,
        created_at_ms: 1,
        install_renderer: false,
        origin: AppInstanceOrigin::Imported,
        auto_start: true,
    }
}

fn journal(phase: BotMigrationPhase) -> MigrationJournal {
    let source = config();
    let mut target = without_auto_start(source.clone());
    target.bot.runtime_target = RuntimeTarget::server("target");
    let source_app = app();
    let mut target_app = source_app.clone();
    target_app.host_id = "remote:target".into();
    target_app.install_dir = "/home/bot/ncd/apps/nonebot2/target-app".into();
    target_app.auto_start = false;
    let adopt = adopt::AdoptSnapshot {
        version: 1,
        instance_id: source_app.id.as_str().to_string(),
        host_id: source_app.host_id.clone(),
        install_dir: source_app.install_dir.clone(),
        captured_at_ms: 1,
        files: vec![AdoptedFile {
            rel_path: ".env.prod".into(),
            existed: true,
            text: Some("ACCESS_TOKEN=fixture-private-adopt-snapshot\n".into()),
            skip_reason: None,
        }],
        supervisors: vec!["fixture-bot.service".into()],
    };
    MigrationJournal {
        job: BotMigrationJob {
            id: JOB_ID.into(),
            bot_id: "10001".into(),
            source_host_id: "remote:source".into(),
            target_host_id: "remote:target".into(),
            phase,
            message: "fixture migration".into(),
            apps: vec![BotMigrationApp {
                instance_id: source_app.id.as_str().to_string(),
                display_name: source_app.display_name.clone(),
                framework_id: source_app.framework_id.as_str().to_string(),
                source_dir: source_app.install_dir.clone(),
            }],
            error: None,
            rollback_available: true,
            target_started: false,
            updated_at_ms: 1,
        },
        source,
        target,
        apps: vec![AppMove {
            source: source_app,
            target: target_app,
            adopt: Some(adopt),
        }],
        source_was_running: false,
        source_touched: true,
        config_switched: false,
    }
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(AppInstanceStore::empty(temp.path()));
    let bus = Arc::new(BroadcastEventBus::default());
    let resolver = Arc::new(NoRemoteResolver {
        calls: AtomicUsize::new(0),
    });
    let bots = Arc::new(MemoryBots {
        config: tokio::sync::Mutex::new(config()),
        reservations: std::sync::Mutex::new(Vec::new()),
        writes: AtomicUsize::new(0),
        running: AtomicBool::new(false),
        online: AtomicBool::new(false),
    });
    let manager = Arc::new(AppManager::new(
        Arc::new(AppFrameworkRegistry::with_builtin()),
        Arc::clone(&store),
        Arc::new(NativeAppRuntime::new(Arc::clone(&bus), store)),
        resolver.clone(),
        bots.clone(),
        bus,
        temp.path(),
    ));
    Fixture {
        _temp: temp,
        manager,
        resolver,
        bots,
    }
}

#[test]
fn preview_fingerprint_expires_after_bot_app_or_destination_changes() {
    let source = config();
    let apps = vec![app()];
    let original = migration_fingerprint(&source, &apps, "remote:target").unwrap();
    assert_eq!(
        original,
        migration_fingerprint(&source, &apps, "remote:target").unwrap()
    );

    let mut edited_bot = source.clone();
    edited_bot.bot.offline_auto_restart = false;
    assert_ne!(
        original,
        migration_fingerprint(&edited_bot, &apps, "remote:target").unwrap()
    );
    let mut edited_apps = apps.clone();
    edited_apps[0].port += 1;
    assert_ne!(
        original,
        migration_fingerprint(&source, &edited_apps, "remote:target").unwrap()
    );
    assert_ne!(
        original,
        migration_fingerprint(&source, &apps, "remote:other").unwrap()
    );
}

#[test]
fn source_autostart_suppression_keeps_original_snapshot_intact() {
    let source = config();
    let suppressed = without_auto_start(source.clone());
    assert!(!suppressed.advanced.auto_start);
    assert!(!suppressed.bot.offline_auto_restart);
    assert!(!suppressed.bot.auto_restart_schedule.enable);
    assert!(source.advanced.auto_start);
    assert!(source.bot.offline_auto_restart);
    assert!(source.bot.auto_restart_schedule.enable);
    assert_eq!(suppressed.bot.auto_restart_schedule.duration, 6);
    assert_eq!(suppressed.connect, source.connect);
    assert_eq!(suppressed.bot.runtime_target, source.bot.runtime_target);
}

#[tokio::test]
async fn journal_roundtrip_retains_private_rollback_snapshots_outside_public_job() {
    let f = fixture();
    let expected = journal(BotMigrationPhase::RecoveryRequired);
    f.manager.migration_save(&expected).await.unwrap();
    let restored = f.manager.migration_load(JOB_ID).await.unwrap();
    assert_eq!(
        serde_json::to_value(&restored).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert_eq!(
        restored.apps[0].adopt.as_ref().unwrap().supervisors,
        ["fixture-bot.service"]
    );
    let public = serde_json::to_string(&restored.job).unwrap();
    assert!(!public.contains("fixture-private-adopt-snapshot"));
    assert!(!public.contains("private-config-snapshot"));
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unfinished_journal_rejects_duplicate_migration_before_resolving_a_host() {
    let f = fixture();
    f.manager
        .migration_save(&journal(BotMigrationPhase::RecoveryRequired))
        .await
        .unwrap();
    let error = f
        .manager
        .start_bot_migration(
            BotMigrationRequest {
                bot_id: "10001".into(),
                target_server_id: "target".into(),
            },
            "stale-preview-fingerprint".into(),
        )
        .await
        .unwrap_err();
    assert!(error.contains("上一个迁移"), "{error}");
    assert_eq!(f.bots.writes.load(Ordering::SeqCst), 0);
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.manager.migration_load(JOB_ID).await.unwrap().job.phase,
        BotMigrationPhase::RecoveryRequired
    );
}

#[tokio::test]
async fn active_migration_lock_rejects_duplicate_request_before_preflight() {
    let f = fixture();
    let _guard = f.manager.migration.gate.lock().await;
    let error = f
        .manager
        .start_bot_migration(
            BotMigrationRequest {
                bot_id: "10001".into(),
                target_server_id: "target".into(),
            },
            String::new(),
        )
        .await
        .unwrap_err();
    assert!(error.contains("已有迁移"), "{error}");
    assert!(f.manager.migration_journals().await.unwrap().is_empty());
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn interrupted_copy_recovers_to_a_fenced_journal_and_suppresses_app_autostart() {
    let f = fixture();
    let interrupted = journal(BotMigrationPhase::Copying);
    f.manager
        .store
        .upsert(interrupted.apps[0].source.clone())
        .await
        .unwrap();
    f.manager.migration_save(&interrupted).await.unwrap();

    f.manager.recover_bot_migrations().await.unwrap();

    let recovered = f.manager.migration_load(JOB_ID).await.unwrap();
    assert_eq!(recovered.job.phase, BotMigrationPhase::RecoveryRequired);
    assert!(recovered.job.rollback_available);
    assert!(!f.manager.get_instance(&app().id).await.unwrap().auto_start);
    assert_eq!(
        *f.bots.reservations.lock().unwrap(),
        [(String::from("10001"), true)]
    );
    assert!(f.manager.ensure_framework_write_allowed().await.is_err());
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn preparation_interruption_before_touching_source_keeps_original_autostart() {
    let f = fixture();
    let mut interrupted = journal(BotMigrationPhase::Preparing);
    interrupted.source_touched = false;
    interrupted.job.rollback_available = false;
    f.manager
        .store
        .upsert(interrupted.apps[0].source.clone())
        .await
        .unwrap();
    f.manager.migration_save(&interrupted).await.unwrap();

    f.manager.recover_bot_migrations().await.unwrap();

    let recovered = f.manager.migration_load(JOB_ID).await.unwrap();
    assert_eq!(recovered.job.phase, BotMigrationPhase::Failed);
    assert!(!recovered.job.rollback_available);
    assert!(f.manager.get_instance(&app().id).await.unwrap().auto_start);
    assert_eq!(
        *f.bots.reservations.lock().unwrap(),
        [
            (String::from("10001"), true),
            (String::from("10001"), false)
        ]
    );
    assert!(f.manager.ensure_framework_write_allowed().await.is_ok());
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn framework_write_fence_tracks_handoff_state_and_has_task_local_bypass() {
    let f = fixture();
    for phase in [
        BotMigrationPhase::Copying,
        BotMigrationPhase::Switching,
        BotMigrationPhase::AwaitingLogin,
        BotMigrationPhase::RecoveryRequired,
    ] {
        f.manager.migration_save(&journal(phase)).await.unwrap();
        assert!(
            f.manager.ensure_framework_write_allowed().await.is_err(),
            "{phase:?}"
        );
        assert!(
            MIGRATION_APP_CONTEXT
                .scope((), f.manager.ensure_framework_write_allowed())
                .await
                .is_ok()
        );
        assert!(f.manager.ensure_framework_write_allowed().await.is_err());
    }
    for phase in [BotMigrationPhase::Completed, BotMigrationPhase::RolledBack] {
        f.manager.migration_save(&journal(phase)).await.unwrap();
        assert!(
            f.manager.ensure_framework_write_allowed().await.is_ok(),
            "{phase:?}"
        );
    }
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn incomplete_handoff_rejects_public_mutations_before_host_or_component_work() {
    let f = fixture();
    let pending = journal(BotMigrationPhase::RecoveryRequired);
    let id = pending.apps[0].source.id.clone();
    f.manager
        .store
        .upsert(pending.apps[0].source.clone())
        .await
        .unwrap();
    f.manager.migration_save(&pending).await.unwrap();

    let errors = [
        f.manager.start_instance(&id).await.unwrap_err().to_string(),
        f.manager.stop_instance(&id).await.unwrap_err().to_string(),
        f.manager
            .delete_instance(&id, true)
            .await
            .unwrap_err()
            .to_string(),
        f.manager
            .set_instance_auto_start(&id, false)
            .await
            .unwrap_err()
            .to_string(),
        f.manager
            .install_instance(&id, None, None)
            .await
            .unwrap_err()
            .to_string(),
        f.manager.unlink(&id).await.unwrap_err().to_string(),
        f.manager
            .write_config_text(&id, "env", "PORT=19000\n", None)
            .await
            .unwrap_err()
            .to_string(),
    ];
    for error in errors {
        assert!(error.contains("迁移"), "{error}");
    }
    let unchanged = f.manager.get_instance(&id).await.unwrap();
    assert_eq!(unchanged.state, AppInstanceState::Stopped);
    assert!(unchanged.auto_start);
    assert_eq!(f.bots.writes.load(Ordering::SeqCst), 0);
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn rollback_after_target_start_requires_data_loss_acknowledgement_before_actions() {
    let f = fixture();
    let mut pending = journal(BotMigrationPhase::RecoveryRequired);
    pending.job.target_started = true;
    f.manager.migration_save(&pending).await.unwrap();

    let error = f
        .manager
        .rollback_bot_migration(JOB_ID.into(), false)
        .await
        .unwrap_err();

    assert!(error.contains("目标端可能已有新数据"), "{error}");
    assert_eq!(
        f.manager.migration_load(JOB_ID).await.unwrap().job.phase,
        BotMigrationPhase::RecoveryRequired
    );
    assert!(f.bots.reservations.lock().unwrap().is_empty());
    assert_eq!(f.bots.writes.load(Ordering::SeqCst), 0);
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn stopped_target_protocol_requires_recovery_instead_of_waiting_for_login() {
    let f = fixture();
    let mut pending = journal(BotMigrationPhase::AwaitingLogin);
    pending.source_was_running = true;
    pending.config_switched = true;
    pending.apps.clear();
    *f.bots.config.lock().await = pending.target.clone();

    f.manager
        .migration_finish_if_ready(&mut pending)
        .await
        .unwrap();

    assert_eq!(pending.job.phase, BotMigrationPhase::RecoveryRequired);
    assert!(
        pending
            .job
            .error
            .as_deref()
            .unwrap()
            .contains("协议端进程已退出")
    );
    assert!(pending.job.rollback_available);
    assert_eq!(
        f.manager.migration_load(JOB_ID).await.unwrap().job.phase,
        BotMigrationPhase::RecoveryRequired
    );
    assert!(!f.bots.config.lock().await.advanced.auto_start);
    assert!(f.bots.reservations.lock().unwrap().is_empty());
    assert_eq!(f.bots.writes.load(Ordering::SeqCst), 0);
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn running_target_waits_for_qq_login_without_releasing_the_handoff() {
    let f = fixture();
    let mut pending = journal(BotMigrationPhase::AwaitingLogin);
    pending.source_was_running = true;
    pending.config_switched = true;
    pending.apps.clear();
    *f.bots.config.lock().await = pending.target.clone();
    f.bots.running.store(true, Ordering::SeqCst);

    f.manager
        .migration_finish_if_ready(&mut pending)
        .await
        .unwrap();

    assert_eq!(pending.job.phase, BotMigrationPhase::AwaitingLogin);
    assert!(pending.job.error.is_none());
    assert!(!f.bots.config.lock().await.advanced.auto_start);
    assert!(f.bots.reservations.lock().unwrap().is_empty());
    assert_eq!(f.bots.writes.load(Ordering::SeqCst), 0);
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn finishing_a_stopped_bot_restores_auto_start_policies_on_the_target_only() {
    let f = fixture();
    let mut pending = journal(BotMigrationPhase::AwaitingLogin);
    pending.config_switched = true;
    *f.bots.config.lock().await = pending.target.clone();
    f.manager
        .store
        .upsert(pending.apps[0].target.clone())
        .await
        .unwrap();
    f.manager.migration_save(&pending).await.unwrap();

    MIGRATION_APP_CONTEXT
        .scope((), f.manager.migration_finish_if_ready(&mut pending))
        .await
        .unwrap();

    let saved = f.bots.config.lock().await.clone();
    assert_eq!(saved.bot.runtime_target, RuntimeTarget::server("target"));
    assert!(saved.advanced.auto_start);
    assert!(saved.bot.offline_auto_restart);
    assert_eq!(
        saved.bot.auto_restart_schedule,
        pending.source.bot.auto_restart_schedule
    );
    let saved_app = f.manager.get_instance(&app().id).await.unwrap();
    assert_eq!(saved_app.host_id, "remote:target");
    assert!(saved_app.auto_start);
    assert_eq!(pending.job.phase, BotMigrationPhase::Completed);
    assert_eq!(
        f.manager.migration_load(JOB_ID).await.unwrap().job.phase,
        BotMigrationPhase::Completed
    );
    assert_eq!(
        *f.bots.reservations.lock().unwrap(),
        [(String::from("10001"), false)]
    );
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn latest_history_entry_is_returned_without_restarting_a_finished_migration() {
    let f = fixture();
    let older = journal(BotMigrationPhase::Completed);
    let mut newer = journal(BotMigrationPhase::RolledBack);
    newer.job.id = "fae9b790-9d92-4e90-b8d9-dacb39c4a734".into();
    newer.job.updated_at_ms = older.job.updated_at_ms + 100;
    f.manager.migration_save(&older).await.unwrap();
    f.manager.migration_save(&newer).await.unwrap();

    let latest = f
        .manager
        .get_bot_migration("10001".into())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(latest.id, newer.job.id);
    assert_eq!(latest.phase, BotMigrationPhase::RolledBack);
    assert_eq!(f.manager.migration_journals().await.unwrap().len(), 2);
    assert!(f.bots.reservations.lock().unwrap().is_empty());
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn corrupted_journal_fails_closed_and_reserves_existing_protocol_instances() {
    let f = fixture();
    let directory = f.manager.data_root.join("config/bot-migrations");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join(format!("{JOB_ID}.json")), b"{broken json").unwrap();

    assert!(f.manager.recover_bot_migrations().await.is_err());
    assert!(f.manager.ensure_framework_write_allowed().await.is_err());
    assert_eq!(
        *f.bots.reservations.lock().unwrap(),
        [(String::from("10001"), true)]
    );
    assert_eq!(f.resolver.calls.load(Ordering::SeqCst), 0);
}
