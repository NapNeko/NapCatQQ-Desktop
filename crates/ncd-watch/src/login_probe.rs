//! NapCat WebUI 账号在线探活(远端本机 127.0.0.1)
//!
//! 对齐 Desktop login_poller / online_from_login_status:
//! - auth → CheckLoginStatus(isLogin/isOffline) → GetQQLoginInfo(online)
//! - 优先 GetQQLoginInfo.online
//! - online 缺失时: isLogin=true → 在线; isOffline=true → 离线
//! - isLogin=false 且无 online/isOffline 时: 账号未登录 → LoggedOut(冷启动不告警)
//! - 请求失败 → Unknown(不因瞬时 HTTP 误报)
//!
//! 进程 pgrep 不能代表 QQ 已登录;有 webui 凭据时以本模块结果作为掉线主信号。
//!
//! 凭据跨轮复用:上游 /api/auth/login 按来源 IP 限流(Docker 默认 loginRate: 3 / 60s),
//! 每 15s 登录一次会把同机 Desktop 隧道和用户浏览器的额度一起吃光。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::NotifyBotTarget;
use crate::probe::LoginStatus;

const TIMEOUT_SECS: u64 = 5;
/// 上游凭据最长 1 小时有效,留 10 分钟余量
const CREDENTIAL_REUSE: Duration = Duration::from_secs(50 * 60);
/// 明确被拒 / 限流后至少等过上游 60s 限流窗口
const REJECT_BACKOFF: Duration = Duration::from_secs(65);

/// 按 bot 记住 WebUI 凭据。key 用 (port, 登录 hash),不存明文 token;
/// token 或端口变了就当新 bot 处理
#[derive(Debug, Default)]
pub struct WebUiLoginCache {
    entries: HashMap<String, CacheEntry>,
}

#[derive(Debug)]
struct CacheEntry {
    port: u16,
    login_hash: String,
    credential: Option<(String, Instant)>,
    retry_after: Option<Instant>,
}

impl WebUiLoginCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn entry(&mut self, bot_id: &str, port: u16, login_hash: String) -> &mut CacheEntry {
        let entry = self
            .entries
            .entry(bot_id.to_string())
            .or_insert_with(|| CacheEntry {
                port,
                login_hash: login_hash.clone(),
                credential: None,
                retry_after: None,
            });
        if entry.port != port || entry.login_hash != login_hash {
            *entry = CacheEntry {
                port,
                login_hash,
                credential: None,
                retry_after: None,
            };
        }
        entry
    }
}

/// 上游信封:失败也是 HTTP 200 + {code:-1,message},没有 data
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
    #[serde(default = "Option::default")]
    data: Option<T>,
}

#[derive(Debug, Default, Deserialize)]
struct LoginData {
    #[serde(rename = "Credential", default)]
    credential: Option<String>,
    #[serde(rename = "require2FA", default)]
    require_2fa: bool,
}

#[derive(Debug, Default, Deserialize)]
struct CheckLoginStatusData {
    // 旧实现也认 snake_case，保留兼容
    #[serde(rename = "isLogin", alias = "is_login", default)]
    is_login: Option<bool>,
    #[serde(rename = "isOffline", alias = "is_offline", default)]
    is_offline: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct QqLoginInfoData {
    #[serde(default)]
    online: Option<bool>,
}

enum LoginError {
    /// token 错 / 限流 / 2FA:短时间内再登也不会变,进退避
    Rejected(String),
    Other(String),
}

enum ProbeError {
    /// 凭据失效(401/403 或信封 Unauthorized),丢掉下轮重登
    Unauthorized(String),
    Other(String),
}

fn login_hash(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    h.update(b".napcat");
    hex::encode(h.finalize())
}

fn is_unauthorized_message(message: &str) -> bool {
    let m = message.trim();
    m.eq_ignore_ascii_case("Unauthorized") || m.eq_ignore_ascii_case("Server token not initialized")
}

/// 对单个 bot 查账号在线态;无 port/token 或非 napcat 返回 Unknown
pub async fn probe_login_status(
    bot: &NotifyBotTarget,
    cache: &mut WebUiLoginCache,
) -> (LoginStatus, String) {
    if !bot.backend.eq_ignore_ascii_case("napcat") {
        return (LoginStatus::Unknown, "login probe: non-napcat".into());
    }
    let Some(port) = bot.webui_port.filter(|p| *p > 0) else {
        return (LoginStatus::Unknown, "login probe: no webui_port".into());
    };
    let token = bot.webui_token.as_deref().filter(|s| !s.trim().is_empty());
    let Some(token) = token else {
        return (LoginStatus::Unknown, "login probe: no webui_token".into());
    };

    let hash = login_hash(token);
    let entry = cache.entry(&bot.bot_id, port, hash.clone());
    let now = Instant::now();
    if let Some(until) = entry.retry_after {
        if until > now {
            let left = until.saturating_duration_since(now).as_secs();
            return (
                LoginStatus::Unknown,
                format!("webui:{port} login backoff ({left}s left)"),
            );
        }
        entry.retry_after = None;
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .no_proxy()
        .build()
    {
        Ok(c) => c,
        Err(e) => return (LoginStatus::Unknown, format!("webui:{port} {e}")),
    };

    let cached = entry
        .credential
        .as_ref()
        .filter(|(_, at)| now.saturating_duration_since(*at) < CREDENTIAL_REUSE)
        .map(|(c, _)| c.clone());
    let credential = match cached {
        Some(c) => c,
        None => match fetch_credential(&client, port, &hash).await {
            Ok(c) => {
                entry.credential = Some((c.clone(), Instant::now()));
                c
            }
            Err(LoginError::Rejected(msg)) => {
                entry.credential = None;
                entry.retry_after = Some(Instant::now() + REJECT_BACKOFF);
                return (
                    LoginStatus::Unknown,
                    format!("webui:{port} {msg}; retry in {}s", REJECT_BACKOFF.as_secs()),
                );
            }
            Err(LoginError::Other(e)) => {
                return (LoginStatus::Unknown, format!("webui:{port} {e}"));
            }
        },
    };

    match probe_account_online(&client, port, &credential).await {
        Ok((st, detail)) => (st, detail),
        Err(ProbeError::Unauthorized(detail)) => {
            entry.credential = None;
            (
                LoginStatus::Unknown,
                format!("webui:{port} credential dropped: {detail}"),
            )
        }
        Err(ProbeError::Other(detail)) => (LoginStatus::Unknown, detail),
    }
}

/// 是否具备 WebUI 登录探活条件(有则掉线主信号走账号态,不用进程冒充在线)
pub fn has_webui_probe(bot: &NotifyBotTarget) -> bool {
    bot.backend.eq_ignore_ascii_case("napcat")
        && bot.webui_port.filter(|p| *p > 0).is_some()
        && bot
            .webui_token
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
}

/// 单个状态接口的结果:Ok(data) / Err(说明, 是否鉴权失效)
async fn read_status<T: serde::de::DeserializeOwned + Default>(
    resp: Result<reqwest::Response, reqwest::Error>,
    what: &str,
) -> Result<T, (String, bool)> {
    let resp = resp.map_err(|e| (format!("{what}: {e}"), false))?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err((format!("{what} status {status}"), true));
    }
    if !status.is_success() {
        return Err((format!("{what} status {status}"), false));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| (format!("{what}: {e}"), false))?;
    let env: Envelope<T> = serde_json::from_slice(&bytes)
        .map_err(|e| (format!("{what} decode ({} bytes): {e}", bytes.len()), false))?;
    if env.code != 0 {
        let unauthorized = is_unauthorized_message(&env.message);
        return Err((format!("{what}: {}", env.message.trim()), unauthorized));
    }
    Ok(env.data.unwrap_or_default())
}

async fn probe_account_online(
    client: &reqwest::Client,
    port: u16,
    credential: &str,
) -> Result<(LoginStatus, String), ProbeError> {
    // 与 Desktop do_status_poll 一致:并发 CheckLoginStatus + GetQQLoginInfo
    let login_url = format!("http://127.0.0.1:{port}/api/QQLogin/CheckLoginStatus");
    let info_url = format!("http://127.0.0.1:{port}/api/QQLogin/GetQQLoginInfo");
    let auth = format!("Bearer {credential}");

    let login_fut = client
        .post(&login_url)
        .header(reqwest::header::AUTHORIZATION, &auth)
        .send();
    let info_fut = client
        .post(&info_url)
        .header(reqwest::header::AUTHORIZATION, &auth)
        .send();
    let (login_resp, info_resp) = tokio::join!(login_fut, info_fut);

    let (login_res, info_res) = tokio::join!(
        read_status::<CheckLoginStatusData>(login_resp, "check login"),
        read_status::<QqLoginInfoData>(info_resp, "GetQQLoginInfo"),
    );

    let mut is_login: Option<bool> = None;
    let mut is_offline: Option<bool> = None;
    let mut login_err = String::new();
    let mut unauthorized = false;
    match login_res {
        Ok(data) => {
            is_login = data.is_login;
            is_offline = data.is_offline;
        }
        Err((e, unauth)) => {
            unauthorized |= unauth;
            login_err = e;
        }
    }

    let mut online: Option<bool> = None;
    let mut info_err = String::new();
    match info_res {
        Ok(data) => online = data.online,
        Err((e, unauth)) => {
            unauthorized |= unauth;
            info_err = e;
        }
    }

    // 对齐 Desktop online_from_login_status + online 字段合并:
    // GetQQLoginInfo.online 优先;缺失时用 isLogin/isOffline 推断
    if online.is_none() {
        if is_login == Some(true) {
            online = Some(true);
        } else if is_offline == Some(true) {
            online = Some(false);
        } else if is_login == Some(false) {
            // Desktop 此处返回 None;watch 侧无状态机,未登录即视为账号不在线
            online = Some(false);
        }
    }

    let detail = format!(
        "webui:{port} online={online:?} isLogin={is_login:?} isOffline={is_offline:?}{extra}",
        extra = {
            let mut s = String::new();
            if !login_err.is_empty() {
                s.push_str("; ");
                s.push_str(&login_err);
            }
            if !info_err.is_empty() {
                s.push_str("; ");
                s.push_str(&info_err);
            }
            s
        }
    );

    if unauthorized && online.is_none() {
        return Err(ProbeError::Unauthorized(detail));
    }
    match online {
        Some(true) => Ok((LoginStatus::LoggedIn, detail)),
        Some(false) => Ok((LoginStatus::LoggedOut, detail)),
        None => {
            if !login_err.is_empty() || !info_err.is_empty() {
                Err(ProbeError::Other(detail))
            } else {
                Ok((LoginStatus::Unknown, detail))
            }
        }
    }
}

/// 失败形态见上游 napcat-webui-backend api/Auth.ts LoginHandler
async fn fetch_credential(
    client: &reqwest::Client,
    port: u16,
    hash: &str,
) -> Result<String, LoginError> {
    let login_url = format!("http://127.0.0.1:{port}/api/auth/login");
    let resp = client
        .post(&login_url)
        .json(&serde_json::json!({ "hash": hash }))
        .send()
        .await
        .map_err(|e| LoginError::Other(format!("auth login: {e}")))?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(LoginError::Rejected(format!("auth login status {status}")));
    }
    if !status.is_success() {
        return Err(LoginError::Other(format!("auth login status {status}")));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| LoginError::Other(format!("auth login: {e}")))?;
    let env: Envelope<LoginData> = serde_json::from_slice(&bytes)
        .map_err(|e| LoginError::Other(format!("auth decode ({} bytes): {e}", bytes.len())))?;
    if env.code != 0 {
        let message = env.message.trim().to_string();
        let lower = message.to_ascii_lowercase();
        let rejected = matches!(
            lower.as_str(),
            "token is invalid"
                | "token is empty"
                | "login rate limit"
                | "server token not initialized"
                | "unauthorized"
                | "invalid or expired code"
        );
        let text = format!("auth login rejected: {message}");
        return Err(if rejected {
            LoginError::Rejected(text)
        } else {
            LoginError::Other(text)
        });
    }
    let data = env.data.unwrap_or_default();
    if let Some(c) = data.credential.filter(|s| !s.is_empty()) {
        return Ok(c);
    }
    if data.require_2fa {
        return Err(LoginError::Rejected(
            "auth login requires two-factor code".into(),
        ));
    }
    Err(LoginError::Other("auth missing Credential".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn napcat_bot(port: u16, token: &str) -> NotifyBotTarget {
        NotifyBotTarget {
            bot_id: "1".into(),
            qq_id: 1,
            bot_name: String::new(),
            backend: "napcat".into(),
            deployment: "docker".into(),
            container_name: None,
            pid_file: None,
            process_match: None,
            webui_port: Some(port),
            webui_token: Some(token.into()),
            enabled: true,
            webui_port_guessed: false,
        }
    }

    #[test]
    fn non_napcat_unknown() {
        let bot = NotifyBotTarget {
            bot_id: "1".into(),
            qq_id: 1,
            bot_name: String::new(),
            backend: "snowluma".into(),
            deployment: "native".into(),
            container_name: None,
            pid_file: None,
            process_match: None,
            webui_port: Some(6099),
            webui_token: Some("t".into()),
            enabled: true,
            webui_port_guessed: false,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut cache = WebUiLoginCache::new();
        let (st, _) = rt.block_on(probe_login_status(&bot, &mut cache));
        assert_eq!(st, LoginStatus::Unknown);
    }

    #[test]
    fn has_webui_probe_requires_napcat_port_token() {
        let mut bot = NotifyBotTarget {
            bot_id: "1".into(),
            qq_id: 1,
            bot_name: String::new(),
            backend: "napcat".into(),
            deployment: "native".into(),
            container_name: None,
            pid_file: None,
            process_match: None,
            webui_port: Some(6099),
            webui_token: Some("tok".into()),
            enabled: true,
            webui_port_guessed: false,
        };
        assert!(has_webui_probe(&bot));
        bot.webui_token = Some("  ".into());
        assert!(!has_webui_probe(&bot));
    }

    async fn mount_login(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    async fn mount_status_ok(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/QQLogin/CheckLoginStatus"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "message": "success",
                "data": { "isLogin": true, "isOffline": false, "qrcodeurl": "" }
            })))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/QQLogin/GetQQLoginInfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0, "message": "success", "data": { "online": true }
            })))
            .mount(server)
            .await;
    }

    async fn login_calls(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|r| r.url.path() == "/api/auth/login")
            .count()
    }

    fn ok_login() -> serde_json::Value {
        serde_json::json!({ "code": 0, "message": "success", "data": { "Credential": "cred" } })
    }

    #[tokio::test]
    async fn second_round_reuses_credential() {
        let server = MockServer::start().await;
        mount_login(&server, ok_login()).await;
        mount_status_ok(&server).await;
        let bot = napcat_bot(server.address().port(), "tok");
        let mut cache = WebUiLoginCache::new();

        let (first, _) = probe_login_status(&bot, &mut cache).await;
        let (second, _) = probe_login_status(&bot, &mut cache).await;

        assert_eq!(first, LoginStatus::LoggedIn);
        assert_eq!(second, LoginStatus::LoggedIn);
        assert_eq!(login_calls(&server).await, 1);
    }

    #[tokio::test]
    async fn login_preserves_whitespace_in_token_hash() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_json(
                serde_json::json!({ "hash": login_hash(" tok ") }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(ok_login()))
            .mount(&server)
            .await;
        mount_status_ok(&server).await;
        let mut cache = WebUiLoginCache::new();
        let (status, _) =
            probe_login_status(&napcat_bot(server.address().port(), " tok "), &mut cache).await;
        assert_eq!(status, LoginStatus::LoggedIn);
    }

    #[tokio::test]
    async fn unauthorized_envelope_drops_credential() {
        let server = MockServer::start().await;
        mount_login(&server, ok_login()).await;
        for api in [
            "/api/QQLogin/CheckLoginStatus",
            "/api/QQLogin/GetQQLoginInfo",
        ] {
            Mock::given(method("POST"))
                .and(path(api))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "code": -1, "message": "Unauthorized"
                })))
                .mount(&server)
                .await;
        }
        let bot = napcat_bot(server.address().port(), "tok");
        let mut cache = WebUiLoginCache::new();

        let (st, detail) = probe_login_status(&bot, &mut cache).await;
        assert_eq!(st, LoginStatus::Unknown);
        assert!(detail.contains("credential dropped"), "{detail}");
        let _ = probe_login_status(&bot, &mut cache).await;
        assert_eq!(login_calls(&server).await, 2);
    }

    #[tokio::test]
    async fn rejected_login_backs_off() {
        let server = MockServer::start().await;
        mount_login(
            &server,
            serde_json::json!({ "code": -1, "message": "token is invalid" }),
        )
        .await;
        let bot = napcat_bot(server.address().port(), "wrong");
        let mut cache = WebUiLoginCache::new();

        let (st, detail) = probe_login_status(&bot, &mut cache).await;
        assert_eq!(st, LoginStatus::Unknown);
        assert!(detail.contains("token is invalid"), "{detail}");
        let (_, detail) = probe_login_status(&bot, &mut cache).await;
        assert!(detail.contains("backoff"), "{detail}");
        assert_eq!(login_calls(&server).await, 1);
    }

    #[tokio::test]
    async fn rate_limited_login_backs_off() {
        let server = MockServer::start().await;
        mount_login(
            &server,
            serde_json::json!({ "code": -1, "message": "login rate limit" }),
        )
        .await;
        let bot = napcat_bot(server.address().port(), "tok");
        let mut cache = WebUiLoginCache::new();

        let _ = probe_login_status(&bot, &mut cache).await;
        let _ = probe_login_status(&bot, &mut cache).await;
        assert_eq!(login_calls(&server).await, 1);
    }

    #[tokio::test]
    async fn token_change_resets_cache() {
        let server = MockServer::start().await;
        mount_login(
            &server,
            serde_json::json!({ "code": -1, "message": "token is invalid" }),
        )
        .await;
        let mut bot = napcat_bot(server.address().port(), "old");
        let mut cache = WebUiLoginCache::new();

        let _ = probe_login_status(&bot, &mut cache).await;
        // Desktop 写来新 token:退避与旧凭据都不能沿用
        bot.webui_token = Some("new".into());
        let _ = probe_login_status(&bot, &mut cache).await;
        assert_eq!(login_calls(&server).await, 2);
    }
}
