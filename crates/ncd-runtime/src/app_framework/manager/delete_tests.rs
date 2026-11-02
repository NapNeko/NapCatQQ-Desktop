use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

enum HostStatus {
    Removed,
    Offline,
    Unreadable,
}

struct UnavailableHost {
    status: HostStatus,
    connections: AtomicUsize,
}

#[async_trait::async_trait]
impl HostResolver for UnavailableHost {
    async fn is_registered(&self, _: &RuntimeTarget) -> Result<bool, ncd_server::HostResolveError> {
        match self.status {
            HostStatus::Removed => Ok(false),
            HostStatus::Offline => Ok(true),
            HostStatus::Unreadable => Err("读取服务器档案失败".into()),
        }
    }

    async fn resolve(
        &self,
        _: &RuntimeTarget,
    ) -> Result<Arc<dyn Host>, ncd_server::HostResolveError> {
        self.connections.fetch_add(1, Ordering::Relaxed);
        Err("SSH 连接失败".into())
    }
}

struct NoBots;

#[async_trait::async_trait]
impl BotConfigPort for NoBots {
    async fn bot_config(&self, _: &BotId) -> Result<Option<BotConfig>, String> {
        Ok(None)
    }

    async fn upsert_bot_config(&self, _: BotConfig) -> Result<(), String> {
        Ok(())
    }
}

async fn fixture(
    root: &Path,
    status: HostStatus,
    origin: AppInstanceOrigin,
) -> (AppManager, AppInstanceId, Arc<UnavailableHost>) {
    let bus = Arc::new(BroadcastEventBus::default());
    let store = Arc::new(AppInstanceStore::empty(root));
    let resolver = Arc::new(UnavailableHost {
        status,
        connections: AtomicUsize::new(0),
    });
    let manager = AppManager::new(
        Arc::new(AppFrameworkRegistry::with_builtin()),
        Arc::clone(&store),
        Arc::new(NativeAppRuntime::new(Arc::clone(&bus), Arc::clone(&store))),
        resolver.clone(),
        Arc::new(NoBots),
        bus,
        root,
    );
    let id = AppInstanceId::new("xiuxian");
    store
        .upsert(AppInstance {
            id: id.clone(),
            framework_id: AppFrameworkId::new("nonebot2"),
            display_name: "bot-xiuxian".into(),
            placement: AppPlacement::RemoteNative,
            host_id: "remote:removed-server".into(),
            install_dir: "/home/bot-xiuxian".into(),
            port: 13120,
            state: AppInstanceState::Stopped,
            link: None,
            installed_version: Some("2.5.0".into()),
            last_error: None,
            created_at_ms: 1,
            install_renderer: false,
            origin,
            auto_start: false,
        })
        .await
        .unwrap();
    manager
        .adopt_store
        .save(&adopt::AdoptSnapshot {
            version: 1,
            instance_id: id.as_str().into(),
            host_id: "remote:removed-server".into(),
            install_dir: "/home/bot-xiuxian".into(),
            captured_at_ms: 1,
            files: vec![],
            supervisors: vec![],
        })
        .await
        .unwrap();
    (manager, id, resolver)
}

#[tokio::test]
async fn removed_host_allows_imported_and_created_records_to_be_deleted_without_connecting() {
    for origin in [AppInstanceOrigin::Imported, AppInstanceOrigin::Created] {
        let root = tempfile::tempdir().unwrap();
        let (manager, id, resolver) = fixture(root.path(), HostStatus::Removed, origin).await;

        manager.delete_instance(&id, false).await.unwrap();

        assert!(manager.store.get(&id).await.is_none());
        assert!(manager.adopt_store.load(&id).await.unwrap().is_none());
        assert_eq!(resolver.connections.load(Ordering::Relaxed), 0);
    }
}

#[tokio::test]
async fn removed_host_cannot_claim_remote_files_were_deleted() {
    let root = tempfile::tempdir().unwrap();
    let (manager, id, resolver) = fixture(
        root.path(),
        HostStatus::Removed,
        AppInstanceOrigin::Imported,
    )
    .await;

    let error = manager.delete_instance(&id, true).await.unwrap_err();

    assert!(error.to_string().contains("请只移除实例记录"));
    assert!(manager.store.get(&id).await.is_some());
    assert!(manager.adopt_store.load(&id).await.unwrap().is_some());
    assert_eq!(resolver.connections.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn removed_host_with_bot_link_does_not_reconnect_for_app_unlink() {
    let root = tempfile::tempdir().unwrap();
    let (manager, id, resolver) = fixture(
        root.path(),
        HostStatus::Removed,
        AppInstanceOrigin::Imported,
    )
    .await;
    manager
        .store
        .update(&id, |instance| {
            instance.link = Some(AppLinkRecord {
                bot_id: BotId::new("10001"),
                mode: OneBotLinkMode::ReverseWs,
                connection_name: app_link_connection_name(&id),
                linked_at_ms: 1,
                resident_forward_port: Some(30001),
            });
        })
        .await
        .unwrap();

    manager.delete_instance(&id, false).await.unwrap();

    assert!(manager.store.get(&id).await.is_none());
    assert_eq!(resolver.connections.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn offline_host_preserves_imported_record_and_restore_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let (manager, id, resolver) = fixture(
        root.path(),
        HostStatus::Offline,
        AppInstanceOrigin::Imported,
    )
    .await;

    assert!(manager.delete_instance(&id, false).await.is_err());

    assert!(manager.store.get(&id).await.is_some());
    assert!(manager.adopt_store.load(&id).await.unwrap().is_some());
    assert_eq!(resolver.connections.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn unreadable_profiles_do_not_allow_record_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let (manager, id, resolver) = fixture(
        root.path(),
        HostStatus::Unreadable,
        AppInstanceOrigin::Imported,
    )
    .await;

    let error = manager.delete_instance(&id, false).await.unwrap_err();

    assert!(error.to_string().contains("读取服务器档案失败"));
    assert!(manager.store.get(&id).await.is_some());
    assert!(manager.adopt_store.load(&id).await.unwrap().is_some());
    assert_eq!(resolver.connections.load(Ordering::Relaxed), 0);
}
