//! HTTP 动作客户端：对着 wiremock 走一遍成功、鉴权、错误码、非 JSON、超时。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ncd_onebot::client::{ClientError, HttpActionClient};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TIMEOUT: Duration = Duration::from_secs(2);

fn client(server: &MockServer, token: Option<&str>) -> HttpActionClient {
    HttpActionClient::new(&server.uri(), token.map(str::to_owned)).unwrap()
}

#[tokio::test]
async fn ok_reply_is_returned_with_text_and_value() {
    let server = MockServer::start().await;
    let reply = json!({"status": "ok", "retcode": 0, "data": {"user_id": 10001}, "message": ""});
    Mock::given(method("POST"))
        .and(path("/get_login_info"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(&reply))
        .expect(1)
        .mount(&server)
        .await;

    let raw = client(&server, None)
        .call("get_login_info", &json!({}), TIMEOUT)
        .await
        .unwrap();
    assert_eq!(raw.value, reply);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&raw.text).unwrap(),
        reply
    );
}

#[tokio::test]
async fn params_are_sent_as_the_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/send_group_msg"))
        .and(body_json(json!({"group_id": 123, "message": "你好"})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"status": "ok", "retcode": 0})),
        )
        .expect(1)
        .mount(&server)
        .await;

    client(&server, None)
        .call(
            "send_group_msg",
            &json!({"group_id": 123, "message": "你好"}),
            TIMEOUT,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn ob11_failure_is_still_a_reply_not_an_error() {
    let server = MockServer::start().await;
    let reply = json!({"status": "failed", "retcode": 1400, "data": null, "wording": "参数错误"});
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&reply))
        .mount(&server)
        .await;

    let raw = client(&server, None)
        .call("send_msg", &json!({}), TIMEOUT)
        .await
        .unwrap();
    assert_eq!(raw.value, reply);
}

#[tokio::test]
async fn bearer_token_is_sent_when_configured() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(header("authorization", "Bearer s3cret"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"status": "ok", "retcode": 0})),
        )
        .expect(1)
        .mount(&server)
        .await;

    client(&server, Some("s3cret"))
        .call("get_status", &json!({}), TIMEOUT)
        .await
        .unwrap();
}

#[tokio::test]
async fn no_authorization_header_without_a_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"status": "ok", "retcode": 0})),
        )
        .mount(&server)
        .await;

    client(&server, None)
        .call("get_status", &json!({}), TIMEOUT)
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].headers.contains_key("authorization"));
}

#[tokio::test]
async fn unauthorized_statuses_map_to_unauthorized() {
    for status in [401u16, 403] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string("nope"))
            .mount(&server)
            .await;
        let err = client(&server, Some("bad"))
            .call("get_status", &json!({}), TIMEOUT)
            .await
            .unwrap_err();
        assert!(
            matches!(err, ClientError::Unauthorized(s) if s == status),
            "{status} 应映射为 Unauthorized，实际 {err:?}"
        );
    }
}

#[tokio::test]
async fn other_non_2xx_is_status_with_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;
    let err = client(&server, None)
        .call("get_status", &json!({}), TIMEOUT)
        .await
        .unwrap_err();
    match err {
        ClientError::Status { status, body } => {
            assert_eq!(status, 500);
            assert_eq!(body, "boom");
        }
        other => panic!("应是 Status，实际 {other:?}"),
    }
}

#[tokio::test]
async fn unknown_action_404_is_status() {
    // wiremock 对没配的路由默认回 404
    let server = MockServer::start().await;
    let err = client(&server, None)
        .call("no_such_action", &json!({}), TIMEOUT)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::Status { status: 404, .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn non_json_body_is_protocol_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>hi</html>"))
        .mount(&server)
        .await;
    let err = client(&server, None)
        .call("get_status", &json!({}), TIMEOUT)
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Protocol(_)), "{err:?}");
}

#[tokio::test]
async fn json_but_not_an_object_is_protocol_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([1, 2, 3])))
        .mount(&server)
        .await;
    let err = client(&server, None)
        .call("get_status", &json!({}), TIMEOUT)
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Protocol(_)), "{err:?}");
}

#[tokio::test]
async fn slow_response_times_out() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": "ok", "retcode": 0}))
                .set_delay(Duration::from_secs(10)),
        )
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let err = client(&server, None)
        .call("get_status", &json!({}), Duration::from_millis(150))
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Timeout), "{err:?}");
    // 延迟远长于超时、判定线又放得很宽：只要证明没有等完延迟，CI 机器再慢也不会误报
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "应在超时点返回，而不是等完延迟：{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn unreachable_endpoint_is_connect_error() {
    // 端口 0 谁也不会监听，连接会立刻失败。不用「绑一个端口再放掉」：
    // Windows 上连一个没人听的本机端口要重试约 2 秒才报错，测试会被拖慢
    let client = HttpActionClient::new("http://127.0.0.1:0", None).unwrap();
    let err = client
        .call("get_status", &json!({}), TIMEOUT)
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Connect(_)), "{err:?}");
}

#[tokio::test]
async fn empty_action_is_rejected_before_sending() {
    let server = MockServer::start().await;
    let err = client(&server, None)
        .call("", &json!({}), TIMEOUT)
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Protocol(_)), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}
