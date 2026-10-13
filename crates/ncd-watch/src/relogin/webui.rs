// 本机 WebUI 登录会话；错误与限流不等同于 QQ 掉线。
use ncd_onebot::client::read_body_limited;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub struct WebUi {
    http: reqwest::Client,
    sessions: HashMap<String, Session>,
}
struct Session {
    token: Option<String>,
    at: Instant,
    retry: Option<Instant>,
}

impl WebUi {
    pub fn new() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| "无法建立 WebUI 客户端")?;
        Ok(Self {
            http,
            sessions: HashMap::new(),
        })
    }
    pub fn prune(&mut self) {
        self.sessions
            .retain(|_, s| s.at.elapsed() < Duration::from_secs(3600));
    }

    pub async fn request(
        &mut self,
        _key: &str,
        port: u16,
        secret: &str,
        snowluma: bool,
        path: &str,
    ) -> Result<Value, String> {
        if port == 0 || secret.is_empty() {
            return Err("缺少 WebUI 凭据".into());
        }
        let fingerprint = hex::encode(Sha256::digest(format!("{port}:{snowluma}:{secret}")));
        if !self.sessions.contains_key(&fingerprint) {
            self.sessions.insert(
                fingerprint.clone(),
                Session {
                    token: None,
                    at: Instant::now(),
                    retry: None,
                },
            );
        }
        let session = self
            .sessions
            .get_mut(&fingerprint)
            .ok_or("登录会话不存在")?;
        let now = Instant::now();
        if session.retry.is_some_and(|until| until > now) {
            return Err("WebUI 鉴权冷却中".into());
        }
        if session.at.elapsed() > Duration::from_secs(3000) {
            session.token = None;
        }
        if session.token.is_none() {
            let (endpoint, body) = if snowluma {
                ("/api/login", json!({"password":secret}))
            } else {
                (
                    "/api/auth/login",
                    json!({"hash":hex::encode(Sha256::digest(format!("{secret}.napcat")))}),
                )
            };
            // 在请求前开始退避，取消或超时也不会下一轮继续撞登录限流。
            session.retry = Some(now + Duration::from_secs(if snowluma { 900 } else { 65 }));
            let response = self
                .http
                .post(format!("http://127.0.0.1:{port}{endpoint}"))
                .json(&body)
                .send()
                .await
                .map_err(|_| "WebUI 登录请求失败")?;
            if !response.status().is_success() {
                return Err("WebUI 登录被拒绝".into());
            }
            let body = read_json(response).await?;
            let token = if snowluma {
                body.get("token").and_then(Value::as_str)
            } else if body.get("code").and_then(Value::as_i64).unwrap_or(0) == 0 {
                body.get("data")
                    .and_then(|d| d.get("Credential"))
                    .and_then(Value::as_str)
            } else {
                None
            };
            session.token = Some(
                token
                    .filter(|t| !t.is_empty())
                    .ok_or("WebUI 未返回凭据")?
                    .to_owned(),
            );
            session.retry = None;
            session.at = Instant::now();
        }
        let url = format!("http://127.0.0.1:{port}{path}");
        let request = if snowluma {
            self.http.get(url)
        } else {
            self.http.post(url)
        };
        let response = request
            .bearer_auth(session.token.as_deref().unwrap_or_default())
            .timeout(Duration::from_secs(if path.ends_with("/RefreshQRcode") {
                12
            } else {
                5
            }))
            .send()
            .await
            .map_err(|_| "WebUI 请求失败")?;
        if matches!(response.status().as_u16(), 401 | 403) {
            session.token = None;
            session.retry = Some(Instant::now() + Duration::from_secs(65));
            return Err("WebUI 凭据失效".into());
        }
        if !response.status().is_success() {
            return Err("WebUI 状态请求失败".into());
        }
        let value = read_json(response).await?;
        if snowluma {
            Ok(value)
        } else if value.get("code").and_then(Value::as_i64).unwrap_or(0) == 0 {
            value
                .get("data")
                .cloned()
                .ok_or_else(|| "WebUI 缺少状态数据".into())
        } else {
            if value
                .get("message")
                .and_then(Value::as_str)
                .is_some_and(|m| m.eq_ignore_ascii_case("Unauthorized"))
            {
                session.token = None;
                session.retry = Some(Instant::now() + Duration::from_secs(65));
            }
            Err("WebUI 拒绝状态请求".into())
        }
    }
}

pub async fn read_json(response: reqwest::Response) -> Result<Value, String> {
    let bytes = read_body_limited(response, 1024 * 1024)
        .await
        .map_err(|_| "WebUI 响应过大或不完整")?;
    serde_json::from_slice(&bytes).map_err(|_| "WebUI 响应格式无效".into())
}

#[derive(Deserialize)]
pub struct Processes {
    pub list: Vec<Process>,
}
#[derive(Clone, Deserialize)]
pub struct Process {
    pub pid: u32,
    pub path: String,
    pub uin: String,
    pub status: String,
    #[serde(default, rename = "loggedIn")]
    pub logged_in: Option<bool>,
    #[serde(default)]
    pub connected: Option<bool>,
}

pub fn account_state(process: &Process, expected: &str) -> ncd_relogin::AccountState {
    use ncd_relogin::AccountState::*;
    if !process.uin.is_empty() && process.uin != expected {
        return Unknown;
    }
    match process.logged_in {
        Some(true) if process.uin == expected && process.connected == Some(true) => Online,
        Some(false) => Offline,
        _ if process.status == "online"
            && process.uin == expected
            && process.connected != Some(false) =>
        {
            Online
        }
        _ if process.status == "disconnected" => Offline,
        _ => Unknown,
    }
}
