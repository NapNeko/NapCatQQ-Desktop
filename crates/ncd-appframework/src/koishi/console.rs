//! 经 Koishi 控制台连接发送 OneBot 对接配置请求。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use ncd_traits::AppFrameworkError;
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use super::manifest::CONSOLE_API_PATH;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

struct Conn {
    port: u16,
    out: mpsc::UnboundedSender<Message>,
    pending: Pending,
    alive: Arc<AtomicBool>,
    next_id: AtomicU64,
}

impl Conn {
    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

/// 请求结果里 `error` 的几种上游说法
fn map_console_error(kind: &str, error: String) -> AppFrameworkError {
    if error == "unauthorized" {
        return AppFrameworkError::DashboardAuth(
            "Koishi 控制台开了登录（auth 插件），桌面端没法替你改；停掉实例再改，或者在控制台里改"
                .into(),
        );
    }
    if error == "not implemented" {
        return AppFrameworkError::Runtime(format!(
            "Koishi 控制台不认 {kind}：config 插件（插件配置页）可能被停用了"
        ));
    }
    AppFrameworkError::Runtime(format!("Koishi 控制台执行 {kind} 失败：{error}"))
}

fn error_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(m) => m
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string()),
        other => other.to_string(),
    }
}

/// 每个实例一条长连接
#[derive(Default)]
pub struct KoishiConsole {
    conns: Mutex<HashMap<String, Arc<Conn>>>,
}

impl KoishiConsole {
    pub fn new() -> Self {
        Self::default()
    }

    async fn conn(&self, instance_id: &str, port: u16) -> Result<Arc<Conn>, AppFrameworkError> {
        let mut conns = self.conns.lock().await;
        if let Some(c) = conns.get(instance_id)
            && c.port == port
            && c.is_alive()
        {
            return Ok(c.clone());
        }
        let conn = Arc::new(connect(port).await?);
        conns.insert(instance_id.to_string(), conn.clone());
        Ok(conn)
    }

    pub async fn forget(&self, instance_id: &str) {
        self.conns.lock().await.remove(instance_id);
    }

    /// 发一条请求等回应
    pub async fn request(
        &self,
        instance_id: &str,
        port: u16,
        kind: &str,
        args: Vec<Value>,
    ) -> Result<Value, AppFrameworkError> {
        self.request_inner(instance_id, port, kind, args, false)
            .await
    }

    /// 同上，但连接断开也算成功：`manager/app-reload` 写完配置就让 worker 退出重拉，回应发不出来
    pub async fn request_until_restart(
        &self,
        instance_id: &str,
        port: u16,
        kind: &str,
        args: Vec<Value>,
    ) -> Result<Value, AppFrameworkError> {
        let out = self
            .request_inner(instance_id, port, kind, args, true)
            .await;
        self.forget(instance_id).await;
        out
    }

    async fn request_inner(
        &self,
        instance_id: &str,
        port: u16,
        kind: &str,
        args: Vec<Value>,
        close_is_ok: bool,
    ) -> Result<Value, AppFrameworkError> {
        let conn = self.conn(instance_id, port).await?;
        let id = conn.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        conn.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, tx);
        let payload = json!({ "type": kind, "args": args, "id": id });
        if conn.out.send(Message::Text(payload.to_string())).is_err() {
            conn.pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id);
            self.forget(instance_id).await;
            return Err(AppFrameworkError::Runtime("Koishi 控制台连接已断开".into()));
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(error))) => Err(map_console_error(kind, error)),
            // 发送端被丢 = 连接断了
            Ok(Err(_)) if close_is_ok => Ok(Value::Null),
            Ok(Err(_)) => {
                self.forget(instance_id).await;
                Err(AppFrameworkError::Runtime("Koishi 控制台连接已断开".into()))
            }
            Err(_) => {
                conn.pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(&id);
                Err(AppFrameworkError::Runtime(format!(
                    "Koishi 控制台 {} 秒内没回应 {kind}",
                    REQUEST_TIMEOUT.as_secs()
                )))
            }
        }
    }
}

async fn connect(port: u16) -> Result<Conn, AppFrameworkError> {
    let url = format!("ws://127.0.0.1:{port}{CONSOLE_API_PATH}");
    let (ws, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(&url))
        .await
        .map_err(|_| AppFrameworkError::Runtime(format!("连 Koishi 控制台超时（{url}）")))?
        .map_err(|e| AppFrameworkError::Runtime(format!("连不上 Koishi 控制台（{url}）：{e}")))?;
    let (mut sink, mut stream) = ws.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let pending: Pending = Arc::default();
    let alive = Arc::new(AtomicBool::new(true));

    let alive_w = alive.clone();
    tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
        alive_w.store(false, Ordering::SeqCst);
        let _ = sink.close().await;
    });

    let reader = ReaderCtx {
        pending: pending.clone(),
        alive: alive.clone(),
    };
    tokio::spawn(async move {
        while let Some(frame) = stream.next().await {
            let text = match frame {
                Ok(Message::Text(t)) => t,
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            };
            reader.dispatch(&text);
        }
        reader.alive.store(false, Ordering::SeqCst);
        // 丢掉所有等回应的发送端，请求方据此知道连接没了
        reader
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    });

    Ok(Conn {
        port,
        out: out_tx,
        pending,
        alive,
        next_id: AtomicU64::new(1),
    })
}

/// 只路由对接请求的响应，不接管控制台推送。
struct ReaderCtx {
    pending: Pending,
    alive: Arc<AtomicBool>,
}

impl ReaderCtx {
    fn dispatch(&self, text: &str) {
        // 上游市场推送体积较大，对接只需要请求响应。
        if !text.contains("\"response\"") {
            return;
        }
        let Ok(msg) = serde_json::from_str::<Value>(text) else {
            return;
        };
        if msg.get("type").and_then(Value::as_str) != Some("response") {
            return;
        }
        let Some(body) = msg.get("body") else { return };
        let Some(id) = body.get("id").and_then(Value::as_u64) else {
            return;
        };
        let Some(tx) = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id)
        else {
            return;
        };
        let result = match body.get("error") {
            Some(error) if !error.is_null() => Err(error_text(error)),
            _ => Ok(body.get("value").cloned().unwrap_or(Value::Null)),
        };
        let _ = tx.send(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_route_to_their_request_and_pushes_are_ignored() {
        let reader = ReaderCtx {
            pending: Arc::default(),
            alive: Arc::new(AtomicBool::new(true)),
        };
        let (tx, mut rx) = oneshot::channel();
        reader.pending.lock().unwrap().insert(3, tx);
        reader.dispatch(r#"{"type":"data","body":{"id":3,"key":"config","value":{}}}"#);
        assert!(rx.try_recv().is_err());
        reader.dispatch(r#"{ "type": "response", "body": {"id":3,"value":{"ok":true}} }"#);
        assert_eq!(rx.try_recv().unwrap().unwrap(), json!({"ok":true}));
        let (tx, mut rx) = oneshot::channel();
        reader.pending.lock().unwrap().insert(4, tx);
        reader.dispatch(r#"{"type":"response","body":{"id":4,"error":"unauthorized"}}"#);
        assert_eq!(rx.try_recv().unwrap().unwrap_err(), "unauthorized");
    }

    #[test]
    fn console_errors_are_explained() {
        assert!(matches!(
            map_console_error("manager/reload", "unauthorized".into()),
            AppFrameworkError::DashboardAuth(_)
        ));
        let e = map_console_error("manager/reload", "not implemented".into()).to_string();
        assert!(e.contains("config 插件"));
        assert_eq!(error_text(&json!({"message":"boom"})), "boom");
    }
}
