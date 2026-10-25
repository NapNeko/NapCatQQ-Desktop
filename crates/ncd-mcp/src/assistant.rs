//! 每个诊断会话拥有独立的回环 MCP 服务和只作用于目标 Bot 的授权。

mod http;
mod operations;
mod rpc;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use ncd_domain::{BackendType, McpServerStatus};
use ncd_runtime::onebot_debug::DebugManager;
use ncd_traits::assistant::{
    AssistantContext, AssistantInteraction, AssistantToolConnection, AssistantToolHost,
};
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::gate::GateKeeper;
use crate::inspection::McpInspection;
use crate::server::{LimitedListener, ServerCtx};
use crate::tools::ToolsCtx;

const MAX_SESSIONS: usize = 8;
const RETIRED_SECRETS: usize = 64;
const SESSION_REQUESTS: usize = 4;
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

type OperationGate = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;

#[derive(Default)]
struct SessionSecrets {
    active: HashMap<String, String>,
    retired: VecDeque<String>,
}

impl SessionSecrets {
    fn retire(&mut self, session_id: &str) {
        if let Some(token) = self.active.remove(session_id) {
            self.retired.push_back(token);
            while self.retired.len() > RETIRED_SECRETS {
                self.retired.pop_front();
            }
        }
    }

    fn redact_exact(&self, text: &str) -> String {
        let mut clean = text.to_owned();
        for token in self.active.values().chain(self.retired.iter()) {
            clean = clean.replace(token, "[REDACTED]");
        }
        clean
    }
}

pub struct AssistantMcpHost {
    debug: Arc<DebugManager>,
    inspection: Arc<McpInspection>,
    sessions: Mutex<HashMap<String, SessionRun>>,
    secrets: Arc<StdMutex<SessionSecrets>>,
    operation_gate: OperationGate,
}

struct SessionRun {
    cancelled: CancellationToken,
    task: JoinHandle<()>,
}

impl Drop for SessionRun {
    fn drop(&mut self) {
        self.cancelled.cancel();
        self.task.abort();
    }
}

pub(super) struct SessionCtx {
    server: ServerCtx,
    session_id: String,
    bot_id: String,
    backend: BackendType,
    inspection: Arc<McpInspection>,
    interaction: Arc<dyn AssistantInteraction>,
    operation_gate: OperationGate,
    operation_lock: Mutex<()>,
    secrets: Arc<StdMutex<SessionSecrets>>,
}

impl SessionCtx {
    fn redact(&self, text: &str) -> String {
        // 先清除完整随机令牌，避免通用脱敏截断长文本后留下令牌前缀。
        let clean = lock(&self.secrets).redact_exact(text);
        crate::security::sanitize_text(&clean)
    }

    fn clean_value(&self, value: Value) -> Value {
        let encoded = value.to_string();
        let clean = lock(&self.secrets).redact_exact(&encoded);
        let value = serde_json::from_str(&clean).unwrap_or(Value::Null);
        crate::security::sanitize_output(value)
    }
}

impl AssistantMcpHost {
    pub fn new(debug: Arc<DebugManager>, inspection: Arc<McpInspection>) -> Self {
        Self {
            debug,
            inspection,
            sessions: Mutex::new(HashMap::new()),
            secrets: Arc::new(StdMutex::new(SessionSecrets::default())),
            operation_gate: Arc::new(|| Err("桌面端尚未授权助手执行 Bot 操作".to_owned())),
        }
    }

    pub fn with_operation_gate(mut self, gate: OperationGate) -> Self {
        self.operation_gate = gate;
        self
    }
}

#[async_trait]
impl AssistantToolHost for AssistantMcpHost {
    async fn context(&self, bot_id: &str) -> Result<AssistantContext, String> {
        let (bot_name, _, summary) =
            tokio::time::timeout(LOOKUP_TIMEOUT, self.inspection.assistant_context(bot_id))
                .await
                .map_err(|_| "读取诊断目标超时".to_owned())??;
        Ok(AssistantContext {
            bot_name: self.redact(&bot_name),
            summary: self.redact(&summary.to_string()),
        })
    }

    async fn open(
        &self,
        session_id: &str,
        bot_id: &str,
        interaction: Arc<dyn AssistantInteraction>,
        cancelled: CancellationToken,
    ) -> Result<AssistantToolConnection, String> {
        if session_id.is_empty()
            || session_id.len() > 128
            || session_id.chars().any(char::is_control)
        {
            return Err("诊断会话标识无效".to_owned());
        }
        if cancelled.is_cancelled() {
            return Err("诊断会话已取消".to_owned());
        }
        let (_, backend, _) =
            tokio::time::timeout(LOOKUP_TIMEOUT, self.inspection.assistant_context(bot_id))
                .await
                .map_err(|_| "读取诊断目标超时".to_owned())??;
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|id, run| {
            let keep = !run.cancelled.is_cancelled() && !run.task.is_finished();
            if !keep {
                lock(&self.secrets).retire(id);
            }
            keep
        });
        if sessions.contains_key(session_id) {
            return Err("诊断会话已拥有 MCP 连接".to_owned());
        }
        if sessions.len() >= MAX_SESSIONS {
            return Err("最多同时打开 8 个诊断会话".to_owned());
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| "无法建立诊断助手的本机工具连接".to_owned())?;
        let port = listener
            .local_addr()
            .map_err(|_| "无法取得诊断工具监听端口".to_owned())?
            .port();
        let token = random_id(32)?;
        let cancel = cancelled.child_token();
        if cancel.is_cancelled() {
            return Err("诊断会话已取消".to_owned());
        }
        let ctx = Arc::new(SessionCtx {
            server: ServerCtx {
                tools: ToolsCtx {
                    debug: self.debug.clone(),
                    runtime: Some(self.inspection.clone()),
                    gate: Arc::new(GateKeeper::new()),
                    status: Arc::new(move || McpServerStatus {
                        enabled: true,
                        listening: true,
                        port: Some(port),
                        last_error: None,
                    }),
                },
                token: token.clone(),
                bound_port: port,
                requests: Semaphore::new(SESSION_REQUESTS),
                cancel: cancel.clone(),
            },
            session_id: session_id.to_owned(),
            bot_id: bot_id.to_owned(),
            backend,
            inspection: self.inspection.clone(),
            interaction,
            operation_gate: self.operation_gate.clone(),
            operation_lock: Mutex::new(()),
            secrets: self.secrets.clone(),
        });
        lock(&self.secrets)
            .active
            .insert(session_id.to_owned(), token.clone());
        let shutdown = cancel.clone();
        let listener = LimitedListener::new(listener, cancel.clone());
        let router = http::router(ctx);
        let task = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
            {
                tracing::warn!(target: "ncd_mcp::assistant", error = %error, "诊断工具连接已停止");
            }
        });
        sessions.insert(
            session_id.to_owned(),
            SessionRun {
                cancelled: cancel,
                task,
            },
        );
        Ok(AssistantToolConnection {
            url: format!("http://127.0.0.1:{port}/mcp"),
            bearer_token: token,
        })
    }

    async fn close(&self, session_id: &str) {
        let run = self.sessions.lock().await.remove(session_id);
        if let Some(run) = run {
            run.cancelled.cancel();
            drop(run);
        }
        lock(&self.secrets).retire(session_id);
    }

    fn redact(&self, text: &str) -> String {
        let clean = lock(&self.secrets).redact_exact(text);
        crate::security::sanitize_text(&clean)
    }
}

fn random_id(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0_u8; bytes];
    OsRng
        .try_fill_bytes(&mut buffer)
        .map_err(|_| "无法生成诊断会话凭据".to_owned())?;
    Ok(hex::encode(buffer))
}

fn lock<T>(mutex: &StdMutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
