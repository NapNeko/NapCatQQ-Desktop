use super::*;
use crate::SecretStoreImpl;
use crate::runtime_router::DockerSecretProvider;

#[test]
fn docker_recreation_detects_container_changes_and_keeps_protocol_hot_reload() {
    use ncd_domain::docker::{BotDockerConfig, PortMapping};
    use ncd_test_support::BotConfigBuilder;

    for backend in [BackendType::NapCat, BackendType::SnowLuma] {
        let previous = BotConfigBuilder::new()
            .qq_id(10001)
            .runtime_target(RuntimeTarget::server("remote"))
            .deployment_type(ncd_domain::DeploymentType::Docker)
            .backend_type(backend)
            .build();
        let mut current = previous.clone();
        current.docker = Some(BotDockerConfig::default());
        assert!(!docker_container_config_changed(&previous, &current));
        current.docker.as_mut().unwrap().engine = Some(ncd_domain::docker::ContainerEngine::Docker);
        assert!(!docker_container_config_changed(&previous, &current));
        current.bot.name = "renamed".into();
        assert!(!docker_container_config_changed(&previous, &current));

        current.docker.as_mut().unwrap().port_mappings = previous
            .effective_docker_ports()
            .into_iter()
            .rev()
            .collect();
        assert!(!docker_container_config_changed(&previous, &current));

        current.docker.as_mut().unwrap().port_mappings = vec![PortMapping::new(8080, 3000)];
        assert!(docker_container_config_changed(&previous, &current));
        current.docker = Some(BotDockerConfig {
            memory_mb: Some(512),
            ..Default::default()
        });
        assert!(docker_container_config_changed(&previous, &current));

        let mut raw = serde_json::to_value(&previous).unwrap();
        raw["connect"]["httpServers"] = serde_json::json!([
            {"name": "new-http", "host": "127.0.0.1", "port": 4000}
        ]);
        let with_listener: BotConfig = serde_json::from_value(raw).unwrap();
        assert!(docker_container_config_changed(&previous, &with_listener));
        let mut token_change = with_listener.clone();
        token_change.connect.http_servers[0].base.token = "changed".into();
        assert!(!docker_container_config_changed(
            &with_listener,
            &token_change
        ));
        token_change.bot.runtime_target = RuntimeTarget::server("different");
        assert!(!docker_container_config_changed(
            &with_listener,
            &token_change
        ));
    }
}

fn temp_secret_store() -> (tempfile::TempDir, Arc<dyn SecretStore + Send + Sync>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SecretStoreImpl::new_with_force_fallback(dir.path(), true));
    (dir, store)
}

#[test]
fn docker_webui_token_is_stable_and_not_predictable() {
    let (_dir, store) = temp_secret_store();
    let secrets = DockerSecretProvider::new(Some(store));

    let first = secrets.napcat_webui_token(10001).unwrap();
    let second = secrets.napcat_webui_token(10001).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(first, "10001");
    assert_ne!(first, "ncbot-10001");
    assert_ne!(first, "ncbot10001");
    assert_ne!(first, "test-webui-token");
}

#[test]
fn docker_webui_token_replaces_blank_secret() {
    let (_dir, store) = temp_secret_store();
    let secrets = DockerSecretProvider::new(Some(Arc::clone(&store)));
    let key = DockerSecretProvider::napcat_webui_key(10002);
    store.put(&key, "   ").unwrap();

    let token = secrets.napcat_webui_token(10002).unwrap();

    assert_eq!(store.get(&key).unwrap().as_deref(), Some(token.as_str()));
    assert_eq!(token.len(), 64);
}

#[test]
fn dot_path_sets_object_field() {
    let mut root = serde_json::json!({ "a": { "b": 1 } });
    set_value_at_dot_path(&mut root, "a.c", serde_json::json!("x")).unwrap();
    assert_eq!(root["a"]["c"], serde_json::json!("x"));
}

#[test]
fn dot_path_sets_field_inside_array_element() {
    // ConfigDrift 的连接数组路径:network.httpClients.0.token
    let mut root = serde_json::json!({
        "network": { "httpClients": [ { "token": "old" }, { "token": "keep" } ] }
    });
    set_value_at_dot_path(
        &mut root,
        "network.httpClients.0.token",
        serde_json::json!("new"),
    )
    .unwrap();
    assert_eq!(root["network"]["httpClients"][0]["token"], "new");
    assert_eq!(root["network"]["httpClients"][1]["token"], "keep");
}

#[test]
fn dot_path_null_removes_object_key_in_array_element() {
    let mut root = serde_json::json!({
        "network": { "httpClients": [ { "token": "drop", "url": "u" } ] }
    });
    set_value_at_dot_path(
        &mut root,
        "network.httpClients.0.token",
        serde_json::Value::Null,
    )
    .unwrap();
    assert!(root["network"]["httpClients"][0].get("token").is_none());
    assert_eq!(root["network"]["httpClients"][0]["url"], "u");
}

#[test]
fn dot_path_array_index_out_of_bounds_errors() {
    let mut root = serde_json::json!({ "list": [ { "x": 1 } ] });
    let err = set_value_at_dot_path(&mut root, "list.3.x", serde_json::json!(2)).unwrap_err();
    assert!(err.contains("越界"));
}

#[test]
fn dot_path_non_numeric_array_segment_errors() {
    let mut root = serde_json::json!({ "list": [ { "x": 1 } ] });
    let err = set_value_at_dot_path(&mut root, "list.foo.x", serde_json::json!(2)).unwrap_err();
    assert!(err.contains("不是合法下标"));
}

#[path = "host_connection_tests.rs"]
mod host_connection_tests;
