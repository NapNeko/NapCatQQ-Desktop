//! WebSocket 客户端：对着本机起的真 WebSocket 服务端，验证鉴权头、echo 配对、
//! 事件转发、断线处理和超时。
//!
//! 服务端在整个测试期间一直占着监听端口（监听器随服务任务存活），
//! 不做「绑定 → 释放 → 再绑定」，避免端口被别的进程抢走。

// result_large_err：升级回调的签名由 tungstenite 规定（`Result<Response, ErrorResponse>`），改不了
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::result_large_err
)]

use std::future::Future;
use std::time::Duration;

use futures_util::{FutureExt, SinkExt, StreamExt};
use ncd_onebot::client::{ClientError, WsClient, connect_ws};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http;
use tokio_tungstenite::{WebSocketStream, accept_hdr_async};

type ServerWs = WebSocketStream<TcpStream>;

const CALL_TIMEOUT: Duration = Duration::from_secs(2);

/// 升级阶段服务端的态度
#[derive(Clone, Copy)]
enum Gate {
    Open,
    /// 必须带 `Authorization: Bearer <token>`：缺失回 401，不对回 403
    RequireBearer(&'static str),
    /// 一律用这个状态码拒绝升级
    Reject(u16),
}

fn rejection(status: u16, body: &str) -> ErrorResponse {
    http::Response::builder()
        .status(status)
        .body(Some(body.to_owned()))
        .unwrap()
}

/// 起一个只接一条连接的服务端，返回它的地址和任务句柄。
/// 监听器在服务任务里一直存活到处理函数结束
async fn serve<F, Fut>(gate: Gate, handler: F) -> (String, JoinHandle<()>)
where
    F: FnOnce(ServerWs) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let callback =
            move |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
                match gate {
                    Gate::Open => Ok(response),
                    Gate::Reject(status) => Err(rejection(status, "rejected")),
                    Gate::RequireBearer(token) => {
                        match request
                            .headers()
                            .get("authorization")
                            .map(|v| v.to_str().unwrap())
                        {
                            None => Err(rejection(401, "Unauthorized")),
                            Some(v) if v == format!("Bearer {token}") => Ok(response),
                            Some(_) => Err(rejection(403, "Forbidden")),
                        }
                    }
                }
            };
        let Ok(ws) = accept_hdr_async(stream, callback).await else {
            return;
        };
        handler(ws).await;
    });
    (
        format!("ws://{addr}/api/Debug/ws?adapterName=debug-primary"),
        handle,
    )
}

/// 服务端读下一个 JSON 文本帧（跳过 Ping 等控制帧）
async fn next_json(ws: &mut ServerWs) -> Value {
    loop {
        match ws.next().await {
            Some(Ok(Message::Text(text))) => return serde_json::from_str(&text).unwrap(),
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            other => panic!("服务端期望收到文本帧，实际 {other:?}"),
        }
    }
}

async fn send_json(ws: &mut ServerWs, value: Value) {
    ws.send(Message::Text(value.to_string())).await.unwrap();
}

/// 按请求里的 echo 回一个成功包，`data` 里带回动作名方便客户端核对
fn reply_to(request: &Value) -> Value {
    json!({
        "status": "ok",
        "retcode": 0,
        "data": {"action": request["action"]},
        "message": "",
        "wording": "",
        "echo": request["echo"],
    })
}

/// 给可能永远不结束的等待加个上限，出问题时测试失败而不是挂住
async fn within<T>(fut: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(3), fut)
        .await
        .expect("等待超时")
}

async fn connect(url: &str, events: usize) -> (WsClient, mpsc::Receiver<Value>) {
    let (tx, rx) = mpsc::channel(events);
    let client = connect_ws(url, None, tx).await.unwrap();
    (client, rx)
}

#[tokio::test]
async fn call_sends_action_frame_and_returns_the_reply() {
    let (url, server) = serve(Gate::Open, |mut ws| async move {
        let request = next_json(&mut ws).await;
        assert_eq!(request["action"], "get_login_info");
        assert_eq!(request["params"], json!({"a": 1}));
        let echo = request["echo"].as_str().expect("echo 应是字符串");
        assert!(
            uuid::Uuid::parse_str(echo).is_ok(),
            "echo 应是 uuid：{echo}"
        );
        send_json(&mut ws, reply_to(&request)).await;
        // 保持连接直到客户端走
        while ws.next().await.is_some() {}
    })
    .await;

    let (client, _events) = connect(&url, 8).await;
    let raw = client
        .call("get_login_info", &json!({"a": 1}), CALL_TIMEOUT)
        .await
        .unwrap();
    assert_eq!(raw.value["data"]["action"], "get_login_info");
    assert_eq!(raw.value["retcode"], 0);
    assert_eq!(serde_json::from_str::<Value>(&raw.text).unwrap(), raw.value);
    assert!(!client.is_closed());
    drop(client);
    within(server).await.unwrap();
}

#[tokio::test]
async fn bearer_token_is_sent_in_the_upgrade_header() {
    let (url, server) = serve(Gate::RequireBearer("s3cret"), |mut ws| async move {
        let request = next_json(&mut ws).await;
        send_json(&mut ws, reply_to(&request)).await;
        while ws.next().await.is_some() {}
    })
    .await;

    let (tx, _rx) = mpsc::channel(8);
    let client = connect_ws(&url, Some("s3cret"), tx).await.unwrap();
    client
        .call("get_status", &json!({}), CALL_TIMEOUT)
        .await
        .unwrap();
    drop(client);
    within(server).await.unwrap();
}

#[tokio::test]
async fn missing_token_is_unauthorized_401() {
    let (url, server) = serve(Gate::RequireBearer("s3cret"), |_ws| async {}).await;
    let (tx, _rx) = mpsc::channel(8);
    let err = connect_ws(&url, None, tx).await.unwrap_err();
    assert!(matches!(err, ClientError::Unauthorized(401)), "{err:?}");
    within(server).await.unwrap();
}

#[tokio::test]
async fn wrong_token_is_unauthorized_403() {
    let (url, server) = serve(Gate::RequireBearer("s3cret"), |_ws| async {}).await;
    let (tx, _rx) = mpsc::channel(8);
    let err = connect_ws(&url, Some("wrong"), tx).await.unwrap_err();
    assert!(matches!(err, ClientError::Unauthorized(403)), "{err:?}");
    within(server).await.unwrap();
}

#[tokio::test]
async fn other_upgrade_failures_are_connect_errors() {
    for status in [404u16, 500] {
        let (url, server) = serve(Gate::Reject(status), |_ws| async {}).await;
        let (tx, _rx) = mpsc::channel(8);
        let err = connect_ws(&url, None, tx).await.unwrap_err();
        match err {
            ClientError::Connect(message) => {
                assert!(message.contains(&status.to_string()), "{message}");
            }
            other => panic!("{status} 应是 Connect，实际 {other:?}"),
        }
        within(server).await.unwrap();
    }
}

#[tokio::test]
async fn unreachable_endpoint_is_a_connect_error() {
    // 端口 0 谁也不会监听，连接会立刻失败。不用「绑一个端口再放掉」：
    // Windows 上连一个没人听的本机端口要重试约 2 秒才报错，测试会被拖慢
    let (tx, _rx) = mpsc::channel(8);
    let err = connect_ws("ws://127.0.0.1:0/", None, tx).await.unwrap_err();
    assert!(matches!(err, ClientError::Connect(_)), "{err:?}");
}

#[tokio::test]
async fn malformed_or_unsupported_urls_are_connect_errors() {
    for url in ["not a url", "http://127.0.0.1:1/", ""] {
        let (tx, _rx) = mpsc::channel(8);
        let err = connect_ws(url, None, tx).await.unwrap_err();
        assert!(matches!(err, ClientError::Connect(_)), "{url:?} → {err:?}");
    }
}

#[tokio::test]
async fn concurrent_calls_are_matched_by_echo_when_answered_in_reverse() {
    let (url, server) = serve(Gate::Open, |mut ws| async move {
        let first = next_json(&mut ws).await;
        let second = next_json(&mut ws).await;
        // 后到的先回
        send_json(&mut ws, reply_to(&second)).await;
        send_json(&mut ws, reply_to(&first)).await;
        while ws.next().await.is_some() {}
    })
    .await;

    let (client, _events) = connect(&url, 8).await;
    let other = client.clone();
    let params = json!({});
    let (a, b) = within(async {
        tokio::join!(
            client.call("action_a", &params, CALL_TIMEOUT),
            other.call("action_b", &params, CALL_TIMEOUT),
        )
    })
    .await;
    assert_eq!(a.unwrap().value["data"]["action"], "action_a");
    assert_eq!(b.unwrap().value["data"]["action"], "action_b");
    drop((client, other));
    within(server).await.unwrap();
}

#[tokio::test]
async fn events_are_forwarded_and_replies_are_not() {
    let lifecycle = json!({"post_type": "meta_event", "meta_event_type": "lifecycle", "sub_type": "connect", "self_id": 10001});
    let message = json!({"post_type": "message", "message_type": "group", "message_id": 7, "raw": {"big": "x"}});
    let (lifecycle_out, message_out) = (lifecycle.clone(), message.clone());
    let (url, server) = serve(Gate::Open, move |mut ws| async move {
        send_json(&mut ws, lifecycle_out).await;
        send_json(&mut ws, message_out).await;
        let request = next_json(&mut ws).await;
        send_json(&mut ws, reply_to(&request)).await;
        while ws.next().await.is_some() {}
    })
    .await;

    let (client, mut events) = connect(&url, 8).await;
    client
        .call("get_status", &json!({}), CALL_TIMEOUT)
        .await
        .unwrap();
    assert_eq!(within(events.recv()).await.unwrap(), lifecycle);
    assert_eq!(within(events.recv()).await.unwrap(), message);
    assert!(events.try_recv().is_err(), "回包不应混进事件通道");
    assert_eq!(client.dropped(), 0);
    drop(client);
    within(server).await.unwrap();
}

#[tokio::test]
async fn full_event_channel_drops_and_counts_without_blocking_replies() {
    let (url, server) = serve(Gate::Open, |mut ws| async move {
        for n in 0..4 {
            send_json(&mut ws, json!({"post_type": "message", "message_id": n})).await;
        }
        let request = next_json(&mut ws).await;
        send_json(&mut ws, reply_to(&request)).await;
        while ws.next().await.is_some() {}
    })
    .await;

    // 容量 1 且一直不读：后 3 条必须被丢弃，而回包照样能到
    let (client, mut events) = connect(&url, 1).await;
    client
        .call("get_status", &json!({}), CALL_TIMEOUT)
        .await
        .unwrap();
    assert_eq!(client.dropped(), 3);
    assert_eq!(events.recv().await.unwrap()["message_id"], 0);
    drop(client);
    within(server).await.unwrap();
}

#[tokio::test]
async fn frames_that_are_neither_reply_nor_event_are_ignored() {
    let (url, server) = serve(Gate::Open, |mut ws| async move {
        ws.send(Message::Text("这不是 JSON".to_owned()))
            .await
            .unwrap();
        send_json(&mut ws, json!({"foo": "bar"})).await;
        // 没人在等的 echo（比如超时之后才到的回包）
        send_json(
            &mut ws,
            json!({"status": "ok", "retcode": 0, "echo": "nobody"}),
        )
        .await;
        // echo 不是字符串
        send_json(&mut ws, json!({"status": "ok", "retcode": 0, "echo": 5})).await;
        let request = next_json(&mut ws).await;
        send_json(&mut ws, reply_to(&request)).await;
        while ws.next().await.is_some() {}
    })
    .await;

    let (client, mut events) = connect(&url, 8).await;
    client
        .call("get_status", &json!({}), CALL_TIMEOUT)
        .await
        .unwrap();
    assert!(events.try_recv().is_err());
    assert_eq!(client.dropped(), 0);
    drop(client);
    within(server).await.unwrap();
}

#[tokio::test]
async fn server_close_fails_pending_call_and_resolves_closed() {
    for graceful in [true, false] {
        let (url, server) = serve(Gate::Open, move |mut ws| async move {
            let _request = next_json(&mut ws).await;
            if graceful {
                ws.close(None).await.unwrap();
            }
            // 不优雅的情形：直接丢掉连接，没有 Close 帧
        })
        .await;

        let (client, _events) = connect(&url, 8).await;
        let params = json!({});
        let pending = client.call("get_status", &params, Duration::from_secs(10));
        let err = within(pending).await.unwrap_err();
        assert!(
            matches!(err, ClientError::Closed),
            "graceful={graceful}：{err:?}"
        );
        within(client.closed()).await;
        assert!(client.is_closed());
        // 已关闭后再调用立即失败，不会干等；请求根本没发出去
        let err = client
            .call("get_status", &json!({}), Duration::from_secs(10))
            .await
            .unwrap_err();
        assert!(matches!(err, ClientError::NotSent), "{err:?}");
        within(server).await.unwrap();
    }
}

#[tokio::test]
async fn call_times_out_and_a_late_reply_is_ignored() {
    let (url, server) = serve(Gate::Open, |mut ws| async move {
        let first = next_json(&mut ws).await;
        let second = next_json(&mut ws).await;
        // 第一条的回包拖到第二条来了才发
        send_json(&mut ws, reply_to(&first)).await;
        send_json(&mut ws, reply_to(&second)).await;
        while ws.next().await.is_some() {}
    })
    .await;

    let (client, mut events) = connect(&url, 8).await;
    let started = std::time::Instant::now();
    let err = client
        .call("slow", &json!({}), Duration::from_millis(100))
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Timeout), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!client.is_closed(), "单次调用超时不应拖垮连接");

    let raw = client.call("fast", &json!({}), CALL_TIMEOUT).await.unwrap();
    assert_eq!(raw.value["data"]["action"], "fast");
    assert!(events.try_recv().is_err(), "晚到的回包不该被当成事件");
    drop(client);
    within(server).await.unwrap();
}

#[tokio::test]
async fn local_close_fails_pending_calls_and_closes_the_socket() {
    let (received_tx, received_rx) = oneshot::channel();
    let (url, server) = serve(Gate::Open, |mut ws| async move {
        let _request = next_json(&mut ws).await;
        received_tx.send(()).unwrap();
        // 一直不回，直到客户端把连接关掉
        while ws.next().await.is_some() {}
    })
    .await;

    let (client, _events) = connect(&url, 8).await;
    let caller = client.clone();
    let pending = tokio::spawn(async move {
        caller
            .call("never_answered", &json!({}), Duration::from_secs(10))
            .await
    });
    within(received_rx).await.unwrap();
    client.close();
    assert!(client.is_closed());
    let err = within(pending).await.unwrap().unwrap_err();
    assert!(matches!(err, ClientError::Closed), "{err:?}");
    within(client.closed()).await;
    // 本端关闭后服务端应看到连接结束
    within(server).await.unwrap();
}

#[tokio::test]
async fn dropping_the_last_handle_closes_the_socket() {
    let (url, server) = serve(Gate::Open, |mut ws| async move {
        let request = next_json(&mut ws).await;
        send_json(&mut ws, reply_to(&request)).await;
        // 客户端全部句柄丢掉后，这里应读到连接结束
        while ws.next().await.is_some() {}
    })
    .await;

    let (client, _events) = connect(&url, 8).await;
    let spare = client.clone();
    drop(client);
    // 还有一个句柄活着：连接照常可用
    assert!(!spare.is_closed());
    spare
        .call("get_status", &json!({}), CALL_TIMEOUT)
        .await
        .unwrap();
    assert!(!server.is_finished());
    drop(spare);
    within(server).await.unwrap();
}

/// 调用方在帧还排在写队列里时就放弃了（超时和取消走的是同一条路：`call` 的 future 被丢弃，
/// 登记被摘掉），这一帧不该再发给上游。
///
/// 用「只轮询一次就丢弃」来复现，而不是真等一个很短的超时：单线程运行时下，
/// `call` 里同步执行到首次挂起（此时帧已入队）、被丢弃，整个过程写任务都没机会运行，
/// 顺序是确定的；换成真实的微小超时，定时器精度会让写任务有可能抢在超时之前把帧发出去，测试就不稳了
#[tokio::test]
async fn frame_still_queued_when_the_caller_gave_up_is_not_sent() {
    let (url, server) = serve(Gate::Open, |mut ws| async move {
        // 第一个到达的必须是 fresh；stale 若被发出，它排在 fresh 前面，这里会先读到它
        let first = next_json(&mut ws).await;
        assert_eq!(first["action"], "fresh", "已被放弃的调用不该发出：{first}");
        send_json(&mut ws, reply_to(&first)).await;
        // 客户端走之前不该再有任何请求帧
        while let Some(message) = ws.next().await {
            if let Ok(Message::Text(text)) = message {
                panic!("多出了不该有的请求帧：{text}");
            }
        }
    })
    .await;

    let (client, _events) = connect(&url, 8).await;
    let params = json!({});
    let stale = client
        .call("stale", &params, Duration::from_secs(10))
        .now_or_never();
    assert!(stale.is_none(), "stale 调用应停在等回包上");

    let raw = client.call("fresh", &params, CALL_TIMEOUT).await.unwrap();
    assert_eq!(raw.value["data"]["action"], "fresh");
    drop(client);
    within(server).await.unwrap();
}
