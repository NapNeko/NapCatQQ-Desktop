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

use sha2::{Digest, Sha256};

use crate::config::NotifyBotTarget;
use crate::probe::LoginStatus;

const TIMEOUT_SECS: u64 = 5;

/// 对单个 bot 查账号在线态;无 port/token 或非 napcat 返回 Unknown
pub async fn probe_login_status(bot: &NotifyBotTarget) -> (LoginStatus, String) {
    if !bot.backend.eq_ignore_ascii_case("napcat") {
        return (LoginStatus::Unknown, "login probe: non-napcat".into());
    }
    let Some(port) = bot.webui_port.filter(|p| *p > 0) else {
        return (LoginStatus::Unknown, "login probe: no webui_port".into());
    };
    let token = bot
        .webui_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(token) = token else {
        return (LoginStatus::Unknown, "login probe: no webui_token".into());
    };

    match probe_account_online(port, token).await {
        Ok((st, detail)) => (st, detail),
        Err(e) => (LoginStatus::Unknown, format!("webui:{port} {e}")),
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

async fn probe_account_online(port: u16, token: &str) -> Result<(LoginStatus, String), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;

    let credential = fetch_credential(&client, port, token).await?;

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

    let mut is_login: Option<bool> = None;
    let mut is_offline: Option<bool> = None;
    let mut login_err = String::new();
    match login_resp {
        Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>().await {
            Ok(body) => {
                let data = body.get("data").cloned().unwrap_or(serde_json::Value::Null);
                is_login = data
                    .get("isLogin")
                    .or_else(|| data.get("is_login"))
                    .and_then(|v| v.as_bool());
                is_offline = data
                    .get("isOffline")
                    .or_else(|| data.get("is_offline"))
                    .and_then(|v| v.as_bool());
            }
            Err(e) => login_err = format!("login decode: {e}"),
        },
        Ok(resp) => login_err = format!("check login status {}", resp.status()),
        Err(e) => login_err = format!("check login: {e}"),
    }

    let mut online: Option<bool> = None;
    let mut info_err = String::new();
    match info_resp {
        Ok(resp) if resp.status().is_success() => match resp.json::<serde_json::Value>().await {
            Ok(body) => {
                let data = body.get("data").cloned().unwrap_or(serde_json::Value::Null);
                online = data.get("online").and_then(|v| v.as_bool());
            }
            Err(e) => info_err = format!("info decode: {e}"),
        },
        Ok(resp) => info_err = format!("GetQQLoginInfo status {}", resp.status()),
        Err(e) => info_err = format!("GetQQLoginInfo: {e}"),
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

    match online {
        Some(true) => Ok((LoginStatus::LoggedIn, detail)),
        Some(false) => Ok((LoginStatus::LoggedOut, detail)),
        None => {
            if !login_err.is_empty() || !info_err.is_empty() {
                Err(detail)
            } else {
                Ok((LoginStatus::Unknown, detail))
            }
        }
    }
}

async fn fetch_credential(
    client: &reqwest::Client,
    port: u16,
    token: &str,
) -> Result<String, String> {
    let hash = {
        let mut h = Sha256::new();
        h.update(token.as_bytes());
        h.update(b".napcat");
        hex::encode(h.finalize())
    };
    let login_url = format!("http://127.0.0.1:{port}/api/auth/login");
    let resp = client
        .post(&login_url)
        .json(&serde_json::json!({ "hash": hash }))
        .send()
        .await
        .map_err(|e| format!("auth login: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("auth login status {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("auth decode: {e}"))?;
    body.get("data")
        .and_then(|d| d.get("Credential"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| "auth missing Credential".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (st, _) = rt.block_on(probe_login_status(&bot));
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
        };
        assert!(has_webui_probe(&bot));
        bot.webui_token = Some("  ".into());
        assert!(!has_webui_probe(&bot));
    }
}
