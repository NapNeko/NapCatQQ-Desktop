//! AstrBot Dashboard HTTP：只打本机 loopback，JWT 只活在进程里。
//! 代登录两条路：上游新版（`desktop_runtime.py`，v4.28.x 起）的 `/api/v1/auth/desktop-session`
//! 免密码（进程由桌面端带会话密钥 env 拉起才可用，404/401 自动落回密码）；密码登录按核心
//! 版本两套契约——v4.26 起服务端把请求体当明文再哈希比对，更早的核心要求请求体是 md5(明文)，
//! 所以明文先试、被当以凭据拒绝再试 md5。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ncd_traits::AppFrameworkError;
use reqwest::{Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::dashboard_auth::{is_hash_format, md5_hex};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const TOKEN_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
const TOKEN_SKEW: Duration = Duration::from_secs(60);

/// 按实例记住登录拿到的 JWT。DashboardClient 每次调用都现建，没有这一份每个请求都得重新登录；
/// 归 AstrBotAdapter 所有，跟着编排层手里那份注册表活，不做成进程级的全局表
#[derive(Default)]
pub struct DashboardSessions {
    tokens: Mutex<HashMap<String, CachedToken>>,
}

// 手写而不是 derive：里面是登录凭据，不能跟着 Debug 进日志
impl std::fmt::Debug for DashboardSessions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DashboardSessions").finish_non_exhaustive()
    }
}

impl DashboardSessions {
    fn cached(&self, instance_id: &str, username: &str) -> Option<String> {
        let map = self.tokens.lock().ok()?;
        let hit = map.get(instance_id)?;
        if hit.username != username || Instant::now() + TOKEN_SKEW >= hit.exp {
            return None;
        }
        Some(hit.token.clone())
    }

    fn store(&self, instance_id: &str, username: &str, token: &str) {
        if let Ok(mut map) = self.tokens.lock() {
            map.insert(
                instance_id.to_string(),
                CachedToken {
                    token: token.to_string(),
                    username: username.to_string(),
                    exp: Instant::now() + TOKEN_TTL,
                },
            );
        }
    }

    fn clear(&self, instance_id: &str) {
        if let Ok(mut map) = self.tokens.lock() {
            map.remove(instance_id);
        }
    }
}

#[derive(Clone)]
struct CachedToken {
    token: String,
    username: String,
    exp: Instant,
}

fn is_loopback(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    matches!(h, "127.0.0.1" | "localhost" | "::1")
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    status: Option<String>,
    message: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct LoginData {
    token: Option<String>,
    username: Option<String>,
}

/// 上游 `astrbot/dashboard/api/auth.py::DESKTOP_SESSION_HEADER`
const DESKTOP_SESSION_HEADER: &str = "X-AstrBot-Desktop-Session";

#[derive(Debug)]
pub struct DashboardClient {
    instance_id: String,
    base: String,
    port: u16,
    http: Client,
    token: Mutex<Option<String>>,
    sessions: Arc<DashboardSessions>,
}

impl DashboardClient {
    pub fn connect(
        sessions: &Arc<DashboardSessions>,
        instance_id: &str,
        host: &str,
        port: u16,
    ) -> Result<Self, AppFrameworkError> {
        if !is_loopback(host) {
            return Err(AppFrameworkError::DashboardUnreachable(
                "Dashboard 客户端只允许 127.0.0.1 / localhost".into(),
            ));
        }
        if port == 0 {
            return Err(AppFrameworkError::DashboardUnreachable(
                "WebUI 口无效".into(),
            ));
        }
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|e| AppFrameworkError::DashboardUnreachable(e.to_string()))?;
        Ok(Self {
            instance_id: instance_id.to_string(),
            base: format!("http://{}:{port}", authority_host(host)),
            port,
            http,
            token: Mutex::new(None),
            sessions: Arc::clone(sessions),
        })
    }

    fn token(&self) -> Option<String> {
        self.token.lock().ok().and_then(|t| t.clone())
    }

    fn set_token(&self, token: Option<String>) {
        if let Ok(mut slot) = self.token.lock() {
            *slot = token;
        }
    }

    /// 缓存命中就不发包。要确认对面还活着请用 [`Self::verify`]。
    pub async fn login(
        &self,
        username: &str,
        password: &str,
        desktop_secret: Option<&str>,
    ) -> Result<(), AppFrameworkError> {
        if let Some(token) = self.sessions.cached(&self.instance_id, username) {
            self.set_token(Some(token));
            return Ok(());
        }
        self.relogin(username, password, desktop_secret).await
    }

    /// 桌面托管会话优先（免密码）；对面没这接口或密钥对不上再落回密码登录
    async fn relogin(
        &self,
        username: &str,
        password: &str,
        desktop_secret: Option<&str>,
    ) -> Result<(), AppFrameworkError> {
        let mut held = None;
        if let Some(secret) = desktop_secret.filter(|s| !s.is_empty()) {
            match self.desktop_session_login(secret, username).await {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(e) => held = Some(e),
            }
        }
        match self.login_fresh(username, password).await {
            Ok(()) => Ok(()),
            // 没记住密码时，托管会话的传输层错误比「没有密码」更接近真相
            Err(e) if password.is_empty() => Err(held.unwrap_or(e)),
            Err(e) => Err(e),
        }
    }

    /// Ok(false) = 老核心没这个接口（404）或进程不是桌面端这轮带密钥拉起的（401）
    async fn desktop_session_login(
        &self,
        secret: &str,
        username_hint: &str,
    ) -> Result<bool, AppFrameworkError> {
        let url = format!("{}/api/v1/auth/desktop-session", self.base);
        let resp = self
            .http
            .post(&url)
            .header(DESKTOP_SESSION_HEADER, secret)
            .send()
            .await
            .map_err(map_transport)?;
        if matches!(
            resp.status(),
            StatusCode::NOT_FOUND | StatusCode::UNAUTHORIZED
        ) {
            return Ok(false);
        }
        let env: Envelope<LoginData> = parse_envelope(resp).await?;
        let Some(token) = env
            .data
            .as_ref()
            .and_then(|d| d.token.as_deref())
            .filter(|s| !s.is_empty())
        else {
            return Ok(false);
        };
        let username = env
            .data
            .as_ref()
            .and_then(|d| d.username.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or(username_hint);
        self.sessions.store(&self.instance_id, username, token);
        self.set_token(Some(token.to_string()));
        Ok(true)
    }

    async fn login_fresh(&self, username: &str, password: &str) -> Result<(), AppFrameworkError> {
        if username.trim().is_empty() || password.is_empty() {
            return Err(AppFrameworkError::DashboardAuth(
                "没有可用的 WebUI 密码。到连接页写下密码，或打开 WebUI 登录".into(),
            ));
        }
        // 手上只有落盘哈希时两套装约都登不上：新核心要明文，给旧核心发 md5(哈希) 也是错
        if is_hash_format(password) {
            return Err(AppFrameworkError::DashboardAuth(
                "检测到配置中存储的是密码哈希（非明文），无法代为登录。请在浏览器打开 WebUI 手动登录，或在桌面端重置密码后再试。".into(),
            ));
        }
        match self.login_once(username, password).await {
            Ok(()) => Ok(()),
            // v4.26 起服务端把请求体当明文再哈希比对；更早的核心要求请求体就是 md5(明文)
            Err(e) if is_credential_rejection(&e) => {
                self.login_once(username, &md5_hex(password)).await
            }
            Err(e) => Err(e),
        }
    }

    async fn login_once(&self, username: &str, password: &str) -> Result<(), AppFrameworkError> {
        let body = json!({
            "username": username,
            "password": password,
        });
        let env: Envelope<LoginData> = self
            .send_json(Method::POST, "/api/auth/login", Some(&body), false, None)
            .await?;
        reject_totp(&env.message, &env.data)?;
        let token = env
            .data
            .as_ref()
            .and_then(|d| d.token.as_deref())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppFrameworkError::DashboardAuth(
                    env.message.unwrap_or_else(|| "登录未返回 token".into()),
                )
            })?;
        self.sessions.store(&self.instance_id, username, token);
        self.set_token(Some(token.to_string()));
        Ok(())
    }

    /// 真发一个鉴权请求。缓存里的 token 可能对应已经停掉的实例或改过的密码。
    pub async fn verify(
        &self,
        username: &str,
        password: &str,
        desktop_secret: Option<&str>,
    ) -> Result<(), AppFrameworkError> {
        let _: Envelope<Value> = self
            .send_json(
                Method::GET,
                "/api/config/abconfs",
                None::<&Value>,
                true,
                Some((username, password, desktop_secret)),
            )
            .await?;
        Ok(())
    }

    pub async fn get_config(&self) -> Result<Value, AppFrameworkError> {
        let env: Envelope<Value> = self
            .send_json(Method::GET, "/api/config/get", None::<&Value>, true, None)
            .await?;
        unwrap_data(env, "读取配置失败")
    }

    pub async fn update_astrbot_config(
        &self,
        conf_id: &str,
        config: &Value,
    ) -> Result<(), AppFrameworkError> {
        let body = json!({ "conf_id": conf_id, "config": config });
        let env: Envelope<Value> = self
            .send_json(
                Method::POST,
                "/api/config/astrbot/update",
                Some(&body),
                true,
                None,
            )
            .await?;
        expect_ok(env, "保存配置失败")
    }

    pub async fn platform_new(&self, row: &Value) -> Result<(), AppFrameworkError> {
        let env: Envelope<Value> = self
            .send_json(
                Method::POST,
                "/api/config/platform/new",
                Some(row),
                true,
                None,
            )
            .await?;
        expect_ok(env, "新增平台失败")
    }

    pub async fn platform_update(
        &self,
        origin_id: &str,
        row: &Value,
    ) -> Result<(), AppFrameworkError> {
        let body = json!({ "id": origin_id, "config": row });
        let env: Envelope<Value> = self
            .send_json(
                Method::POST,
                "/api/config/platform/update",
                Some(&body),
                true,
                None,
            )
            .await?;
        expect_ok(env, "更新平台失败")
    }

    /// 上游只有 update，且它本身是 upsert（`original_id` 找不到就 append）。没有 `/new`。
    pub async fn provider_source_upsert(
        &self,
        original_id: &str,
        config: &Value,
    ) -> Result<(), AppFrameworkError> {
        let body = json!({ "original_id": original_id, "config": config });
        let env: Envelope<Value> = self
            .send_json(
                Method::POST,
                "/api/config/provider_sources/update",
                Some(&body),
                true,
                None,
            )
            .await?;
        expect_ok(env, "保存提供商失败")
    }

    pub async fn provider_source_delete(&self, id: &str) -> Result<(), AppFrameworkError> {
        let body = json!({ "id": id });
        let env: Envelope<Value> = self
            .send_json(
                Method::POST,
                "/api/config/provider_sources/delete",
                Some(&body),
                true,
                None,
            )
            .await?;
        expect_ok(env, "删除提供商失败")
    }

    pub async fn provider_source_models(
        &self,
        source_id: &str,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let path = format!(
            "/api/config/provider_sources/models?source_id={}",
            urlencoding_lite(source_id)
        );
        let env: Envelope<Value> = self
            .send_json(Method::GET, &path, None::<&Value>, true, None)
            .await?;
        let data = unwrap_data(env, "拉取模型列表失败")?;
        let models = data
            .get("models")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(models
            .iter()
            .filter_map(|v| v.as_str().map(ToOwned::to_owned))
            .collect())
    }

    pub async fn provider_new(&self, row: &Value) -> Result<(), AppFrameworkError> {
        let env: Envelope<Value> = self
            .send_json(
                Method::POST,
                "/api/config/provider/new",
                Some(row),
                true,
                None,
            )
            .await?;
        expect_ok(env, "新增模型失败")
    }

    pub async fn provider_update(
        &self,
        origin_id: &str,
        row: &Value,
    ) -> Result<(), AppFrameworkError> {
        let body = json!({ "id": origin_id, "config": row });
        let env: Envelope<Value> = self
            .send_json(
                Method::POST,
                "/api/config/provider/update",
                Some(&body),
                true,
                None,
            )
            .await?;
        expect_ok(env, "更新模型失败")
    }

    pub async fn provider_delete(&self, id: &str) -> Result<(), AppFrameworkError> {
        let body = json!({ "id": id });
        let env: Envelope<Value> = self
            .send_json(
                Method::POST,
                "/api/config/provider/delete",
                Some(&body),
                true,
                None,
            )
            .await?;
        expect_ok(env, "删除模型失败")
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, AppFrameworkError> {
        let env: Envelope<T> = self
            .send_json(Method::GET, path, None::<&Value>, true, None)
            .await?;
        unwrap_data(env, "请求失败")
    }

    pub async fn post_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<T, AppFrameworkError> {
        let env: Envelope<T> = self
            .send_json(Method::POST, path, Some(body), true, None)
            .await?;
        unwrap_data(env, "请求失败")
    }

    pub async fn post_ok(&self, path: &str, body: &Value) -> Result<(), AppFrameworkError> {
        let env: Envelope<Value> = self
            .send_json(Method::POST, path, Some(body), true, None)
            .await?;
        expect_ok(env, "请求失败")
    }

    pub async fn post_multipart_ok(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<(), AppFrameworkError> {
        let url = format!("{}{path}", self.base);
        let mut req = self.http.post(&url).timeout(UPLOAD_TIMEOUT).multipart(form);
        if let Some(token) = self.token() {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await.map_err(map_transport)?;
        parse_envelope::<Value>(resp)
            .await
            .and_then(|env| expect_ok(env, "上传失败"))
    }

    async fn send_json<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
        authed: bool,
        retry_login: Option<(&str, &str, Option<&str>)>,
    ) -> Result<Envelope<T>, AppFrameworkError> {
        let url = format!("{}{path}", self.base);
        let mut req = self.http.request(method.clone(), &url);
        if authed {
            let token = self
                .token()
                .ok_or_else(|| AppFrameworkError::DashboardAuth("尚未登录 Dashboard".into()))?;
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req.send().await.map_err(map_transport)?;
        if resp.status() == StatusCode::UNAUTHORIZED && authed {
            self.sessions.clear(&self.instance_id);
            self.set_token(None);
            if let Some((user, pass, secret)) = retry_login {
                // 重登写回自己的 token，否则同一个 client 的后续请求会每次都 401 再重登
                Box::pin(self.relogin(user, pass, secret)).await?;
                return Box::pin(self.send_json(method, path, body, true, None)).await;
            }
            return Err(AppFrameworkError::DashboardAuth(
                "WebUI 登录已过期，请重试".into(),
            ));
        }
        parse_envelope(resp).await
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

/// IPv6 字面量进 URL 必须带方括号。
fn authority_host(host: &str) -> String {
    let bare = host.trim();
    if bare.starts_with('[') || !bare.contains(':') {
        bare.to_string()
    } else {
        format!("[{bare}]")
    }
}

/// 密码不对才值得换 md5 再试；TOTP / 没发 token 这类重试也是白搭
fn is_credential_rejection(e: &AppFrameworkError) -> bool {
    match e {
        AppFrameworkError::DashboardAuth(m) => {
            m.contains("用户名或密码") || m.contains("Incorrect username")
        }
        _ => false,
    }
}

fn reject_totp(
    message: &Option<String>,
    data: &Option<LoginData>,
) -> Result<(), AppFrameworkError> {
    let msg = message.as_deref().unwrap_or("");
    let looks_2fa = msg.to_ascii_lowercase().contains("totp")
        || msg.contains("验证码")
        || msg.contains("双因素")
        || data.as_ref().and_then(|d| d.token.as_deref()).is_none() && msg.contains("二次");
    if looks_2fa {
        return Err(AppFrameworkError::DashboardAuth(
            "WebUI 开了双因素认证，Desktop 不能代登录。到 AstrBot WebUI 操作，或先关掉 TOTP".into(),
        ));
    }
    Ok(())
}

fn unwrap_data<T>(env: Envelope<T>, fallback: &str) -> Result<T, AppFrameworkError> {
    if env.status.as_deref() == Some("error") {
        return Err(map_status_error(env.message.as_deref(), fallback));
    }
    env.data
        .ok_or_else(|| AppFrameworkError::Integration(fallback.into()))
}

fn expect_ok<T>(env: Envelope<T>, fallback: &str) -> Result<(), AppFrameworkError> {
    if env.status.as_deref() == Some("error") {
        return Err(map_status_error(env.message.as_deref(), fallback));
    }
    Ok(())
}

fn map_status_error(message: Option<&str>, fallback: &str) -> AppFrameworkError {
    let msg = message.unwrap_or(fallback);
    if msg.contains("TOTP") || msg.contains("验证码") || msg.contains("双因素") {
        return AppFrameworkError::DashboardAuth(msg.into());
    }
    AppFrameworkError::Integration(msg.into())
}

fn map_transport(e: reqwest::Error) -> AppFrameworkError {
    AppFrameworkError::DashboardUnreachable(e.without_url().to_string())
}

async fn parse_envelope<T: DeserializeOwned>(
    resp: reqwest::Response,
) -> Result<Envelope<T>, AppFrameworkError> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppFrameworkError::DashboardUnreachable(e.without_url().to_string()))?;
    if status == StatusCode::UNAUTHORIZED {
        return Err(AppFrameworkError::DashboardAuth("用户名或密码错误".into()));
    }
    if !status.is_success() {
        return Err(AppFrameworkError::DashboardUnreachable(format!(
            "WebUI 返回 HTTP {}",
            status.as_u16()
        )));
    }
    serde_json::from_str(&text)
        .map_err(|e| AppFrameworkError::Integration(format!("Dashboard 响应无法解析: {e}")))
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sessions() -> Arc<DashboardSessions> {
        Arc::new(DashboardSessions::default())
    }

    #[test]
    fn connect_rejects_non_loopback() {
        let err = DashboardClient::connect(&sessions(), "a1", "10.0.0.1", 6185).unwrap_err();
        assert!(matches!(err, AppFrameworkError::DashboardUnreachable(_)));
    }

    #[test]
    fn connect_rejects_zero_port() {
        let err = DashboardClient::connect(&sessions(), "a1", "127.0.0.1", 0).unwrap_err();
        assert!(matches!(err, AppFrameworkError::DashboardUnreachable(_)));
    }

    #[test]
    fn ipv6_literal_gets_brackets_in_base_url() {
        let client = DashboardClient::connect(&sessions(), "a1", "::1", 6185).unwrap();
        assert_eq!(client.base, "http://[::1]:6185");
        let bracketed = DashboardClient::connect(&sessions(), "a1", "[::1]", 6185).unwrap();
        assert_eq!(bracketed.base, "http://[::1]:6185");
    }

    #[test]
    fn sessions_debug_hides_tokens() {
        let s = sessions();
        s.store("a1", "astrbot", "jwt-secret");
        assert!(!format!("{s:?}").contains("jwt-secret"));
    }

    // 两个适配器实例（比如测试里各建一套）各记各的，不会串到对方的登录态
    #[test]
    fn sessions_are_per_owner() {
        let a = sessions();
        let b = sessions();
        a.store("i1", "astrbot", "jwt-a");
        assert_eq!(a.cached("i1", "astrbot").as_deref(), Some("jwt-a"));
        assert_eq!(b.cached("i1", "astrbot"), None);
        assert_eq!(a.cached("i1", "other-user"), None);
        a.clear("i1");
        assert_eq!(a.cached("i1", "astrbot"), None);
    }

    #[tokio::test]
    async fn login_stores_token_and_retries_once_on_401() {
        let server = MockServer::start().await;
        let uri = server.uri();
        let port: u16 = uri.rsplit(':').next().unwrap().parse().unwrap();
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "token": "jwt-1", "username": "astrbot" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/config/get"))
            .and(header("authorization", "Bearer jwt-1"))
            .respond_with(ResponseTemplate::new(401))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/config/get"))
            .and(header("authorization", "Bearer jwt-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "config": { "ok": true } }
            })))
            .mount(&server)
            .await;

        let client = DashboardClient::connect(&sessions(), "retry-1", "127.0.0.1", port).unwrap();
        client.login("astrbot", "Abcdefg1", None).await.unwrap();
        let env: Envelope<Value> = client
            .send_json(
                Method::GET,
                "/api/config/get",
                None::<&Value>,
                true,
                Some(("astrbot", "Abcdefg1", None)),
            )
            .await
            .unwrap();
        assert_eq!(env.status.as_deref(), Some("ok"));
        // 重登后 token 必须写回自身，否则后面每个请求都会再 401
        assert_eq!(client.token().as_deref(), Some("jwt-1"));
    }

    #[tokio::test]
    async fn verify_hits_the_network_even_with_a_cached_token() {
        let server = MockServer::start().await;
        let port: u16 = server.uri().rsplit(':').next().unwrap().parse().unwrap();
        Mock::given(method("GET"))
            .and(path("/api/config/abconfs"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let cache = sessions();
        cache.store("stale-1", "astrbot", "jwt-stale");
        let client = DashboardClient::connect(&cache, "stale-1", "127.0.0.1", port).unwrap();
        client.login("astrbot", "Abcdefg1", None).await.unwrap();
        let err = client
            .verify("astrbot", "Abcdefg1", None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppFrameworkError::DashboardAuth(_)));
        // 401 之后缓存里那份过期的 token 也要作废，下次才会真去登录
        assert_eq!(cache.cached("stale-1", "astrbot"), None);
    }

    #[tokio::test]
    async fn envelope_error_does_not_fallback() {
        let server = MockServer::start().await;
        let port: u16 = server.uri().rsplit(':').next().unwrap().parse().unwrap();
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "token": "jwt-2" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/config/astrbot/update"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "error",
                "message": "校验失败"
            })))
            .mount(&server)
            .await;
        let client = DashboardClient::connect(&sessions(), "err-1", "127.0.0.1", port).unwrap();
        client.login("astrbot", "Abcdefg1", None).await.unwrap();
        let err = client
            .update_astrbot_config("default", &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, AppFrameworkError::Integration(m) if m.contains("校验失败")));
    }

    #[tokio::test]
    async fn login_without_password_is_auth() {
        let err = DashboardClient::connect(&sessions(), "x", "127.0.0.1", 6185)
            .unwrap()
            .login("astrbot", "", None)
            .await
            .unwrap_err();
        assert!(matches!(err, AppFrameworkError::DashboardAuth(_)));
    }

    #[tokio::test]
    async fn desktop_session_logs_in_without_password() {
        let server = MockServer::start().await;
        let port: u16 = server.uri().rsplit(':').next().unwrap().parse().unwrap();
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/desktop-session"))
            .and(header("x-astrbot-desktop-session", "s3cret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "token": "jwt-desktop", "username": "astrbot" }
            })))
            .mount(&server)
            .await;

        let client = DashboardClient::connect(&sessions(), "ds-1", "127.0.0.1", port).unwrap();
        client.login("astrbot", "", Some("s3cret")).await.unwrap();
        assert_eq!(client.token().as_deref(), Some("jwt-desktop"));
    }

    #[tokio::test]
    async fn desktop_session_404_falls_back_to_password_login() {
        let server = MockServer::start().await;
        let port: u16 = server.uri().rsplit(':').next().unwrap().parse().unwrap();
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/desktop-session"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        // 新核心契约：请求体就是明文
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_json(
                json!({ "username": "astrbot", "password": "Abcdefg1" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "token": "jwt-plain" }
            })))
            .mount(&server)
            .await;

        let client = DashboardClient::connect(&sessions(), "ds-2", "127.0.0.1", port).unwrap();
        client
            .login("astrbot", "Abcdefg1", Some("s3cret"))
            .await
            .unwrap();
        assert_eq!(client.token().as_deref(), Some("jwt-plain"));
    }

    #[tokio::test]
    async fn old_core_that_rejects_plaintext_gets_md5() {
        let server = MockServer::start().await;
        let port: u16 = server.uri().rsplit(':').next().unwrap().parse().unwrap();
        // 旧核心（≤ v4.25）拿请求体直接和落盘的 md5 比：明文先来必然被拒，再试 md5
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_json(
                json!({ "username": "astrbot", "password": "Abcdefg1" }),
            ))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_json(json!({
                "username": "astrbot",
                "password": md5_hex("Abcdefg1")
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "token": "jwt-md5" }
            })))
            .mount(&server)
            .await;

        let client = DashboardClient::connect(&sessions(), "md5-1", "127.0.0.1", port).unwrap();
        client.login("astrbot", "Abcdefg1", None).await.unwrap();
        assert_eq!(client.token().as_deref(), Some("jwt-md5"));
    }
}
