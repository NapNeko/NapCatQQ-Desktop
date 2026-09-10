//! 导入 / 冷启动时认已有对接:反向看 Bot websocket_clients,正向看应用连 Bot 的 wsServers
//!
//! 只在唯一命中时认领,避免两台 Bot 共用 3001 口误绑.不改 Bot / 应用配置,只填 AppLinkRecord

use std::collections::HashSet;

use ncd_domain::{
    APP_LINK_ADOPTED_FORWARD, AppLinkRecord, BotConfig, BotId, OneBotLinkMode,
    is_loopback_host, parse_ws_url, runtime_target_matches_host,
};

use crate::metrics::now_ms;

const REVERSE_SAME_HOST_PORT: i32 = 10;
const FORWARD_SAME_HOST_PORT: i32 = 8;
const TOKEN_BONUS: i32 = 5;
const NCD_NAME_BONUS: i32 = 1;
const ONEBOT_PATH_BONUS: i32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    bot_id: BotId,
    connection_name: String,
    score: i32,
}

pub fn discover_existing_link(
    app_host_id: &str,
    app_port: u16,
    app_token: Option<&str>,
    outbound_urls: &[String],
    claimed: &HashSet<(String, String)>,
    bots: &[BotConfig],
) -> Option<AppLinkRecord> {
    let mut hits: Vec<Candidate> = Vec::new();
    for bot in bots {
        hits.extend(reverse_hits(
            app_host_id,
            app_port,
            app_token,
            claimed,
            bot,
        ));
        hits.extend(forward_hits(
            app_host_id,
            app_token,
            outbound_urls,
            bot,
        ));
    }
    pick_unique(hits).map(|c| AppLinkRecord {
        bot_id: c.bot_id,
        mode: OneBotLinkMode::ReverseWs,
        connection_name: c.connection_name,
        linked_at_ms: now_ms(),
        resident_forward_port: None,
    })
}

fn reverse_hits(
    app_host_id: &str,
    app_port: u16,
    app_token: Option<&str>,
    claimed: &HashSet<(String, String)>,
    bot: &BotConfig,
) -> Vec<Candidate> {
    let bot_id = BotId::new(bot.bot.qq_id.to_string());
    let same_host = runtime_target_matches_host(&bot.bot.runtime_target, app_host_id);
    let mut out = Vec::new();
    for client in &bot.connect.websocket_clients {
        if !client.base.enable {
            continue;
        }
        let name = client.base.name.clone();
        if claimed.contains(&(bot_id.as_str().to_string(), name.clone())) {
            continue;
        }
        let Some(url) = parse_ws_url(&client.url) else {
            continue;
        };
        let port_match = app_port != 0 && url.port == app_port;
        let token = tokens_match(app_token, &client.base.token);
        let loopback = is_loopback_host(&url.host);
        let path_ok = looks_like_onebot_path(&url.path);
        let accept = (same_host && port_match && (loopback || token))
            || (token && port_match)
            || (same_host && token && path_ok);
        if !accept {
            continue;
        }
        let mut score = 0;
        if same_host && port_match {
            score += REVERSE_SAME_HOST_PORT;
        }
        if token {
            score += TOKEN_BONUS;
        }
        if name.starts_with(ncd_domain::APP_LINK_CONNECTION_PREFIX) {
            score += NCD_NAME_BONUS;
        }
        if path_ok {
            score += ONEBOT_PATH_BONUS;
        }
        if score == 0 {
            continue;
        }
        out.push(Candidate {
            bot_id: bot_id.clone(),
            connection_name: name,
            score,
        });
    }
    out
}

fn forward_hits(
    app_host_id: &str,
    app_token: Option<&str>,
    outbound_urls: &[String],
    bot: &BotConfig,
) -> Vec<Candidate> {
    let bot_id = BotId::new(bot.bot.qq_id.to_string());
    let same_host = runtime_target_matches_host(&bot.bot.runtime_target, app_host_id);
    let mut out = Vec::new();
    for raw in outbound_urls {
        let Some(url) = parse_ws_url(raw) else {
            continue;
        };
        if !is_loopback_host(&url.host) {
            continue;
        }
        for server in &bot.connect.websocket_servers {
            if !server.base.enable || server.port == 0 || server.port != url.port {
                continue;
            }
            let token = tokens_match(app_token, &server.base.token);
            if !same_host && !token {
                continue;
            }
            let mut score = 0;
            if same_host {
                score += FORWARD_SAME_HOST_PORT;
            }
            if token {
                score += TOKEN_BONUS;
            }
            if score == 0 {
                continue;
            }
            out.push(Candidate {
                bot_id: bot_id.clone(),
                connection_name: APP_LINK_ADOPTED_FORWARD.to_string(),
                score,
            });
        }
    }
    out
}

fn pick_unique(hits: Vec<Candidate>) -> Option<Candidate> {
    if hits.is_empty() {
        return None;
    }
    let mut best: Vec<Candidate> = Vec::new();
    let mut best_score = i32::MIN;
    for hit in hits {
        if hit.score > best_score {
            best_score = hit.score;
            best.clear();
            best.push(hit);
        } else if hit.score == best_score {
            best.push(hit);
        }
    }
    let mut bots: HashSet<&str> = HashSet::new();
    for c in &best {
        bots.insert(c.bot_id.as_str());
    }
    if bots.len() != 1 {
        return None;
    }
    best.into_iter().max_by_key(|c| c.score)
}

fn tokens_match(app: Option<&str>, other: &str) -> bool {
    let app = app.map(str::trim).filter(|s| !s.is_empty());
    let other = other.trim();
    match app {
        Some(app) if !other.is_empty() => app == other,
        _ => false,
    }
}

fn looks_like_onebot_path(path: &str) -> bool {
    let p = path.trim_end_matches('/');
    p.is_empty() || p.contains("onebot") || p == "/" || p == "/ws"
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::{
        AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, ConnectConfig,
        DeploymentType, MessagePostFormat, NetworkBaseFields, RuntimeTarget, WebsocketClientConfig,
        WebsocketServerConfig, WsRole,
    };

    fn bot(qq: u64, target: RuntimeTarget) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "b".into(),
                qq_id: qq,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: target,
                backend_type: BackendType::NapCat,
                deployment_type: DeploymentType::default(),
                snowluma_start_mode: None,
                webui_password_takeover: false,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            status_command: None,
        }
    }

    fn ws_client(name: &str, url: &str, token: &str) -> WebsocketClientConfig {
        WebsocketClientConfig {
            base: NetworkBaseFields {
                enable: true,
                name: name.into(),
                message_post_format: MessagePostFormat::Array,
                token: token.into(),
                debug: false,
            },
            url: url.into(),
            report_self_message: false,
            heart_interval: 30000,
            reconnect_interval: 30000,
            role: WsRole::Universal,
        }
    }

    fn ws_server(port: u16, token: &str) -> WebsocketServerConfig {
        WebsocketServerConfig {
            base: NetworkBaseFields {
                enable: true,
                name: "ws".into(),
                message_post_format: MessagePostFormat::Array,
                token: token.into(),
                debug: false,
            },
            host: "0.0.0.0".into(),
            port,
            report_self_message: false,
            enable_force_push_event: false,
            heart_interval: 30000,
            path: "/".into(),
            role: WsRole::Universal,
        }
    }

    fn discover(
        host: &str,
        port: u16,
        token: Option<&str>,
        outbound: &[&str],
        bots: &[BotConfig],
    ) -> Option<AppLinkRecord> {
        let urls: Vec<String> = outbound.iter().map(|s| (*s).to_string()).collect();
        discover_existing_link(host, port, token, &urls, &HashSet::new(), bots)
    }

    #[test]
    fn reverse_same_host_port_without_token() {
        let mut b = bot(10001, RuntimeTarget::server("km"));
        b.connect
            .websocket_clients
            .push(ws_client("xiuxian", "ws://127.0.0.1:13120/onebot/v11/ws", ""));
        let hit = discover("remote:km", 13120, None, &[], &[b]).unwrap();
        assert_eq!(hit.bot_id.as_str(), "10001");
        assert_eq!(hit.connection_name, "xiuxian");
    }

    #[test]
    fn forward_same_host_ws_server() {
        let mut b = bot(20002, RuntimeTarget::server("km"));
        b.connect.websocket_servers.push(ws_server(3001, ""));
        let hit = discover(
            "remote:km",
            13120,
            None,
            &["ws://127.0.0.1:3001"],
            &[b],
        )
        .unwrap();
        assert_eq!(hit.bot_id.as_str(), "20002");
        assert_eq!(hit.connection_name, APP_LINK_ADOPTED_FORWARD);
    }

    #[test]
    fn prefers_reverse_when_both_point_at_same_bot() {
        let mut b = bot(10001, RuntimeTarget::Local);
        b.connect
            .websocket_clients
            .push(ws_client("ncd-app:old", "ws://127.0.0.1:8080/onebot/v11/ws", "tok"));
        b.connect.websocket_servers.push(ws_server(3001, ""));
        let hit = discover(
            "local",
            8080,
            Some("tok"),
            &["ws://127.0.0.1:3001"],
            &[b],
        )
        .unwrap();
        assert_eq!(hit.connection_name, "ncd-app:old");
    }

    #[test]
    fn ambiguous_two_bots_is_none() {
        let mut a = bot(1, RuntimeTarget::Local);
        a.connect
            .websocket_clients
            .push(ws_client("a", "ws://127.0.0.1:8080/onebot/v11/ws", ""));
        let mut b = bot(2, RuntimeTarget::Local);
        b.connect
            .websocket_clients
            .push(ws_client("b", "ws://127.0.0.1:8080/onebot/v11/ws", ""));
        assert!(discover("local", 8080, None, &[], &[a, b]).is_none());
    }

    #[test]
    fn reverse_ws_path_scores_as_onebot() {
        let mut b = bot(10001, RuntimeTarget::Local);
        b.connect
            .websocket_clients
            .push(ws_client("ncd-app:a1", "ws://127.0.0.1:6199/ws", "tok"));
        let hit = discover("local", 6199, Some("tok"), &[], &[b]).unwrap();
        assert_eq!(hit.bot_id.as_str(), "10001");
        assert_eq!(hit.connection_name, "ncd-app:a1");
    }

    #[test]
    fn skips_claimed_reverse_client() {
        let mut b = bot(10001, RuntimeTarget::Local);
        b.connect
            .websocket_clients
            .push(ws_client("xiuxian", "ws://127.0.0.1:8080/onebot/v11/ws", ""));
        let mut claimed = HashSet::new();
        claimed.insert(("10001".into(), "xiuxian".into()));
        assert!(discover_existing_link(
            "local",
            8080,
            None,
            &[],
            &claimed,
            &[b]
        )
        .is_none());
    }

    #[test]
    fn ignores_other_host_forward_without_token() {
        let mut b = bot(10001, RuntimeTarget::server("other"));
        b.connect.websocket_servers.push(ws_server(3001, ""));
        assert!(discover(
            "remote:km",
            13120,
            None,
            &["ws://127.0.0.1:3001"],
            &[b]
        )
        .is_none());
    }
}
