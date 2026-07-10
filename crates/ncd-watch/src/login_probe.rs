//! NapCat WebUI 登录态探活(远端本机 127.0.0.1)
//!
//! 对齐 Desktop login_poller 的最小子集: token → Credential → CheckLoginStatus。
//! 失败一律 LoginStatus::Unknown,不误报掉线。

use sha2::{Digest, Sha256};

use crate::config::NotifyBotTarget;
use crate::probe::LoginStatus;

const TIMEOUT_SECS: u64 = 5;

/// 对单个 bot 查登录态;无 port/token 或非 napcat 返回 Unknown
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

    match check_is_login(port, token).await {
        Ok(true) => (LoginStatus::LoggedIn, format!("webui:{port} isLogin=true")),
        Ok(false) => (LoginStatus::LoggedOut, format!("webui:{port} isLogin=false")),
        Err(e) => (LoginStatus::Unknown, format!("webui:{port} {e}")),
    }
}

async fn check_is_login(port: u16, token: &str) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;

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
    let credential = body
        .get("data")
        .and_then(|d| d.get("Credential"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "auth missing Credential".to_string())?
        .to_string();

    let status_url = format!("http://127.0.0.1:{port}/api/QQLogin/CheckLoginStatus");
    let resp = client
        .post(&status_url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {credential}"))
        .send()
        .await
        .map_err(|e| format!("check login: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("check login status {}", resp.status()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("login decode: {e}"))?;
    let data = body.get("data").cloned().unwrap_or(serde_json::Value::Null);
    if let Some(v) = data.get("isLogin").and_then(|v| v.as_bool()) {
        return Ok(v);
    }
    // 兼容偶发 snake_case
    if let Some(v) = data.get("is_login").and_then(|v| v.as_bool()) {
        return Ok(v);
    }
    Err("check login missing isLogin".into())
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
}
