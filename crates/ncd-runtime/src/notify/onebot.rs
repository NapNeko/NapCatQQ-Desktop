//! 通过仍在线的 Bot 的 OneBot HTTP 发私聊/群消息
//!
//! messenger 选择只接受本机环回 HTTP 服务;远端 host 对 Desktop 通常不可达,直接跳过。

use ncd_domain::{OfflineAlert, OfflineOneBotSettings, render_template};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneBotMessenger {
    /// 本机 OneBot HTTP 根地址,例如 http://127.0.0.1:3000
    pub base_url: String,
    pub access_token: String,
}

/// 从 Bot `connect.httpServers` 投影出的本机候选
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHttpServerCandidate {
    pub enable: bool,
    pub host: String,
    pub port: u16,
    pub token: String,
}

/// messenger 解析被跳过的原因(投递侧只记日志,不打断其它渠道)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessengerResolveSkip {
    EmptyMessenger,
    SameAsOfflineBot,
    NotRunning,
    NoLocalHttpEndpoint,
}

/// 按固定规则挑选本机 messenger 端点:
/// 1. messenger id 非空
/// 2. messenger ≠ 掉线 bot
/// 3. messenger 处于 Running
/// 4. 取第一个 enable 且 port>0 的 HTTP 服务
/// 5. host 空/`0.0.0.0` 视为 `127.0.0.1`;仅接受环回,其它 host 跳过
pub fn resolve_local_onebot_messenger(
    messenger_bot_id: &str,
    exclude_bot_id: &str,
    is_running: bool,
    http_servers: &[LocalHttpServerCandidate],
) -> Result<OneBotMessenger, MessengerResolveSkip> {
    let messenger = messenger_bot_id.trim();
    if messenger.is_empty() {
        return Err(MessengerResolveSkip::EmptyMessenger);
    }
    if messenger == exclude_bot_id {
        return Err(MessengerResolveSkip::SameAsOfflineBot);
    }
    if !is_running {
        return Err(MessengerResolveSkip::NotRunning);
    }

    let server = http_servers
        .iter()
        .find(|s| s.enable && s.port > 0)
        .ok_or(MessengerResolveSkip::NoLocalHttpEndpoint)?;

    let host = if server.host.trim().is_empty() || server.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        server.host.trim()
    };
    if host != "127.0.0.1" && host != "localhost" {
        return Err(MessengerResolveSkip::NoLocalHttpEndpoint);
    }

    Ok(OneBotMessenger {
        base_url: format!("http://{host}:{}", server.port),
        access_token: server.token.clone(),
    })
}

impl OneBotMessenger {
    pub async fn send_alert(
        &self,
        settings: &OfflineOneBotSettings,
        alert: &OfflineAlert,
    ) -> Result<(), String> {
        if !settings.enabled {
            return Ok(());
        }
        let targets = settings.effective_target_ids();
        if targets.is_empty() {
            return Err("OneBot 目标 ID 未配置".to_string());
        }
        let text = render_template(&settings.message_template, &alert.template_vars());
        let is_group = settings.target_type.eq_ignore_ascii_case("group");
        let path = if is_group {
            "/send_group_msg"
        } else {
            "/send_private_msg"
        };
        let id_key = if is_group { "group_id" } else { "user_id" };
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;

        let mut failures = Vec::new();
        let mut ok_count = 0usize;
        for target_id in targets {
            let body = serde_json::json!({
                id_key: target_id,
                "message": text,
            });
            let mut req = client.post(&url).json(&body);
            if !self.access_token.is_empty() {
                req = req.header(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {}", self.access_token),
                );
            }
            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    ok_count += 1;
                }
                Ok(resp) => {
                    failures.push(format!("{target_id}: HTTP {}", resp.status().as_u16()));
                }
                Err(err) => {
                    failures.push(format!("{target_id}: {err}"));
                }
            }
        }

        if failures.is_empty() {
            return Ok(());
        }
        if ok_count == 0 {
            return Err(format!("OneBot 全部失败: {}", failures.join("; ")));
        }
        Err(format!(
            "OneBot 部分失败({ok_count} 成功): {}",
            failures.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(enable: bool, host: &str, port: u16, token: &str) -> LocalHttpServerCandidate {
        LocalHttpServerCandidate {
            enable,
            host: host.into(),
            port,
            token: token.into(),
        }
    }

    #[test]
    fn skips_empty_messenger() {
        let err = resolve_local_onebot_messenger("  ", "10001", true, &[]).unwrap_err();
        assert_eq!(err, MessengerResolveSkip::EmptyMessenger);
    }

    #[test]
    fn skips_when_messenger_is_offline_bot() {
        let err = resolve_local_onebot_messenger(
            "10001",
            "10001",
            true,
            &[server(true, "127.0.0.1", 3000, "t")],
        )
        .unwrap_err();
        assert_eq!(err, MessengerResolveSkip::SameAsOfflineBot);
    }

    #[test]
    fn skips_when_not_running() {
        let err = resolve_local_onebot_messenger(
            "10002",
            "10001",
            false,
            &[server(true, "127.0.0.1", 3000, "t")],
        )
        .unwrap_err();
        assert_eq!(err, MessengerResolveSkip::NotRunning);
    }

    #[test]
    fn skips_when_no_enabled_http_server() {
        let servers = [
            server(false, "127.0.0.1", 3000, "t"),
            server(true, "127.0.0.1", 0, "t"),
        ];
        let err = resolve_local_onebot_messenger("10002", "10001", true, &servers).unwrap_err();
        assert_eq!(err, MessengerResolveSkip::NoLocalHttpEndpoint);
    }

    #[test]
    fn skips_non_loopback_host() {
        let err = resolve_local_onebot_messenger(
            "10002",
            "10001",
            true,
            &[server(true, "192.168.1.8", 3000, "t")],
        )
        .unwrap_err();
        assert_eq!(err, MessengerResolveSkip::NoLocalHttpEndpoint);
    }

    #[test]
    fn maps_empty_and_zero_host_to_loopback() {
        let empty_host = resolve_local_onebot_messenger(
            "10002",
            "10001",
            true,
            &[server(true, "", 3001, "tok-a")],
        )
        .unwrap();
        assert_eq!(empty_host.base_url, "http://127.0.0.1:3001");
        assert_eq!(empty_host.access_token, "tok-a");

        let zero_host = resolve_local_onebot_messenger(
            "10002",
            "10001",
            true,
            &[server(true, "0.0.0.0", 3002, "tok-b")],
        )
        .unwrap();
        assert_eq!(zero_host.base_url, "http://127.0.0.1:3002");
        assert_eq!(zero_host.access_token, "tok-b");
    }

    #[test]
    fn accepts_localhost_and_picks_first_enabled() {
        let servers = [
            server(false, "127.0.0.1", 2999, "skip"),
            server(true, "localhost", 3000, "use-me"),
            server(true, "127.0.0.1", 3001, "later"),
        ];
        let m = resolve_local_onebot_messenger("10002", "10001", true, &servers).unwrap();
        assert_eq!(m.base_url, "http://localhost:3000");
        assert_eq!(m.access_token, "use-me");
    }
}
