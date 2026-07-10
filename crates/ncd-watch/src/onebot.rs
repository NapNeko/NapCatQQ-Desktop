//! 同机 OneBot HTTP 投递(对齐 Desktop notify/onebot,source=watch)

use std::collections::HashSet;

use ncd_domain::{OfflineAlert, render_template};

use crate::config::{WatchOneBotMessenger, WatchOneBotSettings};

const TIMEOUT_SECS: u64 = 10;

/// 按顺序挑第一个可用 messenger:非掉线 bot、可选要求本轮进程在线、base_url 非空
pub fn pick_messenger<'a>(
    settings: &'a WatchOneBotSettings,
    offline_bot_id: &str,
    online_bot_ids: Option<&HashSet<String>>,
) -> Option<&'a WatchOneBotMessenger> {
    if !settings.enabled {
        return None;
    }
    settings.messengers.iter().find(|m| {
        let id = m.bot_id.trim();
        if id.is_empty() || id == offline_bot_id {
            return false;
        }
        if m.base_url.trim().is_empty() {
            return false;
        }
        if let Some(online) = online_bot_ids {
            if !online.contains(id) {
                return false;
            }
        }
        true
    })
}

pub async fn send_watch_onebot(
    settings: &WatchOneBotSettings,
    offline_bot_id: &str,
    online_bot_ids: Option<&HashSet<String>>,
    alert: &OfflineAlert,
) -> Result<(), String> {
    if !settings.enabled {
        return Ok(());
    }
    let targets: Vec<u64> = settings
        .target_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .collect();
    if targets.is_empty() {
        return Err("OneBot 目标 ID 未配置".into());
    }
    let Some(messenger) = pick_messenger(settings, offline_bot_id, online_bot_ids) else {
        return Err("无可用同机 OneBot messenger".into());
    };
    send_with_messenger(messenger, settings, alert, &targets).await
}

async fn send_with_messenger(
    messenger: &WatchOneBotMessenger,
    settings: &WatchOneBotSettings,
    alert: &OfflineAlert,
    targets: &[u64],
) -> Result<(), String> {
    let mut vars = alert.template_vars();
    vars.push(("source", "watch".into()));
    let text = render_template(&settings.message_template, &vars);
    let is_group = settings.target_type.eq_ignore_ascii_case("group");
    let path = if is_group {
        "/send_group_msg"
    } else {
        "/send_private_msg"
    };
    let id_key = if is_group { "group_id" } else { "user_id" };
    let url = format!("{}{}", messenger.base_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
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
        if !messenger.access_token.is_empty() {
            req = req.header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", messenger.access_token),
            );
        }
        match req.send().await {
            Ok(resp) if resp.status().is_success() => ok_count += 1,
            Ok(resp) => failures.push(format!("{target_id}: HTTP {}", resp.status().as_u16())),
            Err(err) => failures.push(format!("{target_id}: {err}")),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(messengers: Vec<WatchOneBotMessenger>) -> WatchOneBotSettings {
        WatchOneBotSettings {
            enabled: true,
            messengers,
            target_type: "private".into(),
            target_ids: vec![1],
            message_template: "hi {uin}".into(),
        }
    }

    fn m(id: &str, url: &str) -> WatchOneBotMessenger {
        WatchOneBotMessenger {
            bot_id: id.into(),
            base_url: url.into(),
            access_token: String::new(),
        }
    }

    #[test]
    fn skips_offline_bot_and_picks_next() {
        let s = settings(vec![
            m("10001", "http://127.0.0.1:3001"),
            m("10002", "http://127.0.0.1:3002"),
        ]);
        let picked = pick_messenger(&s, "10001", None).unwrap();
        assert_eq!(picked.bot_id, "10002");
    }

    #[test]
    fn skips_when_not_online_this_round() {
        let s = settings(vec![m("10002", "http://127.0.0.1:3002")]);
        let mut online = HashSet::new();
        online.insert("10003".into());
        assert!(pick_messenger(&s, "10001", Some(&online)).is_none());
        online.insert("10002".into());
        assert_eq!(
            pick_messenger(&s, "10001", Some(&online)).unwrap().bot_id,
            "10002"
        );
    }

    #[test]
    fn disabled_or_empty_url() {
        let mut s = settings(vec![m("10002", "")]);
        assert!(pick_messenger(&s, "10001", None).is_none());
        s.messengers[0].base_url = "http://127.0.0.1:1".into();
        s.enabled = false;
        assert!(pick_messenger(&s, "10001", None).is_none());
    }
}
