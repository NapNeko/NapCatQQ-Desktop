//! Koishi 控制台的数据通道（`ws://127.0.0.1:<port>/status`）。
//!
//! 协议（`@koishijs/console` 的 `client.ts`）：发 `{type, args, id}`，回 `{type:'response', body:{id, value|error}}`；
//! 服务端另外随时推 `{type:'data', body:{key, value}}`。一连上它会把所有数据服务推一遍，
//! 其中插件市场那份有 5 MB，所以每个实例只留一条长连接，推送里只收 `status` / `config`，其余丢掉。
//! 口是桌面端这边的回环口（远端实例经 SSH -L），换了口或断了就重连。

use std::collections::{HashMap, HashSet};
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
/// 只留这几份推送：Bot 在线 / 性能、运行中的配置、文件树（explorer）、
/// 控制台入口数据（entry，指令列表在里面）、数据库统计（database）
const KEPT_KEYS: &[&str] = &["status", "config", "explorer", "entry", "database"];
/// 沙盒消息缓冲上限：只给试聊页回看用
const SANDBOX_KEEP: usize = 200;

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

struct Conn {
    port: u16,
    out: mpsc::UnboundedSender<Message>,
    pending: Pending,
    data: Arc<std::sync::Mutex<HashMap<String, Value>>>,
    /// 沙盒（试聊）消息：上游只推给“拥有”这个 platform 的控制台连接，所以留在连接上，前端轮询拿
    sandbox: Arc<std::sync::Mutex<Vec<Value>>>,
    /// 沙盒里已经建过档的 (platform, user)：指令按库里用户的权限判，没建档（authority 0）指令会静默不理
    sandbox_users: Arc<std::sync::Mutex<HashSet<(String, String)>>>,
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

    /// 沙盒：以 `platform` 的身份发一条消息（`channel` 私聊是 `@用户名`，群聊是 `#`）
    pub async fn sandbox_send(
        &self,
        instance_id: &str,
        port: u16,
        platform: &str,
        user: &str,
        channel: &str,
        content: &str,
    ) -> Result<(), AppFrameworkError> {
        // 先建档：上游沙盒指令按数据库里用户的权限判，库里没这个人就永远不回话
        let conn = self.conn(instance_id, port).await?;
        let seen = (platform.to_string(), user.to_string());
        let known = conn
            .sandbox_users
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&seen);
        if !known {
            self.request(
                instance_id,
                port,
                "sandbox/get-user",
                vec![json!(platform), json!(user)],
            )
            .await?;
            conn.sandbox_users
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(seen);
        }
        self.request(
            instance_id,
            port,
            "sandbox/send-message",
            vec![
                json!(platform),
                json!(user),
                json!(channel),
                json!(content),
            ],
        )
        .await
        .map(|_| ())
        .map_err(|e| match e {
            AppFrameworkError::Runtime(m) if m.contains("not implemented") => {
                AppFrameworkError::Runtime("sandbox 插件被停用了：在插件页把它启用再试".into())
            }
            other => other,
        })
    }

    /// 沙盒消息缓冲的当前内容（顺带把连接建起来，建完推送才会进来）
    pub async fn sandbox_messages(
        &self,
        instance_id: &str,
        port: u16,
    ) -> Result<Vec<Value>, AppFrameworkError> {
        let conn = self.conn(instance_id, port).await?;
        Ok(conn
            .sandbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone())
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
    let sandbox: Arc<std::sync::Mutex<Vec<Value>>> = Arc::default();
    let alive = Arc::new(AtomicBool::new(true));
    let changed = Arc::new(Notify::new());

    let alive_w = alive.clone();
    let out_writer = out_tx.clone();
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
        data: data.clone(),
        sandbox: sandbox.clone(),
        out: out_writer,
        alive: alive.clone(),
        changed: changed.clone(),
        next_id: Arc::new(AtomicU64::new(1000)),
    };
    tokio::spawn(async move {
        while let Some(frame) = stream.next().await {
            let text = match frame {
                Ok(Message::Text(t)) => t,
                Ok(Message::Close(_)) | Err(_) => break,
                Ok(_) => continue,
            };
            reader.dispatch(&text);
            reader.changed.notify_waiters();
        }
        reader.alive.store(false, Ordering::SeqCst);
        // 丢掉所有等回应的发送端，请求方据此知道连接没了
        reader
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        reader.changed.notify_waiters();
    });

    Ok(Conn {
        port,
        out: out_tx,
        pending,
        data,
        sandbox,
        sandbox_users: Arc::default(),
        alive,
        changed,
        next_id: AtomicU64::new(1),
    })
}

/// 读循环的上下文：响应路由、推送存储、沙盒缓冲，以及回话（sandbox/request 要答）
struct ReaderCtx {
    pending: Pending,
    data: Arc<std::sync::Mutex<HashMap<String, Value>>>,
    sandbox: Arc<std::sync::Mutex<Vec<Value>>>,
    out: mpsc::UnboundedSender<Message>,
    alive: Arc<AtomicBool>,
    changed: Arc<Notify>,
    /// 回话用的 id 段（和请求用的错开，互不相认）
    next_id: Arc<AtomicU64>,
}

impl ReaderCtx {
    fn dispatch(&self, text: &str) {
        // 大推送（市场）不是要的就别整份解析；沙盒消息和入口数据更新个头小，直接放行
        let passthrough = text.starts_with(r#"{"type":"sandbox/"#)
            || text.starts_with(r#"{"type":"entry-data""#);
        if !passthrough
            && !text.starts_with(r#"{"type":"response""#)
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
                let Some(tx) = self
                    .pending
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
                    self.data
                        .lock()
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
                let mut store = self
                    .data
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
            Some("entry-data") => {
                // 某个控制台入口的数据更新（指令列表改了就会推）：就地换 entry 推送里那份
                let Some(id) = body.get("id").and_then(Value::as_str) else {
                    return;
                };
                let mut store = self
                    .data
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(Value::Object(entry)) = store.get_mut("entry")
                    && let Some(Value::Object(one)) = entry.get_mut(id)
                {
                    one.insert("data".to_string(), body.get("data").cloned().unwrap_or(Value::Null));
                }
            }
            Some("sandbox/message") => {
                let mut buf = self
                    .sandbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if buf.len() >= SANDBOX_KEEP {
                    let drop = buf.len() - SANDBOX_KEEP + 1;
                    buf.drain(..drop);
                }
                buf.push(body);
            }
            Some("sandbox/clear") => {
                self.sandbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();
            }
            Some("sandbox/request") => {
                // Bot 侧反过来问消息 / 频道信息（撤回、引用回显）：照上游网页客户端的桩答
                let method = body.get("method").and_then(Value::as_str).unwrap_or("");
                let data = body.get("data").cloned().unwrap_or(Value::Null);
                let nonce = body.get("nonce").cloned().unwrap_or(Value::Null);
                let result = self.sandbox_answer(method, &data);
                let id = self.next_id.fetch_add(1, Ordering::SeqCst);
                let payload = json!({ "type": "sandbox/response", "args": [nonce, result], "id": id });
                let _ = self.out.send(Message::Text(payload.to_string()));
            }
            _ => {}
        }
    }

    fn sandbox_answer(&self, method: &str, data: &Value) -> Value {
        let str_of = |k: &str| data.get(k).and_then(Value::as_str).unwrap_or("");
        match method {
            "deleteMessage" => {
                let (channel, id) = (str_of("channelId"), str_of("messageId"));
                self.sandbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .retain(|m| {
                        !(m.get("id").and_then(Value::as_str) == Some(id)
                            && m.get("channel").and_then(Value::as_str) == Some(channel))
                    });
                Value::Null
            }
            "getMessage" => {
                let (channel, id) = (str_of("channelId"), str_of("messageId"));
                self.sandbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .iter()
                    .find(|m| {
                        m.get("id").and_then(Value::as_str) == Some(id)
                            && m.get("channel").and_then(Value::as_str) == Some(channel)
                    })
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            "getChannel" => json!({ "channelId": "#" }),
            "getChannelList" => json!({ "data": { "channelId": "#" } }),
            "getGuild" => json!({ "guildId": "#" }),
            "getGuildList" => json!({ "data": { "guildId": "#" } }),
            "getGuildMember" => {
                let user = str_of("userId");
                json!({ "userId": user, "username": user })
            }
            "getGuildMemberList" => {
                let buf = self
                    .sandbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut users: Vec<Value> = Vec::new();
                for m in buf.iter() {
                    let Some(ch) = m.get("channel").and_then(Value::as_str) else {
                        continue;
                    };
                    let Some(user) = ch.strip_prefix('@') else { continue };
                    if users.iter().any(|u| u.get("userId").and_then(Value::as_str) == Some(user)) {
                        continue;
                    }
                    users.push(json!({ "userId": user, "username": user }));
                }
                json!({ "data": users })
            }
            _ => Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader() -> (ReaderCtx, mpsc::UnboundedReceiver<Message>) {
        let (out, rx) = mpsc::unbounded_channel();
        (
            ReaderCtx {
                pending: Arc::default(),
                data: Arc::default(),
                sandbox: Arc::default(),
                out,
                alive: Arc::new(AtomicBool::new(true)),
                changed: Arc::new(Notify::new()),
                next_id: Arc::new(AtomicU64::new(1000)),
            },
            rx,
        )
    }

    #[test]
    fn dispatch_routes_responses_and_keeps_only_wanted_pushes() {
        let (ctx, _rx) = reader();
        let (tx, mut rx) = oneshot::channel();
        ctx.pending.lock().unwrap().insert(3, tx);
        ctx.dispatch(r#"{"type":"response","body":{"id":3,"value":{"ok":1}}}"#);
        assert_eq!(rx.try_recv().unwrap().unwrap(), json!({"ok":1}));

        let (tx, mut rx) = oneshot::channel();
        ctx.pending.lock().unwrap().insert(4, tx);
        ctx.dispatch(r#"{"type":"response","body":{"id":4,"error":"unauthorized"}}"#);
        assert_eq!(rx.try_recv().unwrap().unwrap_err(), "unauthorized");

        ctx.dispatch(r#"{"type":"data","body":{"key":"market","value":{"big":true}}}"#);
        ctx.dispatch(r#"{"type":"data","body":{"key":"status","value":{"bots":{},"cpu":[0,0]}}}"#);
        ctx.dispatch(r#"{"type":"patch","body":{"key":"status","value":{"cpu":[1,1]}}}"#);
        ctx.dispatch(
            r#"{"type":"data","body":{"key":"explorer","value":[{"type":"file","name":"koishi.yml"}]}}"#,
        );
        let data = ctx.data.lock().unwrap();
        assert!(!data.contains_key("market"));
        assert_eq!(data["status"]["cpu"], json!([1, 1]));
        assert_eq!(data["status"]["bots"], json!({}));
        assert_eq!(data["explorer"][0]["name"], json!("koishi.yml"));
    }

    #[test]
    fn sandbox_messages_buffered_and_requests_answered() {
        let (ctx, mut rx) = reader();
        ctx.dispatch(r#"{"type":"sandbox/message","body":{"id":"m1","user":"koishi","channel":"@u","content":"hi"}}"#);
        ctx.dispatch(r#"{"type":"sandbox/message","body":{"id":"m2","user":"u","channel":"@u","content":"help"}}"#);
        assert_eq!(ctx.sandbox.lock().unwrap().len(), 2);

        // Bot 问 getMessage：从缓冲里捞出来回过去
        ctx.dispatch(
            r#"{"type":"sandbox/request","body":{"method":"getMessage","data":{"channelId":"@u","messageId":"m1"},"nonce":"n1"}}"#,
        );
        let answered: Value =
            serde_json::from_str(&rx.try_recv().unwrap().to_string()).unwrap();
        assert_eq!(answered["type"], json!("sandbox/response"));
        assert_eq!(answered["args"][0], json!("n1"));
        assert_eq!(answered["args"][1]["content"], json!("hi"));

        // 撤回：缓冲里删掉，回 null
        ctx.dispatch(
            r#"{"type":"sandbox/request","body":{"method":"deleteMessage","data":{"channelId":"@u","messageId":"m1"},"nonce":"n2"}}"#,
        );
        let answered: Value =
            serde_json::from_str(&rx.try_recv().unwrap().to_string()).unwrap();
        assert_eq!(answered["args"][1], Value::Null);
        assert_eq!(ctx.sandbox.lock().unwrap().len(), 1);

        ctx.dispatch(r#"{"type":"sandbox/clear"}"#);
        assert!(ctx.sandbox.lock().unwrap().is_empty());
    }

    #[test]
    fn entry_data_updates_merge_into_entry_push() {
        let (ctx, _rx) = reader();
        ctx.dispatch(
            r#"{"type":"data","body":{"key":"entry","value":{"abc":{"files":[],"data":{"help":{"name":"help"}}}}}}"#,
        );
        ctx.dispatch(
            r#"{"type":"entry-data","body":{"id":"abc","data":{"help":{"name":"help2"}}}}"#,
        );
        let data = ctx.data.lock().unwrap();
        assert_eq!(data["entry"]["abc"]["data"]["help"]["name"], json!("help2"));
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
