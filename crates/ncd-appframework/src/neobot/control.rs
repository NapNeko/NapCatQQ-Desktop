//! NeoBot 面板控制口：请它优雅关闭。
//!
//! 为什么要走 HTTP 而不是发信号：Windows 上 `loop.add_signal_handler` 在 Proactor
//! 下不可用，NeoBot 只接得到 Ctrl+C；而桌面端 spawn 到的是**无控制台**的子进程，
//! `GenerateConsoleCtrlEvent` 也打不到。桌面端唯一能用的「请它退」手段就是它自己的
//! 面板接口（NeoBot 1.2.3 起提供 `POST /api/admin/shutdown`）。
//!
//! 鉴权只有一条路：拿面板密码登录换 token，再带着 token 请求。
//!
//! 两个容易想当然的地方（都对着上游 server.py 的 _auth_middleware 核过）：
//! - 面板**设了密码**时，/api/* 一律要 X-Token（POST 还要 X-CSRF-Token），
//!   **回环来源不能绕过**——「本机就是可信的」在这里不成立；
//! - 面板**没设密码**时，/api/* 一律 403（引导去本机完成 setup），回环也只放行
//!   /api/auth/setup。也就是说没密码并不等于「本机随便调」。
//!
//! 密码来自桌面端密钥库（用户在「Web 控制台」页的「面板凭据」里填过）。没填就只能报未授权，
//! 由前端引导用户去填。

use std::time::Duration;

use ncd_host::{Host, HostPath};
use serde::Deserialize;
use serde_json::json;

/// 需要 NeoBot 至少这个版本才有 `/api/admin/shutdown`。
/// 桌面端据此决定要不要走优雅关闭；低版本只能直接收树。
pub const MIN_VERSION_WITH_SHUTDOWN_ENDPOINT: &str = "1.2.3";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_PATH: &str = "/api/admin/shutdown";
const LOGIN_PATH: &str = "/api/auth/login";

#[derive(Debug, Deserialize)]
struct LoginResponse {
    #[serde(default)]
    token: String,
    #[serde(default)]
    csrf_token: String,
}

/// 请求结果。调用方据此决定「继续等进程退出」还是「直接收树」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownRequest {
    /// 端点接受了请求（200）。**不等于已关闭**——调用方仍须轮询进程存活。
    Accepted,
    /// 端点不存在（404）：实例版本低于 `MIN_VERSION_WITH_SHUTDOWN_ENDPOINT`
    EndpointMissing,
    /// 本机也没拿到授权（面板设了密码而桌面端不知道）
    Unauthorized,
    /// 面板打不通：没在跑、口不对、或端口占用
    Unreachable(String),
    /// 其它非成功响应
    Failed(String),
}

/// 生产环境固定连回环：这是关停整个进程的入口，不该被主机名解析带到别处。
const LOOPBACK_HOST: &str = "127.0.0.1";

/// 面板控制口地址。**只连回环**：这是关停整个进程的入口。
pub fn shutdown_url(dashboard_port: u16) -> String {
    shutdown_url_at(LOOPBACK_HOST, dashboard_port)
}

fn shutdown_url_at(host: &str, dashboard_port: u16) -> String {
    format!("http://{host}:{dashboard_port}{SHUTDOWN_PATH}")
}

fn login_url_at(host: &str, dashboard_port: u16) -> String {
    format!("http://{host}:{dashboard_port}{LOGIN_PATH}")
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .pool_idle_timeout(Duration::from_secs(30))
        // 本机回环不该走代理；用户环境里的 HTTP_PROXY 会把 127.0.0.1 也代理走
        .no_proxy()
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败：{e}"))
}

/// 用面板密码换 token + CSRF。密码不可用（未设 / 记错）时返回 None。
async fn login(
    client: &reqwest::Client,
    host: &str,
    dashboard_port: u16,
    password: &str,
) -> Option<LoginResponse> {
    let resp = client
        .post(login_url_at(host, dashboard_port))
        .json(&json!({ "password": password }))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: LoginResponse = resp.json().await.ok()?;
    (!parsed.token.is_empty()).then_some(parsed)
}

/// 请 NeoBot 优雅关闭。
///
/// `password` 是用户在桌面端填过的面板密码（密钥库里的那个）。面板设了密码就必须带它——
/// 回环来源不能绕过鉴权（见文件头）；没填就只能报未授权，由编排层收树。
/// 返回 `Accepted` 只代表端点接受了；真正退出由调用方轮询确认。
pub async fn request_graceful_shutdown(
    dashboard_port: u16,
    password: Option<&str>,
) -> ShutdownRequest {
    request_graceful_shutdown_at(LOOPBACK_HOST, dashboard_port, password).await
}

/// 与 `request_graceful_shutdown` 同逻辑，但可指定 host——**仅供测试**注入 wiremock。
/// 生产路径一律走上面的回环包装，不要把 host 暴露成可配置项。
async fn request_graceful_shutdown_at(
    host: &str,
    dashboard_port: u16,
    password: Option<&str>,
) -> ShutdownRequest {
    if dashboard_port == 0 {
        return ShutdownRequest::Unreachable("实例没有可用面板端口".to_string());
    }
    let client = match client() {
        Ok(c) => c,
        Err(e) => return ShutdownRequest::Unreachable(e),
    };

    // 先不带凭据试一次，只为省掉「面板没设密码时的一次无谓登录」：
    // 面板设了密码时这一发必然 401/403（回环不放行），下面会走登录重试。
    // 注意别把它当成「本机就能过」——未设密码时 /api/* 也一律 403。
    let first = match client.post(shutdown_url_at(host, dashboard_port)).send().await {
        Ok(r) => r,
        Err(e) => return ShutdownRequest::Unreachable(e.to_string()),
    };
    if first.status().is_success() {
        return ShutdownRequest::Accepted;
    }
    if first.status().as_u16() == 404 {
        // 端点不存在 = 版本太老，再带凭据试也没用
        return ShutdownRequest::EndpointMissing;
    }
    if first.status().as_u16() != 401 && first.status().as_u16() != 403 {
        return ShutdownRequest::Failed(format!("面板返回 {}", first.status()));
    }

    // 401/403 有两种可能：面板设了密码，或远端管理被关。
    // 前者能用密码解决，后者不行——都试一次密码登录，失败就如实报未授权。
    let Some(password) = password.filter(|p| !p.is_empty()) else {
        return ShutdownRequest::Unauthorized;
    };
    let Some(session) = login(&client, host, dashboard_port, password).await else {
        return ShutdownRequest::Unauthorized;
    };
    let second = match client
        .post(shutdown_url_at(host, dashboard_port))
        .header("X-Token", &session.token)
        .header("X-CSRF-Token", &session.csrf_token)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return ShutdownRequest::Unreachable(e.to_string()),
    };
    if second.status().is_success() {
        return ShutdownRequest::Accepted;
    }
    if second.status().as_u16() == 404 {
        return ShutdownRequest::EndpointMissing;
    }
    if second.status().as_u16() == 401 || second.status().as_u16() == 403 {
        return ShutdownRequest::Unauthorized;
    }
    ShutdownRequest::Failed(format!("面板返回 {}", second.status()))
}

/// 面板配置里的端口；读不到就用调用方给的回落口。
pub async fn dashboard_port(
    host: &dyn Host,
    install_dir: &HostPath,
    fallback: u16,
) -> u16 {
    match crate::neobot::config::read_neobot_config(host, install_dir).await {
        Ok((cfg, _)) => {
            let port = cfg.dashboard.port;
            if port > 0 { port } else { fallback }
        }
        Err(_) => fallback,
    }
}

/// 面板会话（登录换来的 token + CSRF）。
///
/// 面板每个请求都要 X-Token + X-CSRF-Token，而登录有代价（要过 PBKDF2-HMAC-SHA256），
/// 所以登录一次就存下来复用；401/403 时丢掉重登一次。
#[derive(Debug, Clone)]
pub struct PanelSession {
    pub token: String,
    pub csrf: String,
}

/// 按实例缓存面板会话。仿 AstrBot 的 DashboardSessions：适配器与 IPC 层共享同一个 Arc，
/// 换一个入口不会各自登录一遍。
#[derive(Default)]
pub struct PanelSessions {
    inner: std::sync::Mutex<std::collections::HashMap<String, PanelSession>>,
}

impl PanelSessions {
    pub fn new() -> Self {
        Self::default()
    }

    fn get(&self, instance_id: &str) -> Option<PanelSession> {
        self.inner.lock().ok()?.get(instance_id).cloned()
    }

    fn store(&self, instance_id: &str, session: PanelSession) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(instance_id.to_string(), session);
        }
    }

    /// 会话失效时清掉，下次调用会重新登录
    pub fn clear(&self, instance_id: &str) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(instance_id);
        }
    }

    /// 带缓存的登录：已有会话直接用；没有或已失效就用密码换一个。
    /// 密码拿不到（用户没填）且没有缓存会话时返回 None。
    async fn ensure(
        &self,
        client: &reqwest::Client,
        instance_id: &str,
        port: u16,
        password: Option<&str>,
    ) -> Option<PanelSession> {
        if let Some(s) = self.get(instance_id) {
            return Some(s);
        }
        let password = password.filter(|p| !p.is_empty())?;
        let session = login(client, LOOPBACK_HOST, port, password).await?;
        let session = PanelSession {
            token: session.token,
            csrf: session.csrf_token,
        };
        self.store(instance_id, session.clone());
        Some(session)
    }
}

/// 面板调用的结果。IPC 只透出这几态，前端按态决定重试还是提示。
#[derive(Debug, Clone)]
pub enum PanelOutcome {
    Ok(serde_json::Value),
    /// 拿不到凭据 / 凭据不对：需要用户填面板密码
    Unauthorized,
    /// 面板打不通
    Unreachable(String),
    /// 其它失败（含状态码）
    Failed(String),
}

/// 允许经桌面端转发的路径前缀。
///
/// 这是一道**白名单**：面板端点里既有读也有写（装插件、改配置、关进程），
/// 转发层不该变成「能打任意 URL 的代理」。只放行面板自己的 /api/，
/// 且客户端固定连回环，路径里不许出现主机名、穿越或空白。
const ALLOWED_PREFIX: &str = "/api/";

pub fn path_is_allowed(path: &str) -> bool {
    path.starts_with(ALLOWED_PREFIX)
        && !path.contains("://")
        && !path.contains("..")
        && !path.contains('\\')
        && !path.contains(char::is_whitespace)
}

/// 代前端调一次面板接口（自动登录；401/403 时清会话重登一次再试）。
///
/// method 只认 GET / POST —— 面板其余方法桌面端用不到，不放行能少一类口子。
#[allow(clippy::too_many_arguments)]
pub async fn panel_call(
    sessions: &PanelSessions,
    instance_id: &str,
    port: u16,
    password: Option<&str>,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> PanelOutcome {
    if port == 0 {
        return PanelOutcome::Unreachable("实例没有可用面板端口".to_string());
    }
    if !path_is_allowed(path) {
        return PanelOutcome::Failed(format!("不允许的面板路径：{path}"));
    }
    let method = method.to_ascii_uppercase();
    if method != "GET" && method != "POST" {
        return PanelOutcome::Failed(format!("不支持的方法：{method}"));
    }
    let client = match client() {
        Ok(c) => c,
        Err(e) => return PanelOutcome::Unreachable(e),
    };
    let url = format!("http://{LOOPBACK_HOST}:{port}{path}");

    for attempt in 0..2 {
        if attempt == 1 {
            // 上一次的会话可能过期了：清掉再登录一次
            sessions.clear(instance_id);
        }
        let Some(session) = sessions.ensure(&client, instance_id, port, password).await else {
            return PanelOutcome::Unauthorized;
        };
        let mut req = if method == "GET" {
            client.get(&url)
        } else {
            client.post(&url)
        };
        req = req
            .header("X-Token", &session.token)
            .header("X-CSRF-Token", &session.csrf);
        if let Some(b) = &body {
            req = req.json(b);
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => return PanelOutcome::Unreachable(e.to_string()),
        };
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            if attempt == 0 {
                continue;
            }
            return PanelOutcome::Unauthorized;
        }
        if !status.is_success() {
            return PanelOutcome::Failed(format!("面板返回 {status}"));
        }
        return match resp.json::<serde_json::Value>().await {
            Ok(v) => PanelOutcome::Ok(v),
            Err(e) => PanelOutcome::Failed(format!("面板回包不是 JSON：{e}")),
        };
    }
    PanelOutcome::Unauthorized
}

#[cfg(test)]
mod panel_call_tests {
    use super::*;

    #[test]
    fn whitelist_only_allows_plain_api_paths() {
        assert!(path_is_allowed("/api/overview"));
        assert!(path_is_allowed("/api/plugins"));
        assert!(path_is_allowed("/api/plugins/demo/toggle"));
        // 不能借转发层打别处：绝对 URL、穿越、反斜杠、空白、非 /api 前缀
        assert!(!path_is_allowed("http://evil.test/api/x"));
        assert!(!path_is_allowed("/api/../../etc/passwd"));
        assert!(!path_is_allowed("/api/x\\..\\y"));
        assert!(!path_is_allowed("/api/a b"));
        assert!(!path_is_allowed("/healthz"));
        assert!(!path_is_allowed(""));
    }

    #[tokio::test]
    async fn panel_call_refuses_disallowed_path_without_network() {
        let sessions = PanelSessions::new();
        let out = panel_call(&sessions, "i1", 1, None, "GET", "/healthz", None).await;
        assert!(matches!(out, PanelOutcome::Failed(_)), "{out:?}");
    }

    #[tokio::test]
    async fn panel_call_reports_unauthorized_without_password() {
        let sessions = PanelSessions::new();
        let out = panel_call(&sessions, "i1", 1, None, "GET", "/api/overview", None).await;
        assert!(matches!(out, PanelOutcome::Unauthorized), "{out:?}");
    }

    #[tokio::test]
    async fn panel_call_refuses_unsupported_method() {
        let sessions = PanelSessions::new();
        let out = panel_call(&sessions, "i1", 1, None, "DELETE", "/api/plugins", None).await;
        assert!(matches!(out, PanelOutcome::Failed(_)), "{out:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    /// MockServer 监听在 127.0.0.1:{port}，把 port 抽出来喂给被测函数
    fn port_of(server: &MockServer) -> u16 {
        server
            .uri()
            .rsplit(':')
            .next()
            .expect("uri 有端口")
            .parse()
            .expect("端口是数字")
    }

    fn accepted() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true }))
    }

    fn login_ok() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(
            serde_json::json!({ "token": "tok-abc", "csrf_token": "csrf-xyz" }),
        )
    }

    #[test]
    fn shutdown_url_is_loopback_only() {
        let url = shutdown_url(9981);
        assert_eq!(url, "http://127.0.0.1:9981/api/admin/shutdown");
        assert!(
            url.contains("127.0.0.1"),
            "关停入口绝不能用可被解析到别处的名字"
        );
    }

    #[test]
    fn min_version_is_the_one_that_shipped_the_endpoint() {
        // 与 NeoBot 侧 admin_shutdown 落地的版本保持一致；改这里要同时确认上游
        assert_eq!(MIN_VERSION_WITH_SHUTDOWN_ENDPOINT, "1.2.3");
    }

    /// 0 口是"实例还没有可用面板口"，不该发出任何请求
    #[tokio::test]
    async fn zero_port_never_sends_a_request() {
        let outcome = request_graceful_shutdown(0, None).await;
        assert!(matches!(outcome, ShutdownRequest::Unreachable(_)), "{outcome:?}");
    }

    #[tokio::test]
    async fn accepted_when_panel_allows_loopback() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(accepted())
            .expect(1)
            .mount(&server)
            .await;

        let outcome = request_graceful_shutdown_at("127.0.0.1", port_of(&server), None).await;
        assert_eq!(outcome, ShutdownRequest::Accepted);
    }

    /// 端点不存在 = 实例版本低于 1.2.3。**404 之后不能再试登录**：那只会多打一次
    /// 没用的请求，还会把"版本太老"误报成"没授权"。
    #[tokio::test]
    async fn endpoint_missing_when_404_and_no_login_attempted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(LOGIN_PATH))
            .respond_with(login_ok())
            .expect(0)
            .mount(&server)
            .await;

        let outcome =
            request_graceful_shutdown_at("127.0.0.1", port_of(&server), Some("hunter2-neobot"))
                .await;
        assert_eq!(outcome, ShutdownRequest::EndpointMissing);
    }

    #[tokio::test]
    async fn unauthorized_when_rejected_and_no_password_known() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let outcome = request_graceful_shutdown_at("127.0.0.1", port_of(&server), None).await;
        assert_eq!(outcome, ShutdownRequest::Unauthorized);
    }

    /// 面板设过密码：第一次被拒 → 登录 → 带 X-Token + X-CSRF-Token 重试成功。
    /// 三个 matcher 的优先级靠"后挂的优先"：先挂 401，再挂登录，最后挂带 header 的 200。
    #[tokio::test]
    async fn logs_in_and_retries_with_token_and_csrf() {
        let server = MockServer::start().await;
        // 第一次 401 **只生效一次**：靠 up_to_n_times 让响应耗尽，
        // 不依赖 matcher 优先级（wiremock 挑「最新挂载」的，多个同路径 mock 容易踩坑）
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(401))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(LOGIN_PATH))
            .and(body_json(serde_json::json!({ "password": "hunter2-neobot" })))
            .respond_with(login_ok())
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(accepted())
            .expect(1)
            .mount(&server)
            .await;

        let outcome =
            request_graceful_shutdown_at("127.0.0.1", port_of(&server), Some("hunter2-neobot"))
                .await;
        assert_eq!(outcome, ShutdownRequest::Accepted);
    }

    /// 密码不对：登录本身被拒，如实报未授权而不是往上冒别的错
    #[tokio::test]
    async fn unauthorized_when_login_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(LOGIN_PATH))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let outcome =
            request_graceful_shutdown_at("127.0.0.1", port_of(&server), Some("wrong-password"))
                .await;
        assert_eq!(outcome, ShutdownRequest::Unauthorized);
    }

    /// 登录成了、但这台实例不让远端管理（重试仍 403）→ 未授权
    #[tokio::test]
    async fn unauthorized_when_retry_still_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(LOGIN_PATH))
            .respond_with(login_ok())
            .mount(&server)
            .await;

        let outcome =
            request_graceful_shutdown_at("127.0.0.1", port_of(&server), Some("hunter2-neobot"))
                .await;
        assert_eq!(outcome, ShutdownRequest::Unauthorized);
    }

    /// 登录成功但响应里没有 token：当作未授权，不能拿空 token 去重试
    #[tokio::test]
    async fn empty_token_is_treated_as_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(LOGIN_PATH))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "token": "", "csrf_token": "" })),
            )
            .mount(&server)
            .await;

        let outcome =
            request_graceful_shutdown_at("127.0.0.1", port_of(&server), Some("hunter2-neobot"))
                .await;
        assert_eq!(outcome, ShutdownRequest::Unauthorized);
    }

    #[tokio::test]
    async fn failed_carries_status_for_other_codes() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let outcome = request_graceful_shutdown_at("127.0.0.1", port_of(&server), None).await;
        match outcome {
            ShutdownRequest::Failed(msg) => assert!(msg.contains("500"), "要带上状态码：{msg}"),
            other => panic!("期望 Failed，得到 {other:?}"),
        }
    }

    /// 面板没起来：连不上要报 Unreachable，不能 panic 也不能静默当成功。
    ///
    /// 端口取「bind 之后立刻释放」的那个：本机回环上刚释放的端口几乎不可能被抢，
    /// 比 drop(MockServer) 可靠——后者不会立刻关监听（实测会拿到 404 而不是连接失败）。
    #[tokio::test]
    async fn unreachable_when_panel_is_down() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            listener.local_addr().expect("addr").port()
        };

        let outcome = request_graceful_shutdown_at("127.0.0.1", port, None).await;
        assert!(
            matches!(outcome, ShutdownRequest::Unreachable(_)),
            "得到 {outcome:?}"
        );
    }

    /// 重试那次必须**真的带上** X-Token 与 X-CSRF-Token：
    /// 只断言"最终 200"不够——不带凭据也可能命中某个宽松的 mock，
    /// 而少一个 header 在真机上会被面板判成未授权。
    #[tokio::test]
    async fn retry_request_carries_token_and_csrf_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(403))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(LOGIN_PATH))
            .respond_with(login_ok())
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(accepted())
            .mount(&server)
            .await;

        let outcome =
            request_graceful_shutdown_at("127.0.0.1", port_of(&server), Some("hunter2-neobot"))
                .await;
        assert_eq!(outcome, ShutdownRequest::Accepted);

        let received = server.received_requests().await.unwrap_or_default();
        let shutdowns: Vec<&Request> = received
            .iter()
            .filter(|r| r.url.path() == SHUTDOWN_PATH)
            .collect();
        assert_eq!(shutdowns.len(), 2, "应该先试一次、再带凭据重试一次");

        let hdr = |req: &Request, name: &str| -> Option<String> {
            req.headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        // 第一次不带凭据（面板未设密码时本机来源直接放行的路径）
        assert!(hdr(shutdowns[0], "X-Token").is_none());
        // 第二次带上登录换来的 token 与 CSRF
        assert_eq!(hdr(shutdowns[1], "X-Token").as_deref(), Some("tok-abc"));
        assert_eq!(hdr(shutdowns[1], "X-CSRF-Token").as_deref(), Some("csrf-xyz"));
    }

    /// 请求体必须用 `password` 字段（NeoBot 侧读的就是这个名字）
    #[tokio::test]
    async fn login_body_uses_password_field() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(SHUTDOWN_PATH))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(LOGIN_PATH))
            .respond_with(login_ok())
            .expect(1)
            .mount(&server)
            .await;

        let _ = request_graceful_shutdown_at(
            "127.0.0.1",
            port_of(&server),
            Some("hunter2-neobot"),
        )
        .await;

        let received: Vec<Request> = server.received_requests().await.unwrap_or_default();
        let login = received
            .iter()
            .find(|r| r.url.path() == LOGIN_PATH)
            .expect("应该发过一次登录");
        let body: serde_json::Value = serde_json::from_slice(&login.body).expect("登录体是 JSON");
        assert_eq!(body["password"], "hunter2-neobot");
    }
}
