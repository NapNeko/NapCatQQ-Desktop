use super::*;

use std::time::Duration;

use crate::remote_snowluma::layout::RemoteSnowLumaLayout;
use crate::snowluma::status_poller::SnowLumaLoginState;
use crate::snowluma::webui_client::ReqwestSnowLumaWebUiClient;
use ncd_traits::events::{EventFilter, EventSubscription};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn backend(bus: Arc<BroadcastEventBus>) -> RemoteSnowLumaBackend {
    let tunnels = Arc::new(RemoteSnowLumaTunnelRegistry::new());
    let daemon = Arc::new(RemoteSnowLumaDaemon::from_layout(
        "server".into(),
        Arc::new(ncd_host::local::LocalWindowsHost::new()),
        Arc::clone(&tunnels),
        Arc::clone(&bus),
        RemoteSnowLumaLayout {
            home: "/home/test".into(),
            paths: SnowLumaRemotePaths::from_remote_home("/home/test"),
            node_bin: "/opt/snowluma/node".into(),
            qq_bin: "/opt/QQ/qq".into(),
            qq_install_base: "/".into(),
        },
    ));
    RemoteSnowLumaBackend::new(
        "server",
        daemon,
        bus,
        tunnels,
        Arc::new(ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator::default()),
    )
}

async fn deps(
    bus: Arc<BroadcastEventBus>,
    uin: &str,
    pid: u32,
    status: &str,
    password: &str,
) -> (MockServer, PollerDeps) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .and(body_json(json!({ "password": password })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "test" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/processes"))
        .and(header("authorization", "Bearer test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "list": [{ "pid": pid, "name": "qq", "path": "/opt/QQ/qq", "uin": uin, "status": status }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/qq-list"))
        .and(header("authorization", "Bearer test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "list": [] })))
        .mount(&server)
        .await;
    let http = Arc::new(
        ReqwestSnowLumaWebUiClient::new(server.address().port(), password.into()).unwrap(),
    );
    (
        server,
        PollerDeps {
            event_bus: bus,
            http,
            proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(
                pid,
            )),
            expected_uin: Some(uin.into()),
        },
    )
}

async fn next_state(sub: &mut EventSubscription, bot: &BotId) -> SnowLumaLoginState {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(DomainEvent::SnowLumaLoginStateChanged { bot_id, state }) = sub.next().await
                && &bot_id == bot
            {
                return state;
            }
        }
    })
    .await
    .expect("login state")
}

#[tokio::test]
async fn replacing_remote_poller_uses_new_endpoint_and_credentials() {
    let bus = Arc::new(BroadcastEventBus::default());
    let backend = backend(Arc::clone(&bus));
    let bot = BotId::new("10001");
    let mut sub = bus.subscribe(EventFilter::bot(bot.clone()));
    let (old_server, old_deps) = deps(Arc::clone(&bus), "10001", 10, "online", "old").await;
    backend.replace_poller(bot.clone(), 10, old_deps).await;
    assert_eq!(
        next_state(&mut sub, &bot).await,
        SnowLumaLoginState::LoggedIn
    );

    let (_new_server, new_deps) = deps(Arc::clone(&bus), "10001", 20, "loaded", "new").await;
    backend.replace_poller(bot.clone(), 20, new_deps).await;
    assert_eq!(
        next_state(&mut sub, &bot).await,
        SnowLumaLoginState::WaitingForQrScan
    );
    let qr_client = backend
        .login_clients
        .lock()
        .await
        .get(&bot)
        .cloned()
        .expect("replacement provides the QR gate client");
    let processes = qr_client.list_processes().await.unwrap();
    assert_eq!(processes.len(), 1);
    assert_eq!(processes[0].pid, 20);
    let old_requests = old_server.received_requests().await.unwrap().len();
    tokio::time::sleep(Duration::from_millis(2100)).await;
    assert_eq!(
        old_server.received_requests().await.unwrap().len(),
        old_requests
    );
    backend.detach_login_pollers().await;
}

#[tokio::test]
async fn detach_remote_pollers_clears_both_bots_and_allows_same_state_on_reconnect() {
    let bus = Arc::new(BroadcastEventBus::default());
    let backend = backend(Arc::clone(&bus));
    let bot_a = BotId::new("10001");
    let bot_b = BotId::new("10002");
    let mut sub = bus.subscribe(EventFilter::all());
    let (_server_a, deps_a) = deps(Arc::clone(&bus), "10001", 10, "online", "first").await;
    let (_server_b, deps_b) = deps(Arc::clone(&bus), "10002", 20, "online", "first").await;
    backend.replace_poller(bot_a.clone(), 10, deps_a).await;
    assert_eq!(
        next_state(&mut sub, &bot_a).await,
        SnowLumaLoginState::LoggedIn
    );
    backend.replace_poller(bot_b.clone(), 20, deps_b).await;
    assert_eq!(
        next_state(&mut sub, &bot_b).await,
        SnowLumaLoginState::LoggedIn
    );

    let qr_a = Arc::new(QrActivity::new());
    let qr_b = Arc::new(QrActivity::new());
    {
        let mut activities = backend.qr_activity.lock().await;
        activities.insert(bot_a.clone(), Arc::clone(&qr_a));
        activities.insert(bot_b.clone(), Arc::clone(&qr_b));
    }

    backend.detach_login_pollers().await;
    let mut unavailable = std::collections::HashSet::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while unavailable.len() < 2 {
            if let Some(DomainEvent::SnowLumaLoginProbeUnavailable { bot_id }) = sub.next().await {
                unavailable.insert(bot_id);
            }
        }
    })
    .await
    .unwrap();
    assert!(unavailable.contains(&bot_a));
    assert!(unavailable.contains(&bot_b));
    assert!(backend.pollers.lock().await.is_empty());
    assert!(backend.login_clients.lock().await.is_empty());
    assert!(qr_a.cancel.is_cancelled());
    assert!(qr_b.cancel.is_cancelled());

    let (_new_server, new_deps) = deps(Arc::clone(&bus), "10001", 10, "online", "second").await;
    backend.replace_poller(bot_a.clone(), 10, new_deps).await;
    assert_eq!(
        next_state(&mut sub, &bot_a).await,
        SnowLumaLoginState::LoggedIn
    );
    backend.detach_login_pollers().await;
}

#[tokio::test]
async fn disposing_poller_invalidates_only_its_qr_client_and_activity() {
    let bus = Arc::new(BroadcastEventBus::default());
    let backend = backend(Arc::clone(&bus));
    let bot_a = BotId::new("10001");
    let bot_b = BotId::new("10002");
    let (_server_a, deps_a) = deps(Arc::clone(&bus), "10001", 10, "loaded", "first").await;
    let (_server_b, deps_b) = deps(Arc::clone(&bus), "10002", 20, "loaded", "second").await;
    backend.replace_poller(bot_a.clone(), 10, deps_a).await;
    backend.replace_poller(bot_b.clone(), 20, deps_b).await;
    let qr_a = Arc::new(QrActivity::new());
    let qr_b = Arc::new(QrActivity::new());
    {
        let mut activities = backend.qr_activity.lock().await;
        activities.insert(bot_a.clone(), Arc::clone(&qr_a));
        activities.insert(bot_b.clone(), Arc::clone(&qr_b));
    }

    backend.dispose_poller(&bot_a).await;
    assert!(qr_a.cancel.is_cancelled());
    assert!(!qr_b.cancel.is_cancelled());
    {
        let clients = backend.login_clients.lock().await;
        assert!(!clients.contains_key(&bot_a));
        assert!(clients.contains_key(&bot_b));
    }
    {
        let pollers = backend.pollers.lock().await;
        assert!(!pollers.contains_key(&bot_a));
        assert!(pollers.contains_key(&bot_b));
    }
    backend.detach_login_pollers().await;
}
