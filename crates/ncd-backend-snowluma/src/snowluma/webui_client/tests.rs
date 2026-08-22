#![allow(unsafe_code)]
use super::*;

#[test]
fn ordered_candidates_dedups_and_pins_current_first() {
    let list = ordered_candidates("127.0.0.1");
    assert_eq!(list[0], "127.0.0.1");
    // 后续顺序保持 CANDIDATE_HOSTS 中其余两个,去重后总长 3
    assert_eq!(list.len(), 3);
    assert!(list.contains(&"localhost".to_string()));
    assert!(list.contains(&"[::1]".to_string()));
}

#[test]
fn ordered_candidates_handles_unknown_current_by_appending_defaults() {
    let list = ordered_candidates("custom-host");
    assert_eq!(list[0], "custom-host");
    // 三个默认候选全部追加
    assert_eq!(list.len(), 4);
}

#[test]
fn validate_host_rejects_non_loopback() {
    let err = validate_host("evil.example.com").unwrap_err();
    match err {
        SnowLumaWebUiError::Http { endpoint, cause } => {
            assert_eq!(endpoint, "<host-guard>");
            assert!(cause.contains("evil.example.com"));
        }
        other => panic!("expected Http {{ endpoint, cause }}, got {other:?}"),
    }
}

#[test]
fn validate_host_accepts_loopback_aliases() {
    assert!(validate_host("localhost").is_ok());
    assert!(validate_host("127.0.0.1").is_ok());
    assert!(validate_host("[::1]").is_ok());
}

#[test]
fn url_for_assembles_loopback_url_correctly() {
    assert_eq!(
        ReqwestSnowLumaWebUiClient::url_for("127.0.0.1", 5099, "/api/status"),
        "http://127.0.0.1:5099/api/status"
    );
    assert_eq!(
        ReqwestSnowLumaWebUiClient::url_for("[::1]", 5099, "/api/login"),
        "http://[::1]:5099/api/login"
    );
}

/// deliverable #11:smoke check 构造器在默认配置下成功
#[test]
fn client_builder_constructs_with_no_proxy() {
    let client = ReqwestSnowLumaWebUiClient::new(5099, "pwd".into());
    assert!(
        client.is_ok(),
        "ReqwestSnowLumaWebUiClient::new should succeed with default config"
    );
}

// -----------------------------------------------------------------------
// wiremock 端到端
//
// 起一个绑定在 127.0.0.1:0(OS 分配端口)的假 SnowLuma WebUI 服务
// 验证 ReqwestSnowLumaWebUiClient 在真实 HTTP 链路上的行为
//
// wiremock 0.6 默认 MockServer::start().await 监听 127.0.0.1,与本
// 客户端的 host guard(仅放行 localhost / 127.0.0.1 / [::1])天然匹配

use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 取出 wiremock 在 127.0.0.1 上分配到的随机端口
fn mock_server_port(server: &MockServer) -> u16 {
    let addr = server.address();
    assert_eq!(
        addr.ip().to_string(),
        "127.0.0.1",
        "wiremock must bind to 127.0.0.1 only (host guard)"
    );
    addr.port()
}

/// /api/status 任意 HTTP 响应即视为 ready这里直接 200 OK
#[tokio::test]
async fn wait_ready_succeeds_when_status_endpoint_responds() {
    let server = MockServer::start().await;
    let port = mock_server_port(&server);

    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0..)
        .mount(&server)
        .await;

    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    let result = client
        .wait_ready(Duration::from_secs(2), Box::new(|| false))
        .await;
    assert!(
        result.is_ok(),
        "wait_ready should succeed when /api/status responds: {result:?}"
    );
}

/// dead_check 第一轮即返回 true → wait_ready 立即 Ok(()),不发任何 HTTP
#[tokio::test]
async fn wait_ready_returns_ok_when_dead_check_true() {
    // 用一个明显未监听的端口 1,并配 5s 超时;只要 dead_check 立刻命中
    // 就不会真的去连,函数应在远小于超时的时间返回
    let client = ReqwestSnowLumaWebUiClient::new(1, "pwd".into()).expect("build client");

    let started = std::time::Instant::now();
    let result = client
        .wait_ready(Duration::from_secs(5), Box::new(|| true))
        .await;
    let elapsed = started.elapsed();

    assert!(
        result.is_ok(),
        "dead_check=true must short-circuit wait_ready"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "wait_ready should return immediately when dead_check is true (took {elapsed:?})"
    );
}

/// 全部候选 host 都连不上 → NotReady
/// 使用端口 1:所有 loopback 候选 (localhost / 127.0.0.1 / [::1])
/// 上 connect 到端口 1 都会立刻 ECONNREFUSED(is_connect),不需要等待
/// reqwest 的 5s 超时,因此 800ms 足够覆盖至少一轮候选探测 + 500ms sleep
/// 不复用 wiremock 释放的端口是为了避免与并发执行的其它测试争抢
#[tokio::test]
async fn wait_ready_returns_not_ready_on_timeout() {
    let port: u16 = 1;
    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    // 800ms 至少能覆盖一轮三候选探测 + 500ms sleep
    let result = client
        .wait_ready(Duration::from_millis(800), Box::new(|| false))
        .await;

    let err = result.expect_err("expected NotReady on closed port");
    match err {
        SnowLumaWebUiError::NotReady(d, last_errors) => {
            assert_eq!(d, Duration::from_millis(800));
            assert!(
                !last_errors.is_empty(),
                "expected at least one host probe error in last_errors"
            );
        }
        other => panic!("expected NotReady, got {other:?}"),
    }
}

/// LoginRequest body 字段名锁定:{"password": "<pwd>"}
#[tokio::test]
async fn login_serializes_password_in_request_body() {
    let server = MockServer::start().await;
    let port = mock_server_port(&server);

    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0..)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .and(body_partial_json(json!({ "password": "pwd" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "abc123" })))
        .expect(1)
        .mount(&server)
        .await;

    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    client
        .wait_ready(Duration::from_secs(2), Box::new(|| false))
        .await
        .expect("wait_ready");
    client.login().await.expect("login should succeed");
    // server.drop 时校验 expect(1),body_partial_json 同时锁定字段名
}

/// GET /api/processes 响应是 {"list": [...]} wrapped 形态,需要解包
#[tokio::test]
async fn list_processes_unwraps_wrapped_list() {
    let server = MockServer::start().await;
    let port = mock_server_port(&server);

    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "tok" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/processes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "list": [{
        "pid": 12345,
        "name": "QQ.exe",
        "path": "C:/qq",
        "uin": "100200",
        "status": "loaded",
        "error": ""
        }]
        })))
        .mount(&server)
        .await;

    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    let processes = client
        .list_processes()
        .await
        .expect("list_processes should succeed");
    assert_eq!(processes.len(), 1, "expected exactly one process");
    assert_eq!(processes[0].pid, 12345);
    assert_eq!(processes[0].name, "QQ.exe");
    assert_eq!(processes[0].uin, "100200");
    assert!(matches!(processes[0].status, HookProcessStatus::Loaded));
}

/// POST /api/processes/:pid/load 成功路径:success=true 且 process 非空 →
/// 返回 HookProcessInfo
#[tokio::test]
async fn load_process_success_path() {
    let server = MockServer::start().await;
    let port = mock_server_port(&server);

    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "tok" })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/processes/12345/load"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "success": true,
        "process": {
        "pid": 12345,
        "name": "QQ.exe",
        "path": "C:/qq",
        "uin": "100200",
        "status": "loaded",
        "error": ""
        },
        "error": ""
        })))
        .mount(&server)
        .await;

    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    let info = client
        .load_process(12345)
        .await
        .expect("load_process should succeed");
    assert_eq!(info.pid, 12345);
    assert_eq!(info.uin, "100200");
    assert!(matches!(info.status, HookProcessStatus::Loaded));
}

/// POST /api/processes/:pid/load 服务端拒绝路径:success=false →
/// ServerRejected { endpoint, message }
#[tokio::test]
async fn load_process_server_rejected() {
    let server = MockServer::start().await;
    let port = mock_server_port(&server);

    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "tok" })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/processes/12345/load"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "success": false,
        "process": null,
        "error": "process already loaded"
        })))
        .mount(&server)
        .await;

    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    let err = client
        .load_process(12345)
        .await
        .expect_err("expected ServerRejected");
    match err {
        SnowLumaWebUiError::ServerRejected { endpoint, message } => {
            assert!(
                endpoint.contains("/api/processes/12345/load"),
                "endpoint should preserve original path, got {endpoint}"
            );
            assert_eq!(message, "process already loaded");
        }
        other => panic!("expected ServerRejected, got {other:?}"),
    }
}

/// 401 自动重试一次:第一次 /api/processes 返回 401 → 客户端清 token +
/// 重新登录 + 重试 → 第二次返回 200最终 Ok(empty list)
/// wiremock 默认按 mount 顺序倒序匹配(最新挂的 mock 优先)
/// 配合 up_to_n_times(1) 实现"第一次走 A,之后走 B"的状态机
#[tokio::test]
async fn auto_retries_login_on_401_then_succeeds() {
    let server = MockServer::start().await;
    let port = mock_server_port(&server);

    // 低优先级(先挂载):第一次失败之后的回落响应
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "second" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/processes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "list": [] })))
        .mount(&server)
        .await;

    // 高优先级(后挂载) + up_to_n_times(1):仅命中一次
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "first" })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/processes"))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    let processes = client
        .list_processes()
        .await
        .expect("list_processes should succeed after 401 retry");
    assert!(
        processes.is_empty(),
        "expected empty list on second-try success, got {processes:?}"
    );
}

/// GET /api/auth/state 反序列化 mustChangePassword (camelCase)
#[tokio::test]
async fn get_auth_state_decodes_must_change_password() {
    let server = MockServer::start().await;
    let port = mock_server_port(&server);

    Mock::given(method("GET"))
        .and(path("/api/auth/state"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "mustChangePassword": true })),
        )
        .mount(&server)
        .await;

    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    let state = client
        .get_auth_state()
        .await
        .expect("get_auth_state should succeed");
    assert!(
        state.must_change_password,
        "mustChangePassword=true should decode to must_change_password=true"
    );
}

/// 设置 HTTP_PROXY 环境变量后 client 仍走 loopback —— no_proxy 起作用
/// 注意:Rust 测试默认并发执行,std::env::set_var 会跨测试污染环境
/// 因此用 #[ignore] 标记,仅在显式 cargo test -- --ignored
/// --test-threads=1 下运行
#[tokio::test]
#[ignore = "env-var test is racy under parallel test execution; \
 run with --ignored --test-threads=1"]
async fn no_proxy_env_does_not_break_loopback() {
    let saved = std::env::var("HTTP_PROXY").ok();
    // SAFETY: edition 2024 把 set_var/remove_var 标为 unsafe(其它线程可能
    // 同时读环境变量)本测试用 #[ignore] 强制 --test-threads=1 单线程
    // 运行,不存在并发读者,操作对外部世界仅留下需还原的 HTTP_PROXY
    // 在断言之前已恢复,符合"无外部观察者读到不一致状态"的安全契约
    unsafe { std::env::set_var("HTTP_PROXY", "http://bogus-proxy.invalid:9") }

    let server = MockServer::start().await;
    let port = mock_server_port(&server);
    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0..)
        .mount(&server)
        .await;

    let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
    let result = client
        .wait_ready(Duration::from_secs(2), Box::new(|| false))
        .await;

    // 还原环境变量再断言,避免断言失败留下脏状态
    // SAFETY: 同上 —— #[ignore] 强制单线程
    unsafe {
        match saved {
            Some(v) => std::env::set_var("HTTP_PROXY", v),
            None => std::env::remove_var("HTTP_PROXY"),
        }
    }

    assert!(
        result.is_ok(),
        "no_proxy must bypass HTTP_PROXY env var: {result:?}"
    );
}
