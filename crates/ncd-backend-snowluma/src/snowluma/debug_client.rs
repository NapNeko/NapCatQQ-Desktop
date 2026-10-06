//! SnowLuma WebUI 调试接口客户端（`/api/debug/*`）
//!
//! 调试台用它取动作目录、按账号调用动作、订阅 SSE 事件流。和 NapCat 一样刻意独立于
//! `SnowLumaWebUiClient` trait：那个 trait 的 mock 实现很多，调试台又只用这一小组接口。
//!
//! 与 NapCat 不同，SnowLuma 用真实 HTTP 状态码表达失败，成功时回包是裸 JSON（没有信封）。

use std::sync::Mutex as StdMutex;
use std::time::Duration;

use ncd_onebot::client::{BodyError, MAX_BODY_BYTES, RawCall, read_text_limited};
use reqwest::header::{ACCEPT, ACCEPT_ENCODING};
use reqwest::{Method, RequestBuilder, Response};
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock};

/// 优先 127.0.0.1（不经过名字解析），连不上再退回 localhost：
/// 少数机器上 WebUI 只监听在 localhost 解析出的那个地址族
const HOST_CANDIDATES: [&str; 2] = ["127.0.0.1", "localhost"];
/// 只限制建连；invoke 的超时逐次给，SSE 不能有整体超时
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 登录 / 取目录都是本机快速接口，给个兜底避免上游卡死时一直挂着
const META_TIMEOUT: Duration = Duration::from_secs(15);
/// 错误响应里没有 JSON message 时，截取原文的最大字符数（防止把整页 HTML 塞进错误）
const ERROR_TEXT_LIMIT: usize = 200;

/// 调试客户端的失败分类。每一类对应界面上一种不同的提示
#[derive(Debug, thiserror::Error)]
pub enum SnowLumaDebugError {
    #[error("SnowLuma 版本太老，没有调试接口")]
    TooOld,
    #[error("SnowLuma WebUI 鉴权失败")]
    Unauthorized,
    /// 429 / 密码错误 / 需要两步验证。这几种情况下重试只会让上游锁得更久
    #[error("SnowLuma WebUI 登录受限：{0}")]
    LoginBlocked(String),
    #[error("需要先在 SnowLuma WebUI 同意协议或修改密码")]
    NeedsConsent,
    #[error("账号不在线")]
    NotOnline,
    #[error("请求超时")]
    Timeout,
    #[error("连接 SnowLuma WebUI 失败：{0}")]
    Http(String),
    #[error("SnowLuma 返回 HTTP {status}：{message}")]
    Status { status: u16, message: String },
    #[error("解析 SnowLuma 返回失败：{0}")]
    Decode(String),
}

impl SnowLumaDebugError {
    /// 传输层错误：超时单独一类，其余（含建连失败）统一归为 Http
    fn from_transport(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout
        } else {
            Self::Http(err.to_string())
        }
    }
}

/// 一个 SnowLuma 实例的调试接口客户端。token 懒登录、失效自动换新
pub struct SnowLumaDebugClient {
    http: reqwest::Client,
    port: u16,
    password: String,
    /// 探测成功的那个主机名；首次请求前为空
    host: RwLock<Option<String>>,
    // 同一把锁同时保护「读缓存」和「登录」，并发请求不会各自登录一遍
    token: Mutex<Option<String>>,
    /// 登录被拒（密码错误 / 429 / 需两步验证）后记下原因，此后本实例不再打 /api/login。
    /// 上游按来源 IP 计失败次数、5 次锁 15 分钟，重复尝试会把用户改对的新密码也锁在外面。
    /// 只在同步代码里短暂持有，用标准库的锁即可
    blocked: StdMutex<Option<String>>,
}

impl SnowLumaDebugClient {
    pub fn new(port: u16, password: String) -> Result<Self, SnowLumaDebugError> {
        // 只访问本机 WebUI，不能被系统代理劫走；不设全局超时，见 CONNECT_TIMEOUT
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .no_proxy()
            .build()
            .map_err(|e| SnowLumaDebugError::Http(e.to_string()))?;
        Ok(Self {
            http,
            port,
            password,
            host: RwLock::new(None),
            token: Mutex::new(None),
            blocked: StdMutex::new(None),
        })
    }

    /// `GET /api/debug/actions`：动作目录（裸 JSON，`{actions, categories}`）
    pub async fn actions(&self) -> Result<Value, SnowLumaDebugError> {
        let resp = self
            .authed(
                Method::GET,
                "/api/debug/actions",
                None,
                RequestKind::Timed(META_TIMEOUT),
            )
            .await?;
        let status = resp.status();
        let text = read_text(resp).await?;
        if status.is_success() {
            return serde_json::from_str(&text)
                .map_err(|e| SnowLumaDebugError::Decode(e.to_string()));
        }
        if status.as_u16() == 404 {
            return Err(SnowLumaDebugError::TooOld);
        }
        Err(classify_failure(status.as_u16(), &text))
    }

    /// `POST /api/debug/invoke`：以某个在线账号调用动作，回包是裸的 OneBot 回复。
    /// OneBot 自己的失败（retcode 非 0）也是 HTTP 200，原样交给上层。
    pub async fn invoke(
        &self,
        uin: &str,
        action: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<RawCall, SnowLumaDebugError> {
        let body = json!({ "uin": uin, "action": action, "params": params });
        let resp = self
            .authed(
                Method::POST,
                "/api/debug/invoke",
                Some(&body),
                RequestKind::Timed(timeout),
            )
            .await?;
        let status = resp.status();
        let text = read_text(resp).await?;
        if status.is_success() {
            let value = serde_json::from_str(&text)
                .map_err(|e| SnowLumaDebugError::Decode(e.to_string()))?;
            return Ok(RawCall { text, value });
        }
        if status.as_u16() == 404 {
            // 路由存在但账号没上线，与路由根本不存在（老版本）都是 404，
            // 靠回包里的 message 区分：老版本回的是框架默认的非 JSON / 其它文案
            return Err(if is_account_offline_body(&text) {
                SnowLumaDebugError::NotOnline
            } else {
                SnowLumaDebugError::TooOld
            });
        }
        Err(classify_failure(status.as_u16(), &text))
    }

    /// `GET /api/debug/stream`：打开 SSE 长连接，把响应交给调用方按流读。
    /// 上游只给连接时已在线的账号挂钩子，所以账号上线后要由调用方重连
    pub async fn open_stream(&self) -> Result<Response, SnowLumaDebugError> {
        let resp = self
            .authed(
                Method::GET,
                "/api/debug/stream",
                None,
                RequestKind::EventStream,
            )
            .await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        if status.as_u16() == 404 {
            return Err(SnowLumaDebugError::TooOld);
        }
        let text = read_text(resp).await.unwrap_or_default();
        Err(classify_failure(status.as_u16(), &text))
    }

    /// `POST /api/debug/invoke-stream`：调用一个流式动作，帧按 SSE 一路推回（不丢帧）。
    /// 成功时把响应交给调用方按流读，最后一帧之前都是中间帧
    pub async fn invoke_stream(
        &self,
        uin: &str,
        action: &str,
        params: &Value,
    ) -> Result<Response, SnowLumaDebugError> {
        let body = json!({ "uin": uin, "action": action, "params": params });
        let resp = self
            .authed(
                Method::POST,
                "/api/debug/invoke-stream",
                Some(&body),
                RequestKind::EventStream,
            )
            .await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        if status.as_u16() == 404 {
            // 与 invoke 同一套区分：账号没上线和路由根本不存在（老版本，没有这个路由）都是 404
            let text = read_text(resp).await.unwrap_or_default();
            return Err(if is_account_offline_body(&text) {
                SnowLumaDebugError::NotOnline
            } else {
                SnowLumaDebugError::TooOld
            });
        }
        let text = read_text(resp).await.unwrap_or_default();
        Err(classify_failure(status.as_u16(), &text))
    }

    /// `POST /api/debug/upload?filename=<name>`：把本机文件按原始字节流放到 SnowLuma 所在
    /// 机器上，返回那边的路径和大小；body 由调用方按自己的节奏拆块（进度顺带在流里记）。
    ///
    /// 401 不重试：请求体是一次性消耗品，已经发出去的拿不回来重新发。没设请求级超时，
    /// 整体时限由调用方用别的方式兜
    pub async fn upload_file(
        &self,
        file_name: &str,
        body: reqwest::Body,
    ) -> Result<SnowLumaUpload, SnowLumaDebugError> {
        // login 成功后主机已经探定；上传只发一次，拿起已经记住的那台，不再做主机回退
        let token = self.ensure_token().await?;
        let host = self
            .host
            .read()
            .await
            .clone()
            .unwrap_or_else(|| HOST_CANDIDATES[0].to_owned());
        let resp = self
            .http
            .post(format!(
                "{}/api/debug/upload",
                Self::base_url(&host, self.port)
            ))
            .bearer_auth(&token)
            .query(&[("filename", file_name)])
            .body(body)
            .send()
            .await
            .map_err(SnowLumaDebugError::from_transport)?;
        let status = resp.status();
        if status.as_u16() == 401 {
            self.invalidate(&token).await;
            return Err(SnowLumaDebugError::Unauthorized);
        }
        if status.as_u16() == 404 {
            return Err(SnowLumaDebugError::TooOld);
        }
        let text = read_text(resp).await?;
        if !status.is_success() {
            return Err(classify_failure(status.as_u16(), &text));
        }
        let value: Value =
            serde_json::from_str(&text).map_err(|e| SnowLumaDebugError::Decode(e.to_string()))?;
        let ok = value.get("status").and_then(Value::as_str) == Some("ok");
        match (
            ok,
            value.get("path").and_then(Value::as_str),
            value.get("size").and_then(Value::as_u64),
        ) {
            (true, Some(path), Some(size)) => Ok(SnowLumaUpload {
                path: path.to_owned(),
                size,
            }),
            _ => Err(SnowLumaDebugError::Decode(
                "上传响应缺少 status/path/size 字段".into(),
            )),
        }
    }

    fn base_url(host: &str, port: u16) -> String {
        format!("http://{host}:{port}")
    }

    fn blocked_reason(&self) -> Option<String> {
        self.blocked
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn latch_blocked(&self, reason: &str) {
        *self
            .blocked
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(reason.to_owned());
    }

    /// 发请求并处理主机回退：已探测过就直接用记住的主机；
    /// 否则先试 127.0.0.1，仅在「连不上」时才换 localhost，其它错误换主机也没用
    async fn send(
        &self,
        build: impl Fn(String) -> RequestBuilder,
    ) -> Result<Response, SnowLumaDebugError> {
        let known = self.host.read().await.clone();
        if let Some(host) = known {
            return build(Self::base_url(&host, self.port))
                .send()
                .await
                .map_err(SnowLumaDebugError::from_transport);
        }
        let mut last_connect_error = None;
        for host in HOST_CANDIDATES {
            match build(Self::base_url(host, self.port)).send().await {
                Ok(resp) => {
                    *self.host.write().await = Some(host.to_owned());
                    return Ok(resp);
                }
                Err(err) if err.is_connect() => last_connect_error = Some(err),
                Err(err) => return Err(SnowLumaDebugError::from_transport(err)),
            }
        }
        Err(last_connect_error
            .map(SnowLumaDebugError::from_transport)
            .unwrap_or_else(|| SnowLumaDebugError::Http("没有可用的本机地址".into())))
    }

    /// 登录拿 token。任何「拒绝登录」的回答都会锁存，见 [Self::blocked]
    async fn login(&self) -> Result<String, SnowLumaDebugError> {
        let body = json!({ "password": self.password });
        let resp = self
            .send(|base| {
                self.http
                    .post(format!("{base}/api/login"))
                    .timeout(META_TIMEOUT)
                    .json(&body)
            })
            .await?;
        let status = resp.status().as_u16();
        let text = read_text(resp).await?;
        let json: Option<Value> = serde_json::from_str(&text).ok();
        let message = json_message(json.as_ref()).unwrap_or_else(|| truncate_text(&text));

        // 需要两步验证：不管 HTTP 状态码是什么都不能继续（调试台不支持 TOTP）
        if json
            .as_ref()
            .and_then(|v| v.get("needsTotp"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(self.block_login("需要两步验证（TOTP），调试台暂不支持"));
        }
        if status == 429 {
            let reason = if message.is_empty() {
                "登录尝试过于频繁，请稍后再试"
            } else {
                message.as_str()
            };
            return Err(self.block_login(reason));
        }
        if (status == 401 || status == 403) && text.contains("密码错误") {
            return Err(self.block_login("密码错误"));
        }
        if status == 403 && requires_consent(json.as_ref()) {
            return Err(SnowLumaDebugError::NeedsConsent);
        }
        if !(200..300).contains(&status) {
            return Err(classify_failure(status, &text));
        }

        let json = json.ok_or_else(|| SnowLumaDebugError::Decode("登录响应不是 JSON".into()))?;
        if json.get("success").and_then(Value::as_bool) == Some(true) {
            return json
                .get("token")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| SnowLumaDebugError::Decode("登录响应缺少 token".into()));
        }
        // success 不是 true：上游拒绝了这个密码，同样不再重试
        let reason = if message.is_empty() {
            "登录被拒绝"
        } else {
            message.as_str()
        };
        Err(self.block_login(reason))
    }

    fn block_login(&self, reason: &str) -> SnowLumaDebugError {
        self.latch_blocked(reason);
        SnowLumaDebugError::LoginBlocked(reason.to_owned())
    }

    async fn ensure_token(&self) -> Result<String, SnowLumaDebugError> {
        let mut guard = self.token.lock().await;
        if let Some(cached) = guard.as_ref() {
            return Ok(cached.clone());
        }
        if let Some(reason) = self.blocked_reason() {
            return Err(SnowLumaDebugError::LoginBlocked(reason));
        }
        let fresh = self.login().await?;
        *guard = Some(fresh.clone());
        Ok(fresh)
    }

    /// 丢弃用过的 token。只在缓存里还是这一份时才清：
    /// 别的并发请求可能已经换上了新 token，不能被我们误清
    async fn invalidate(&self, used: &str) {
        let mut guard = self.token.lock().await;
        if guard.as_deref() == Some(used) {
            *guard = None;
        }
    }

    /// 带 token 发一次请求，返回原始响应由调用方按路由语义解释。
    /// 中间件 401（token 过期 / 上游重启）时丢掉 token、重新登录、重试一次；
    /// 第二次还是 401 就原样返回，由调用方归为 Unauthorized
    async fn authed(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        kind: RequestKind,
    ) -> Result<Response, SnowLumaDebugError> {
        for attempt in 0..2 {
            let token = self.ensure_token().await?;
            let resp = self
                .send(|base| {
                    let mut req = self
                        .http
                        .request(method.clone(), format!("{base}{path}"))
                        .bearer_auth(&token);
                    match kind {
                        RequestKind::Timed(timeout) => req = req.timeout(timeout),
                        // SSE 不能有整体超时；显式要求不压缩，
                        // 否则中间层的 gzip 会攒够一块才吐，事件就被憋住了
                        RequestKind::EventStream => {
                            req = req
                                .header(ACCEPT, "text/event-stream")
                                .header(ACCEPT_ENCODING, "identity");
                        }
                    }
                    if let Some(body) = body {
                        req = req.json(body);
                    }
                    req
                })
                .await?;
            if resp.status().as_u16() == 401 && attempt == 0 {
                self.invalidate(&token).await;
                continue;
            }
            return Ok(resp);
        }
        Err(SnowLumaDebugError::Unauthorized)
    }
}

/// 请求的两种形态：有整体超时的普通请求，和不能有超时的事件流
#[derive(Clone, Copy)]
enum RequestKind {
    Timed(Duration),
    EventStream,
}

/// `/api/debug/upload` 放好的一份文件
#[derive(Debug, Clone, PartialEq)]
pub struct SnowLumaUpload {
    /// 文件在 SnowLuma 所在机器上的路径，可直接作为发送动作的 file 参数
    pub path: String,
    pub size: u64,
}

/// 从 `{"message": "..."}` 里取文案
fn json_message(json: Option<&Value>) -> Option<String> {
    json.and_then(|v| v.get("message"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn truncate_text(text: &str) -> String {
    text.trim().chars().take(ERROR_TEXT_LIMIT).collect()
}

/// 403 里 `consentRequired` / `mustChangePassword` 为 true，
/// 说明要先去 WebUI 里同意协议或改掉初始密码，接口才会放行
fn requires_consent(json: Option<&Value>) -> bool {
    let flag = |key: &str| {
        json.and_then(|v| v.get(key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    flag("consentRequired") || flag("mustChangePassword")
}

/// invoke 的 404 回包是不是「账号不在线」：必须是 JSON 且 message 命中
fn is_account_offline_body(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| json_message(Some(&v)))
        .is_some_and(|message| message.contains("账号不在线"))
}

/// 把非成功响应归类。404 的含义因路由而异，由调用方在调用本函数前先处理
fn classify_failure(status: u16, text: &str) -> SnowLumaDebugError {
    let json: Option<Value> = serde_json::from_str(text).ok();
    if status == 403 && requires_consent(json.as_ref()) {
        return SnowLumaDebugError::NeedsConsent;
    }
    if status == 401 {
        return SnowLumaDebugError::Unauthorized;
    }
    SnowLumaDebugError::Status {
        status,
        message: json_message(json.as_ref()).unwrap_or_else(|| truncate_text(text)),
    }
}

/// 读完响应体（有上限，见 [`MAX_BODY_BYTES`]）。流式的 `/api/debug/stream` 不走这里
async fn read_text(resp: Response) -> Result<String, SnowLumaDebugError> {
    read_text_limited(resp, MAX_BODY_BYTES)
        .await
        .map_err(|err| match err {
            BodyError::Read(err) => SnowLumaDebugError::from_transport(err),
            too_large @ BodyError::TooLarge { .. } => {
                SnowLumaDebugError::Decode(too_large.to_string())
            }
        })
}

/// SSE `data:` 行里一帧的语义。字段缺失或类型不对的帧归为 [SlStreamFrame::Unknown]，
/// 不能让一帧坏数据中断整条流
#[derive(Debug, Clone, PartialEq)]
pub enum SlStreamFrame {
    /// 连接建立
    Ready,
    /// 某账号收到的 OneBot 事件
    Event {
        uin: String,
        event: Value,
    },
    /// 某账号执行了一次动作（包含调试台自己发起的调用，上游不标来源）
    Action {
        uin: String,
        action: String,
        params: Value,
        response: Value,
        ms: u64,
    },
    /// 上游因消费太慢丢了 `count` 帧
    Dropped {
        count: u32,
    },
    Unknown,
}

/// 解析一条 SSE `data:` 载荷。`uin` 上游有时给数字有时给字符串，统一成字符串
pub fn parse_stream_frame(data: &str) -> SlStreamFrame {
    let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(data) else {
        return SlStreamFrame::Unknown;
    };
    let kind = map.get("kind").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "ready" => SlStreamFrame::Ready,
        "event" => {
            let (Some(uin), Some(event)) = (frame_uin(&map), map.remove("event")) else {
                return SlStreamFrame::Unknown;
            };
            SlStreamFrame::Event { uin, event }
        }
        "action" => {
            let (Some(uin), Some(action)) = (
                frame_uin(&map),
                map.get("action").and_then(Value::as_str).map(str::to_owned),
            ) else {
                return SlStreamFrame::Unknown;
            };
            SlStreamFrame::Action {
                uin,
                action,
                params: map.remove("params").unwrap_or(Value::Null),
                response: map.remove("response").unwrap_or(Value::Null),
                ms: map.get("ms").and_then(frame_millis).unwrap_or(0),
            }
        }
        "dropped" => match map.get("count").and_then(Value::as_u64) {
            Some(count) => SlStreamFrame::Dropped {
                count: u32::try_from(count).unwrap_or(u32::MAX),
            },
            None => SlStreamFrame::Unknown,
        },
        _ => SlStreamFrame::Unknown,
    }
}

fn frame_uin(map: &serde_json::Map<String, Value>) -> Option<String> {
    match map.get("uin")? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// 耗时可能是整数也可能带小数，小数按四舍五入取整
fn frame_millis(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|ms| ms.is_finite() && *ms >= 0.0)
            .map(|ms| ms.round() as u64)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    use super::*;

    /// 取出 wiremock 在 127.0.0.1 上分配到的随机端口
    fn mock_server_port(server: &MockServer) -> u16 {
        let addr = server.address();
        assert_eq!(
            addr.ip().to_string(),
            "127.0.0.1",
            "wiremock must bind to 127.0.0.1 only"
        );
        addr.port()
    }

    fn client_for(server: &MockServer) -> SnowLumaDebugClient {
        SnowLumaDebugClient::new(mock_server_port(server), "pwd".into()).expect("build client")
    }

    async fn mount_login(server: &MockServer, token: &str) {
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .and(body_partial_json(json!({ "password": "pwd" })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "success": true, "token": token })),
            )
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn actions_logs_in_then_returns_bare_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .and(body_partial_json(json!({ "password": "pwd" })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "success": true, "token": "t1" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/debug/actions"))
            .and(header("authorization", "Bearer t1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "actions": [{ "name": "send_msg", "params": [] }],
                "categories": [{ "category": "message", "count": 1 }],
            })))
            .expect(1)
            .mount(&server)
            .await;

        let actions = client_for(&server).actions().await.expect("actions");
        assert_eq!(actions["actions"][0]["name"], json!("send_msg"));
        assert_eq!(actions["categories"][0]["count"], json!(1));
    }

    #[tokio::test]
    async fn actions_404_maps_to_too_old() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("GET"))
            .and(path("/api/debug/actions"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
            .mount(&server)
            .await;

        let err = client_for(&server).actions().await.expect_err("404");
        assert!(matches!(err, SnowLumaDebugError::TooOld), "got {err:?}");
    }

    #[tokio::test]
    async fn invoke_returns_bare_reply_and_sends_body() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke"))
            .and(header("authorization", "Bearer t1"))
            .and(body_partial_json(json!({
                "uin": "10001",
                "action": "get_login_info",
                "params": { "no_cache": true },
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "retcode": 0,
                "data": { "user_id": 10001 },
            })))
            .expect(1)
            .mount(&server)
            .await;

        let raw = client_for(&server)
            .invoke(
                "10001",
                "get_login_info",
                &json!({ "no_cache": true }),
                Duration::from_secs(5),
            )
            .await
            .expect("invoke");
        assert_eq!(raw.value["retcode"], json!(0));
        assert_eq!(raw.value["data"]["user_id"], json!(10001));
        let reparsed: Value = serde_json::from_str(&raw.text).expect("text is json");
        assert_eq!(reparsed, raw.value);
    }

    #[tokio::test]
    async fn invoke_404_offline_account_maps_to_not_online() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({ "status": "failed", "message": "账号不在线" })),
            )
            .mount(&server)
            .await;

        let err = client_for(&server)
            .invoke(
                "10001",
                "get_login_info",
                &json!({}),
                Duration::from_secs(5),
            )
            .await
            .expect_err("offline");
        assert!(matches!(err, SnowLumaDebugError::NotOnline), "got {err:?}");
    }

    #[tokio::test]
    async fn invoke_404_with_other_body_maps_to_too_old() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke"))
            .respond_with(
                ResponseTemplate::new(404).set_body_string("Cannot POST /api/debug/invoke"),
            )
            .mount(&server)
            .await;

        let err = client_for(&server)
            .invoke(
                "10001",
                "get_login_info",
                &json!({}),
                Duration::from_secs(5),
            )
            .await
            .expect_err("route missing");
        assert!(matches!(err, SnowLumaDebugError::TooOld), "got {err:?}");
    }

    #[tokio::test]
    async fn invoke_400_maps_to_status_with_message() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "status": "failed", "message": "缺少 action" })),
            )
            .mount(&server)
            .await;

        let err = client_for(&server)
            .invoke("10001", "", &json!({}), Duration::from_secs(5))
            .await
            .expect_err("400");
        match err {
            SnowLumaDebugError::Status { status, message } => {
                assert_eq!(status, 400);
                assert_eq!(message, "缺少 action");
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn invoke_times_out_with_per_call_timeout() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "status": "ok", "retcode": 0 }))
                    .set_delay(Duration::from_millis(600)),
            )
            .mount(&server)
            .await;

        let err = client_for(&server)
            .invoke("10001", "slow", &json!({}), Duration::from_millis(100))
            .await
            .expect_err("must time out");
        assert!(matches!(err, SnowLumaDebugError::Timeout), "got {err:?}");
    }

    #[tokio::test]
    async fn middleware_401_relogs_in_once_and_retries() {
        let server = MockServer::start().await;
        let logins = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&logins);
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(move |_req: &Request| {
                let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "success": true, "token": format!("t{n}") }))
            })
            .mount(&server)
            .await;
        // 第一份 token 被上游判为过期
        Mock::given(method("GET"))
            .and(path("/api/debug/actions"))
            .and(header("authorization", "Bearer t1"))
            .respond_with(ResponseTemplate::new(401).set_body_json(
                json!({ "status": "failed", "message": "Token expired or invalid" }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/debug/actions"))
            .and(header("authorization", "Bearer t2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "actions": [] })))
            .expect(1)
            .mount(&server)
            .await;

        let actions = client_for(&server).actions().await.expect("retry succeeds");
        assert_eq!(actions["actions"], json!([]));
        assert_eq!(logins.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn persistent_401_maps_to_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "success": true, "token": "t" })),
            )
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/debug/actions"))
            .respond_with(ResponseTemplate::new(401))
            .expect(2)
            .mount(&server)
            .await;

        let err = client_for(&server).actions().await.expect_err("401 twice");
        assert!(
            matches!(err, SnowLumaDebugError::Unauthorized),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn wrong_password_latches_and_stops_login_attempts() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(json!({ "success": false, "message": "密码错误" })),
            )
            // 两次调用只允许真正打一次登录
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server);
        let first = client.actions().await.expect_err("first");
        assert!(
            matches!(&first, SnowLumaDebugError::LoginBlocked(msg) if msg.contains("密码错误")),
            "got {first:?}"
        );
        let second = client
            .invoke(
                "10001",
                "get_login_info",
                &json!({}),
                Duration::from_secs(5),
            )
            .await
            .expect_err("second");
        assert!(
            matches!(&second, SnowLumaDebugError::LoginBlocked(msg) if msg.contains("密码错误")),
            "got {second:?}"
        );
    }

    #[tokio::test]
    async fn rate_limited_login_maps_to_login_blocked() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_json(json!({ "success": false, "message": "尝试次数过多" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client.actions().await.expect_err("429");
        assert!(
            matches!(&err, SnowLumaDebugError::LoginBlocked(msg) if msg == "尝试次数过多"),
            "got {err:?}"
        );
        // 已锁存：再调一次也不会重新打登录（expect(1) 会在 drop 时校验）
        client.actions().await.expect_err("latched");
    }

    #[tokio::test]
    async fn needs_totp_maps_to_login_blocked() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "success": false, "needsTotp": true })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let err = client_for(&server).actions().await.expect_err("totp");
        assert!(
            matches!(&err, SnowLumaDebugError::LoginBlocked(msg) if msg.contains("两步验证")),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn consent_403_maps_to_needs_consent() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("GET"))
            .and(path("/api/debug/actions"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_json(json!({ "status": "failed", "consentRequired": true })),
            )
            .mount(&server)
            .await;

        let err = client_for(&server).actions().await.expect_err("consent");
        assert!(
            matches!(err, SnowLumaDebugError::NeedsConsent),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn must_change_password_403_maps_to_needs_consent() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_json(json!({ "status": "failed", "mustChangePassword": true })),
            )
            .mount(&server)
            .await;

        let err = client_for(&server)
            .invoke(
                "10001",
                "get_login_info",
                &json!({}),
                Duration::from_secs(5),
            )
            .await
            .expect_err("must change password");
        assert!(
            matches!(err, SnowLumaDebugError::NeedsConsent),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn open_stream_sends_bearer_and_accept_and_returns_response() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("GET"))
            .and(path("/api/debug/stream"))
            .and(header("authorization", "Bearer t1"))
            .and(header("accept", "text/event-stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string("data: {\"kind\":\"ready\"}\n\n"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let resp = client_for(&server).open_stream().await.expect("stream");
        assert!(resp.status().is_success());
        let body = resp.text().await.expect("body");
        assert_eq!(
            parse_stream_frame(body.trim_start_matches("data: ").trim()),
            SlStreamFrame::Ready
        );
    }

    #[tokio::test]
    async fn open_stream_404_maps_to_too_old() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("GET"))
            .and(path("/api/debug/stream"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let err = client_for(&server).open_stream().await.expect_err("404");
        assert!(matches!(err, SnowLumaDebugError::TooOld), "got {err:?}");
    }

    #[tokio::test]
    async fn connect_failure_maps_to_http_error() {
        // 先占一个端口再释放，保证这个端口上没有任何服务在听
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            listener.local_addr().expect("addr").port()
        };
        let client = SnowLumaDebugClient::new(port, "pwd".into()).expect("build client");
        let err = client.actions().await.expect_err("nothing listening");
        assert!(matches!(err, SnowLumaDebugError::Http(_)), "got {err:?}");
    }

    /// 127.0.0.1 上没人听、localhost 解析出的 IPv6 地址上有服务时，应回退成功并记住主机。
    /// 机器没有 IPv6 loopback 或 localhost 不解析出 IPv6 时，这个场景造不出来，直接跳过
    #[tokio::test]
    async fn falls_back_to_localhost_when_ipv4_loopback_refused() {
        let Ok(listener) = std::net::TcpListener::bind("[::1]:0") else {
            return;
        };
        let port = listener.local_addr().expect("addr").port();
        let resolves_ipv6 = tokio::net::lookup_host(("localhost", port))
            .await
            .map(|mut addrs| addrs.any(|a| a.is_ipv6()))
            .unwrap_or(false);
        // 这个端口的 127.0.0.1 上也不能有别的进程在听，否则就不会触发回退
        let ipv4_free = std::net::TcpStream::connect(("127.0.0.1", port)).is_err();
        if !resolves_ipv6 || !ipv4_free {
            return;
        }
        let server = MockServer::builder().listener(listener).start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("GET"))
            .and(path("/api/debug/actions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "actions": [] })))
            .mount(&server)
            .await;

        let client = SnowLumaDebugClient::new(port, "pwd".into()).expect("build client");
        client.actions().await.expect("falls back to localhost");
        assert_eq!(client.host.read().await.as_deref(), Some("localhost"));
    }

    #[tokio::test]
    async fn invoke_stream_returns_the_sse_body_after_login() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke-stream"))
            .and(header("authorization", "Bearer t1"))
            .and(body_partial_json(json!({
                "uin": "10001",
                "action": "download_file_stream",
                "params": {},
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        "data: {\"status\":\"ok\",\"retcode\":0,\"data\":{\"type\":\"stream\"}}\n\n\
                         data: {\"status\":\"ok\",\"retcode\":0,\"data\":{\"type\":\"response\"}}\n\n",
                    ),
            )
            .expect(1)
            .mount(&server)
            .await;

        let resp = client_for(&server)
            .invoke_stream("10001", "download_file_stream", &json!({}))
            .await
            .expect("invoke_stream");
        assert!(resp.status().is_success());
        let text = resp.text().await.expect("body");
        assert_eq!(text.matches("data: ").count(), 2);
    }

    #[tokio::test]
    async fn invoke_stream_404_distinguishes_offline_account_from_missing_route() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke-stream"))
            .respond_with(
                ResponseTemplate::new(404).set_body_string("Cannot POST /api/debug/invoke-stream"),
            )
            .mount(&server)
            .await;
        let err = client_for(&server)
            .invoke_stream("10001", "download_file_stream", &json!({}))
            .await
            .expect_err("route missing");
        assert!(matches!(err, SnowLumaDebugError::TooOld), "got {err:?}");

        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/invoke-stream"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json!({ "status": "failed", "message": "账号不在线" })),
            )
            .mount(&server)
            .await;
        let err = client_for(&server)
            .invoke_stream("10001", "download_file_stream", &json!({}))
            .await
            .expect_err("offline");
        assert!(matches!(err, SnowLumaDebugError::NotOnline), "got {err:?}");
    }

    #[tokio::test]
    async fn upload_file_posts_raw_body_and_returns_remote_path() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/upload"))
            .and(header("authorization", "Bearer t1"))
            .and(wiremock::matchers::query_param("filename", "a.png"))
            .and(wiremock::matchers::body_string("hello world"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "path": "/tmp/webui-upload/ab__a.png",
                "size": 11,
            })))
            .expect(1)
            .mount(&server)
            .await;

        let upload = client_for(&server)
            .upload_file(
                "a.png",
                reqwest::Body::from("hello world".as_bytes().to_vec()),
            )
            .await
            .expect("upload");
        assert_eq!(
            upload,
            SnowLumaUpload {
                path: "/tmp/webui-upload/ab__a.png".into(),
                size: 11,
            }
        );
    }

    #[tokio::test]
    async fn upload_file_maps_404_to_too_old_and_400_to_status() {
        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/upload"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let err = client_for(&server)
            .upload_file("a.png", reqwest::Body::from("x".as_bytes().to_vec()))
            .await
            .expect_err("404");
        assert!(matches!(err, SnowLumaDebugError::TooOld), "got {err:?}");

        let server = MockServer::start().await;
        mount_login(&server, "t1").await;
        Mock::given(method("POST"))
            .and(path("/api/debug/upload"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "status": "failed", "message": "上传超出大小上限" })),
            )
            .mount(&server)
            .await;
        let err = client_for(&server)
            .upload_file("a.png", reqwest::Body::from("x".as_bytes().to_vec()))
            .await
            .expect_err("400");
        match err {
            SnowLumaDebugError::Status { status, message } => {
                assert_eq!(status, 400);
                assert_eq!(message, "上传超出大小上限");
            }
            other => panic!("expected Status, got {other:?}"),
        }
    }

    #[test]
    fn parse_stream_frame_reads_all_five_kinds() {
        assert_eq!(
            parse_stream_frame(r#"{"kind":"ready"}"#),
            SlStreamFrame::Ready
        );
        assert_eq!(
            parse_stream_frame(r#"{"kind":"event","uin":"123","event":{"post_type":"message"}}"#),
            SlStreamFrame::Event {
                uin: "123".into(),
                event: json!({ "post_type": "message" }),
            }
        );
        assert_eq!(
            parse_stream_frame(
                r#"{"kind":"action","uin":"123","action":"send_msg","params":{"a":1},"response":{"retcode":0},"ms":42}"#
            ),
            SlStreamFrame::Action {
                uin: "123".into(),
                action: "send_msg".into(),
                params: json!({ "a": 1 }),
                response: json!({ "retcode": 0 }),
                ms: 42,
            }
        );
        assert_eq!(
            parse_stream_frame(r#"{"kind":"dropped","count":7}"#),
            SlStreamFrame::Dropped { count: 7 }
        );
        assert_eq!(
            parse_stream_frame(r#"{"kind":"something-new"}"#),
            SlStreamFrame::Unknown
        );
    }

    #[test]
    fn parse_stream_frame_normalizes_numeric_uin() {
        assert_eq!(
            parse_stream_frame(r#"{"kind":"event","uin":123456789,"event":{}}"#),
            SlStreamFrame::Event {
                uin: "123456789".into(),
                event: json!({}),
            }
        );
        match parse_stream_frame(
            r#"{"kind":"action","uin":10001,"action":"get_status","params":{},"response":{},"ms":3}"#,
        ) {
            SlStreamFrame::Action { uin, .. } => assert_eq!(uin, "10001"),
            other => panic!("expected Action, got {other:?}"),
        }
    }

    #[test]
    fn parse_stream_frame_tolerates_bad_input() {
        for data in [
            "",
            "not json",
            "[]",
            "null",
            r#"{"kind":"event","event":{}}"#,
            r#"{"kind":"event","uin":"1"}"#,
            r#"{"kind":"action","uin":"1"}"#,
            r#"{"kind":"dropped"}"#,
            r#"{"kind":7}"#,
        ] {
            assert_eq!(
                parse_stream_frame(data),
                SlStreamFrame::Unknown,
                "input: {data}"
            );
        }
    }

    #[test]
    fn parse_stream_frame_defaults_optional_action_fields() {
        assert_eq!(
            parse_stream_frame(r#"{"kind":"action","uin":"1","action":"get_status","ms":1.6}"#),
            SlStreamFrame::Action {
                uin: "1".into(),
                action: "get_status".into(),
                params: Value::Null,
                response: Value::Null,
                ms: 2,
            }
        );
    }
}
