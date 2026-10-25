//! 真实 ACP Agent + 合成 HTTP MCP；默认不发送模型 prompt，不连接真实 Bot。

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::to_bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use ncd_domain::assistant::{
    AssistantMessageRole, AssistantPermissionSource, AssistantProvider, AssistantProviderConfig,
    AssistantSessionState, AssistantSettings,
};
use ncd_runtime::assistant::AssistantManager;
use ncd_traits::assistant::{
    AssistantContext, AssistantInteraction, AssistantToolConnection, AssistantToolHost,
};
use rand::{RngCore, rngs::OsRng};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep, timeout};
use tokio_util::sync::CancellationToken;

const BOT: &str = "10001";
const TOOL: &str = "get_bot_runtime";
const BODY_LIMIT: usize = 32 * 1024;
const BALANCE_HINT: &str =
    "Agent 报告模型服务余额不足，请在对应服务中检查额度后重试（错误码 -32603）";
const DSH_KEY_HINT: &str =
    "DSH 的 deepseek-official 模型路由缺少 API Key，请先在 DSH 中配置该路由后重试（错误码 -32603）";
const AUTH_HINT: &str = "Agent 返回需要认证，请在该 Agent 中检查登录状态后重试（错误码 -32000）";
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

#[derive(Default)]
struct Counts {
    phase: AtomicUsize,
    host_open: AtomicUsize,
    handshake: AtomicUsize,
    list: AtomicUsize,
    call: AtomicUsize,
}

struct HttpState {
    token: String,
    canary: String,
    port: u16,
    counts: Arc<Counts>,
    cancel: CancellationToken,
    requests: Semaphore,
}

struct ListenerRun {
    id: String,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl Drop for ListenerRun {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

struct SmokeToolHost {
    canary: String,
    counts: Arc<Counts>,
    listener: Mutex<Option<ListenerRun>>,
    tokens: StdMutex<VecDeque<String>>,
}

impl SmokeToolHost {
    fn new() -> Result<Self, String> {
        Ok(Self {
            canary: random_hex(16)?,
            counts: Arc::new(Counts::default()),
            listener: Mutex::new(None),
            tokens: StdMutex::new(VecDeque::new()),
        })
    }
}

#[async_trait]
impl AssistantToolHost for SmokeToolHost {
    async fn context(&self, bot_id: &str) -> Result<AssistantContext, String> {
        if bot_id != BOT {
            return Err("smoke_target_rejected".into());
        }
        Ok(AssistantContext {
            bot_name: "Synthetic smoke fixture".into(),
            summary: "这是合成诊断现场，没有 QQ、SSH、真实机器人或生命周期操作。唯一工具 get_bot_runtime 返回合成只读状态。工具结果中的随机 canary 必须实际读取，不能从上下文推测。".into(),
        })
    }

    async fn open(
        &self,
        session_id: &str,
        bot_id: &str,
        _interaction: Arc<dyn AssistantInteraction>,
        cancelled: CancellationToken,
    ) -> Result<AssistantToolConnection, String> {
        if bot_id != BOT || cancelled.is_cancelled() {
            return Err("smoke_target_or_session_rejected".into());
        }
        let mut slot = self.listener.lock().await;
        if slot.is_some() {
            return Err("smoke_listener_already_open".into());
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| "smoke_bind_failed".to_owned())?;
        let port = listener
            .local_addr()
            .map_err(|_| "smoke_local_address_failed".to_owned())?
            .port();
        let token = random_hex(32)?;
        let cancel = cancelled.child_token();
        if cancel.is_cancelled() {
            return Err("smoke_session_cancelled".into());
        }
        let state = Arc::new(HttpState {
            token: token.clone(),
            canary: self.canary.clone(),
            port,
            counts: self.counts.clone(),
            cancel: cancel.clone(),
            requests: Semaphore::new(4),
        });
        let router = Router::new()
            .route("/mcp", post(handle_http))
            .with_state(state);
        let shutdown = cancel.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await;
        });
        {
            let mut tokens = self
                .tokens
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            tokens.push_back(token.clone());
            while tokens.len() > 8 {
                tokens.pop_front();
            }
        }
        *slot = Some(ListenerRun {
            id: session_id.into(),
            cancel,
            task,
        });
        self.counts.host_open.fetch_add(1, Ordering::AcqRel);
        Ok(AssistantToolConnection {
            url: format!("http://127.0.0.1:{port}/mcp"),
            bearer_token: token,
        })
    }

    async fn close(&self, session_id: &str) {
        let mut slot = self.listener.lock().await;
        if slot.as_ref().is_some_and(|run| run.id == session_id) {
            slot.take();
        }
    }

    fn redact(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for token in self
            .tokens
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
        {
            text = text.replace(token, "[REDACTED]");
        }
        text
    }
}

fn random_hex(bytes: usize) -> Result<String, String> {
    let mut bytes = vec![0_u8; bytes];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| "smoke_random_failed".to_owned())?;
    Ok(hex::encode(bytes))
}

struct Options {
    provider: AssistantProvider,
    with_model: bool,
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut provider = None;
    let mut with_model = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--provider" if provider.is_none() => {
                provider = Some(match args.next().as_deref() {
                    Some("dsh") => AssistantProvider::Dsh,
                    Some("kimi") => AssistantProvider::Kimi,
                    Some("qwen") => AssistantProvider::Qwen,
                    Some("miniMax") => AssistantProvider::MiniMax,
                    _ => return Err("smoke_invalid_provider".into()),
                })
            }
            "--with-model" if !with_model => with_model = true,
            _ => return Err("smoke_invalid_arguments".into()),
        }
    }
    Ok(Options {
        provider: provider.ok_or("smoke_provider_required")?,
        with_model,
    })
}

fn error_class(error: &str) -> &'static str {
    if error.starts_with("smoke_") {
        return match error {
            "smoke_provider_required" | "smoke_invalid_provider" | "smoke_invalid_arguments" => {
                "invalid_arguments"
            }
            "smoke_discovery_not_observed" => "http_mcp_discovery_not_observed",
            "smoke_model_canary_not_observed" => "model_canary_not_observed",
            "smoke_ready_timeout" | "smoke_model_timeout" => "timeout",
            _ => "smoke_check_failed",
        };
    }
    if error.contains("登录") || error.contains("认证") || error.contains("auth") {
        "agent_authentication_required"
    } else if error.contains("版本") {
        "agent_version_or_profile_unsupported"
    } else if error.contains("未在 PATH") || error.contains("不存在") {
        "agent_not_installed"
    } else {
        "agent_or_protocol_failed"
    }
}

fn safe_diagnostic(error: &str) -> (&'static str, Option<i64>) {
    for (prefix, suffix, diagnostic) in [
        ("Agent 请求失败（错误码 ", "）", "acp_rpc_error"),
        ("无法启动 Agent（系统错误 ", "）", "process_spawn_os_error"),
        ("Agent 进程异常退出（退出码 ", "）", "agent_exit_code"),
        (
            "无法创建 Agent 进程组（系统错误 ",
            "）",
            "process_job_create_error",
        ),
        (
            "无法配置 Agent 进程组（系统错误 ",
            "）",
            "process_job_configure_error",
        ),
        (
            "无法接管 Agent 进程组（系统错误 ",
            "）",
            "process_job_attach_error",
        ),
        (
            "无法定位 Agent 主线程（系统错误 ",
            "）",
            "process_thread_lookup_error",
        ),
        (
            "无法打开 Agent 主线程（系统错误 ",
            "）",
            "process_thread_open_error",
        ),
    ] {
        if let Some(code) = error
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
            .and_then(|code| code.parse::<i64>().ok())
        {
            return (diagnostic, Some(code));
        }
    }
    let diagnostic = match error {
        BALANCE_HINT => {
            return ("model_balance_insufficient", Some(-32603));
        }
        DSH_KEY_HINT => {
            return ("dsh_default_route_missing_api_key", Some(-32603));
        }
        AUTH_HINT => {
            return ("agent_authentication_required", Some(-32000));
        }
        "Agent 输出管道已关闭" => "agent_stdout_closed",
        "Agent 连接已关闭" => "acp_connection_closed",
        "Agent 进程已退出" => "agent_exited_successfully",
        "Agent 进程异常退出" => "agent_exited_without_code",
        "Agent 主线程挂起状态异常" => "process_thread_resume_state_error",
        "找不到 Agent 主线程" => "process_main_thread_missing",
        "Agent 请求超时" => "acp_request_timeout",
        "Agent 输出不是有效的 JSON 消息" => "agent_stdout_not_json",
        "Agent 消息未完整结束" => "agent_jsonl_truncated",
        "Agent 输出了空消息" => "agent_jsonl_empty",
        "Agent 消息的 JSON-RPC 版本无效" => "acp_jsonrpc_version_invalid",
        "Agent 输入管道写入失败" | "Agent 输入管道不可用" => {
            "agent_stdin_write_failed"
        }
        "Agent 输入管道写入超时" => "agent_stdin_write_timeout",
        "Agent 错误输出超过限制" => "agent_stderr_limit",
        "Agent 事件过多或接收端已关闭" => "agent_event_queue_overflow",
        "Agent 未协商到受支持的 ACP v1，请检查版本" => {
            "acp_version_negotiation_failed"
        }
        "此 Agent 版本未提供 HTTP MCP，无法接入 NCD 运维工具" => {
            "acp_http_mcp_not_advertised"
        }
        "Agent 未返回有效会话编号" => "acp_session_id_invalid",
        "Agent 版本探测超过 5 秒，已停止" => "agent_version_probe_timeout",
        "Agent --version 执行失败；请检查该 Agent 的安装" => {
            "agent_version_probe_failed"
        }
        "保存诊断档案失败" | "保存诊断会话失败" | "保存助手设置失败" => {
            "fixture_storage_write_failed"
        }
        "创建诊断工作目录失败" => "fixture_workspace_create_failed",
        "读取诊断档案失败" | "加载诊断助手失败" => "fixture_storage_read_failed",
        "诊断已停止" => "manager_cancelled",
        "smoke_discovery_not_observed" => "http_mcp_discovery_missing",
        "smoke_agent_not_ready" => "manager_session_not_ready",
        _ => "unclassified_runtime_error",
    };
    (diagnostic, None)
}

fn phase_name(phase: usize) -> &'static str {
    match phase {
        0 => "settings",
        1 => "create_session",
        2 => "http_mcp_discovery",
        3 => "model_prompt",
        4 => "model_result",
        _ => "finished",
    }
}

async fn run(
    manager: &Arc<AssistantManager>,
    host: &Arc<SmokeToolHost>,
    options: &Options,
) -> Result<bool, String> {
    manager
        .save_settings(AssistantSettings {
            default_provider: Some(options.provider),
            providers: vec![AssistantProviderConfig {
                provider: options.provider,
                enabled: true,
                allow_native_tools: false,
                ..Default::default()
            }],
            ..Default::default()
        })
        .await?;
    host.counts.phase.store(1, Ordering::Release);
    let session = timeout(
        Duration::from_secs(90),
        manager.create_session(options.provider, BOT.into()),
    )
    .await
    .map_err(|_| "smoke_ready_timeout".to_owned())??;
    if session.info.state != AssistantSessionState::Ready || !session.info.capabilities.mcp_http {
        return Err(session
            .info
            .error
            .unwrap_or_else(|| "smoke_agent_not_ready".into()));
    }
    host.counts.phase.store(2, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(10);
    while host.counts.handshake.load(Ordering::Acquire) == 0
        || host.counts.list.load(Ordering::Acquire) == 0
    {
        if Instant::now() >= deadline {
            return Err("smoke_discovery_not_observed".into());
        }
        sleep(Duration::from_millis(50)).await;
    }
    if manager.get_session(&session.info.id).await?.info.state != AssistantSessionState::Ready {
        return Err("smoke_agent_not_ready".into());
    }
    if !options.with_model {
        if host.counts.call.load(Ordering::Acquire) != 0 {
            return Err("smoke_unexpected_default_tool_call".into());
        }
        return Ok(false);
    }
    host.counts.phase.store(3, Ordering::Release);
    manager.send_message(&session.info.id,
        "这是合成 MCP 冒烟测试。必须实际调用 ncd MCP 的 get_bot_runtime 工具一次，读取返回值的 canary 字段，并在最终答案中原样复述它。上下文不含 canary，不能猜测。仅此合成只读工具获得授权，不执行其他操作。".into()).await?;
    host.counts.phase.store(4, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut answered = HashSet::new();
    loop {
        let snapshot = manager.get_session(&session.info.id).await?;
        for permission in &snapshot.permissions {
            if !answered.insert(permission.id.clone()) {
                continue;
            }
            let safe = permission.source == AssistantPermissionSource::Agent
                && permission.title.contains(TOOL);
            let option = if safe {
                permission
                    .options
                    .iter()
                    .find(|option| option.kind == "allow_once")
                    .map(|option| option.id.clone())
            } else {
                None
            };
            manager
                .respond_permission(&session.info.id, &permission.id, option)
                .await?;
        }
        let canary_seen = snapshot.messages.iter().any(|message| {
            message.role == AssistantMessageRole::Assistant && message.text.contains(&host.canary)
        });
        if host.counts.call.load(Ordering::Acquire) > 0
            && canary_seen
            && snapshot.info.state == AssistantSessionState::Ready
        {
            return Ok(true);
        }
        if matches!(
            snapshot.info.state,
            AssistantSessionState::Failed
                | AssistantSessionState::Disconnected
                | AssistantSessionState::Closed
        ) {
            return Err(snapshot
                .info
                .error
                .unwrap_or_else(|| "smoke_model_canary_not_observed".into()));
        }
        if Instant::now() >= deadline {
            return Err("smoke_model_timeout".into());
        }
        sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> std::process::ExitCode {
    let options = match options() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("passed=false error_class={}", error_class(&error));
            return std::process::ExitCode::from(2);
        }
    };
    let root = match tempfile::tempdir() {
        Ok(root) => root,
        Err(_) => {
            eprintln!("passed=false error_class=temporary_directory_failed");
            return std::process::ExitCode::FAILURE;
        }
    };
    let host = match SmokeToolHost::new() {
        Ok(host) => Arc::new(host),
        Err(_) => {
            eprintln!("passed=false error_class=random_failed");
            return std::process::ExitCode::FAILURE;
        }
    };
    let tool_host: Arc<dyn AssistantToolHost> = host.clone();
    let manager = Arc::new(AssistantManager::new(root.path().to_path_buf(), tool_host));
    let result = run(&manager, &host, &options).await;
    manager.shutdown().await;
    let passed = result.is_ok();
    let canary_seen = matches!(&result, Ok(true));
    println!(
        "passed={passed} model_prompt_requested={} model_canary_seen={canary_seen} phase={} host_open_count={} initialize_count={} tools_list_count={} tool_call_count={}",
        options.with_model,
        phase_name(host.counts.phase.load(Ordering::Acquire)),
        host.counts.host_open.load(Ordering::Acquire),
        host.counts.handshake.load(Ordering::Acquire),
        host.counts.list.load(Ordering::Acquire),
        host.counts.call.load(Ordering::Acquire)
    );
    if let Err(error) = result {
        let (diagnostic, numeric_code) = safe_diagnostic(&error);
        eprintln!(
            "error_class={} diagnostic={diagnostic} numeric_code={}",
            error_class(&error),
            numeric_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "none".into())
        );
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let all = headers.get_all(name);
    let mut values = all.iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        None
    } else {
        Some(first)
    }
}

fn allowed_authority(authority: &str, port: u16) -> bool {
    authority == format!("127.0.0.1:{port}") || authority == format!("localhost:{port}")
}

async fn handle_http(State(state): State<Arc<HttpState>>, request: Request) -> Response {
    let mut response = guarded_http(&state, request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

async fn guarded_http(state: &HttpState, request: Request) -> Response {
    let headers = request.headers();
    if !single_header(headers, "host").is_some_and(|host| allowed_authority(host, state.port))
        || request
            .uri()
            .authority()
            .is_some_and(|authority| !allowed_authority(authority.as_str(), state.port))
        || (headers.contains_key("origin")
            && !single_header(headers, "origin").is_some_and(|origin| {
                origin == format!("http://127.0.0.1:{}", state.port)
                    || origin == format!("http://localhost:{}", state.port)
            }))
    {
        return (StatusCode::FORBIDDEN, "smoke_loopback_only").into_response();
    }
    let authorized = single_header(headers, "authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| bool::from(token.as_bytes().ct_eq(state.token.as_bytes())));
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "smoke_token_required",
        )
            .into_response();
    }
    if state.cancel.is_cancelled() {
        return StatusCode::GONE.into_response();
    }
    if headers.contains_key("mcp-protocol-version")
        && !single_header(headers, "mcp-protocol-version")
            .is_some_and(|version| VERSIONS.contains(&version))
    {
        return (StatusCode::BAD_REQUEST, "smoke_protocol_version_rejected").into_response();
    }
    if !single_header(headers, "content-type").is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > BODY_LIMIT)
    {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let Ok(_permit) = state.requests.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    tokio::select! {
        biased;
        _ = state.cancel.cancelled() => StatusCode::GONE.into_response(),
        response = timeout(Duration::from_secs(10), parse_http(state, request)) => {
            match response { Ok(response) => response, Err(_) => StatusCode::GATEWAY_TIMEOUT.into_response() }
        }
    }
}

async fn parse_http(state: &HttpState, request: Request) -> Response {
    let body = match timeout(
        Duration::from_secs(5),
        to_bytes(request.into_body(), BODY_LIMIT),
    )
    .await
    {
        Ok(Ok(body)) => body,
        Ok(Err(_)) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Err(_) => return StatusCode::REQUEST_TIMEOUT.into_response(),
    };
    let body: Value = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(_) => return rpc_error(Value::Null, -32700, "Parse error"),
    };
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    let valid_id = id.as_i64().is_some()
        || id.as_u64().is_some()
        || id.as_str().is_some_and(|id| id.len() <= 128);
    if !body.is_object()
        || body["jsonrpc"] != "2.0"
        || body["method"].as_str().is_none()
        || (body.get("id").is_some() && !valid_id)
    {
        return rpc_error(Value::Null, -32600, "Invalid Request");
    }
    let method = body["method"].as_str().unwrap_or_default();
    if body.get("id").is_none() {
        return if method.starts_with("notifications/") {
            StatusCode::ACCEPTED.into_response()
        } else {
            rpc_error(Value::Null, -32600, "Request id required")
        };
    }
    let params = body.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return rpc_error(id, -32602, "params must be an object");
    }
    match method {
        "initialize" => {
            let version = params["protocolVersion"]
                .as_str()
                .filter(|version| VERSIONS.contains(version))
                .unwrap_or("2025-11-25");
            state.counts.handshake.fetch_add(1, Ordering::AcqRel);
            rpc_result(
                id,
                json!({
                    "protocolVersion":version, "capabilities":{"tools":{"listChanged":false}},
                    "serverInfo":{"name":"ncd-synthetic-agent-smoke", "version":"1.0.0"},
                    "instructions":"合成只读现场；只允许当前 Bot 10001 的 get_bot_runtime，不存在任何真实 Bot 操作。"
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => {
            state.counts.list.fetch_add(1, Ordering::AcqRel);
            rpc_result(
                id,
                json!({"tools":[{
                    "name":TOOL, "description":"读取当前合成 Bot 的只读状态与随机 canary；没有真实 QQ 或主机。",
                    "inputSchema":{"type":"object", "properties":{"bot_id":{"type":"string", "enum":[BOT]}}, "additionalProperties":false},
                    "annotations":{"readOnlyHint":true, "destructiveHint":false, "idempotentHint":true, "openWorldHint":false}
                }]}),
            )
        }
        "tools/call" => {
            if params["name"] != TOOL
                || params.as_object().is_none_or(|params| {
                    params
                        .keys()
                        .any(|key| !matches!(key.as_str(), "name" | "arguments" | "_meta"))
                })
            {
                return rpc_error(
                    id,
                    -32602,
                    "Only the synthetic get_bot_runtime tool is available",
                );
            }
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if args.as_object().is_none_or(|args| {
                args.keys().any(|key| key != "bot_id")
                    || args
                        .get("bot_id")
                        .is_some_and(|bot| bot.as_str() != Some(BOT))
            }) {
                return rpc_error(id, -32602, "Synthetic Bot target cannot be overridden");
            }
            state.counts.call.fetch_add(1, Ordering::AcqRel);
            let result = json!({"bot_id":BOT, "synthetic":true, "runtime":{"state":"stopped"}, "canary":state.canary});
            rpc_result(
                id,
                json!({"content":[{"type":"text", "text":result.to_string()}], "structuredContent":result, "isError":false}),
            )
        }
        _ => rpc_error(id, -32601, "Method not found"),
    }
}

fn rpc_result(id: Value, result: Value) -> Response {
    Json(json!({"jsonrpc":"2.0", "id":id, "result":result})).into_response()
}
fn rpc_error(id: Value, code: i64, message: &str) -> Response {
    Json(json!({"jsonrpc":"2.0", "id":id, "error":{"code":code, "message":message}}))
        .into_response()
}
