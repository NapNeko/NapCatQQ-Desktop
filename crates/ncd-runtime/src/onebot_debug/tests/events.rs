//! 事件接收器端到端：订阅、补发、重连、SnowLuma 事件流、NapCat 调试适配器、生命周期、清扫、
//! 编号连续，以及调用写进事件流和历史。
//!
//! 走真网络的用一个本机小服务端扮演上游（HTTP 登录 / 建适配器、SSE、WS 都在同一个口上），
//! 或者 wiremock；不碰网络的用暂停的时钟，30 分钟的清扫一眨眼就过去。

use std::sync::atomic::AtomicBool;

use ncd_domain::daemon_state::SnowLumaLoginState;
use ncd_domain::onebot_debug::{
    DebugCallRecord, DebugEvent, DebugEventBatch, DebugEventBody, DebugHistoryQuery,
    DebugReceiverState, DebugSubscribeResponse,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;

use super::*;
use crate::events::{BroadcastEventBus, DomainEvent, EventBus};

// ─── 记录收到什么的窗口 ──────────────────────────────────────────────────────

#[derive(Default)]
pub(super) struct RecordingSink {
    batches: StdMutex<Vec<DebugEventBatch>>,
    /// 置位后 send 返回 false，模拟窗口已经关了
    gone: AtomicBool,
}

impl DebugEventSink for RecordingSink {
    fn send(&self, batch: &DebugEventBatch) -> bool {
        if self.gone.load(Ordering::SeqCst) {
            return false;
        }
        self.batches.lock().unwrap().push(batch.clone());
        true
    }
}

impl RecordingSink {
    pub(super) fn events(&self) -> Vec<DebugEvent> {
        self.batches
            .lock()
            .unwrap()
            .iter()
            .flat_map(|b| b.events.clone())
            .collect()
    }

    fn batch_count(&self) -> usize {
        self.batches.lock().unwrap().len()
    }

    fn ob11(&self) -> Vec<(u64, Value)> {
        self.events()
            .into_iter()
            .filter_map(|e| match e.body {
                DebugEventBody::Ob11 { payload } => Some((e.seq, payload)),
                _ => None,
            })
            .collect()
    }

    pub(super) fn calls(&self) -> Vec<DebugCallRecord> {
        self.events()
            .into_iter()
            .filter_map(|e| match e.body {
                DebugEventBody::Call { record } => Some(record),
                _ => None,
            })
            .collect()
    }

    pub(super) fn states(&self) -> Vec<DebugReceiverState> {
        self.events()
            .into_iter()
            .filter_map(|e| match e.body {
                DebugEventBody::Receiver { state, .. } => Some(state),
                _ => None,
            })
            .collect()
    }

    pub(super) fn connected_count(&self) -> usize {
        self.states()
            .iter()
            .filter(|s| **s == DebugReceiverState::Connected)
            .count()
    }

    /// 装着 Ob11 事件的批次数
    fn ob11_batches(&self) -> usize {
        self.batches
            .lock()
            .unwrap()
            .iter()
            .filter(|b| {
                b.events
                    .iter()
                    .any(|e| matches!(e.body, DebugEventBody::Ob11 { .. }))
            })
            .count()
    }
}

fn seqs(events: &[DebugEvent]) -> Vec<u64> {
    events.iter().map(|e| e.seq).collect()
}

fn strictly_increasing(seqs: &[u64]) -> bool {
    seqs.windows(2).all(|w| w[0] < w[1])
}

/// 等到条件成立；超过 5 秒（暂停时钟下是虚拟的 5 秒）算失败
pub(super) async fn wait_until(what: &str, cond: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(tokio::time::Instant::now() < deadline, "等不到：{what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub(super) async fn subscribe(
    h: &Harness,
    bot: &BotId,
    source: DebugChannelId,
) -> (Arc<RecordingSink>, DebugSubscribeResponse) {
    let sink = Arc::new(RecordingSink::default());
    let response = h
        .manager
        .subscribe(
            bot.as_str(),
            source,
            Arc::clone(&sink) as Arc<dyn DebugEventSink>,
        )
        .await
        .unwrap();
    (sink, response)
}

// ─── 扮演上游的本机小服务端 ──────────────────────────────────────────────────

/// 推给流式连接的「断开」指令
const DROP_STREAMS: &str = "\u{0}drop";

/// 一个口上同时扮演 NapCat / SnowLuma WebUI 的登录与建适配器、SnowLuma 的 SSE 事件流、
/// NapCat 调试适配器和 OneBot 服务的 WS。推送的帧同时发给所有开着的流式连接
pub(super) struct Upstream {
    pub(super) port: u16,
    push: broadcast::Sender<String>,
    streams: Arc<AtomicUsize>,
    /// 已经结束的 WS 连接数（不管是哪头关的）
    ws_closed: Arc<AtomicUsize>,
    paths: Arc<StdMutex<Vec<String>>>,
}

impl Upstream {
    pub(super) async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (push, _) = broadcast::channel(64);
        let streams = Arc::new(AtomicUsize::new(0));
        let ws_closed = Arc::new(AtomicUsize::new(0));
        let paths = Arc::new(StdMutex::new(Vec::new()));
        let (tx, count, closed, seen) = (
            push.clone(),
            Arc::clone(&streams),
            Arc::clone(&ws_closed),
            Arc::clone(&paths),
        );
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(serve(
                    stream,
                    tx.clone(),
                    Arc::clone(&count),
                    Arc::clone(&closed),
                    Arc::clone(&seen),
                ));
            }
        });
        Self {
            port,
            push,
            streams,
            ws_closed,
            paths,
        }
    }

    pub(super) fn ws_closed(&self) -> usize {
        self.ws_closed.load(Ordering::SeqCst)
    }

    pub(super) fn push(&self, frame: Value) {
        let _ = self.push.send(frame.to_string());
    }

    fn drop_streams(&self) {
        let _ = self.push.send(DROP_STREAMS.to_owned());
    }

    pub(super) fn streams(&self) -> usize {
        self.streams.load(Ordering::SeqCst)
    }

    fn paths(&self) -> Vec<String> {
        self.paths.lock().unwrap().clone()
    }
}

struct RequestHead {
    method: String,
    path: String,
    headers: HashMap<String, String>,
}

async fn read_head(stream: &mut TcpStream) -> Option<RequestHead> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_owned();
    let path = first.next()?.to_owned();
    let headers: HashMap<String, String> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    // 把请求体读完再回：不读完就关连接，对面可能收到 RST 而不是回包
    let length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut have = buf.len() - end - 4;
    while have < length {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        have += n;
    }
    Some(RequestHead {
        method,
        path,
        headers,
    })
}

async fn reply_json(stream: &mut TcpStream, status: &str, body: Value) {
    let body = body.to_string();
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn serve(
    mut stream: TcpStream,
    push: broadcast::Sender<String>,
    streams: Arc<AtomicUsize>,
    ws_closed: Arc<AtomicUsize>,
    paths: Arc<StdMutex<Vec<String>>>,
) {
    let Some(head) = read_head(&mut stream).await else {
        return;
    };
    paths.lock().unwrap().push(head.path.clone());
    if head
        .headers
        .get("upgrade")
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
    {
        let Some(key) = head.headers.get("sec-websocket-key") else {
            return;
        };
        // 先订阅再回 101：客户端看到连上之后推的帧一定收得到
        let rx = push.subscribe();
        let accept = derive_accept_key(key.as_bytes());
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        );
        if stream.write_all(response.as_bytes()).await.is_err() {
            return;
        }
        streams.fetch_add(1, Ordering::SeqCst);
        let ws = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
        serve_ws(ws, rx).await;
        ws_closed.fetch_add(1, Ordering::SeqCst);
        return;
    }
    match (head.method.as_str(), head.path.as_str()) {
        ("POST", "/api/auth/login") => {
            reply_json(
                &mut stream,
                "200 OK",
                json!({"code": 0, "message": "success", "data": {"Credential": "cred"}}),
            )
            .await;
        }
        ("POST", "/api/Debug/create") => {
            reply_json(
                &mut stream,
                "200 OK",
                json!({"code": 0, "message": "success",
                       "data": {"adapterName": "debug-primary", "token": "adapter-tok"}}),
            )
            .await;
        }
        ("POST", "/api/login") => {
            reply_json(
                &mut stream,
                "200 OK",
                json!({"success": true, "token": "t"}),
            )
            .await;
        }
        ("GET", "/api/debug/stream") => {
            let mut rx = push.subscribe();
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).await.is_err() {
                return;
            }
            streams.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(b"data: {\"kind\":\"ready\"}\n\n").await;
            loop {
                let frame = match rx.recv().await {
                    Ok(frame) => frame,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                if frame == DROP_STREAMS {
                    return;
                }
                let line = format!("data: {frame}\n\n");
                if stream.write_all(line.as_bytes()).await.is_err() {
                    return;
                }
            }
        }
        _ => reply_json(&mut stream, "404 Not Found", json!({})).await,
    }
}

async fn serve_ws(mut ws: WebSocketStream<TcpStream>, mut rx: broadcast::Receiver<String>) {
    loop {
        tokio::select! {
            frame = rx.recv() => match frame {
                Ok(frame) if frame == DROP_STREAMS => return,
                Ok(frame) => {
                    if ws.send(Message::Text(frame)).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            },
            incoming = ws.next() => match incoming {
                // 动作调用：原样回一个成功回包
                Some(Ok(Message::Text(text))) => {
                    let req: Value = serde_json::from_str(&text).unwrap_or_default();
                    let mut reply = ob11_ok(json!({}));
                    reply["echo"] = req["echo"].clone();
                    if ws.send(Message::Text(reply.to_string())).await.is_err() {
                        return;
                    }
                }
                Some(Ok(_)) => {}
                _ => return,
            },
        }
    }
}

pub(super) fn group_message(id: i64) -> Value {
    json!({"post_type": "message", "message_type": "group", "message_id": id, "group_id": 1,
           "user_id": 2, "message": [{"type": "text", "data": {"text": format!("m{id}")}}]})
}

pub(super) fn event_ws_bot(qq: u64, port: u16) -> BotConfig {
    let mut config = local_bot(qq, BackendType::NapCat);
    config.connect.websocket_servers = vec![ws_server("ev", "127.0.0.1", port, WsRole::Event)];
    config
}

// ─── OneBot WS 服务当来源 ────────────────────────────────────────────────────

#[tokio::test]
async fn chat_feature_releases_receivers_and_restores_background_preferences() {
    use ncd_domain::chat_desktop::ChatAccountPreference;

    let h = harness();
    let up = Upstream::start().await;
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    h.bots.set_napcat(&bot, up.port);
    let chat = crate::chat::ChatManager::new(
        Arc::clone(&h.bots) as Arc<dyn DebugBotPort>,
        Arc::new(LocalOnlyHostResolver::new(
            Arc::clone(&h.host) as Arc<dyn Host>
        )),
        h.data.path().to_path_buf(),
    );
    let preference = ChatAccountPreference {
        bot_id: bot.as_str().into(),
        self_id: "10001".into(),
        enabled: true,
        background: true,
        tray: true,
        ..Default::default()
    };
    let (debug, _) = subscribe(&h, &bot, DebugChannelId::Auto).await;
    chat.set_preference(preference.clone()).await.unwrap();
    let viewer = Arc::new(RecordingSink::default());
    chat.subscribe(bot.as_str(), Arc::clone(&viewer) as Arc<dyn DebugEventSink>)
        .await
        .unwrap();
    wait_until("聊天和调试台连接", || up.streams() == 2).await;
    up.push(group_message(1));
    wait_until("聊天收到消息", || viewer.ob11().len() == 1).await;

    chat.set_enabled(false).await;
    wait_until("关闭聊天后调试连接继续运行", || {
        up.ws_closed() == 1
    })
    .await;
    assert!(chat.desktop_status().await.accounts.is_empty());
    assert_eq!(
        chat.subscribe(bot.as_str(), Arc::clone(&viewer) as Arc<dyn DebugEventSink>)
            .await
            .unwrap_err(),
        DebugError::FeatureDisabled
    );
    assert!(matches!(
        chat.call(request(
            "disabled",
            &bot,
            DebugChannelId::Auto,
            "get_login_info"
        ))
        .await
        .result,
        DebugCallResult::Err {
            error: DebugError::FeatureDisabled
        }
    ));
    chat.set_preference(preference.clone()).await.unwrap();
    assert_eq!(up.streams(), 2, "关闭期间没有新建连接");
    let archive = chat
        .load_archive(bot.as_str().into(), "10001".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(archive.messages.len(), 1);

    up.push(group_message(2));
    wait_until("调试台仍收到消息", || debug.ob11().len() == 2).await;
    assert_eq!(viewer.ob11().len(), 1, "旧聊天页面订阅已移除");

    chat.set_enabled(true).await;
    wait_until("重新开启恢复后台连接", || up.streams() == 3).await;
    assert_eq!(
        chat.desktop_status().await.accounts[0].preference,
        preference
    );
    assert!(h.manager.is_enabled());
    chat.shutdown().await;
    h.manager.close_all().await;
}

#[tokio::test]
async fn ws_events_reach_every_viewer_once_and_late_viewers_get_the_backlog() {
    let h = harness();
    let up = Upstream::start().await;
    let bot = h.bots.add(event_ws_bot(10_001, up.port), true);

    let (first, response) = subscribe(&h, &bot, ws_id("ev")).await;
    assert_eq!(response.receiver.source, ws_id("ev"));
    assert_eq!(response.receiver.buffered, 0);
    assert_eq!(response.receiver.viewers, 1);
    assert_eq!(first.batch_count(), 0, "新接收器没有可补发的");
    assert!(!response.subscription_id.is_empty());

    wait_until("第一个窗口看到已连上", || {
        first.connected_count() == 1
    })
    .await;
    for id in 1..=3 {
        up.push(group_message(id));
    }
    wait_until("三条事件", || first.ob11().len() == 3).await;
    let got = first.ob11();
    let ids: Vec<i64> = got
        .iter()
        .map(|(_, p)| p["message_id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [1, 2, 3]);
    assert!(strictly_increasing(
        &got.iter().map(|(s, _)| *s).collect::<Vec<_>>()
    ));
    assert!(
        first.ob11_batches() <= 2,
        "应攒成至多两批：{}",
        first.ob11_batches()
    );

    // 第二个窗口：已有的三条作为补发先到，再加入实时推送
    let (second, response) = subscribe(&h, &bot, ws_id("ev")).await;
    assert_eq!(response.receiver.viewers, 2);
    assert!(response.receiver.buffered >= 3);
    assert_eq!(second.ob11().len(), 3, "补发里应有已收到的三条");

    up.push(group_message(4));
    wait_until("两个窗口都收到第四条", || {
        first.ob11().len() == 4 && second.ob11().len() == 4
    })
    .await;
    let second_seqs = seqs(&second.events());
    assert!(
        strictly_increasing(&second_seqs),
        "不重不乱：{second_seqs:?}"
    );
    let ob11_seqs = |sink: &RecordingSink| sink.ob11().iter().map(|(s, _)| *s).collect::<Vec<_>>();
    assert_eq!(ob11_seqs(&first), ob11_seqs(&second));
    assert_eq!(up.streams(), 1, "一个 Bot 只有一条事件连接");

    // 补拉与接收器列表
    let tail = h.manager.read_events(bot.as_str(), got[1].0, 100).await;
    assert!(tail.iter().all(|e| e.seq > got[1].0));
    assert!(!tail.is_empty());
    let receivers = h.manager.receivers().await;
    assert_eq!(receivers.len(), 1);
    assert_eq!(receivers[0].viewers, 2);
    assert_eq!(receivers[0].state, DebugReceiverState::Connected);

    h.manager.unsubscribe(&response.subscription_id).await;
    assert_eq!(h.manager.receivers().await[0].viewers, 1);

    // 显式换来源：缓冲保留、编号接着走
    let last = *seqs(&first.events()).last().unwrap();
    let (_third, response) = subscribe(&h, &bot, DebugChannelId::Internal).await;
    assert_eq!(response.receiver.source, DebugChannelId::Internal);
    assert_eq!(response.receiver.state, DebugReceiverState::Connecting);
    assert!(response.receiver.buffered >= 4, "换来源不丢缓冲");
    wait_until("换来源后推了新的连接状态", || {
        seqs(&first.events()).last().is_some_and(|s| *s > last)
    })
    .await;
}

#[tokio::test]
async fn a_dropped_connection_reconnects_and_reports_the_gap() {
    let h = harness();
    let up = Upstream::start().await;
    let bot = h.bots.add(event_ws_bot(10_001, up.port), true);
    let (sink, _) = subscribe(&h, &bot, ws_id("ev")).await;
    wait_until("连上", || sink.connected_count() == 1).await;

    up.drop_streams();
    // 第一次重试在 1 秒后
    wait_until("重新连上", || sink.connected_count() == 2).await;
    assert_eq!(up.streams(), 2);

    let bodies: Vec<DebugEventBody> = sink.events().into_iter().map(|e| e.body).collect();
    let reconnecting = bodies
        .iter()
        .position(|b| {
            matches!(
                b,
                DebugEventBody::Receiver {
                    state: DebugReceiverState::Reconnecting {
                        attempt: 1,
                        retry_in_ms: 1000
                    },
                    ..
                }
            )
        })
        .expect("应有一条「1 秒后重连」");
    let connected_again = bodies
        .iter()
        .rposition(|b| {
            matches!(
                b,
                DebugEventBody::Receiver {
                    state: DebugReceiverState::Connected,
                    ..
                }
            )
        })
        .unwrap();
    assert!(reconnecting < connected_again);
    match &bodies[connected_again + 1..] {
        [DebugEventBody::Gap { from_ms, to_ms }, ..] => assert!(from_ms <= to_ms),
        other => panic!("重新连上之后应紧跟一条 Gap：{other:?}"),
    }

    // 新连接照常收；NapCat 的 OneBot WS 服务推的事件同样去掉 raw
    let mut message = group_message(9);
    message["raw"] = json!({"elements": ["很大的一块"]});
    up.push(message);
    wait_until("重连后的事件", || sink.ob11().len() == 1).await;
    assert!(sink.ob11()[0].1.get("raw").is_none());
    assert_eq!(sink.ob11()[0].1["message_id"], 9);

    // 连上没撑满 5 秒又断：退避不归零，下一次等 2 秒
    up.drop_streams();
    wait_until("第二次断线后的重连等待", || {
        sink.states().iter().any(|s| {
            *s == DebugReceiverState::Reconnecting {
                attempt: 2,
                retry_in_ms: 2000,
            }
        })
    })
    .await;
}

// ─── SnowLuma 事件流 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn snowluma_stream_keeps_this_account_and_skips_our_own_calls() {
    let h = harness();
    let bot = h.bots.add(local_bot(20_002, BackendType::SnowLuma), true);
    let webui = MockServer::start().await;
    h.bots.set_snowluma(&bot, port_of(&webui));
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"success": true, "token": "t"})),
        )
        .mount(&webui)
        .await;
    let frames = [
        json!({"kind": "ready"}),
        json!({"kind": "event", "uin": "20002", "event": group_message(1)}),
        json!({"kind": "event", "uin": "30003", "event": group_message(2)}),
        // 调试台自己发的，参数键序和我们登记时不同
        json!({"kind": "action", "uin": "20002", "action": "send_msg",
               "params": {"a": 1, "b": 2}, "response": {"status": "ok", "retcode": 0}, "ms": 3}),
        // 别人（插件）发的
        json!({"kind": "action", "uin": 20002, "action": "send_group_msg",
               "params": {"group_id": 1}, "response": {"status": "ok", "retcode": 0, "data": {"message_id": 5}}, "ms": 7}),
        json!({"kind": "action", "uin": "30003", "action": "get_status",
               "params": {}, "response": {"status": "ok", "retcode": 0}, "ms": 1}),
        json!({"kind": "dropped", "count": 3}),
    ];
    let mut body = String::new();
    for frame in &frames {
        body.push_str(&format!("data: {frame}\n\n"));
        body.push_str(": heartbeat\n\n");
    }
    Mock::given(method("GET"))
        .and(path("/api/debug/stream"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body),
        )
        // 之后的重连拿到 404（上游太老）就停，不会把同一段再收一遍
        .up_to_n_times(1)
        .mount(&webui)
        .await;

    h.manager
        .note_own_call(&bot, "send_msg", &json!({"b": 2, "a": 1}));
    let (sink, response) = subscribe(&h, &bot, DebugChannelId::Auto).await;
    assert_eq!(response.receiver.source, DebugChannelId::Internal);
    wait_until("流里的丢弃计数", || {
        sink.events()
            .iter()
            .any(|e| matches!(e.body, DebugEventBody::Dropped { count: 3 }))
    })
    .await;

    let ob11 = sink.ob11();
    assert_eq!(ob11.len(), 1, "只要这个账号的事件：{ob11:?}");
    assert_eq!(ob11[0].1["message_id"], 1);
    let calls = sink.calls();
    assert_eq!(calls.len(), 1, "自己发的那次被认出来跳过：{calls:?}");
    assert_eq!(calls[0].action, "send_group_msg");
    assert_eq!(calls[0].origin, DebugCallOrigin::Other);
    assert_eq!(calls[0].request_id, None);
    assert_eq!(calls[0].ok, Some(true));
    assert_eq!(calls[0].message_id, Some(5));
    assert_eq!(calls[0].elapsed_ms, Some(7));

    // 流结束后重连遇到 404：写明原因停下，不再重试
    wait_until("上游太老时停下", || {
        sink.states()
            .iter()
            .any(|s| matches!(s, DebugReceiverState::Stopped { .. }))
    })
    .await;
    let receivers = h.manager.receivers().await;
    assert!(matches!(
        &receivers[0].state,
        DebugReceiverState::Stopped { reason } if reason.contains("太老")
    ));
}

#[tokio::test]
async fn snowluma_login_reconnects_the_stream_without_a_gap() {
    let h = harness();
    let up = Upstream::start().await;
    let bot = h.bots.add(local_bot(20_002, BackendType::SnowLuma), true);
    h.bots.set_snowluma(&bot, up.port);
    let (sink, _) = subscribe(&h, &bot, DebugChannelId::Internal).await;
    wait_until("连上事件流", || {
        sink.connected_count() == 1 && up.streams() == 1
    })
    .await;

    // 登录和「检测到 QQ 号」几乎同时到：只重连一次
    h.manager
        .on_bot_event(&DomainEvent::SnowLumaLoginStateChanged {
            bot_id: bot.clone(),
            state: SnowLumaLoginState::LoggedIn,
        })
        .await;
    h.manager
        .on_bot_event(&DomainEvent::SnowLumaUinDetected {
            bot_id: bot.clone(),
            uin: "20002".into(),
        })
        .await;
    wait_until("登录后重连", || {
        up.streams() == 2 && sink.connected_count() == 2
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(up.streams(), 2, "两次要求合成一次重连");
    let bodies: Vec<DebugEventBody> = sink.events().into_iter().map(|e| e.body).collect();
    assert!(
        !bodies.iter().any(|b| matches!(
            b,
            DebugEventBody::Gap { .. }
                | DebugEventBody::Receiver {
                    state: DebugReceiverState::Reconnecting { .. },
                    ..
                }
        )),
        "主动重连不算断线：{bodies:?}"
    );

    up.push(json!({"kind": "event", "uin": "20002", "event": group_message(1)}));
    wait_until("新连接上的事件", || sink.ob11().len() == 1).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(sink.ob11().len(), 1, "旧连接已经断开，事件只到一次");
}

// ─── NapCat 调试适配器 ───────────────────────────────────────────────────────

#[tokio::test]
async fn napcat_adapter_events_arrive_without_raw() {
    let h = harness();
    let up = Upstream::start().await;
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    h.bots.set_napcat(&bot, up.port);
    let (sink, response) = subscribe(&h, &bot, DebugChannelId::Auto).await;
    assert_eq!(response.receiver.source, DebugChannelId::Internal);
    wait_until("连上调试适配器", || sink.connected_count() == 1).await;
    assert!(
        up.paths()
            .iter()
            .any(|p| p.starts_with("/api/Debug/ws?") && p.contains("token=adapter-tok")),
        "WS 应带建适配器拿到的 token：{:?}",
        up.paths()
    );

    up.push(
        json!({"post_type": "meta_event", "meta_event_type": "lifecycle", "sub_type": "connect"}),
    );
    let mut message = group_message(1);
    message["raw"] = json!({"elements": ["很大的一块"]});
    up.push(message);
    wait_until("两条事件", || sink.ob11().len() == 2).await;
    let ob11 = sink.ob11();
    assert!(ob11.iter().all(|(_, p)| p.get("raw").is_none()), "{ob11:?}");
    assert_eq!(ob11[1].1["message_id"], 1);
    assert_eq!(ob11[1].1["message"][0]["data"]["text"], "m1");
    assert_eq!(
        channel(&h.manager, &bot, &DebugChannelId::Internal)
            .await
            .status,
        DebugChannelStatus::Available
    );
}

// ─── 生命周期、清扫、编号 ────────────────────────────────────────────────────

/// 一个收不到事件的 SnowLuma Bot：WebUI 端点一直没就绪，接收器只会反复重试，不碰网络
fn unready_bot(h: &Harness) -> BotId {
    h.bots.add(local_bot(20_002, BackendType::SnowLuma), true)
}

#[tokio::test(start_paused = true)]
async fn bot_stop_stops_the_receiver_with_a_reason() {
    let h = harness();
    let bot = unready_bot(&h);
    let bus = Arc::new(BroadcastEventBus::default());
    tokio::spawn(
        Arc::clone(&h.manager).run_bot_event_listener(Arc::clone(&bus) as Arc<dyn EventBus>),
    );
    let (sink, _) = subscribe(&h, &bot, DebugChannelId::Internal).await;
    // 会话里留一点东西，看它被一起丢掉
    h.manager.list_channels(bot.as_str()).await.unwrap();
    h.manager
        .test_channel(bot.as_str(), DebugChannelId::Internal)
        .await
        .unwrap();
    assert!(h.manager.sessions.lock().await.contains_key(&bot));

    let mut stopped = BotActorSnapshot::new(bot.clone());
    stopped.state = BotActorState::Stopped;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    // 监听任务第一次跑起来才订阅总线，早发的事件没人收：发到它停为止
    while !h.manager.receivers().await.is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "Bot 停了接收器应跟着停"
        );
        bus.publish(DomainEvent::BotStateChanged {
            snapshot: stopped.clone(),
            reason: None,
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        sink.states().last(),
        Some(&DebugReceiverState::Stopped {
            reason: "Bot 已停止".into()
        })
    );
    assert!(!h.manager.sessions.lock().await.contains_key(&bot));
    assert!(h.manager.read_events(bot.as_str(), 0, 100).await.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_gone_sink_is_dropped_and_an_unwatched_receiver_is_swept_after_30_minutes() {
    let h = harness();
    let bot = unready_bot(&h);
    tokio::spawn(Arc::clone(&h.manager).run_idle_sweeper());
    let sink = Arc::new(RecordingSink::default());
    sink.gone.store(true, Ordering::SeqCst);
    let response = h
        .manager
        .subscribe(
            bot.as_str(),
            DebugChannelId::Internal,
            Arc::clone(&sink) as Arc<dyn DebugEventSink>,
        )
        .await
        .unwrap();
    assert_eq!(response.receiver.viewers, 1);

    // 第一次推送时发现窗口已经走了
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(h.manager.receivers().await[0].viewers, 0);

    tokio::time::sleep(Duration::from_secs(29 * 60)).await;
    assert_eq!(h.manager.receivers().await.len(), 1, "不满 30 分钟不清");
    tokio::time::sleep(Duration::from_secs(2 * 60)).await;
    assert!(
        h.manager.receivers().await.is_empty(),
        "30 分钟没人看应停掉"
    );

    // 有人看着的不清
    let (_viewer, _) = subscribe(&h, &bot, DebugChannelId::Internal).await;
    tokio::time::sleep(Duration::from_secs(40 * 60)).await;
    assert_eq!(h.manager.receivers().await.len(), 1);
}

/// 某个窗口页面开的出口：网页重载后它照样 send 成功，只能靠页面加载钩子摘
struct PageSink {
    page: &'static str,
    opened_at: std::time::Instant,
}

impl DebugEventSink for PageSink {
    fn send(&self, _batch: &DebugEventBatch) -> bool {
        true
    }

    fn opened_by(&self) -> Option<(&str, std::time::Instant)> {
        Some((self.page, self.opened_at))
    }
}

#[tokio::test(start_paused = true)]
async fn a_reloading_page_drops_only_its_own_older_sinks() {
    let h = harness();
    let bot = unready_bot(&h);
    let t0 = std::time::Instant::now();
    let attach = |page: &'static str, opened_at: std::time::Instant| {
        let manager = Arc::clone(&h.manager);
        let bot = bot.clone();
        async move {
            manager
                .subscribe(
                    bot.as_str(),
                    DebugChannelId::Internal,
                    Arc::new(PageSink { page, opened_at }) as Arc<dyn DebugEventSink>,
                )
                .await
                .unwrap()
        }
    };
    attach("main", t0).await;
    attach("main", t0).await;
    attach("other", t0).await;
    // 不属于页面的出口（比如以后的 MCP）不受影响
    let (_plain, _) = subscribe(&h, &bot, DebugChannelId::Internal).await;
    assert_eq!(h.manager.receivers().await[0].viewers, 4);

    let loading_since = t0 + Duration::from_millis(1);
    // 新页面的订阅可能比钩子里的清理还早落到后端，它开的时刻晚于开始加载，留着
    attach("main", t0 + Duration::from_millis(2)).await;
    h.manager.page_loading("main", loading_since);
    assert_eq!(
        h.manager.receivers().await[0].viewers,
        3,
        "只摘 main 窗口在开始加载前开的两个"
    );

    h.manager.page_loading("other", loading_since);
    assert_eq!(h.manager.receivers().await[0].viewers, 2);
}

#[tokio::test(start_paused = true)]
async fn seq_keeps_growing_when_the_receiver_is_recreated() {
    let h = harness();
    let bot = unready_bot(&h);
    let (first, _) = subscribe(&h, &bot, DebugChannelId::Internal).await;
    wait_until("第一个接收器的状态事件", || {
        first.events().len() >= 2
    })
    .await;
    assert!(matches!(
        first.states().as_slice(),
        [
            DebugReceiverState::Connecting,
            DebugReceiverState::Reconnecting { .. },
            ..
        ]
    ));

    h.manager.stop_receiver(bot.as_str()).await;
    assert!(h.manager.receivers().await.is_empty());
    assert_eq!(
        first.states().last(),
        Some(&DebugReceiverState::Stopped {
            reason: "已手动停止".into()
        })
    );
    let before = *seqs(&first.events()).last().unwrap();

    let (second, response) = subscribe(&h, &bot, DebugChannelId::Internal).await;
    assert_eq!(response.receiver.buffered, 0);
    assert_eq!(response.receiver.first_seq, before + 1);
    wait_until("第二个接收器的事件", || {
        !second.events().is_empty()
    })
    .await;
    let after = seqs(&second.events());
    assert!(after[0] > before, "{after:?} 应接在 {before} 之后");

    // 关掉调试台再打开也一样
    h.manager.set_enabled(false).await;
    assert!(h.manager.receivers().await.is_empty());
    assert_eq!(
        second.states().last(),
        Some(&DebugReceiverState::Stopped {
            reason: "调试台已关闭".into()
        })
    );
    assert_eq!(
        h.manager
            .subscribe(
                bot.as_str(),
                DebugChannelId::Internal,
                Arc::new(RecordingSink::default()) as Arc<dyn DebugEventSink>
            )
            .await
            .unwrap_err(),
        DebugError::FeatureDisabled
    );
    h.manager.set_enabled(true).await;
    let before = *seqs(&second.events()).last().unwrap();
    let (third, _) = subscribe(&h, &bot, DebugChannelId::Internal).await;
    wait_until("第三个接收器的事件", || !third.events().is_empty()).await;
    assert!(seqs(&third.events())[0] > before);
}

#[tokio::test(start_paused = true)]
async fn subscribe_checks_the_bot_and_the_channel() {
    let h = harness();
    let stopped = h.bots.add(local_bot(10_001, BackendType::NapCat), false);
    let mut config = local_bot(10_002, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", 3000, "")];
    let running = h.bots.add(config, true);
    let sink = || Arc::new(RecordingSink::default()) as Arc<dyn DebugEventSink>;

    let err = |r: Result<DebugSubscribeResponse, DebugError>| r.unwrap_err();
    assert_eq!(
        err(h
            .manager
            .subscribe("999", DebugChannelId::Auto, sink())
            .await),
        DebugError::BotNotFound
    );
    assert_eq!(
        err(h
            .manager
            .subscribe(stopped.as_str(), DebugChannelId::Auto, sink())
            .await),
        DebugError::BotNotRunning
    );
    assert!(matches!(
        err(h
            .manager
            .subscribe(running.as_str(), http_id("main"), sink())
            .await),
        DebugError::ChannelUnavailable { .. }
    ));
    assert!(matches!(
        err(h
            .manager
            .subscribe(running.as_str(), ws_id("nope"), sink())
            .await),
        DebugError::ChannelUnavailable { .. }
    ));
    assert!(h.manager.receivers().await.is_empty());
}

// ─── 调用进事件流和历史 ──────────────────────────────────────────────────────

#[tokio::test]
async fn our_calls_show_up_in_the_event_stream_and_the_history() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "send_group_msg",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!({"message_id": 321}))),
    )
    .await;
    mount_onebot(
        &onebot,
        "get_group_list",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!([]))),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);
    // 没有 WebUI：内部通道的接收器只会重试，但它照样收我们自己的调用
    let (_sink, _) = subscribe(&h, &bot, DebugChannelId::Auto).await;

    let mut send = request("c1", &bot, http_id("main"), "send_group_msg");
    send.origin = DebugCallOrigin::Composer;
    send.params = json!({"group_id": 1, "message": "hi"});
    call_ok(&h.manager, send).await;
    let mut pick = request("c2", &bot, http_id("main"), "get_group_list");
    pick.origin = DebugCallOrigin::Picker;
    call_ok(&h.manager, pick).await;
    // 没挂的动作：wiremock 回 404
    let failed = call_err(&h.manager, request("c3", &bot, http_id("main"), "nope")).await;
    assert!(matches!(failed, DebugError::Transport { .. }));

    let calls: Vec<DebugCallRecord> = h
        .manager
        .read_events(bot.as_str(), 0, 1000)
        .await
        .into_iter()
        .filter_map(|e| match e.body {
            DebugEventBody::Call { record } => Some(record),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 2, "挑参数的查询不进聊天：{calls:?}");
    assert_eq!(calls[0].request_id.as_deref(), Some("c1"));
    assert_eq!(calls[0].origin, DebugCallOrigin::Composer);
    assert_eq!(calls[0].ok, Some(true));
    assert_eq!(calls[0].retcode, Some(0));
    assert_eq!(calls[0].message_id, Some(321));
    assert_eq!(calls[0].channel, Some(http_id("main")));
    assert_eq!(calls[0].params, json!({"group_id": 1, "message": "hi"}));
    assert_eq!(calls[1].request_id.as_deref(), Some("c3"));
    assert_eq!(calls[1].ok, Some(false));
    assert_eq!(calls[1].error.as_deref(), Some("HTTP 404"));

    // 历史在后台写，但查历史会先等排着队的写完：刚调完就查得到
    let page = h.manager.history(all_history()).await;
    assert_eq!(page.total, 2, "Picker 不进历史");
    // 最新的在前
    assert_eq!(page.entries[0].action, "nope");
    assert!(!page.entries[0].ok);
    assert_eq!(page.entries[1].action, "send_group_msg");
    assert!(
        h.data
            .path()
            .join("onebot-debug")
            .join("history.jsonl")
            .exists()
    );
}

#[tokio::test]
async fn a_call_from_an_ended_epoch_records_nothing() {
    let h = harness();
    let bot = unready_bot(&h);
    let view = h.bots.bot(&bot).await.unwrap();
    let old = h.manager.epoch();
    h.manager.close_all().await;
    let (_sink, _) = subscribe(&h, &bot, DebugChannelId::Internal).await;

    let result = DebugCallResult::Err {
        error: DebugError::Cancelled,
    };
    let stale = request("stale", &bot, DebugChannelId::Internal, "send_msg");
    h.manager
        .record_call(&view, &stale, &DebugChannelId::Internal, &result, &old);
    let fresh = request("fresh", &bot, DebugChannelId::Internal, "get_status");
    h.manager.record_call(
        &view,
        &fresh,
        &DebugChannelId::Internal,
        &result,
        &h.manager.epoch(),
    );

    let ids: Vec<Option<String>> = h
        .manager
        .read_events(bot.as_str(), 0, 1000)
        .await
        .into_iter()
        .filter_map(|e| match e.body {
            DebugEventBody::Call { record } => Some(record.request_id),
            _ => None,
        })
        .collect();
    assert_eq!(ids, [Some("fresh".to_owned())]);
    // 查历史会等排着队的写完：「stale」要是排过队，这里也一定看得到
    let page = h.manager.history(all_history()).await;
    assert_eq!(page.total, 1);
    assert_eq!(page.entries[0].action, "get_status");
}

fn all_history() -> DebugHistoryQuery {
    DebugHistoryQuery {
        action: None,
        bot_id: None,
        ok: None,
        text: None,
        limit: 200,
        offset: 0,
    }
}

#[tokio::test]
async fn clearing_history_waits_for_queued_entries_so_they_do_not_come_back() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    let view = h.bots.bot(&bot).await.unwrap();
    let epoch = h.manager.epoch();
    let result = DebugCallResult::Err {
        error: DebugError::Cancelled,
    };
    for n in 0..30 {
        let req = request(
            &format!("q{n}"),
            &bot,
            DebugChannelId::Internal,
            "get_status",
        );
        h.manager
            .record_call(&view, &req, &DebugChannelId::Internal, &result, &epoch);
    }
    h.manager.clear_history().await.unwrap();
    assert_eq!(h.manager.history(all_history()).await.total, 0);
    // 写入任务要是还在追加，这段时间里就会有记录冒出来
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(h.manager.history(all_history()).await.total, 0);

    // 清空之后的照常记
    let req = request("after", &bot, DebugChannelId::Internal, "get_status");
    h.manager
        .record_call(&view, &req, &DebugChannelId::Internal, &result, &epoch);
    assert_eq!(h.manager.history(all_history()).await.total, 1);
}

#[tokio::test]
async fn a_ws_server_that_rejects_the_token_stops_the_receiver() {
    let h = harness();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let bot = h.bots.add(event_ws_bot(10_001, port_of(&server)), true);
    let (sink, _) = subscribe(&h, &bot, ws_id("ev")).await;
    wait_until("鉴权失败时停下", || {
        sink.states()
            .iter()
            .any(|s| matches!(s, DebugReceiverState::Stopped { .. }))
    })
    .await;
    let states = sink.states();
    assert!(
        !states
            .iter()
            .any(|s| matches!(s, DebugReceiverState::Reconnecting { .. })),
        "鉴权失败重试也没用，不该进重连：{states:?}"
    );
    assert!(matches!(
        states.last(),
        Some(DebugReceiverState::Stopped { reason }) if reason.contains("鉴权失败")
    ));
    assert_eq!(
        channel(&h.manager, &bot, &ws_id("ev")).await.status,
        DebugChannelStatus::AuthFailed { status: 401 }
    );
    // 停下的接收器留着，缓冲照样能看
    assert_eq!(h.manager.receivers().await.len(), 1);
}

#[tokio::test]
async fn napcat_webui_available_drops_the_cached_client() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    Mock::given(method("GET"))
        .and(path("/api/Debug/schemas"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 0, "message": "success", "data": {"get_status": {"description": "状态"}},
        })))
        .mount(&webui)
        .await;
    h.manager
        .test_channel(bot.as_str(), DebugChannelId::Internal)
        .await
        .unwrap();
    assert!(h.manager.sessions.lock().await[&bot].napcat.is_some());

    h.manager
        .on_bot_event(&DomainEvent::NapCatWebuiAvailable {
            bot_id: bot.clone(),
            port: port_of(&webui),
            token: "abc".into(),
            host_port: None,
        })
        .await;
    assert!(h.manager.sessions.lock().await[&bot].napcat.is_none());
}

#[tokio::test]
async fn a_large_response_from_an_ended_epoch_is_not_kept() {
    let h = harness();
    let old = h.manager.epoch();
    h.manager.close_all().await;
    h.manager
        .keep_large_response("r1", "很长的回包".into(), &old);
    assert!(
        h.manager
            .save_response("r1", &h.data.path().join("out.json"))
            .await
            .is_err()
    );

    let current = h.manager.epoch();
    h.manager
        .keep_large_response("r2", "很长的回包".into(), &current);
    let out = h.data.path().join("out2.json");
    h.manager.save_response("r2", &out).await.unwrap();
    assert_eq!(std::fs::read_to_string(out).unwrap(), "很长的回包");
}

#[tokio::test]
async fn storage_is_reachable_through_the_manager() {
    let h = harness();
    assert!(h.manager.take_storage_notices().await.is_empty());
    let mut workspace = h.manager.workspace().await;
    workspace.selected_bot = Some("10001".into());
    h.manager.save_workspace(workspace.clone()).await.unwrap();
    assert_eq!(
        h.manager.workspace().await.selected_bot.as_deref(),
        Some("10001")
    );
    assert!(
        h.data
            .path()
            .join("onebot-debug")
            .join("workspace.json")
            .exists()
    );
    assert!(h.manager.collections().await.requests.is_empty());
    assert!(h.manager.take_storage_notices().await.is_empty());
}
