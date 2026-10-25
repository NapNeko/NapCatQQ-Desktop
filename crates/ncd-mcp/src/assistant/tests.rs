use super::*;

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ncd_domain::assistant::AssistantPermissionRequest;
use ncd_domain::{BackendKind, BotFlavor, BotId, BotStatus, RuntimeTarget, StopMode};
use ncd_host::Host;
use ncd_runtime::components::ComponentExecutorDeps;
use ncd_runtime::{
    AppFrameworkRegistry, AppInstanceStore, AppManager, BotManager, BroadcastEventBus,
    ComponentExecutor, ConfigStore, DeploymentTaskManager, DispatchRenderer,
    FileSystemRuntimeLaunchPlanner, HostResolveError, HostResolver, InMemoryCredentialStore,
    LocalBotConfigRepo, LocalConfigStore, NativeAppRuntime, NoopOfflineNotifier,
    ReqwestNapCatWebUiClient, ReqwestSnowLumaWebUiClientFactory, ServerManager, SnowLumaDaemon,
};
use ncd_test_support::{BotConfigBuilder, MockSecretStore};
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotStartCtx, LogSnapshot, TailOpts,
};
use serde_json::json;
use tokio::sync::{Notify, RwLock};
use tower::ServiceExt;

type TestBots = BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>;

#[derive(Default)]
struct Counts {
    starts: AtomicUsize,
    stops: StdMutex<Vec<String>>,
    running: StdMutex<HashSet<String>>,
    host_resolutions: AtomicUsize,
}

struct Backend {
    counts: Arc<Counts>,
    flavor: BotFlavor,
}

#[async_trait]
impl BotBackend for Backend {
    fn id(&self) -> &BotId {
        static ID: std::sync::OnceLock<BotId> = std::sync::OnceLock::new();
        ID.get_or_init(|| BotId::new("assistant-test-backend"))
    }
    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }
    fn flavor(&self) -> BotFlavor {
        self.flavor
    }
    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        self.counts.starts.fetch_add(1, Ordering::SeqCst);
        lock(&self.counts.running).insert(ctx.config.bot_id.to_string());
        Ok(BotStatus::running(ctx.config.bot_id.clone(), 1, 1))
    }
    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        lock(&self.counts.stops).push(bot_id.to_string());
        lock(&self.counts.running).remove(bot_id.as_str());
        Ok(())
    }
    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        Ok(if lock(&self.counts.running).contains(bot_id.as_str()) {
            BotStatus::running(bot_id, 1, 1)
        } else {
            BotStatus::stopped(bot_id)
        })
    }
    async fn tail_log(
        &self,
        _bot_id: BotId,
        _options: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        Ok(LogSnapshot {
            lines: vec!["fixture shared daemon private-other-bot-line".into()],
            total_lines: 1,
        })
    }
}

struct NoHosts(Arc<Counts>);

#[async_trait]
impl HostResolver for NoHosts {
    async fn resolve(&self, _target: &RuntimeTarget) -> Result<Arc<dyn Host>, HostResolveError> {
        self.0.host_resolutions.fetch_add(1, Ordering::SeqCst);
        Err(HostResolveError::message("test forbids host access"))
    }
}

struct Approval {
    answer: Option<&'static str>,
    wait: bool,
    entered: Notify,
    calls: AtomicUsize,
}

impl Approval {
    fn new(answer: Option<&'static str>) -> Arc<Self> {
        Arc::new(Self {
            answer,
            wait: false,
            entered: Notify::new(),
            calls: AtomicUsize::new(0),
        })
    }
    fn waiting() -> Arc<Self> {
        Arc::new(Self {
            answer: None,
            wait: true,
            entered: Notify::new(),
            calls: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl AssistantInteraction for Approval {
    async fn request_permission(
        &self,
        request: AssistantPermissionRequest,
    ) -> Result<Option<String>, String> {
        assert_eq!(
            request
                .options
                .iter()
                .map(|option| option.kind.as_str())
                .collect::<Vec<_>>(),
            ["allow_once", "reject_once"]
        );
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if self.wait {
            std::future::pending::<()>().await;
        }
        Ok(self.answer.map(str::to_owned))
    }
}

struct Harness {
    _root: tempfile::TempDir,
    bots: Arc<TestBots>,
    debug: Arc<DebugManager>,
    inspection: Arc<McpInspection>,
    apps: Arc<AppInstanceStore>,
    counts: Arc<Counts>,
}

impl Harness {
    async fn mark_running(&self, bot_id: &str) {
        lock(&self.counts.running).insert(bot_id.to_owned());
        self.bots
            .test_confirm_actor_running(&BotId::new(bot_id))
            .await
            .expect("running fixture");
    }

    async fn new() -> Self {
        let root = tempfile::tempdir().expect("isolated data root");
        let counts = Arc::new(Counts::default());
        let bus = BroadcastEventBus::default();
        let store = Arc::new(LocalConfigStore::new(root.path()));
        let repo = Arc::new(LocalBotConfigRepo::new(
            store.clone(),
            Arc::new(MockSecretStore::new()),
        ));
        let backend: Arc<dyn BotBackend> = Arc::new(Backend {
            counts: counts.clone(),
            flavor: BotFlavor::NapCat,
        });
        let snowluma: Arc<dyn BotBackend> = Arc::new(Backend {
            counts: counts.clone(),
            flavor: BotFlavor::SnowLuma,
        });
        let daemon = SnowLumaDaemon::new(
            root.path().join("snowluma-data"),
            root.path().join("snowluma-runtime"),
            Arc::new(bus.clone()),
            Arc::new(ReqwestSnowLumaWebUiClientFactory::new()),
        );
        let bots = Arc::new(
            BotManager::new(
                repo,
                store.clone(),
                Arc::new(DispatchRenderer::new(
                    store.config_dir(),
                    store.config_dir(),
                )),
                backend,
                Arc::new(FileSystemRuntimeLaunchPlanner::new(
                    root.path().join("components"),
                )),
                Arc::new(bus.clone()),
                Arc::new(ReqwestNapCatWebUiClient::new().expect("HTTP client")),
                Arc::new(NoopOfflineNotifier),
                Arc::new(RwLock::new(Default::default())),
                Arc::new(RwLock::new(Default::default())),
            )
            .with_snowluma(snowluma, daemon),
        );
        for (id, backend) in [
            (10001, BackendType::NapCat),
            (10002, BackendType::NapCat),
            (10003, BackendType::SnowLuma),
        ] {
            bots.upsert_bot_config(BotConfigBuilder::new().qq_id(id).backend_type(backend).name(format!("fixture-{id}")).build()).await
                .unwrap_or_else(|error| panic!("fixture seed failed: bot={id}, backend={backend:?}, phase=upsert_without_startup, error={error:?}"));
        }
        let resolver: Arc<dyn HostResolver> = Arc::new(NoHosts(counts.clone()));
        let debug = Arc::new(DebugManager::new_ephemeral(
            bots.clone(),
            resolver.clone(),
            root.path().to_path_buf(),
        ));
        let registry = Arc::new(AppFrameworkRegistry::new());
        let apps = Arc::new(AppInstanceStore::empty(root.path()));
        let app_manager = Arc::new(AppManager::new(
            registry.clone(),
            apps.clone(),
            Arc::new(NativeAppRuntime::new(Arc::new(bus.clone()), apps.clone())),
            resolver,
            bots.clone(),
            Arc::new(bus.clone()),
            root.path(),
        ));
        let servers = Arc::new(ServerManager::new(
            root.path(),
            Arc::new(InMemoryCredentialStore::default()),
        ));
        let components = Arc::new(ComponentExecutor::new(ComponentExecutorDeps {
            deployment_tasks: DeploymentTaskManager::new(bus.clone()),
            server_manager: servers.clone(),
            event_bus: bus,
            app_settings: Arc::new(RwLock::new(Default::default())),
            data_root: root.path().to_path_buf(),
            local_snowluma_version: None,
            desktop_product_version: "0.0.0-test".into(),
            registry,
        }));
        let inspection = Arc::new(McpInspection::new(
            bots.clone(),
            app_manager,
            servers,
            components,
        ));
        Self {
            _root: root,
            bots,
            debug,
            inspection,
            apps,
            counts,
        }
    }

    fn ctx(&self, bot: &str, backend: BackendType, approval: Arc<Approval>) -> Arc<SessionCtx> {
        Arc::new(SessionCtx {
            server: ServerCtx {
                tools: ToolsCtx {
                    debug: self.debug.clone(),
                    runtime: Some(self.inspection.clone()),
                    gate: Arc::new(GateKeeper::new()),
                    status: Arc::new(|| McpServerStatus {
                        enabled: true,
                        listening: true,
                        port: Some(31337),
                        last_error: None,
                    }),
                },
                token: "fixture-current-token".into(),
                bound_port: 31337,
                requests: Semaphore::new(4),
                cancel: CancellationToken::new(),
            },
            session_id: "fixture-session".into(),
            bot_id: bot.into(),
            backend,
            inspection: self.inspection.clone(),
            interaction: approval,
            operation_gate: Arc::new(|| Ok(())),
            operation_lock: Mutex::new(()),
            secrets: Arc::new(StdMutex::new(SessionSecrets::default())),
        })
    }

    async fn linked_app(&self, id: &str, bot: &str) {
        let app = serde_json::from_value(json!({
            "id": id, "framework_id": "fixture-framework", "display_name": id, "placement":"local_native",
            "host_id":"local", "install_dir":"/fixture/private-app-path", "port":12345, "state":"stopped",
            "link":{"bot_id":bot, "mode":"reverse_ws", "connection_name":"fixture-secret-connection", "linked_at_ms":1},
        })).expect("app fixture");
        self.apps.upsert(app).await.expect("seed linked app");
    }
}

fn call(name: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{"name":name,"arguments":arguments}})
}

async fn rpc(ctx: &SessionCtx, body: Value) -> Value {
    rpc::handle_body(ctx, &body).await.expect("RPC response")
}

fn tool_value(response: &Value) -> Value {
    if let Some(value) = response["result"].get("structuredContent") {
        return value.clone();
    }
    serde_json::from_str(
        response["result"]["content"][0]["text"]
            .as_str()
            .expect("tool text"),
    )
    .expect("tool JSON")
}

fn failed(response: &Value) -> bool {
    response.get("error").is_some() || response["result"]["isError"] == true
}

async fn http_call(ctx: Arc<SessionCtx>, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("host", "127.0.0.1:31337")
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = http::router(ctx)
        .oneshot(request.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("HTTP response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn http_requires_current_bearer_and_rejects_cancelled_sessions() {
    let h = Harness::new().await;
    let ctx = h.ctx("10001", BackendType::NapCat, Approval::new(None));
    let ping = json!({"jsonrpc":"2.0", "id":1, "method":"ping"});
    assert_eq!(
        http_call(ctx.clone(), None, ping.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http_call(ctx.clone(), Some("fixture-retired-token"), ping.clone())
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http_call(ctx.clone(), Some("fixture-current-token"), ping.clone())
            .await
            .0,
        StatusCode::OK
    );
    ctx.server.cancel.cancel();
    assert_eq!(
        http_call(ctx, Some("fixture-current-token"), ping).await.0,
        StatusCode::GONE
    );
    assert_eq!(h.counts.host_resolutions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn catalog_exposes_only_target_bound_read_tools_and_desktop_operations() {
    let h = Harness::new().await;
    let ctx = h.ctx("10001", BackendType::NapCat, Approval::new(None));
    let response = rpc(
        &ctx,
        json!({"jsonrpc":"2.0", "id":1, "method":"tools/list"}),
    )
    .await;
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tool catalog");
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        names,
        [
            "diagnose_bot",
            "get_bot_runtime",
            "tail_bot_logs",
            "list_channels",
            "list_actions",
            "describe_action",
            "search_actions",
            "get_security_policy",
            "call_action",
            "start_bot",
            "stop_bot",
            "restart_bot"
        ]
        .into_iter()
        .collect()
    );
    for tool in tools {
        let properties = &tool["inputSchema"]["properties"];
        assert!(properties.get("confirm_token").is_none());
        if let Some(bot) = properties.get("bot_id") {
            assert_eq!(bot["enum"], json!(["10001"]));
        }
        if let Some(backend) = properties.get("backend") {
            assert_eq!(backend["enum"], json!(["napcat"]));
        }
    }
    for name in [
        "read_file",
        "list_servers",
        "get_app_runtime",
        "list_app_instances",
        "get_bot_config",
        "confirm_action",
    ] {
        assert!(failed(&rpc(&ctx, call(name, json!({}))).await));
    }
}

#[tokio::test]
async fn calls_cannot_override_bot_backend_or_supply_confirmation_tokens() {
    let h = Harness::new().await;
    let approval = Approval::new(Some("allow_once"));
    let ctx = h.ctx("10001", BackendType::NapCat, approval.clone());
    for (name, args) in [
        ("get_bot_runtime", json!({"bot_id":"10002"})),
        ("tail_bot_logs", json!({"bot_id":"10002"})),
        ("list_actions", json!({"backend":"snowluma"})),
        ("stop_bot", json!({"bot_id":"10002"})),
        ("stop_bot", json!({"confirm_token":"fixture-confirm"})),
        (
            "get_security_policy",
            json!({"confirm_token":"fixture-confirm"}),
        ),
        ("stop_bot", json!({"approved":true})),
    ] {
        assert!(
            failed(&rpc(&ctx, call(name, args)).await),
            "unexpected authorization for {name}"
        );
    }
    assert_eq!(approval.calls.load(Ordering::SeqCst), 0);
    assert!(lock(&h.counts.stops).is_empty());
    assert_eq!(h.counts.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn onebot_scope_contains_only_health_checks_and_rejects_message_and_file_reads() {
    let h = Harness::new().await;
    let ctx = h.ctx("10001", BackendType::NapCat, Approval::new(None));
    let policy = tool_value(&rpc(&ctx, call("get_security_policy", json!({}))).await);
    assert_eq!(
        policy["readOnlyActions"],
        json!([
            "get_login_info",
            "get_status",
            "get_version_info",
            "can_send_image",
            "can_send_record"
        ])
    );
    assert_eq!(policy["otherBotsAllowed"], false);
    for action in [
        "send_group_msg",
        "set_group_ban",
        "get_group_msg_history",
        "get_friend_list",
        "get_group_info",
        "get_file",
        "download_file",
        "get_cookies",
    ] {
        assert!(failed(
            &rpc(
                &ctx,
                call("call_action", json!({"action":action, "params":{}}))
            )
            .await
        ));
    }
    assert_eq!(h.counts.host_resolutions.load(Ordering::SeqCst), 0);
    assert_eq!(h.counts.starts.load(Ordering::SeqCst), 0);
    assert!(lock(&h.counts.stops).is_empty());
}

#[tokio::test]
async fn unapproved_or_persistent_approval_never_executes_bot_operations() {
    let h = Harness::new().await;
    h.mark_running("10001").await;
    for decision in [
        None,
        Some("reject_once"),
        Some("allow_always"),
        Some("fixture-confirm"),
    ] {
        let ctx = h.ctx("10001", BackendType::NapCat, Approval::new(decision));
        assert!(failed(&rpc(&ctx, call("stop_bot", json!({}))).await));
    }
    assert!(lock(&h.counts.stops).is_empty());
    assert_eq!(h.counts.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancellation_during_desktop_approval_prevents_submission() {
    let h = Harness::new().await;
    h.mark_running("10001").await;
    let approval = Approval::waiting();
    let ctx = h.ctx("10001", BackendType::NapCat, approval.clone());
    let task_ctx = ctx.clone();
    let pending = tokio::spawn(async move { rpc(&task_ctx, call("stop_bot", json!({}))).await });
    approval.entered.notified().await;
    ctx.server.cancel.cancel();
    assert!(failed(&pending.await.expect("cancelled RPC")));
    assert!(lock(&h.counts.stops).is_empty());
}

#[tokio::test]
async fn desktop_allow_once_is_rechecked_against_runtime_operation_gate() {
    let h = Harness::new().await;
    h.mark_running("10001").await;
    let mut denied = h.ctx(
        "10001",
        BackendType::NapCat,
        Approval::new(Some("allow_once")),
    );
    Arc::get_mut(&mut denied)
        .expect("unique test context")
        .operation_gate = Arc::new(|| Err("fixture desktop gate closed".into()));
    assert!(failed(&rpc(&denied, call("stop_bot", json!({}))).await));
    assert!(lock(&h.counts.stops).is_empty());
    let allowed = h.ctx(
        "10001",
        BackendType::NapCat,
        Approval::new(Some("allow_once")),
    );
    let response = rpc(&allowed, call("stop_bot", json!({}))).await;
    assert!(!failed(&response), "{response}");
    assert_eq!(*lock(&h.counts.stops), vec!["10001"]);
    assert!(!lock(&h.counts.running).contains("10001"));
    assert_eq!(tool_value(&response)["runtime"]["state"], "stopped");
}

#[tokio::test]
async fn context_includes_only_linked_apps_and_shared_snowluma_logs_are_withheld() {
    let h = Harness::new().await;
    h.linked_app("linked-to-current", "10001").await;
    h.linked_app("private-other-app", "10002").await;
    let (_, _, context) = h
        .inspection
        .assistant_context("10001")
        .await
        .expect("assistant context");
    assert_eq!(
        context["linked_apps"]
            .as_array()
            .expect("linked apps")
            .len(),
        1
    );
    assert_eq!(
        context["linked_apps"][0]["instance_id"],
        "linked-to-current"
    );
    let encoded = context.to_string();
    for private in [
        "private-other-app",
        "fixture-secret-connection",
        "/fixture/private-app-path",
    ] {
        assert!(!encoded.contains(private));
    }
    let direct = h
        .inspection
        .dispatch("tail_bot_logs", &json!({"bot_id":"10003"}))
        .await
        .expect("fixture cached logs");
    assert_eq!(direct["scope"], "shared_daemon");
    assert!(direct.to_string().contains("private-other-bot-line"));
    let ctx = h.ctx("10003", BackendType::SnowLuma, Approval::new(None));
    let response = tool_value(&rpc(&ctx, call("tail_bot_logs", json!({}))).await);
    assert_eq!(response["available"], false);
    assert_eq!(response["lines"], json!([]));
    assert!(!response.to_string().contains("private-other-bot-line"));
    assert_eq!(h.counts.host_resolutions.load(Ordering::SeqCst), 0);
}

#[test]
fn retired_tokens_are_still_redacted_and_are_removed_from_active_authority() {
    let mut secrets = SessionSecrets::default();
    secrets
        .active
        .insert("old-session".into(), "fixture-retired-token".into());
    secrets.retire("old-session");
    assert!(!secrets.active.contains_key("old-session"));
    assert_eq!(
        secrets.redact_exact("before fixture-retired-token after"),
        "before [REDACTED] after"
    );
    for index in 0..100 {
        secrets
            .active
            .insert(format!("session-{index}"), format!("token-{index:03}"));
        secrets.retire(&format!("session-{index}"));
    }
    assert_eq!(secrets.retired.len(), RETIRED_SECRETS);
}
