use std::sync::Arc;

use ncd_server::server_manager::{InMemoryCredentialStore, ServerManager};
use serde_json::json;

#[tokio::test]
async fn read_only_profiles_do_not_migrate_legacy_files() {
    let root = tempfile::tempdir().unwrap();
    let legacy_dir = root.path().join("runtime").join("config");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    let legacy_path = legacy_dir.join("servers.json");
    let payload = json!([{
        "id": "legacy", "name": "server", "host": "example.invalid", "username": "user",
    }])
    .to_string();
    std::fs::write(&legacy_path, &payload).unwrap();
    let manager = ServerManager::new(root.path(), Arc::new(InMemoryCredentialStore::default()));

    let profiles = manager.list_servers_read_only().await;
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id, "legacy");
    assert!(!root.path().join("config").join("servers.json").exists());
    assert_eq!(std::fs::read_to_string(&legacy_path).unwrap(), payload);
    assert!(manager.get_host("legacy").await.is_none());
}
