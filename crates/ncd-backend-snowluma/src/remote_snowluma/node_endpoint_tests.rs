use super::*;

fn response() -> serde_json::Value {
    serde_json::json!({
        "state": "present",
        "node": {
            "pid": 901, "uid": 1000, "start_time": "12345", "exe": "/opt/sl/node",
            "cwd": "/opt/sl", "entry": "/opt/sl/index.mjs", "ns_pids": [901, 19],
            "pid_namespace": "pid:[4026532569]"
        },
        "snapshot": [{"pid": 901, "start_time": "12345"}],
        "host": "127.0.0.1", "port": 13105,
        "listeners": [{"host": "127.0.0.1", "port": 13105}]
    })
}

fn resolve(value: serde_json::Value) -> Result<ResolvedWebuiEndpoint, BotBackendError> {
    endpoint_from_reply(1000, serde_json::from_value(value).unwrap())
}

#[test]
fn verified_owned_endpoint_keeps_node_namespace_context() {
    let endpoint = resolve(response()).unwrap();
    assert_eq!(endpoint.host, "127.0.0.1");
    assert_eq!(endpoint.port, 13105);
    assert_eq!(endpoint.node.ns_pids, [901, 19]);
    assert_eq!(endpoint.node.pid_namespace, "pid:[4026532569]");
}

#[test]
fn selected_address_and_port_must_belong_to_the_verified_node() {
    for (key, value) in [
        ("host", serde_json::json!("::1")),
        ("host", serde_json::json!("example.com")),
        ("port", serde_json::json!(13106)),
        ("port", serde_json::json!(0)),
    ] {
        let mut reply = response();
        reply[key] = value;
        assert!(resolve(reply).is_err());
    }
}

#[test]
fn namespace_and_process_birth_must_be_complete_and_bound() {
    for (key, value) in [
        ("uid", serde_json::json!(0)),
        ("start_time", serde_json::json!("67890")),
        ("ns_pids", serde_json::json!([902, 19])),
        ("ns_pids", serde_json::json!([])),
        ("pid_namespace", serde_json::json!("pid:[]")),
        ("entry", serde_json::json!("index.mjs")),
    ] {
        let mut reply = response();
        reply["node"][key] = value;
        assert!(resolve(reply).is_err());
    }
}

#[test]
fn no_owned_listener_cannot_become_a_default_webui_port() {
    let mut reply = response();
    reply["listeners"] = serde_json::json!([]);
    assert!(resolve(reply).is_err());
    assert!(resolve(serde_json::json!({"state":"absent","snapshot":[]})).is_err());
}

#[test]
fn ipv6_endpoint_is_preserved_instead_of_replaced_by_ipv4_loopback() {
    let mut reply = response();
    reply["host"] = serde_json::json!("::1");
    reply["listeners"][0]["host"] = serde_json::json!("::1");
    assert_eq!(resolve(reply).unwrap().host, "::1");
}
