//! Koishi 控制台的数据通道（`ws://127.0.0.1:<port>/status`）。
//!
//! 协议（`@koishijs/console` 的 `client.ts`）：发 `{type, args, id}`，回 `{type:'response', body:{id, value|error}}`；
//! 服务端另外随时推 `{type:'data', body:{key, value}}`。一连上它会把所有数据服务推一遍，
//! 其中插件市场那份有 5 MB，所以每个实例只留一条长连接，推送里只收 `status` / `config`，其余丢掉。
//! 口是桌面端这边的回环口（远端实例经 SSH -L），换了口或断了就重连。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use ncd_traits::AppFrameworkError;
use serde_json::{Value, json};
use tokio::sync::{Mutex, Notify, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use super::manifest::CONSOLE_API_PATH;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// 只留这几份推送：Bot 在线 / 性能、运行中的配置
const KEPT_KEYS: &[&str] = &["status", "config"];

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

struct Conn {
    port: u16,
    out: mpsc::UnboundedSender<Message>,
    pending: Pending,
    data: Arc<std::sync::Mutex<HashMap<String, Value>>>,
    alive: Arc<AtomicBool>,
    changed: Arc<Notify>,
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

    /// 取某份推送的最新值；还没收到就最多等 `wait`
    pub async fn snapshot(
        &self,
        instance_id: &str,
        port: u16,
        key: &str,
        wait: Duration,
    ) -> Result<Option<Value>, AppFrameworkError> {
        let conn = self.conn(instance_id, port).await?;
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let notified = conn.changed.notified();
            if let Some(v) = conn
                .data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(key)
                .cloned()
            {
                return Ok(Some(v));
            }
            if !conn.is_alive() {
                self.forget(instance_id).await;
                return Err(AppFrameworkError::Runtime("Koishi 控制台连接已断开".into()));
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return Ok(None);
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
    let data: Arc<std::sync::Mutex<HashMap<String, Value>>> = Arc::default();
    let alive = Arc::new(AtomicBool::new(true));
    let changed = Arc::new(Notify::new());

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

    let (pending_r, data_r, alive_r, changed_r) = (
        pending.clone(),
        data.clone(),
        alive.clone(),
        changed.clone(),
    );
    tokio::spawn(async move {
        while let Some(frame) = stream.next().await {
            let text = match frame {
                Ok(Message::Text(t)) => t,
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            };
            dispatch(&text, &pending_r, &data_r);
            changed_r.notify_waiters();
        }
        alive_r.store(false, Ordering::SeqCst);
        // 丢掉所有等回应的发送端，请求方据此知道连接没了
        pending_r
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        changed_r.notify_waiters();
    });

    Ok(Conn {
        port,
        out: out_tx,
        pending,
        data,
        alive,
        changed,
        next_id: AtomicU64::new(1),
    })
}

fn dispatch(text: &str, pending: &Pending, data: &std::sync::Mutex<HashMap<String, Value>>) {
    // 大推送（市场）不是要的就别整份解析
    if !text.starts_with(r#"{"type":"response""#)
        && !KEPT_KEYS
            .iter()
            .any(|k| text.contains(&format!(r#""key":"{k}""#)))
    {
        return;
    }
    let Ok(msg) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let body = msg.get("body").cloned().unwrap_or(Value::Null);
    match msg.get("type").and_then(Value::as_str) {
        Some("response") => {
            let Some(id) = body.get("id").and_then(Value::as_u64) else {
                return;
            };
            let Some(tx) = pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id)
            else {
                return;
            };
            let result = match body.get("error") {
                Some(e) if !e.is_null() => Err(error_text(e)),
                _ => Ok(body.get("value").cloned().unwrap_or(Value::Null)),
            };
            let _ = tx.send(result);
        }
        Some("data") => {
            let Some(key) = body.get("key").and_then(Value::as_str) else {
                return;
            };
            if KEPT_KEYS.contains(&key) {
                let value = body.get("value").cloned().unwrap_or(Value::Null);
                data.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(key.to_string(), value);
            }
        }
        Some("patch") => {
            let Some(key) = body.get("key").and_then(Value::as_str) else {
                return;
            };
            if !KEPT_KEYS.contains(&key) {
                return;
            }
            let mut store = data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let (Some(Value::Object(dst)), Some(Value::Object(src))) =
                (store.get_mut(key), body.get("value"))
            {
                for (k, v) in src {
                    dst.insert(k.clone(), v.clone());
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_routes_responses_and_keeps_only_wanted_pushes() {
        let pending: Pending = Arc::default();
        let data = std::sync::Mutex::new(HashMap::new());
        let (tx, mut rx) = oneshot::channel();
        pending.lock().unwrap().insert(3, tx);
        dispatch(
            r#"{"type":"response","body":{"id":3,"value":{"ok":1}}}"#,
            &pending,
            &data,
        );
        assert_eq!(rx.try_recv().unwrap().unwrap(), json!({"ok":1}));

        let (tx, mut rx) = oneshot::channel();
        pending.lock().unwrap().insert(4, tx);
        dispatch(
            r#"{"type":"response","body":{"id":4,"error":"unauthorized"}}"#,
            &pending,
            &data,
        );
        assert_eq!(rx.try_recv().unwrap().unwrap_err(), "unauthorized");

        dispatch(
            r#"{"type":"data","body":{"key":"market","value":{"big":true}}}"#,
            &pending,
            &data,
        );
        dispatch(
            r#"{"type":"data","body":{"key":"status","value":{"bots":{},"cpu":[0,0]}}}"#,
            &pending,
            &data,
        );
        dispatch(
            r#"{"type":"patch","body":{"key":"status","value":{"cpu":[1,1]}}}"#,
            &pending,
            &data,
        );
        let data = data.lock().unwrap();
        assert!(!data.contains_key("market"));
        assert_eq!(data["status"]["cpu"], json!([1, 1]));
        assert_eq!(data["status"]["bots"], json!({}));
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
