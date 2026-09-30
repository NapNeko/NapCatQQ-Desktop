//! NapCat WebUI 调试接口客户端（`/api/Debug/*`）
//!
//! 调试台通过 NapCat 自带的调试适配器调用任意 OneBot 动作、取动作目录和事件流入口。
//! 这里刻意写成独立结构体而不是扩 `NapCatWebUiClient` trait：那个 trait 有一批 mock
//! 实现，加方法会牵连所有测试替身；调试台又只需要这一小组接口。
//!
//! NapCat 的 WebUI 一律回 HTTP 200 + `{code, data, message}` 信封，鉴权失败也是
//! `{"code":-1,"message":"Unauthorized"}` 而不是 401，所以「是否未授权」要同时看
//! 状态码和信封。

use std::time::Duration;

use ncd_onebot::client::{BodyError, MAX_BODY_BYTES, RawCall, read_body_limited};
use reqwest::Method;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::webui_client::login_hash;

/// 调试适配器的固定名字（上游只有这一个主适配器）
const ADAPTER_NAME: &str = "debug-primary";
/// 只限制建连；`call` 的超时由调用方逐次给，元数据请求另有兜底
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// 登录 / 取目录 / 建适配器都是本机快速接口，给个兜底避免上游卡死时一直挂着
const META_TIMEOUT: Duration = Duration::from_secs(15);

/// 调试客户端的失败分类。每一类对应界面上一种不同的提示
#[derive(Debug, thiserror::Error)]
pub enum NapCatDebugError {
    #[error("NapCat 版本太老，没有调试接口")]
    TooOld,
    #[error("NapCat WebUI 鉴权失败")]
    Unauthorized,
    #[error("NapCat WebUI 开启了两步验证，调试台暂不支持")]
    TwoFactorRequired,
    #[error("请求超时")]
    Timeout,
    #[error("连接 NapCat WebUI 失败：{0}")]
    Http(String),
    #[error("NapCat WebUI 返回 HTTP {0}")]
    Status(u16),
    #[error("解析 NapCat 返回失败：{0}")]
    Decode(String),
    #[error("{message}")]
    Business { code: i64, message: String },
}

impl NapCatDebugError {
    /// 传输层错误：超时单独一类，其余（含建连失败）统一归为 Http
    fn from_transport(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout
        } else {
            Self::Http(err.to_string())
        }
    }

    /// 读响应体阶段的错误：JSON 解不出来算 Decode，其余按传输层处理
    fn from_body(err: reqwest::Error) -> Self {
        if err.is_decode() {
            Self::Decode(err.to_string())
        } else {
            Self::from_transport(err)
        }
    }
}

/// 一个 NapCat 实例的调试接口客户端。凭据懒登录、过期自动换新
pub struct NapCatDebugClient {
    http: reqwest::Client,
    port: u16,
    token: String,
    // 同一把锁同时保护「读缓存」和「登录」，并发请求不会各自登录一遍
    credential: Mutex<Option<String>>,
}

impl NapCatDebugClient {
    pub fn new(port: u16, token: String) -> Result<Self, NapCatDebugError> {
        // 只访问本机 WebUI，不能被系统代理劫走；不设全局超时，见 CONNECT_TIMEOUT
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .no_proxy()
            .build()
            .map_err(|e| NapCatDebugError::Http(e.to_string()))?;
        Ok(Self {
            http,
            port,
            token,
            credential: Mutex::new(None),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// `GET /api/Debug/schemas`：以动作名为键的 schema 对象
    pub async fn schemas(&self) -> Result<Value, NapCatDebugError> {
        let data = self
            .authed(Method::GET, "/api/Debug/schemas", None, META_TIMEOUT)
            .await?;
        if data.is_object() {
            Ok(data)
        } else {
            Err(NapCatDebugError::Decode("schemas 不是对象".into()))
        }
    }

    /// `POST /api/Debug/create`：建（或复用）调试适配器，返回连 WS 用的适配器 token
    pub async fn create_adapter(&self) -> Result<String, NapCatDebugError> {
        let data = self
            .authed(
                Method::POST,
                "/api/Debug/create",
                Some(&json!({})),
                META_TIMEOUT,
            )
            .await?;
        data.get("token")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| NapCatDebugError::Decode("缺少适配器 token".into()))
    }

    /// `POST /api/Debug/call/debug-primary`：调用一个动作。
    ///
    /// 信封里的 `data` 才是 OneBot 回包；OneBot 自己的失败（retcode 非 0）仍是
    /// `code:0`，原样交给上层，只有 WebUI 层的失败（如「不支持的 API」）才是 Err。
    pub async fn call(
        &self,
        action: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<RawCall, NapCatDebugError> {
        let path = format!("/api/Debug/call/{ADAPTER_NAME}");
        let body = json!({ "action": action, "params": params });
        let value = self
            .authed(Method::POST, &path, Some(&body), timeout)
            .await?;
        let text =
            serde_json::to_string(&value).map_err(|e| NapCatDebugError::Decode(e.to_string()))?;
        Ok(RawCall { text, value })
    }

    /// 事件 WS 地址。WS 升级不走 WebUI 鉴权，只认适配器 token，
    /// 而 token 可能含需要转义的字符，所以逐字节编码
    pub fn ws_url(&self, adapter_token: &str) -> String {
        format!(
            "ws://127.0.0.1:{}/api/Debug/ws?adapterName={ADAPTER_NAME}&token={}",
            self.port,
            percent_encode(adapter_token)
        )
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// 登录拿凭据。登录失败一律按鉴权失败处理：拿不到凭据后面什么都做不了，
    /// 具体的上游文案对用户没有额外信息，只留在日志里
    async fn login(&self) -> Result<String, NapCatDebugError> {
        let resp = self
            .http
            .post(self.url("/api/auth/login"))
            .timeout(META_TIMEOUT)
            .json(&json!({ "hash": login_hash(&self.token) }))
            .send()
            .await
            .map_err(NapCatDebugError::from_transport)?;
        let status = resp.status();
        match status.as_u16() {
            // IP 白名单之类会返回真正的 401 / 403
            401 | 403 => return Err(NapCatDebugError::Unauthorized),
            code if !status.is_success() => return Err(NapCatDebugError::Status(code)),
            _ => {}
        }
        let body = read_json(resp).await?;
        let data = unwrap_envelope(body).map_err(|err| match err {
            NapCatDebugError::Business { code, message } => {
                tracing::debug!(code, %message, "NapCat WebUI 登录被拒绝");
                NapCatDebugError::Unauthorized
            }
            other => other,
        })?;
        // 开了两步验证时不会给凭据，必须先于取 Credential 判断
        if data.get("require2FA").and_then(Value::as_bool) == Some(true) {
            return Err(NapCatDebugError::TwoFactorRequired);
        }
        data.get("Credential")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| NapCatDebugError::Decode("登录响应缺少 Credential".into()))
    }

    async fn credential(&self) -> Result<String, NapCatDebugError> {
        let mut guard = self.credential.lock().await;
        if let Some(cached) = guard.as_ref() {
            return Ok(cached.clone());
        }
        let fresh = self.login().await?;
        *guard = Some(fresh.clone());
        Ok(fresh)
    }

    /// 丢弃用过的凭据。只在缓存里还是这一份时才清：
    /// 别的并发请求可能已经换上了新凭据，不能被我们误清
    async fn invalidate(&self, used: &str) {
        let mut guard = self.credential.lock().await;
        if guard.as_deref() == Some(used) {
            *guard = None;
        }
    }

    /// 带凭据发一次 Debug 请求并拆信封。未授权时清掉凭据重新登录、重试一次
    /// （NapCat 重启后旧凭据全部作废，一小时后也会过期）；第二次仍失败才报出去
    async fn authed(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value, NapCatDebugError> {
        for attempt in 0..2 {
            let credential = self.credential().await?;
            let mut req = self
                .http
                .request(method.clone(), self.url(path))
                .bearer_auth(&credential)
                .timeout(timeout);
            if let Some(body) = body {
                req = req.json(body);
            }
            let resp = req.send().await.map_err(NapCatDebugError::from_transport)?;
            match read_debug_response(resp).await {
                Err(NapCatDebugError::Unauthorized) if attempt == 0 => {
                    self.invalidate(&credential).await;
                }
                other => return other,
            }
        }
        Err(NapCatDebugError::Unauthorized)
    }
}

/// 读完响应体（有上限，见 [`MAX_BODY_BYTES`]）并解成 JSON
async fn read_json(resp: reqwest::Response) -> Result<Value, NapCatDebugError> {
    let bytes = read_body_limited(resp, MAX_BODY_BYTES)
        .await
        .map_err(|err| match err {
            BodyError::Read(err) => NapCatDebugError::from_body(err),
            too_large @ BodyError::TooLarge { .. } => {
                NapCatDebugError::Decode(too_large.to_string())
            }
        })?;
    serde_json::from_slice(&bytes).map_err(|e| NapCatDebugError::Decode(e.to_string()))
}

/// 读 `/api/Debug/*` 的响应：路由不存在说明 NapCat 太老，其余走信封
async fn read_debug_response(resp: reqwest::Response) -> Result<Value, NapCatDebugError> {
    let status = resp.status();
    match status.as_u16() {
        404 => return Err(NapCatDebugError::TooOld),
        401 | 403 => return Err(NapCatDebugError::Unauthorized),
        code if !status.is_success() => return Err(NapCatDebugError::Status(code)),
        _ => {}
    }
    unwrap_envelope(read_json(resp).await?)
}

/// 拆 `{code, data, message}`：`code == 0` 取 `data`；未授权文案单独归类，
/// 其余失败带上上游 message
fn unwrap_envelope(body: Value) -> Result<Value, NapCatDebugError> {
    let Value::Object(mut map) = body else {
        return Err(NapCatDebugError::Decode("响应不是 JSON 对象".into()));
    };
    let code = map
        .get("code")
        .and_then(Value::as_i64)
        .ok_or_else(|| NapCatDebugError::Decode("响应缺少 code 字段".into()))?;
    if code == 0 {
        return Ok(map.remove("data").unwrap_or(Value::Null));
    }
    let message = map
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if message.eq_ignore_ascii_case("Unauthorized") {
        return Err(NapCatDebugError::Unauthorized);
    }
    Err(NapCatDebugError::Business { code, message })
}

/// 按 RFC 3986 转义 query 值：只放行 unreserved 字符，其余逐字节 `%XX`
fn percent_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
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

    /// sha256("abc.napcat")
    const ABC_HASH: &str = "42e5515d256cb0ab3de18017ee3adefc15aa70229c27788bab5aee39d5d439e6";

    fn login_ok(credential: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({
            "code": 0,
            "message": "success",
            "data": { "Credential": credential },
        }))
    }

    async fn mount_login(server: &MockServer, credential: &str) {
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_partial_json(json!({ "hash": ABC_HASH })))
            .respond_with(login_ok(credential))
            .mount(server)
            .await;
    }

    fn client_for(server: &MockServer) -> NapCatDebugClient {
        NapCatDebugClient::new(mock_server_port(server), "abc".into()).expect("build client")
    }

    #[tokio::test]
    async fn schemas_logs_in_with_hash_then_sends_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_partial_json(json!({ "hash": ABC_HASH })))
            .respond_with(login_ok("cred-1"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/Debug/schemas"))
            .and(header("authorization", "Bearer cred-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "message": "success",
                "data": { "get_login_info": { "description": "取登录号信息" } },
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = client_for(&server);
        let schemas = client.schemas().await.expect("schemas");
        assert_eq!(
            schemas["get_login_info"]["description"],
            json!("取登录号信息")
        );
    }

    #[tokio::test]
    async fn credential_is_cached_across_requests() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(login_ok("cred-1"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/Debug/schemas"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "code": 0, "data": {}, "message": "success" })),
            )
            .mount(&server)
            .await;

        let client = client_for(&server);
        client.schemas().await.expect("first");
        client.schemas().await.expect("second");
    }

    #[tokio::test]
    async fn missing_debug_route_maps_to_too_old() {
        let server = MockServer::start().await;
        mount_login(&server, "cred-1").await;
        Mock::given(method("GET"))
            .and(path("/api/Debug/schemas"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let err = client_for(&server).schemas().await.expect_err("404");
        assert!(matches!(err, NapCatDebugError::TooOld), "got {err:?}");
    }

    #[tokio::test]
    async fn unauthorized_envelope_relogs_in_once_and_retries() {
        let server = MockServer::start().await;
        let logins = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&logins);
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(move |_req: &Request| {
                let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                login_ok(&format!("cred-{n}"))
            })
            .mount(&server)
            .await;
        // 第一份凭据被上游判为过期：HTTP 200 + code -1 Unauthorized
        Mock::given(method("GET"))
            .and(path("/api/Debug/schemas"))
            .and(header("authorization", "Bearer cred-1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "code": -1, "message": "Unauthorized" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/Debug/schemas"))
            .and(header("authorization", "Bearer cred-2"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "code": 0, "data": { "a": {} }, "message": "success" })),
            )
            .expect(1)
            .mount(&server)
            .await;

        let schemas = client_for(&server).schemas().await.expect("retry succeeds");
        assert!(schemas.get("a").is_some());
        assert_eq!(logins.load(Ordering::SeqCst), 2, "exactly two login calls");
    }

    #[tokio::test]
    async fn persistent_unauthorized_gives_up_after_one_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(login_ok("cred"))
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/Debug/schemas"))
            .respond_with(ResponseTemplate::new(403))
            .expect(2)
            .mount(&server)
            .await;

        let err = client_for(&server).schemas().await.expect_err("403 twice");
        assert!(matches!(err, NapCatDebugError::Unauthorized), "got {err:?}");
    }

    #[tokio::test]
    async fn require_2fa_maps_to_two_factor_required() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "message": "success",
                "data": { "require2FA": true },
            })))
            .mount(&server)
            .await;

        let err = client_for(&server).schemas().await.expect_err("2fa");
        assert!(
            matches!(err, NapCatDebugError::TwoFactorRequired),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn rejected_login_maps_to_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "code": -1, "message": "token error" })),
            )
            .mount(&server)
            .await;

        let err = client_for(&server).schemas().await.expect_err("bad token");
        assert!(matches!(err, NapCatDebugError::Unauthorized), "got {err:?}");
    }

    #[tokio::test]
    async fn create_adapter_returns_adapter_token() {
        let server = MockServer::start().await;
        mount_login(&server, "cred-1").await;
        Mock::given(method("POST"))
            .and(path("/api/Debug/create"))
            .and(header("authorization", "Bearer cred-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "message": "success",
                "data": { "adapterName": "debug-primary", "token": "ws-token", "message": "ok" },
            })))
            .mount(&server)
            .await;

        let token = client_for(&server).create_adapter().await.expect("create");
        assert_eq!(token, "ws-token");
    }

    #[tokio::test]
    async fn call_unwraps_envelope_data_as_raw_reply() {
        let server = MockServer::start().await;
        mount_login(&server, "cred-1").await;
        Mock::given(method("POST"))
            .and(path("/api/Debug/call/debug-primary"))
            .and(header("authorization", "Bearer cred-1"))
            .and(body_partial_json(json!({
                "action": "get_login_info",
                "params": { "no_cache": true },
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "message": "success",
                "data": {
                    "status": "ok",
                    "retcode": 0,
                    "data": { "user_id": 10001 },
                    "message": "",
                    "wording": "",
                },
            })))
            .expect(1)
            .mount(&server)
            .await;

        let raw = client_for(&server)
            .call(
                "get_login_info",
                &json!({ "no_cache": true }),
                Duration::from_secs(5),
            )
            .await
            .expect("call");
        assert_eq!(raw.value["retcode"], json!(0));
        assert_eq!(raw.value["data"]["user_id"], json!(10001));
        // text 与 value 是同一份数据的两种形态
        let reparsed: Value = serde_json::from_str(&raw.text).expect("text is json");
        assert_eq!(reparsed, raw.value);
    }

    #[tokio::test]
    async fn call_keeps_ob11_failure_as_ok() {
        let server = MockServer::start().await;
        mount_login(&server, "cred-1").await;
        Mock::given(method("POST"))
            .and(path("/api/Debug/call/debug-primary"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "message": "success",
                "data": { "status": "failed", "retcode": 400, "message": "参数错误" },
            })))
            .mount(&server)
            .await;

        let raw = client_for(&server)
            .call("send_msg", &json!({}), Duration::from_secs(5))
            .await
            .expect("ob11 failure is still a reply");
        assert_eq!(raw.value["retcode"], json!(400));
    }

    #[tokio::test]
    async fn call_unknown_action_maps_to_business_error() {
        let server = MockServer::start().await;
        mount_login(&server, "cred-1").await;
        Mock::given(method("POST"))
            .and(path("/api/Debug/call/debug-primary"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "code": -1, "message": "不支持的 API: foo" })),
            )
            .mount(&server)
            .await;

        let err = client_for(&server)
            .call("foo", &json!({}), Duration::from_secs(5))
            .await
            .expect_err("unknown action");
        match err {
            NapCatDebugError::Business { code, message } => {
                assert_eq!(code, -1);
                assert_eq!(message, "不支持的 API: foo");
            }
            other => panic!("expected Business, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn call_times_out_with_per_call_timeout() {
        let server = MockServer::start().await;
        mount_login(&server, "cred-1").await;
        Mock::given(method("POST"))
            .and(path("/api/Debug/call/debug-primary"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "code": 0, "data": {}, "message": "success" }))
                    .set_delay(Duration::from_millis(600)),
            )
            .mount(&server)
            .await;

        let err = client_for(&server)
            .call("slow", &json!({}), Duration::from_millis(100))
            .await
            .expect_err("must time out");
        assert!(matches!(err, NapCatDebugError::Timeout), "got {err:?}");
    }

    #[tokio::test]
    async fn non_success_status_maps_to_status_error() {
        let server = MockServer::start().await;
        mount_login(&server, "cred-1").await;
        Mock::given(method("GET"))
            .and(path("/api/Debug/schemas"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let err = client_for(&server).schemas().await.expect_err("500");
        assert!(matches!(err, NapCatDebugError::Status(500)), "got {err:?}");
    }

    #[test]
    fn ws_url_percent_encodes_adapter_token() {
        let client = NapCatDebugClient::new(6099, "abc".into()).expect("build client");
        assert_eq!(client.port(), 6099);
        assert_eq!(
            client.ws_url("a b/c+d&e=é"),
            "ws://127.0.0.1:6099/api/Debug/ws?adapterName=debug-primary&token=a%20b%2Fc%2Bd%26e%3D%C3%A9"
        );
        assert_eq!(
            client.ws_url("Az09-._~"),
            "ws://127.0.0.1:6099/api/Debug/ws?adapterName=debug-primary&token=Az09-._~"
        );
    }
}
