// 本机 WebUI 登录会话；错误与限流不等同于 QQ 掉线。
use ncd_onebot::client::read_body_limited;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct WebUi {
    http: reqwest::Client,
    sessions: HashMap<String, Session>,
}
#[derive(Debug)]
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
#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug)]
pub struct SlProcessBinding {
    pub process: Process,
    pub generation: String,
}

fn unbound_uin(uin: &str) -> bool {
    matches!(uin, "" | "0")
}

fn select_process(processes: Processes, bot: &str, pid: Option<u32>) -> Option<Process> {
    let mut by_account = processes.list.iter().filter(|p| p.uin == bot);
    if let Some(process) = by_account.next() {
        return (process.pid > 0 && by_account.next().is_none()).then(|| process.clone());
    }
    let mut by_pid = processes
        .list
        .into_iter()
        .filter(|p| p.pid > 0 && Some(p.pid) == pid);
    let process = by_pid.next()?;
    (unbound_uin(&process.uin) && by_pid.next().is_none()).then_some(process)
}

/// 未登录的 uin="0" 只借用已验证且代次相同的 PID，或核实进程参数中的账号。
/// 同一 daemon 的其他未登录进程、真实其他账号与 PID 复用都保持未知。
pub async fn process_for_account(
    processes: Processes,
    bot: &str,
    pid_file: Option<&str>,
    container: Option<&str>,
    expected_pid: Option<u32>,
    previous: Option<&SlProcessBinding>,
) -> Option<SlProcessBinding> {
    let pid = if let Some(file) = pid_file {
        let bytes = super::capture::read_limited(file, 32).await.ok()?;
        Some(
            std::str::from_utf8(&bytes)
                .ok()?
                .trim()
                .parse::<u32>()
                .ok()?,
        )
    } else if let Some(container) = container {
        super::capture::docker_anchor_pid(container)
            .await
            .or(previous.map(|b| b.process.pid))
            .or(expected_pid)
    } else {
        previous.map(|b| b.process.pid).or(expected_pid)
    };
    let process = select_process(processes, bot, pid)?;
    let generation = super::capture::generation(container, process.pid).await?;
    if unbound_uin(&process.uin)
        && !previous.is_some_and(|b| b.process.pid == process.pid && b.generation == generation)
    {
        let path = format!("/proc/{}/cmdline", process.pid);
        let bytes = if let Some(container) = container {
            let mut command = tokio::process::Command::new("docker");
            command.args(["exec", container, "cat", &path]);
            super::capture::run(command, &[], 64 * 1024).await.ok()?
        } else {
            super::capture::read_limited(&path, 64 * 1024).await.ok()?
        };
        if !cmdline_matches_account(&bytes, bot)
            || super::capture::generation(container, process.pid)
                .await
                .as_ref()
                != Some(&generation)
        {
            return None;
        }
    }
    Some(SlProcessBinding {
        process,
        generation,
    })
}

fn cmdline_matches_account(bytes: &[u8], bot: &str) -> bool {
    if bytes.last() != Some(&0) {
        return false;
    }
    let Ok(command) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut args = command.trim_end_matches('\0').split('\0');
    if args.next().is_none_or(str::is_empty) {
        return false;
    }
    let mut matched = false;
    while let Some(arg) = args.next() {
        if arg == "--type" || arg.starts_with("--type=") {
            return false;
        }
        let account = if matches!(arg, "-q" | "--qq" | "--qq-id") {
            let Some(account) = args.next() else {
                return false;
            };
            Some(account)
        } else {
            arg.strip_prefix("-q")
                .or_else(|| arg.strip_prefix("--qq="))
                .or_else(|| arg.strip_prefix("--qq-id="))
        };
        if let Some(account) = account {
            if account != bot {
                return false;
            }
            matched = true;
        }
    }
    matched
}

pub fn account_state(process: &Process, expected: &str) -> ncd_relogin::AccountState {
    use ncd_relogin::AccountState::*;
    if !unbound_uin(&process.uin) && process.uin != expected {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sl_unbound_process_selection_requires_unique_pid() {
        let list = |items| serde_json::from_value(json!({"list":items})).unwrap();
        let process = |pid, uin| json!({"pid":pid,"path":"/qq","uin":uin,"status":"available"});
        assert!(
            select_process(list(vec![process(1, "0"), process(2, "0")]), "10001", None).is_none()
        );
        assert_eq!(
            select_process(
                list(vec![process(1, "0"), process(2, "0")]),
                "10001",
                Some(2)
            )
            .unwrap()
            .pid,
            2
        );
        assert!(
            select_process(
                list(vec![process(1, "0"), process(1, "0")]),
                "10001",
                Some(1)
            )
            .is_none()
        );
        assert!(select_process(list(vec![process(1, "20002")]), "10001", Some(1)).is_none());
        assert!(
            select_process(
                list(vec![process(1, "10001"), process(2, "10001")]),
                "10001",
                Some(1)
            )
            .is_none()
        );
    }

    #[test]
    fn sl_process_account_arguments_reject_helpers_and_other_accounts() {
        for command in [
            b"/opt/QQ/qq\0--no-sandbox\0-q\010001\0".as_slice(),
            b"/opt/QQ/qq\0--qq=10001\0".as_slice(),
        ] {
            assert!(cmdline_matches_account(command, "10001"));
        }
        for command in [
            b"/opt/QQ/qq\0-q\020002\0".as_slice(),
            b"/opt/QQ/qq\0-q\010001\0--qq=20002\0".as_slice(),
            b"/opt/QQ/qq\0-q\010001\0--type=renderer\0".as_slice(),
            b"/opt/QQ/qq\0-q\0".as_slice(),
            b"/opt/QQ/qq\0".as_slice(),
        ] {
            assert!(!cmdline_matches_account(command, "10001"));
        }
    }
}
