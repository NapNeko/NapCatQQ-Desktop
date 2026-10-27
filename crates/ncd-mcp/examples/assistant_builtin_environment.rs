//! 手动执行内置助手的只读开发环境任务，模型凭据仅存在于当前进程内存。

use std::{
    collections::HashMap,
    io::{self, Write},
    process::ExitCode,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use ncd_domain::{
    assistant::{
        AssistantBuiltinSettings, AssistantMessageRole, AssistantProvider, AssistantSessionState,
        AssistantSettings,
    },
    errors::SecretError,
};
use ncd_runtime::assistant::AssistantManager;
use ncd_traits::{
    SecretStore,
    assistant::{
        AssistantContext, AssistantInteraction, AssistantToolConnection, AssistantToolHost,
    },
};
use rand::{RngCore, rngs::OsRng};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use tokio_util::sync::CancellationToken;

const TARGET: &str = "environment-only";
const MCP_VERSION: &str = "2025-11-25";
const BODY_LIMIT: usize = 16 * 1024;
const REQUEST_BUDGET: usize = 64;
const TASK: &str = "了解当前环境。请先调用 diagnose_bot 读取 NCD 开发环境摘要，然后用中文概括系统、项目版本、技术栈及本次读取范围。这里没有真实 Bot，不要尝试启停、QQ、SSH 或读取其他文件。";

#[derive(Default)]
struct MemorySecretStore {
    values: StdMutex<HashMap<String, String>>,
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .values
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .get(key)
            .cloned())
    }
    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.values
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .remove(key);
        Ok(())
    }
}

struct RunningServer {
    cancelled: CancellationToken,
    task: Option<JoinHandle<()>>,
}

impl RunningServer {
    async fn stop(mut self) {
        self.cancelled.cancel();
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.cancelled.cancel();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

struct EnvironmentHost {
    summary: Value,
    secrets: StdMutex<Vec<String>>,
    running: Mutex<HashMap<String, RunningServer>>,
    tool_calls: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
}

impl EnvironmentHost {
    fn new(api_key: &str, summary: Value) -> Self {
        Self {
            summary,
            secrets: StdMutex::new(vec![api_key.to_owned()]),
            running: Mutex::new(HashMap::new()),
            tool_calls: Arc::new(AtomicUsize::new(0)),
            requests: Arc::new(AtomicUsize::new(0)),
        }
    }

    async fn close_all(&self) {
        let runs = self
            .running
            .lock()
            .await
            .drain()
            .map(|(_, run)| run)
            .collect::<Vec<_>>();
        for run in runs {
            run.stop().await;
        }
    }
}

#[async_trait]
impl AssistantToolHost for EnvironmentHost {
    async fn context(&self, bot_id: &str) -> Result<AssistantContext, String> {
        if bot_id != TARGET {
            return Err("本例只能读取开发环境摘要".into());
        }
        Ok(AssistantContext { bot_name:"开发环境（无真实 Bot）".into(),
            summary:"这是只读开发环境验证会话，不绑定任何真实 Bot。请通过 diagnose_bot 获取摘要，不得声称已读取真实 Bot 状态。".into() })
    }

    async fn open(
        &self,
        session_id: &str,
        bot_id: &str,
        _: Arc<dyn AssistantInteraction>,
        cancelled: CancellationToken,
    ) -> Result<AssistantToolConnection, String> {
        if bot_id != TARGET
            || session_id.is_empty()
            || session_id.len() > 128
            || cancelled.is_cancelled()
        {
            return Err("开发环境诊断目标无效或已取消".into());
        }
        let mut running = self.running.lock().await;
        if !running.is_empty() {
            return Err("本例只允许一个开发环境会话".into());
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| "无法创建只读环境工具监听".to_owned())?;
        let address = listener
            .local_addr()
            .map_err(|_| "无法取得只读工具端口".to_owned())?;
        let mut random = [0_u8; 32];
        OsRng
            .try_fill_bytes(&mut random)
            .map_err(|_| "无法生成只读工具授权".to_owned())?;
        let bearer = hex::encode(random);
        self.secrets
            .lock()
            .map_err(|_| "开发环境脱敏器不可用".to_owned())?
            .push(bearer.clone());
        if cancelled.is_cancelled() {
            return Err("开发环境会话已取消".into());
        }
        let state = Arc::new(ServerState {
            expected_host: address.to_string(),
            bearer: bearer.clone(),
            summary: self.summary.clone(),
            tool_calls: self.tool_calls.clone(),
            requests: self.requests.clone(),
        });
        let router = Router::new()
            .route("/mcp", post(handle_rpc))
            .layer(DefaultBodyLimit::max(BODY_LIMIT))
            .with_state(state);
        let stop = cancelled.child_token();
        let server_stop = stop.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(server_stop.cancelled_owned())
                .await;
        });
        running.insert(
            session_id.to_owned(),
            RunningServer {
                cancelled: stop,
                task: Some(task),
            },
        );
        Ok(AssistantToolConnection {
            url: format!("http://{address}/mcp"),
            bearer_token: bearer,
        })
    }

    async fn close(&self, session_id: &str) {
        let run = self.running.lock().await.remove(session_id);
        if let Some(run) = run {
            run.stop().await;
        }
    }

    fn redact(&self, text: &str) -> String {
        let Ok(secrets) = self.secrets.lock() else {
            return "[脱敏器不可用，内容已隐藏]".into();
        };
        secrets.iter().fold(text.to_owned(), |text, secret| {
            if secret.is_empty() {
                text
            } else {
                text.replace(secret, "[REDACTED]")
            }
        })
    }
}

struct ServerState {
    expected_host: String,
    bearer: String,
    summary: Value,
    tool_calls: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
}

async fn handle_rpc(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if headers.get("host").and_then(|value| value.to_str().ok())
        != Some(state.expected_host.as_str())
        || headers.contains_key("origin")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !bool::from(bearer.as_bytes().ct_eq(state.bearer.as_bytes())) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if body.len() > BODY_LIMIT {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    if state.requests.fetch_add(1, Ordering::AcqRel) >= REQUEST_BUDGET {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let Ok(request) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !request.is_object() || request["jsonrpc"] != "2.0" {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let id = request.get("id").cloned();
    let Some(method) = request["method"].as_str() else {
        return rpc_error(id, -32600);
    };
    if method == "notifications/initialized" && id.is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    if !id
        .as_ref()
        .is_some_and(|id| id.as_u64().is_some() || id.as_str().is_some_and(|id| id.len() <= 256))
    {
        return rpc_error(id, -32600);
    }
    let params = &request["params"];
    let result = match method {
        "initialize" if params["protocolVersion"] == MCP_VERSION => json!({
            "protocolVersion":MCP_VERSION, "capabilities":{"tools":{}},
            "serverInfo":{"name":"ncd-readonly-environment-example","version":env!("CARGO_PKG_VERSION")}
        }),
        "tools/list" if params.is_object() => json!({"tools":[{
            "name":"diagnose_bot", "description":"读取本次 NCD 开发环境摘要：真实系统平台、架构、项目 UI 版本、技术栈和只读范围。没有真实 Bot，不访问 QQ、SSH、Bot 状态或任意文件。",
            "inputSchema":{"type":"object","properties":{},"additionalProperties":false}
        }]}),
        "tools/call"
            if params["name"] == "diagnose_bot"
                && params["arguments"]
                    .as_object()
                    .is_some_and(|args| args.is_empty()) =>
        {
            state.tool_calls.fetch_add(1, Ordering::AcqRel);
            json!({"content":[{"type":"text","text":state.summary.to_string()}],"isError":false})
        }
        _ => return rpc_error(id, -32602),
    };
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}

fn rpc_error(id: Option<Value>, code: i32) -> Response {
    Json(
        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":"只读开发环境工具请求无效"}}),
    )
    .into_response()
}

fn environment_summary() -> Result<Value, &'static str> {
    let package: Value = serde_json::from_str(include_str!("../../../package.json"))
        .map_err(|_| "项目版本元数据无效")?;
    let version = package["version"]
        .as_str()
        .ok_or("项目版本元数据缺少版本号")?;
    Ok(json!({
        "target":"NCD 开发环境（无真实 Bot）", "os":std::env::consts::OS, "architecture":std::env::consts::ARCH,
        "uiVersion":version, "stack":["Rust","Tauri 2","React"], "readOnly":true,
        "source":"OS/ARCH 来自当前进程，UI 版本来自编译时项目 package.json",
        "scope":"仅本工具提供的开发环境摘要，不读取任意运行时文件、凭据、真实 Bot 状态、QQ 或 SSH，不提供启停工具",
        "realBotStateRead":false
    }))
}

async fn exercise(
    manager: &Arc<AssistantManager>,
    host: &EnvironmentHost,
) -> Result<String, &'static str> {
    let session = manager
        .create_session(AssistantProvider::Builtin, TARGET.into())
        .await
        .map_err(|_| "创建内置会话失败")?;
    if session.info.state != AssistantSessionState::Ready
        || host.requests.load(Ordering::Acquire) != 0
    {
        return Err("创建会话阶段出现非预期活动或失败");
    }
    manager
        .send_message(&session.info.id, TASK.into())
        .await
        .map_err(|_| "提交环境任务失败")?;
    loop {
        let snapshot = manager
            .get_session(&session.info.id)
            .await
            .map_err(|_| "无法读取环境任务状态")?;
        match snapshot.info.state {
            AssistantSessionState::Failed => {
                report_failure_kind(snapshot.info.error.as_deref());
                return Err("内置会话失败，已停止且不会重试");
            }
            AssistantSessionState::Disconnected | AssistantSessionState::Closed => {
                return Err("内置会话已断开或关闭，已停止且不会重试");
            }
            AssistantSessionState::WaitingForInput => {
                return Err("任务请求了额外交互，本例不代替用户答复或批准");
            }
            AssistantSessionState::Ready => {
                let answer = snapshot
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == AssistantMessageRole::Assistant)
                    .map(|message| message.text.trim())
                    .unwrap_or_default();
                if !answer.is_empty() {
                    if host.tool_calls.load(Ordering::Acquire) == 0 {
                        return Err("模型没有调用开发环境摘要工具");
                    }
                    if !answer
                        .chars()
                        .any(|ch| ('\u{3400}'..='\u{9fff}').contains(&ch))
                    {
                        return Err("模型未返回中文环境概括");
                    }
                    return Ok(answer.to_owned());
                }
            }
            AssistantSessionState::Starting | AssistantSessionState::Running => {}
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn report_failure_kind(error: Option<&str>) {
    let error = error.unwrap_or_default();
    if let Some(code) = error
        .split_once("HTTP ")
        .and_then(|(_, rest)| rest.get(..3))
        .and_then(|text| text.parse::<u16>().ok())
        .filter(|code| (300..=599).contains(code))
    {
        let _ = writeln!(
            io::stderr().lock(),
            "failure_kind=model_http_error status={code}"
        );
        return;
    }
    let kind = if error.contains("输出上限") {
        "output_limit"
    } else if error.contains("超时") {
        "timeout"
    } else if error.contains("连接") {
        "connection"
    } else if error.contains("格式") || error.contains("未完整结束") {
        "model_protocol"
    } else if error.contains("工具") {
        "tool_or_budget"
    } else if error.contains("凭据") || error.contains("API Key") {
        "local_credentials"
    } else {
        "task_failed"
    };
    let _ = writeln!(io::stderr().lock(), "failure_kind={kind}");
}

async fn run() -> Result<(), &'static str> {
    let base_url = std::env::var("NCD_ASSISTANT_TEST_BASE_URL")
        .map_err(|_| "缺少 NCD_ASSISTANT_TEST_BASE_URL")?;
    let model =
        std::env::var("NCD_ASSISTANT_TEST_MODEL").map_err(|_| "缺少 NCD_ASSISTANT_TEST_MODEL")?;
    let api_key = std::env::var("NCD_ASSISTANT_TEST_API_KEY")
        .map_err(|_| "缺少 NCD_ASSISTANT_TEST_API_KEY")?;
    let summary = environment_summary()?;
    let root = tempfile::tempdir().map_err(|_| "无法创建临时验证目录")?;
    let host = Arc::new(EnvironmentHost::new(&api_key, summary.clone()));
    let store = Arc::new(MemorySecretStore::default());
    let manager = Arc::new(
        AssistantManager::new(root.path().to_owned(), host.clone()).with_secret_store(store),
    );
    let operation = async {
        let mut settings = AssistantSettings {
            default_provider: Some(AssistantProvider::Builtin),
            builtin: AssistantBuiltinSettings {
                base_url,
                model,
                max_steps: 3,
                max_output_tokens: 4096,
                request_timeout_secs: 90,
                ..Default::default()
            },
            ..Default::default()
        };
        for provider in &mut settings.providers {
            provider.enabled = false;
        }
        manager
            .save_settings(settings)
            .await
            .map_err(|_| "保存临时模型设置失败")?;
        manager
            .set_api_key(api_key.clone())
            .await
            .map_err(|_| "保存仅内存模型凭据失败")?;
        exercise(&manager, &host).await
    };
    let result = tokio::time::timeout(Duration::from_secs(115), operation)
        .await
        .map_err(|_| "环境任务超过时限，已停止")
        .and_then(|result| result);
    let shutdown = tokio::time::timeout(Duration::from_secs(5), manager.shutdown()).await;
    host.close_all().await;
    drop(manager);
    drop(root);
    if shutdown.is_err() {
        return Err("助手关闭超过时限，监听已收回");
    }
    let answer = result?;
    let answer = host
        .redact(&answer)
        .replace(&api_key, "[REDACTED]")
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .take(8192)
        .collect::<String>();
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "内置 Agent 只读环境验证通过；状态 Ready；diagnose_bot 调用 {} 次。",
        host.tool_calls.load(Ordering::Acquire)
    )
    .map_err(|_| "无法输出验证结果")?;
    writeln!(output, "环境摘要：{}", summary).map_err(|_| "无法输出环境摘要")?;
    writeln!(output, "助手回答：\n{answer}").map_err(|_| "无法输出助手回答")?;
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(stage) => {
            let _ = writeln!(io::stderr().lock(), "内置 Agent 环境验证停止：{stage}");
            ExitCode::FAILURE
        }
    }
}
