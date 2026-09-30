//! 会话和连接的收尾：空闲会话连同隧道被回收、接收器停下时关掉它开的 WS 服务连接、
//! Bot 停了之后半路的操作不再把会话建回来；以及调用记录里的参数瘦身、只推事件的 WS
//! 服务的探测、探测失败不拆隧道、调试输出里不带令牌。

use ncd_domain::onebot_debug::{DebugEventBody, DebugHistoryQuery, DebugReceiverState};

use super::super::receiver::{Receiver, run_pump};
use super::events::{Upstream, event_ws_bot, subscribe, wait_until};
use super::*;
use crate::events::DomainEvent;

fn remote_http_bot(h: &Harness, port: u16) -> BotId {
    let mut config = remote_bot(10_001, DeploymentType::Native);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port, "")];
    h.bots.add(config, true)
}

// ─── 空闲会话回收 ────────────────────────────────────────────────────────────

#[tokio::test(start_paused = true)]
async fn idle_sessions_are_reclaimed_and_their_tunnels_closed() {
    let h = harness();
    let bot = h.bots.add(remote_bot(10_001, DeploymentType::Native), true);
    let view = h.bots.bot(&bot).await.unwrap();
    let epoch = h.manager.epoch();
    h.manager
        .ensure_tunnel(&view, "http:main", "127.0.0.1", 3000, &epoch)
        .await
        .unwrap();
    assert_eq!(h.manager.open_tunnels(), 1);

    // 20 分钟后又用了一次（复用已有隧道也算「有人在用」）
    tokio::time::advance(Duration::from_secs(20 * 60)).await;
    h.manager
        .ensure_tunnel(&view, "http:main", "127.0.0.1", 3000, &epoch)
        .await
        .unwrap();
    assert_eq!(h.host.opened().len(), 1, "复用，不重开");

    // 距上次使用 29 分钟：还留着
    tokio::time::advance(Duration::from_secs(29 * 60)).await;
    h.manager.sweep_idle().await;
    assert!(h.manager.sessions.lock().await.contains_key(&bot));
    assert_eq!(h.manager.open_tunnels(), 1);

    // 满 30 分钟：会话丢掉，隧道随之关闭
    tokio::time::advance(Duration::from_secs(2 * 60)).await;
    h.manager.sweep_idle().await;
    assert!(!h.manager.sessions.lock().await.contains_key(&bot));
    assert_eq!(h.manager.open_tunnels(), 0, "隧道应已关闭");
}

#[tokio::test]
async fn a_stopped_ws_receiver_closes_the_connection_it_opened() {
    let h = harness();
    let up = Upstream::start().await;
    let bot = h.bots.add(event_ws_bot(10_001, up.port), true);
    let (sink, _) = subscribe(&h, &bot, ws_id("ev")).await;
    wait_until("连上", || sink.connected_count() == 1).await;
    assert_eq!(up.ws_closed(), 0);

    h.manager.stop_receiver(bot.as_str()).await;
    wait_until("接收器停下后连接被关掉", || up.ws_closed() == 1).await;
    let sessions = h.manager.sessions.lock().await;
    let ws = sessions[&bot].ws.get("ws:ev").cloned();
    assert!(
        ws.is_none_or(|ws| ws.is_closed()),
        "会话里留着的也是已关闭的连接"
    );
}

// ─── Bot 停了之后半路的操作 ──────────────────────────────────────────────────

#[tokio::test]
async fn a_tunnel_that_opens_after_the_bot_stopped_is_discarded() {
    let h = harness();
    h.host.set_delay(Duration::from_millis(300));
    let bot = h.bots.add(remote_bot(10_001, DeploymentType::Native), true);
    let view = h.bots.bot(&bot).await.unwrap();
    let epoch = h.manager.epoch();

    let manager = Arc::clone(&h.manager);
    let task = tokio::spawn(async move {
        manager
            .ensure_tunnel(&view, "http:main", "127.0.0.1", 3000, &epoch)
            .await
    });
    h.host.requested.notified().await;
    // 隧道正开到一半，Bot 停了
    let mut stopped = BotActorSnapshot::new(bot.clone());
    stopped.state = BotActorState::Stopped;
    h.manager
        .on_bot_event(&DomainEvent::BotStateChanged {
            snapshot: stopped,
            reason: None,
        })
        .await;

    let failure = task.await.unwrap().unwrap_err();
    assert_eq!(failure.error, DebugError::Cancelled);
    assert_eq!(h.host.opened().len(), 1, "隧道确实开成了，只是没被留下");
    assert!(!h.manager.sessions.lock().await.contains_key(&bot));
    assert_eq!(h.manager.open_tunnels(), 0);

    // Bot 停过之后才开始的操作照常（比如它又启动了）
    let view = h.bots.bot(&bot).await.unwrap();
    let port = h
        .manager
        .ensure_tunnel(&view, "http:main", "127.0.0.1", 3000, &h.manager.epoch())
        .await
        .unwrap();
    assert_eq!(port, 3000);
    assert_eq!(h.manager.open_tunnels(), 1);
}

// ─── 调用记录里的参数瘦身 ────────────────────────────────────────────────────

#[tokio::test]
async fn call_records_in_the_event_stream_carry_trimmed_params() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "send_group_msg",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!({"message_id": 1}))),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);
    let (_sink, _) = subscribe(&h, &bot, DebugChannelId::Auto).await;

    let image = format!("base64://{}", "A".repeat(300_000));
    let mut send = request("big", &bot, http_id("main"), "send_group_msg");
    send.origin = DebugCallOrigin::Composer;
    send.params = json!({"group_id": 42, "message": [{"type": "image", "data": {"file": image}}]});
    call_ok(&h.manager, send).await;

    let record = h
        .manager
        .read_events(bot.as_str(), 0, 1000)
        .await
        .into_iter()
        .find_map(|e| match e.body {
            DebugEventBody::Call { record } => Some(record),
            _ => None,
        })
        .unwrap();
    assert_eq!(record.params["group_id"], 42, "认会话要用的字段留着");
    assert_eq!(
        record.params["message"][0]["data"]["file"],
        "<已省略 300009 字节>"
    );
    // 历史里也是瘦过身的
    let entry = h
        .manager
        .history(DebugHistoryQuery {
            action: None,
            bot_id: None,
            ok: None,
            text: None,
            limit: 10,
            offset: 0,
        })
        .await;
    let entry = h.manager.history_entry(&entry.entries[0].id).await.unwrap();
    assert_eq!(
        entry.params["message"][0]["data"]["file"],
        "<已省略 300009 字节>"
    );
}

// ─── 探测 ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn probing_an_event_only_ws_channel_only_checks_the_handshake() {
    let h = harness();
    let up = Upstream::start().await;
    let bot = h.bots.add(event_ws_bot(10_001, up.port), true);

    let started = Instant::now();
    let info = h
        .manager
        .test_channel(bot.as_str(), ws_id("ev"))
        .await
        .unwrap();
    assert_eq!(info.status, DebugChannelStatus::Available);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "不该发 get_status 干等回包：{:?}",
        started.elapsed()
    );
    wait_until("探测用的连接用完就关", || up.ws_closed() == 1).await;
    assert_eq!(up.streams(), 1);
}

#[tokio::test]
async fn a_probe_that_gets_a_5xx_keeps_the_tunnel() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(&onebot, "get_status", ResponseTemplate::new(500)).await;
    let bot = remote_http_bot(&h, port_of(&onebot));

    let info = h
        .manager
        .test_channel(bot.as_str(), http_id("main"))
        .await
        .unwrap();
    assert_eq!(
        info.status,
        DebugChannelStatus::Unreachable {
            reason: "HTTP 500".into()
        }
    );
    // 上游回了 500 只说明这一次不行：隧道和客户端都还在，下次不必重开 SSH
    assert_eq!(h.manager.open_tunnels(), 1);
    {
        let sessions = h.manager.sessions.lock().await;
        assert!(sessions[&bot].tunnels.contains_key("http:main"));
        assert!(sessions[&bot].http.contains_key("http:main"));
    }
    h.manager
        .test_channel(bot.as_str(), http_id("main"))
        .await
        .unwrap();
    assert_eq!(h.host.opened().len(), 1, "第二次探测复用同一条隧道");
}

// ─── 调试输出不带令牌 ────────────────────────────────────────────────────────

#[tokio::test]
async fn debug_output_never_shows_tokens() {
    let h = harness();
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server(
        "main",
        "0.0.0.0",
        3000,
        "super-secret-token-value",
    )];
    let bot = h.bots.add(config, true);
    let view = h.bots.bot(&bot).await.unwrap();
    let plans = plan_channels(&view.config);

    let printed = format!("{view:?} {plans:?}");
    assert!(!printed.contains("super-secret-token-value"), "{printed}");
    assert!(
        printed.contains("su***ue"),
        "打码后的样子照常显示：{printed}"
    );
    assert!(printed.contains("10001"));
}

// ─── 两处竞争 ────────────────────────────────────────────────────────────────

fn stopped(bot: &BotId) -> DomainEvent {
    let mut snapshot = BotActorSnapshot::new(bot.clone());
    snapshot.state = BotActorState::Stopped;
    DomainEvent::BotStateChanged {
        snapshot,
        reason: None,
    }
}

#[tokio::test]
async fn a_bot_that_stops_while_subscribe_looks_it_up_gets_no_receiver() {
    let h = harness();
    let bot = h.bots.add(local_bot(20_002, BackendType::SnowLuma), true);
    // 订阅卡在查 Bot 这一步时，停止事件先处理完
    h.bots.set_lookup_delay(Duration::from_millis(300));
    let manager = Arc::clone(&h.manager);
    let id = bot.clone();
    let pending = tokio::spawn(async move {
        manager
            .subscribe(
                id.as_str(),
                DebugChannelId::Internal,
                Arc::new(super::events::RecordingSink::default()) as Arc<dyn DebugEventSink>,
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    h.manager.on_bot_event(&stopped(&bot)).await;

    assert_eq!(
        pending.await.unwrap().unwrap_err(),
        DebugError::BotNotRunning
    );
    assert!(
        h.manager.receivers().await.is_empty(),
        "不该留下一个没人会停的接收器"
    );
}

#[tokio::test(start_paused = true)]
async fn a_pump_whose_bot_stopped_halts_instead_of_retrying_forever() {
    let h = harness();
    let bot = h.bots.add(local_bot(20_002, BackendType::SnowLuma), true);
    // 接收器拿到这一轮之后 Bot 停过（比如停止事件插在建接收器的半路）
    let epoch = h.manager.epoch();
    h.manager.drop_session(&bot).await;
    let receiver = Arc::new(Receiver::new(
        bot.clone(),
        DebugChannelId::Internal,
        1,
        epoch.token.child_token(),
    ));
    let pump = receiver.first_pump();
    let run = run_pump(
        Arc::downgrade(&h.manager),
        Arc::clone(&receiver),
        pump,
        epoch,
        Arc::clone(&h.manager.own_calls),
    );
    // 退避一轮下来要 61 秒；直接停下的话一瞬间就返回
    tokio::time::timeout(Duration::from_secs(60), run)
        .await
        .expect("应写明原因停下，而不是一直重连");
    assert_eq!(
        receiver.info().state,
        DebugReceiverState::Stopped {
            reason: "Bot 已停止".into()
        }
    );
}

#[tokio::test]
async fn a_call_whose_connection_closed_before_sending_is_retried_without_teardown() {
    let h = harness();
    let up = Upstream::start().await;
    let mut config = remote_bot(10_001, DeploymentType::Native);
    config.connect.websocket_servers =
        vec![ws_server("uni", "0.0.0.0", up.port, WsRole::Universal)];
    let bot = h.bots.add(config, true);
    let view = h.bots.bot(&bot).await.unwrap();
    let plans = plan_channels(&view.config);
    let plan = plans.iter().find(|p| p.id == ws_id("uni")).unwrap();
    let epoch = h.manager.epoch();

    // 调用方已经拿到了连接，接收器停下时恰好把它关了
    let client = h
        .manager
        .ensure_ws(&view, plan, None, &epoch)
        .await
        .unwrap();
    assert_eq!(h.manager.open_tunnels(), 1);
    client.close();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let raw = h
        .manager
        .call_on_ws(
            client,
            &view,
            plan,
            "get_status",
            &json!({}),
            deadline,
            &epoch,
        )
        .await
        .expect("上游没收到过这次请求，应换新连接重发一次");
    assert_eq!(raw.value["status"], "ok");
    assert_eq!(up.streams(), 2, "重发走的是新连接");
    // 没发出去不说明通道坏了：隧道还是原来那条
    assert_eq!(h.manager.open_tunnels(), 1);
    assert_eq!(h.host.opened().len(), 1);
    assert_ne!(
        channel(&h.manager, &bot, &ws_id("uni")).await.status,
        DebugChannelStatus::Unreachable {
            reason: "连接已关闭".into()
        }
    );
}
