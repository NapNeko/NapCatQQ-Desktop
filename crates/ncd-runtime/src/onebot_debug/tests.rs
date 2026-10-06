//! DebugManager 端到端：假的 Bot 端口 + wiremock 扮演 NapCat / SnowLuma WebUI 和 OneBot HTTP 服务，
//! 再起一个本机 WS 服务端；远端场景用一个「隧道本机口 = 远端口」的替身主机。
//! 事件接收器的端到端测试在 `tests/events.rs`，共用这里的替身和工具。

mod events;
mod sessions;

#[tokio::test]
async fn chat_transport_survives_debug_disable_without_writing_history() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    mount_napcat_call(&webui, "get_login_info", ob11_ok(json!({"user_id": 10001}))).await;
    let chat = DebugManager::new_ephemeral(
        Arc::clone(&h.bots) as Arc<dyn DebugBotPort>,
        Arc::new(LocalOnlyHostResolver::new(
            Arc::clone(&h.host) as Arc<dyn Host>
        )),
        h.data.path().to_path_buf(),
    );
    h.manager.set_enabled(false).await;
    let result = call_ok(
        &chat,
        request("chat", &bot, DebugChannelId::Internal, "get_login_info"),
    )
    .await;
    assert!(result.ok);
    assert!(!h.data.path().join("onebot-debug").exists());
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use ncd_domain::bot_actor::{BotActorSnapshot, BotActorState};
use ncd_domain::bot_config::{
    BackendType, BotConfig, DeploymentType, HttpServerConfig, MessagePostFormat, NetworkBaseFields,
    WebsocketServerConfig, WsRole,
};
use ncd_domain::ids::BotId;
use ncd_domain::kinds::RuntimeTarget;
use ncd_domain::onebot_debug::{
    DebugCallOrigin, DebugCallOutcome, DebugCallRequest, DebugCallResult, DebugCatalogSource,
    DebugChannelId, DebugChannelInfo, DebugChannelStatus, DebugError, DebugHost,
};
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostCommand, HostError, HostPath,
    HostProcess, HostShell, Locality, Os,
};
use ncd_test_support::BotConfigBuilder;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::host_resolver::LocalOnlyHostResolver;

// ─── 替身 ────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct FakeBots {
    bots: StdMutex<Vec<DebugBotView>>,
    napcat: StdMutex<HashMap<BotId, (u16, String)>>,
    snowluma: StdMutex<HashMap<BotId, (u16, String)>>,
    /// `bot()` 查一个 Bot 要花的时间，模拟读配置慢
    lookup_delay: StdMutex<Duration>,
    recovered_snowluma: StdMutex<HashMap<BotId, (u16, String)>>,
    recovery_calls: AtomicUsize,
}

impl FakeBots {
    fn add(&self, config: BotConfig, running: bool) -> BotId {
        let id = BotId::new(config.bot.qq_id.to_string());
        let mut snapshot = BotActorSnapshot::new(id.clone());
        snapshot.state = if running {
            BotActorState::Running
        } else {
            BotActorState::Stopped
        };
        self.bots.lock().unwrap().push(DebugBotView {
            config,
            snapshot,
            online: Some(true),
        });
        id
    }

    fn set_napcat(&self, id: &BotId, port: u16) {
        self.napcat
            .lock()
            .unwrap()
            .insert(id.clone(), (port, "abc".into()));
    }

    /// 模拟一次重启：快照的 revision 往前走
    fn restart(&self, id: &BotId) {
        let mut bots = self.bots.lock().unwrap();
        let view = bots.iter_mut().find(|v| &v.bot_id() == id).unwrap();
        view.snapshot.revision += 4;
    }

    fn set_lookup_delay(&self, delay: Duration) {
        *self.lookup_delay.lock().unwrap() = delay;
    }

    fn set_snowluma(&self, id: &BotId, port: u16) {
        self.snowluma
            .lock()
            .unwrap()
            .insert(id.clone(), (port, "pw".into()));
    }
}

#[async_trait::async_trait]
impl DebugBotPort for FakeBots {
    async fn list_bots(&self) -> Vec<DebugBotView> {
        self.bots.lock().unwrap().clone()
    }

    async fn bot(&self, bot_id: &BotId) -> Option<DebugBotView> {
        let delay = *self.lookup_delay.lock().unwrap();
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        self.bots
            .lock()
            .unwrap()
            .iter()
            .find(|v| &v.bot_id() == bot_id)
            .cloned()
    }

    async fn napcat_webui(&self, bot_id: &BotId) -> Option<(u16, String)> {
        self.napcat.lock().unwrap().get(bot_id).cloned()
    }

    async fn snowluma_webui(&self, bot_id: &BotId) -> Result<(u16, String), String> {
        self.snowluma
            .lock()
            .unwrap()
            .get(bot_id)
            .cloned()
            .ok_or_else(|| "SnowLuma 还没就绪".to_owned())
    }
    async fn recover_webui(&self, bot_id: &BotId) -> Result<(), String> {
        self.recovery_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(endpoint) = self.recovered_snowluma.lock().unwrap().remove(bot_id) {
            self.snowluma
                .lock()
                .unwrap()
                .insert(bot_id.clone(), endpoint);
        }
        Ok(())
    }
}

#[tokio::test]
async fn missing_remote_webui_recovers_without_an_account_page_or_new_bot_start() {
    let h = harness();
    let mut config = remote_bot(20005, DeploymentType::Native);
    config.bot.backend_type = BackendType::SnowLuma;
    let bot = h.bots.add(config, true);
    h.bots
        .recovered_snowluma
        .lock()
        .unwrap()
        .insert(bot.clone(), (4567, "pw".into()));
    let view = h.bots.bot(&bot).await.unwrap();
    assert_eq!(
        h.manager.internal_endpoint(&view).await.unwrap(),
        (4567, "pw".into())
    );
    assert_eq!(
        h.manager.internal_endpoint(&view).await.unwrap(),
        (4567, "pw".into())
    );
    assert_eq!(h.bots.recovery_calls.load(Ordering::SeqCst), 1);
    assert!(h.bots.bot(&bot).await.unwrap().running());
}

#[tokio::test]
async fn a_missing_local_webui_does_not_trigger_remote_reconciliation() {
    let h = harness();
    let bot = h.bots.add(local_bot(20006, BackendType::SnowLuma), true);
    let view = h.bots.bot(&bot).await.unwrap();
    assert!(h.manager.internal_endpoint(&view).await.is_err());
    assert_eq!(h.bots.recovery_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_chat_ignore_and_hide_updates_preserve_both_persisted_settings() {
    let h = harness();
    let bot = h.bots.add(local_bot(20007, BackendType::SnowLuma), false);
    let chat = crate::chat::ChatManager::new(
        Arc::clone(&h.bots) as Arc<dyn DebugBotPort>,
        Arc::new(LocalOnlyHostResolver::new(
            Arc::clone(&h.host) as Arc<dyn Host>
        )),
        h.data.path().to_path_buf(),
    );
    let (ignored, hidden) = tokio::join!(
        chat.set_group_ignored(bot.to_string(), "20007".into(), "123".into(), true, false),
        chat.set_group_ignored(bot.to_string(), "20007".into(), "456".into(), true, true)
    );
    ignored.unwrap();
    hidden.unwrap();
    let status = chat.desktop_status().await;
    assert_eq!(status.accounts[0].preference.ignored_groups, vec!["123"]);
    assert_eq!(status.accounts[0].preference.hidden_groups, vec!["456"]);
    chat.set_group_ignored(bot.to_string(), "20007".into(), "456".into(), false, true)
        .await
        .unwrap();
    assert_eq!(
        chat.desktop_status().await.accounts[0]
            .preference
            .ignored_groups,
        vec!["123"]
    );
    let saved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(h.data.path().join("config/chat-desktop.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["accounts"][0]["ignoredGroups"], json!(["123"]));
    assert_eq!(saved["accounts"][0]["hiddenGroups"], json!([]));
}

/// 只会开隧道的替身主机：本机口直接等于远端口，于是「远端」的服务就是本机上的 wiremock
#[derive(Default)]
struct LoopbackTunnelHost {
    /// 开过的隧道目标 (远端主机, 远端口)
    opened: StdMutex<Vec<(String, u16)>>,
    /// 开一条隧道要花的时间，模拟 SSH 慢
    delay: StdMutex<Duration>,
    /// 每收到一次开隧道请求通知一次
    requested: tokio::sync::Notify,
}

impl LoopbackTunnelHost {
    fn opened(&self) -> Vec<(String, u16)> {
        self.opened.lock().unwrap().clone()
    }

    fn set_delay(&self, delay: Duration) {
        *self.delay.lock().unwrap() = delay;
    }
}

fn unsupported<T>(operation: &'static str) -> Result<T, HostError> {
    Err(HostError::Unsupported { operation })
}

#[async_trait::async_trait]
impl Host for LoopbackTunnelHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        Locality::Remote
    }
    fn id(&self) -> &str {
        "remote:test"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    async fn read_file(&self, _: &HostPath) -> Result<bytes::Bytes, HostError> {
        unsupported("read_file")
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        unsupported("write_file")
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        unsupported("list_dir")
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unsupported("create_dir_all")
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        unsupported("remove_file")
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unsupported("remove_dir_all")
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        unsupported("exists")
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        unsupported("upload")
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        unsupported("download")
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        unsupported("extract_archive")
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        unsupported("spawn")
    }
    async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
        unsupported("run_to_string")
    }
    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        self.opened
            .lock()
            .unwrap()
            .push((spec.remote_host.clone(), spec.remote_port));
        self.requested.notify_one();
        let delay = *self.delay.lock().unwrap();
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        Ok(TunnelHandle::detached(spec.remote_port, 0))
    }
}

struct Harness {
    manager: Arc<DebugManager>,
    bots: Arc<FakeBots>,
    host: Arc<LoopbackTunnelHost>,
    /// 落盘目录；要活过整个测试
    data: tempfile::TempDir,
}

fn harness() -> Harness {
    let bots = Arc::new(FakeBots::default());
    let host = Arc::new(LoopbackTunnelHost::default());
    let resolver = Arc::new(LocalOnlyHostResolver::new(
        Arc::clone(&host) as Arc<dyn Host>
    ));
    let data = tempfile::tempdir().unwrap();
    let manager = Arc::new(DebugManager::new(
        Arc::clone(&bots) as Arc<dyn DebugBotPort>,
        resolver,
        data.path().to_path_buf(),
    ));
    Harness {
        manager,
        bots,
        host,
        data,
    }
}

// ─── 配置与请求 ──────────────────────────────────────────────────────────────

fn base(name: &str, token: &str) -> NetworkBaseFields {
    NetworkBaseFields {
        enable: true,
        name: name.to_owned(),
        message_post_format: MessagePostFormat::Array,
        token: token.to_owned(),
        debug: false,
    }
}

fn http_server(name: &str, host: &str, port: u16, token: &str) -> HttpServerConfig {
    HttpServerConfig {
        base: base(name, token),
        host: host.to_owned(),
        port,
        enable_cors: false,
        enable_websocket: false,
        path: "/".to_owned(),
    }
}

fn ws_server(name: &str, host: &str, port: u16, role: WsRole) -> WebsocketServerConfig {
    WebsocketServerConfig {
        base: base(name, ""),
        host: host.to_owned(),
        port,
        report_self_message: false,
        enable_force_push_event: false,
        heart_interval: 30_000,
        path: "/".to_owned(),
        role,
    }
}

fn local_bot(qq: u64, backend: BackendType) -> BotConfig {
    BotConfigBuilder::new()
        .name(format!("bot-{qq}"))
        .qq_id(qq)
        .backend_type(backend)
        .runtime_target(RuntimeTarget::Local)
        .build()
}

fn remote_bot(qq: u64, deployment: DeploymentType) -> BotConfig {
    BotConfigBuilder::new()
        .qq_id(qq)
        .backend_type(BackendType::NapCat)
        .runtime_target(RuntimeTarget::server("vps"))
        .deployment_type(deployment)
        .build()
}

fn http_id(name: &str) -> DebugChannelId {
    DebugChannelId::Http { name: name.into() }
}

fn ws_id(name: &str) -> DebugChannelId {
    DebugChannelId::Ws { name: name.into() }
}

fn request(id: &str, bot: &BotId, channel: DebugChannelId, action: &str) -> DebugCallRequest {
    DebugCallRequest {
        request_id: id.to_owned(),
        bot_id: bot.as_str().to_owned(),
        channel,
        action: action.to_owned(),
        params: json!({}),
        timeout_ms: None,
        origin: DebugCallOrigin::Editor,
    }
}

async fn call_ok(manager: &DebugManager, req: DebugCallRequest) -> DebugCallOutcome {
    match manager.call(req).await.result {
        DebugCallResult::Ok { outcome } => outcome,
        DebugCallResult::Err { error } => panic!("应拿到回包，却是 {error:?}"),
    }
}

async fn call_err(manager: &DebugManager, req: DebugCallRequest) -> DebugError {
    match manager.call(req).await.result {
        DebugCallResult::Err { error } => error,
        DebugCallResult::Ok { outcome } => panic!("应失败，却拿到回包 {outcome:?}"),
    }
}

async fn channel(manager: &DebugManager, bot: &BotId, id: &DebugChannelId) -> DebugChannelInfo {
    manager
        .list_channels(bot.as_str())
        .await
        .unwrap()
        .channels
        .into_iter()
        .find(|c| &c.id == id)
        .unwrap()
}

fn ob11_ok(data: Value) -> Value {
    json!({"status": "ok", "retcode": 0, "data": data, "message": "", "wording": "", "echo": null})
}

fn port_of(server: &MockServer) -> u16 {
    server.address().port()
}

// ─── 上游替身 ────────────────────────────────────────────────────────────────

/// NapCat WebUI：登录总是成功（令牌 abc）
async fn napcat_webui() -> MockServer {
    let server = MockServer::start().await;
    mount_napcat_login(&server).await;
    server
}

async fn mount_napcat_login(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 0, "message": "success", "data": {"Credential": "cred"},
        })))
        .mount(server)
        .await;
}

async fn mount_napcat_call(server: &MockServer, action: &str, data: Value) {
    Mock::given(method("POST"))
        .and(path("/api/Debug/call/debug-primary"))
        .and(body_partial_json(json!({"action": action})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"code": 0, "data": data, "message": "success"})),
        )
        .mount(server)
        .await;
}

/// OneBot HTTP 服务：`action` 回固定的响应
async fn mount_onebot(server: &MockServer, action: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(format!("/{action}")))
        .respond_with(response)
        .mount(server)
        .await;
}

/// 本机 OneBot WS 服务端：把每个动作按 echo 原样回一个成功回包，并数有几条连接
async fn spawn_ws_server() -> (u16, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let connections = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&connections);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            counter.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                while let Some(Ok(message)) = ws.next().await {
                    if let Message::Text(text) = message {
                        let req: Value = serde_json::from_str(&text).unwrap();
                        let mut reply = ob11_ok(json!({"action": req["action"]}));
                        reply["echo"] = req["echo"].clone();
                        if ws.send(Message::Text(reply.to_string())).await.is_err() {
                            return;
                        }
                    }
                }
            });
        }
    });
    (port, connections)
}

// ─── 目标与通道 ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn targets_and_channels_describe_each_bot() {
    let h = harness();
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", 3000, "secret-token")];
    let mut disabled = ws_server("off", "0.0.0.0", 3002, WsRole::Universal);
    disabled.base.enable = false;
    config.connect.websocket_servers = vec![
        ws_server("uni", "127.0.0.1", 3001, WsRole::Universal),
        disabled,
    ];
    let local = h.bots.add(config, true);
    h.bots
        .add(remote_bot(20_002, DeploymentType::Docker), false);

    let targets = h.manager.list_targets().await;
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].bot_id, "10001");
    assert_eq!(targets[0].name, "bot-10001");
    assert_eq!(targets[0].qq_id, 10_001);
    assert_eq!(targets[0].backend, BackendType::NapCat);
    assert_eq!(targets[0].host, DebugHost::Local);
    assert!(targets[0].running);
    assert_eq!(targets[0].online, Some(true));
    assert_eq!(
        targets[1].host,
        DebugHost::Docker {
            server_id: "vps".into()
        }
    );
    assert!(!targets[1].running);

    let channels = h.manager.list_channels(local.as_str()).await.unwrap();
    assert_eq!(channels.bot_id, "10001");
    let ids: Vec<_> = channels.channels.iter().map(|c| c.id.clone()).collect();
    assert_eq!(
        ids,
        vec![DebugChannelId::Internal, http_id("main"), ws_id("uni")]
    );
    let internal = &channels.channels[0];
    assert_eq!(internal.label, "内部通道（WebUI）");
    assert_eq!(internal.endpoint.as_deref(), Some("WebUI"));
    assert_eq!(internal.status, DebugChannelStatus::Unknown);
    assert_eq!(internal.token_hint, None);
    let http = &channels.channels[1];
    assert_eq!(http.label, "HTTP · main :3000");
    assert_eq!(http.endpoint.as_deref(), Some("127.0.0.1:3000"));
    assert_eq!(http.token_hint.as_deref(), Some("se***en"));
    assert!(http.can_call && !http.can_receive);
    let ws = &channels.channels[2];
    assert_eq!(ws.endpoint.as_deref(), Some("127.0.0.1:3001"));
    assert!(ws.can_call && ws.can_receive);
    assert_eq!(channels.auto_call, Some(DebugChannelId::Internal));
    assert_eq!(channels.auto_events, Some(DebugChannelId::Internal));

    assert_eq!(
        h.manager.list_channels("999").await.unwrap_err(),
        DebugError::BotNotFound
    );
}

#[tokio::test]
async fn stopped_bot_reports_not_running_and_refuses_calls() {
    let h = harness();
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", 3000, "")];
    let bot = h.bots.add(config, false);
    let mut docker = remote_bot(20_002, DeploymentType::Docker);
    docker.connect.websocket_servers = vec![ws_server("odd", "0.0.0.0", 8080, WsRole::Api)];
    let docker = h.bots.add(docker, false);

    let channels = h.manager.list_channels(bot.as_str()).await.unwrap();
    assert!(
        channels
            .channels
            .iter()
            .all(|c| c.status == DebugChannelStatus::BotNotRunning)
    );
    // 注定连不上的原因比「没在跑」更有用，停着也照样显示
    assert_eq!(
        channel(&h.manager, &docker, &ws_id("odd")).await.status,
        DebugChannelStatus::Unsupported {
            reason: "容器没有映射 8080 端口".into()
        }
    );

    let err = call_err(
        &h.manager,
        request("r1", &bot, DebugChannelId::Auto, "get_status"),
    )
    .await;
    assert_eq!(err, DebugError::BotNotRunning);
    assert_eq!(
        call_err(
            &h.manager,
            request("r2", &BotId::new("999"), DebugChannelId::Auto, "get_status")
        )
        .await,
        DebugError::BotNotFound
    );
}

// ─── 调用 ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn napcat_internal_call_returns_the_ob11_reply() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    mount_napcat_call(&webui, "get_login_info", ob11_ok(json!({"user_id": 10001}))).await;
    mount_napcat_call(
        &webui,
        "send_group_msg",
        json!({"status": "failed", "retcode": 400, "data": null, "message": "参数错误", "wording": "缺少 group_id"}),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/api/Debug/call/debug-primary"))
        .and(body_partial_json(json!({"action": "no_such_action"})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"code": -1, "message": "不支持的 API: no_such_action"})),
        )
        .mount(&webui)
        .await;

    let outcome = call_ok(
        &h.manager,
        request("r1", &bot, DebugChannelId::Auto, "get_login_info"),
    )
    .await;
    assert!(outcome.ok);
    assert_eq!(outcome.channel, DebugChannelId::Internal);
    assert_eq!(outcome.data, json!({"user_id": 10001}));
    assert!(!outcome.truncated);

    let failed = call_ok(
        &h.manager,
        request("r2", &bot, DebugChannelId::Internal, "send_group_msg"),
    )
    .await;
    assert!(!failed.ok);
    assert_eq!(failed.retcode, 400);
    assert_eq!(failed.wording, "缺少 group_id");

    // 未知动作是「拿到了回答」，不是通道故障
    let unknown = call_ok(
        &h.manager,
        request("r3", &bot, DebugChannelId::Auto, "no_such_action"),
    )
    .await;
    assert!(!unknown.ok);
    assert_eq!(unknown.retcode, 1404);
    assert_eq!(unknown.message, "不支持的 API: no_such_action");

    assert_eq!(
        channel(&h.manager, &bot, &DebugChannelId::Internal)
            .await
            .status,
        DebugChannelStatus::Available
    );
}

#[tokio::test]
async fn snowluma_internal_call_invokes_with_the_bot_uin() {
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
    Mock::given(method("POST"))
        .and(path("/api/debug/invoke"))
        .and(body_partial_json(
            json!({"uin": "20002", "action": "get_status"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": "ok", "retcode": 0, "data": {"online": true}})),
        )
        .mount(&webui)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/debug/invoke"))
        .and(body_partial_json(json!({"action": "get_friend_list"})))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"status": "failed", "message": "账号不在线"})),
        )
        .mount(&webui)
        .await;

    let outcome = call_ok(
        &h.manager,
        request("r1", &bot, DebugChannelId::Auto, "get_status"),
    )
    .await;
    assert!(outcome.ok);
    assert_eq!(outcome.channel, DebugChannelId::Internal);

    let err = call_err(
        &h.manager,
        request("r2", &bot, DebugChannelId::Internal, "get_friend_list"),
    )
    .await;
    assert_eq!(err, DebugError::NotLoggedIn);
    assert_eq!(
        channel(&h.manager, &bot, &DebugChannelId::Internal)
            .await
            .status,
        DebugChannelStatus::NotLoggedIn
    );
}

#[tokio::test]
async fn auto_falls_back_to_http_when_the_internal_channel_is_too_old() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "get_status",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!({"good": true}))),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    Mock::given(method("POST"))
        .and(path("/api/Debug/call/debug-primary"))
        .respond_with(ResponseTemplate::new(404))
        // 第一次「自动」调用和最后显式选内部通道的那次；中间那次「自动」不该碰它
        .expect(2)
        .mount(&webui)
        .await;

    let outcome = call_ok(
        &h.manager,
        request("r1", &bot, DebugChannelId::Auto, "get_status"),
    )
    .await;
    assert!(outcome.ok);
    assert_eq!(outcome.channel, http_id("main"));

    let channels = h.manager.list_channels(bot.as_str()).await.unwrap();
    assert_eq!(
        channels.channels[0].status,
        DebugChannelStatus::UpstreamTooOld
    );
    assert_eq!(channels.channels[1].status, DebugChannelStatus::Available);
    assert_eq!(channels.auto_call, Some(http_id("main")));

    // 知道太老之后「自动」直接走 HTTP，不再去碰内部接口
    let again = call_ok(
        &h.manager,
        request("r2", &bot, DebugChannelId::Auto, "get_status"),
    )
    .await;
    assert_eq!(again.channel, http_id("main"));

    // 显式选内部通道时照样去试（上游可能已经升级），仍然太老就照实报
    assert_eq!(
        call_err(
            &h.manager,
            request("r3", &bot, DebugChannelId::Internal, "get_status")
        )
        .await,
        DebugError::UpstreamTooOld
    );
}

#[tokio::test]
async fn http_401_is_auth_failed_and_marks_the_channel() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(&onebot, "get_status", ResponseTemplate::new(401)).await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "127.0.0.1", port_of(&onebot), "bad")];
    let bot = h.bots.add(config, true);

    let err = call_err(
        &h.manager,
        request("r1", &bot, http_id("main"), "get_status"),
    )
    .await;
    assert_eq!(err, DebugError::AuthFailed { status: 401 });
    assert_eq!(
        channel(&h.manager, &bot, &http_id("main")).await.status,
        DebugChannelStatus::AuthFailed { status: 401 }
    );
}

#[tokio::test]
async fn cancel_stops_waiting_right_away() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "slow",
        ResponseTemplate::new(200)
            .set_body_json(ob11_ok(json!({})))
            .set_delay(Duration::from_secs(5)),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);

    let started = Instant::now();
    let manager = Arc::clone(&h.manager);
    let req = request("slow-1", &bot, http_id("main"), "slow");
    let pending = tokio::spawn(async move { manager.call(req).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    h.manager.cancel("slow-1");
    let response = pending.await.unwrap();

    assert_eq!(response.request_id, "slow-1");
    assert_eq!(
        response.result,
        DebugCallResult::Err {
            error: DebugError::Cancelled
        }
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "取消后应立即返回"
    );
    assert!(lock_std(&h.manager.inflight).is_empty(), "在途登记应已摘掉");
    // 取消不代表通道坏了
    assert_eq!(
        channel(&h.manager, &bot, &http_id("main")).await.status,
        DebugChannelStatus::Unknown
    );
}

#[tokio::test]
async fn short_timeouts_are_raised_to_the_minimum() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "slow",
        ResponseTemplate::new(200)
            .set_body_json(ob11_ok(json!({})))
            .set_delay(Duration::from_secs(5)),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);

    let mut req = request("r1", &bot, http_id("main"), "slow");
    // 200 ms 低于下限 1 s，按 1 s 算
    req.timeout_ms = Some(200);
    let started = Instant::now();
    let err = call_err(&h.manager, req).await;
    let elapsed = started.elapsed();
    assert_eq!(err, DebugError::Timeout { ms: 1000 });
    assert!(
        elapsed >= Duration::from_millis(900) && elapsed < Duration::from_secs(3),
        "{elapsed:?}"
    );
}

#[tokio::test]
async fn bad_params_and_receive_only_channels_are_rejected() {
    let h = harness();
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.websocket_servers = vec![ws_server("events", "0.0.0.0", 3001, WsRole::Event)];
    let bot = h.bots.add(config, true);

    let mut req = request("r1", &bot, DebugChannelId::Auto, "get_status");
    req.params = json!([1, 2]);
    assert!(matches!(
        call_err(&h.manager, req).await,
        DebugError::InvalidParams { .. }
    ));
    assert_eq!(
        call_err(
            &h.manager,
            request("r2", &bot, ws_id("events"), "get_status")
        )
        .await,
        DebugError::ChannelUnavailable {
            reason: "这条通道只收事件，不能调用".into()
        }
    );
    assert!(matches!(
        call_err(&h.manager, request("r3", &bot, ws_id("gone"), "get_status")).await,
        DebugError::ChannelUnavailable { .. }
    ));
}

#[tokio::test]
async fn ws_calls_share_one_connection() {
    let h = harness();
    let (port, connections) = spawn_ws_server().await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.websocket_servers = vec![ws_server("uni", "0.0.0.0", port, WsRole::Universal)];
    let bot = h.bots.add(config, true);

    for (i, action) in ["get_status", "get_login_info"].into_iter().enumerate() {
        let outcome = call_ok(
            &h.manager,
            request(&format!("r{i}"), &bot, ws_id("uni"), action),
        )
        .await;
        assert!(outcome.ok);
        assert_eq!(outcome.data, json!({"action": action}));
        assert_eq!(outcome.channel, ws_id("uni"));
    }
    let info = h
        .manager
        .test_channel(bot.as_str(), ws_id("uni"))
        .await
        .unwrap();
    assert_eq!(info.status, DebugChannelStatus::Available);
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn remote_channels_go_through_one_reused_tunnel() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "get_status",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!({}))),
    )
    .await;
    let port = port_of(&onebot);
    let mut config = remote_bot(10_001, DeploymentType::Native);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port, "")];
    let bot = h.bots.add(config, true);

    assert_eq!(
        channel(&h.manager, &bot, &http_id("main"))
            .await
            .endpoint
            .as_deref(),
        Some(format!("隧道 → 远端 127.0.0.1:{port}").as_str())
    );
    for i in 0..2 {
        let outcome = call_ok(
            &h.manager,
            request(&format!("r{i}"), &bot, http_id("main"), "get_status"),
        )
        .await;
        assert!(outcome.ok);
    }
    assert_eq!(
        h.host.opened(),
        vec![("127.0.0.1".to_owned(), port)],
        "隧道应只开一次"
    );
    assert_eq!(
        channel(&h.manager, &bot, &http_id("main")).await.status,
        DebugChannelStatus::Tunneled { local_port: port }
    );
}

#[tokio::test]
async fn oversized_reply_is_truncated_and_can_be_saved() {
    let h = harness();
    let onebot = MockServer::start().await;
    let body = ob11_ok(json!({"blob": "x".repeat(6 * 1024 * 1024)})).to_string();
    mount_onebot(
        &onebot,
        "get_big",
        ResponseTemplate::new(200).set_body_raw(body.clone(), "application/json"),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);

    let outcome = call_ok(&h.manager, request("big", &bot, http_id("main"), "get_big")).await;
    assert!(outcome.truncated);
    assert!(outcome.ok);

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("reply.json");
    h.manager.save_response("big", &file).await.unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), body);
    assert!(h.manager.save_response("other", &file).await.is_err());
}

#[tokio::test]
async fn full_system_face_catalog_reaches_the_picker_without_truncation() {
    let h = harness();
    let onebot = MockServer::start().await;
    let faces: Vec<_> = (0..700).map(|id| json!({"q_sid": id.to_string(), "q_des": "表情", "url": format!("https://qq.test/{}/{}.png", "x".repeat(500), id)})).collect();
    let data = json!({"packs": [{"pack_name": "系统表情", "emojis": faces}]});
    mount_onebot(
        &onebot,
        "fetch_sys_faces",
        ResponseTemplate::new(200).set_body_json(ob11_ok(data.clone())),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);
    let outcome = call_ok(
        &h.manager,
        request("faces", &bot, http_id("main"), "fetch_sys_faces"),
    )
    .await;
    assert!(outcome.ok);
    assert!(!outcome.truncated);
    assert_eq!(outcome.data, data);
}

// ─── 连通测试 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_channel_probes_and_records_the_status() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "get_status",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!({}))),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![
        http_server("main", "0.0.0.0", port_of(&onebot), ""),
        // 端口 1 上什么都没有
        http_server("dead", "127.0.0.1", 1, ""),
    ];
    let bot = h.bots.add(config, true);

    let info = h
        .manager
        .test_channel(bot.as_str(), http_id("main"))
        .await
        .unwrap();
    assert_eq!(info.status, DebugChannelStatus::Available);

    let dead = h
        .manager
        .test_channel(bot.as_str(), http_id("dead"))
        .await
        .unwrap();
    assert!(
        matches!(dead.status, DebugChannelStatus::Unreachable { .. }),
        "{:?}",
        dead.status
    );

    // 「自动」测的是内部通道；WebUI 端点还没出现
    let auto = h
        .manager
        .test_channel(bot.as_str(), DebugChannelId::Auto)
        .await
        .unwrap();
    assert_eq!(auto.id, DebugChannelId::Internal);
    assert!(matches!(
        auto.status,
        DebugChannelStatus::Unreachable { .. }
    ));
}

#[tokio::test]
async fn test_channel_internal_detects_an_old_upstream() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    Mock::given(method("GET"))
        .and(path("/api/Debug/schemas"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&webui)
        .await;

    let info = h
        .manager
        .test_channel(bot.as_str(), DebugChannelId::Internal)
        .await
        .unwrap();
    assert_eq!(info.status, DebugChannelStatus::UpstreamTooOld);
    let channels = h.manager.list_channels(bot.as_str()).await.unwrap();
    assert_eq!(channels.auto_call, None, "只剩太老的内部通道时没有自动通道");
}

// ─── 开关与收尾 ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn disabling_refuses_work_and_close_all_drops_sessions() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "get_status",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!({}))),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);

    call_ok(
        &h.manager,
        request("r1", &bot, http_id("main"), "get_status"),
    )
    .await;
    assert!(!h.manager.sessions.lock().await.is_empty());
    h.manager.close_all().await;
    assert!(h.manager.sessions.lock().await.is_empty());

    h.manager.set_enabled(false).await;
    assert!(!h.manager.is_enabled());
    assert_eq!(
        call_err(
            &h.manager,
            request("r2", &bot, http_id("main"), "get_status")
        )
        .await,
        DebugError::FeatureDisabled
    );
    assert_eq!(
        h.manager
            .test_channel(bot.as_str(), http_id("main"))
            .await
            .unwrap_err(),
        DebugError::FeatureDisabled
    );

    h.manager.set_enabled(true).await;
    call_ok(
        &h.manager,
        request("r3", &bot, http_id("main"), "get_status"),
    )
    .await;
}

// ─── 目录 ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn catalog_without_a_bot_is_the_snapshot() {
    let h = harness();
    let catalog = h.manager.catalog(None, BackendType::NapCat).await;
    assert_eq!(catalog.backend, BackendType::NapCat);
    assert_eq!(catalog.source, DebugCatalogSource::Snapshot);
    assert!(catalog.actions.len() >= 150, "{}", catalog.actions.len());
    assert!(catalog.actions.iter().all(|a| a.supported));
    assert!(
        catalog
            .actions
            .iter()
            .any(|a| a.other_backend_present == Some(true))
    );

    let spec = h
        .manager
        .describe(None, BackendType::SnowLuma, "send_group_msg")
        .await
        .unwrap();
    assert_eq!(spec.source, DebugCatalogSource::Snapshot);
    assert!(
        h.manager
            .describe(None, BackendType::NapCat, "no_such_action")
            .await
            .is_none()
    );
}

#[tokio::test]
async fn live_catalog_is_fetched_once_and_merged_with_the_snapshot() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    Mock::given(method("GET"))
        .and(path("/api/Debug/schemas"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 0, "message": "success",
            "data": {
                "get_login_info": {"description": "取登录号", "tags": ["用户接口"]},
                "brand_new_action": {"description": "只有这个版本才有", "tags": ["扩展接口"]},
            },
        })))
        .expect(1)
        .mount(&webui)
        .await;

    let catalog = h
        .manager
        .catalog(Some(bot.as_str()), BackendType::NapCat)
        .await;
    assert_eq!(catalog.source, DebugCatalogSource::Live);
    let fresh = catalog
        .actions
        .iter()
        .find(|a| a.name == "brand_new_action")
        .unwrap();
    assert!(fresh.supported);
    assert!(
        catalog.actions.iter().any(|a| !a.supported),
        "快照里有、这个版本没有的动作应标不支持"
    );

    // 第二次走会话缓存（上面的 schemas 只许命中一次）
    let spec = h
        .manager
        .describe(Some(bot.as_str()), BackendType::NapCat, "brand_new_action")
        .await
        .unwrap();
    assert_eq!(spec.source, DebugCatalogSource::Live);

    // 问另一个后端的目录：这个 Bot 回答不了，给快照
    let other = h
        .manager
        .catalog(Some(bot.as_str()), BackendType::SnowLuma)
        .await;
    assert_eq!(other.source, DebugCatalogSource::Snapshot);
}

#[tokio::test]
async fn live_catalog_falls_back_to_the_snapshot_when_upstream_is_old() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    Mock::given(method("GET"))
        .and(path("/api/Debug/schemas"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&webui)
        .await;

    for _ in 0..2 {
        let catalog = h
            .manager
            .catalog(Some(bot.as_str()), BackendType::NapCat)
            .await;
        assert_eq!(catalog.source, DebugCatalogSource::Snapshot);
    }
    assert_eq!(
        channel(&h.manager, &bot, &DebugChannelId::Internal)
            .await
            .status,
        DebugChannelStatus::UpstreamTooOld
    );
}

// ─── 收尾时的竞争、重启后的重试、早到的取消、目录失败冷却 ────────────────────────

#[tokio::test]
async fn disabling_during_a_probe_leaves_no_session_or_tunnel() {
    let h = harness();
    h.host.set_delay(Duration::from_millis(300));
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "get_status",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!({}))),
    )
    .await;
    let mut config = remote_bot(10_001, DeploymentType::Native);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);

    let manager = Arc::clone(&h.manager);
    let id = bot.clone();
    let probe =
        tokio::spawn(async move { manager.test_channel(id.as_str(), http_id("main")).await });
    // 隧道正开到一半时关掉调试台
    h.host.requested.notified().await;
    h.manager.set_enabled(false).await;

    let result = tokio::time::timeout(Duration::from_secs(1), probe)
        .await
        .expect("关掉之后探测应立刻收手")
        .unwrap();
    assert_eq!(result.unwrap_err(), DebugError::FeatureDisabled);
    // 等过隧道本该开好的时刻：没有迟到的写入把会话或隧道建回来
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(h.manager.sessions.lock().await.is_empty());
}

#[tokio::test]
async fn a_tunnel_that_opens_after_close_all_is_discarded() {
    let h = harness();
    h.host.set_delay(Duration::from_millis(300));
    let bot = h.bots.add(remote_bot(10_001, DeploymentType::Native), true);
    let view = h.bots.bot(&bot).await.unwrap();
    let epoch = h.manager.epoch();

    // 不经过带取消的外层，直接走到「隧道开好了、回来写表」这一步
    let manager = Arc::clone(&h.manager);
    let task = tokio::spawn(async move {
        manager
            .ensure_tunnel(&view, "http:main", "127.0.0.1", 3000, &epoch)
            .await
    });
    h.host.requested.notified().await;
    // 开关没关，只是收尾（比如退出程序）
    h.manager.close_all().await;

    let failure = task.await.unwrap().unwrap_err();
    assert_eq!(failure.error, DebugError::Cancelled);
    assert_eq!(
        h.host.opened(),
        vec![("127.0.0.1".to_owned(), 3000)],
        "隧道确实开成了，只是没被留下"
    );
    assert!(h.manager.sessions.lock().await.is_empty());
}

#[tokio::test]
async fn a_restart_lets_the_internal_channel_be_tried_again() {
    let h = harness();
    let onebot = MockServer::start().await;
    mount_onebot(
        &onebot,
        "get_status",
        ResponseTemplate::new(200).set_body_json(ob11_ok(json!({}))),
    )
    .await;
    let mut config = local_bot(10_001, BackendType::NapCat);
    config.connect.http_servers = vec![http_server("main", "0.0.0.0", port_of(&onebot), "")];
    let bot = h.bots.add(config, true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    Mock::given(method("POST"))
        .and(path("/api/Debug/call/debug-primary"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&webui)
        .await;

    let outcome = call_ok(
        &h.manager,
        request("r1", &bot, DebugChannelId::Auto, "get_status"),
    )
    .await;
    assert_eq!(outcome.channel, http_id("main"));
    assert_eq!(
        channel(&h.manager, &bot, &DebugChannelId::Internal)
            .await
            .status,
        DebugChannelStatus::UpstreamTooOld
    );

    // 用户升级了 NapCat 并重启了 Bot（WebUI 端口和令牌恰好没变）
    webui.reset().await;
    mount_napcat_login(&webui).await;
    mount_napcat_call(&webui, "get_status", ob11_ok(json!({"upgraded": true}))).await;
    h.bots.restart(&bot);

    let channels = h.manager.list_channels(bot.as_str()).await.unwrap();
    assert_eq!(channels.channels[0].status, DebugChannelStatus::Unknown);
    assert_eq!(channels.auto_call, Some(DebugChannelId::Internal));
    let outcome = call_ok(
        &h.manager,
        request("r2", &bot, DebugChannelId::Auto, "get_status"),
    )
    .await;
    assert_eq!(outcome.channel, DebugChannelId::Internal);
    assert_eq!(outcome.data, json!({"upgraded": true}));
}

#[tokio::test]
async fn cancel_that_arrives_while_the_bot_is_looked_up_is_not_lost() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    h.bots.set_lookup_delay(Duration::from_millis(500));

    let started = Instant::now();
    let manager = Arc::clone(&h.manager);
    let req = request("early", &bot, DebugChannelId::Auto, "get_status");
    let pending = tokio::spawn(async move { manager.call(req).await });
    // 调用已经开始、但还在找 Bot（第一个 await）时取消
    tokio::time::sleep(Duration::from_millis(50)).await;
    h.manager.cancel("early");

    let response = pending.await.unwrap();
    assert_eq!(
        response.result,
        DebugCallResult::Err {
            error: DebugError::Cancelled
        }
    );
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "不该等查找结束：{:?}",
        started.elapsed()
    );
    assert!(lock_std(&h.manager.inflight).is_empty());
}

#[tokio::test]
async fn a_failed_live_catalog_fetch_cools_down_until_the_bot_restarts() {
    let h = harness();
    let bot = h.bots.add(local_bot(10_001, BackendType::NapCat), true);
    let webui = napcat_webui().await;
    h.bots.set_napcat(&bot, port_of(&webui));
    Mock::given(method("GET"))
        .and(path("/api/Debug/schemas"))
        .respond_with(ResponseTemplate::new(500))
        // 第一次失败；冷却期内的第二次不打上游；重启后的第三次重新取
        .expect(2)
        .mount(&webui)
        .await;

    for _ in 0..2 {
        let catalog = h
            .manager
            .catalog(Some(bot.as_str()), BackendType::NapCat)
            .await;
        assert_eq!(catalog.source, DebugCatalogSource::Snapshot);
    }
    assert!(
        h.manager
            .describe(Some(bot.as_str()), BackendType::NapCat, "send_group_msg")
            .await
            .is_some()
    );

    h.bots.restart(&bot);
    let catalog = h
        .manager
        .catalog(Some(bot.as_str()), BackendType::NapCat)
        .await;
    assert_eq!(catalog.source, DebugCatalogSource::Snapshot);
    // 普通失败不等于「太老」
    assert_ne!(
        channel(&h.manager, &bot, &DebugChannelId::Internal)
            .await
            .status,
        DebugChannelStatus::UpstreamTooOld
    );
}
