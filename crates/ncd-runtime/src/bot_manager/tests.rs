use super::*;
use crate::SecretStoreImpl;
use crate::runtime_router::DockerSecretProvider;

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
