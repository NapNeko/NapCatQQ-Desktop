//! Webhook 投递(对齐 Desktop notify 模板变量)

use ncd_domain::{
    OfflineAlert, OfflineAlertKind, OfflineAlertSource, OfflineWebhookChannel, render_template,
};
use ncd_domain::ids::BotId;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

use crate::config::NotifyBotTarget;

const TIMEOUT_SECS: u64 = 10;

pub fn build_offline_alert(bot: &NotifyBotTarget, kind: OfflineAlertKind) -> OfflineAlert {
    let source = if bot.backend.eq_ignore_ascii_case("snowluma") {
        OfflineAlertSource::SnowLuma
    } else {
        OfflineAlertSource::NapCat
    };
    let qq = if bot.qq_id > 0 {
        bot.qq_id
    } else {
        bot.bot_id.parse().unwrap_or(0)
    };
    OfflineAlert {
        bot_id: BotId::new(bot.bot_id.clone()),
        qq_id: qq,
        bot_name: bot.bot_name.clone(),
        kind,
        source,
        at: chrono::Local::now().format("%Y/%m/%d %H:%M:%S").to_string(),
    }
}

/// 向所有启用通道发送;全部失败才 Err
pub async fn send_watch_webhooks(
    channels: &[&OfflineWebhookChannel],
    alert: &OfflineAlert,
) -> Result<(), String> {
    if channels.is_empty() {
        return Err("no webhook channels".into());
    }
    let mut ok = 0usize;
    let mut errors = Vec::new();
    for ch in channels {
        match send_channel(ch, alert).await {
            Ok(()) => ok += 1,
            Err(e) => errors.push(format!("{}: {e}", ch.id)),
        }
    }
    if ok == 0 {
        return Err(errors.join("; "));
    }
    if !errors.is_empty() {
        tracing::warn!(ok, failed = errors.len(), "partial webhook failure: {}", errors.join("; "));
    }
    Ok(())
}

async fn send_channel(channel: &OfflineWebhookChannel, alert: &OfflineAlert) -> Result<(), String> {
    let url = channel.url.trim();
    if url.is_empty() {
        return Err("empty url".into());
    }
    let mut vars = alert.template_vars();
    vars.push(("source", "watch".into()));
    let body = render_template(&channel.body_template, &vars);
    post_or_get(url, channel.secret.trim(), channel.method.trim(), &body).await
}

async fn post_or_get(url: &str, secret: &str, method: &str, body: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if !secret.is_empty() {
        // 与 Desktop 一致:配置只填 token 本体,这里拼 Bearer
        let value = if secret.to_ascii_lowercase().starts_with("bearer ") {
            secret.to_string()
        } else {
            format!("Bearer {secret}")
        };
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&value).map_err(|e| e.to_string())?,
        );
    }

    let method_u = method.to_ascii_uppercase();
    let response = if method_u == "GET" {
        let req = client.get(url).headers(headers);
        let req = match serde_json::from_str::<serde_json::Value>(body) {
            Ok(serde_json::Value::Object(map)) => {
                let pairs: Vec<(String, String)> = map
                    .into_iter()
                    .map(|(k, v)| {
                        let s = match v {
                            serde_json::Value::String(s) => s,
                            other => other.to_string(),
                        };
                        (k, s)
                    })
                    .collect();
                req.query(&pairs)
            }
            _ => req.query(&[("body", body)]),
        };
        req.send().await
    } else {
        client
            .post(url)
            .headers(headers)
            .body(body.to_string())
            .send()
            .await
    }
    .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::OfflineAlertKind;

    #[test]
    fn alert_from_bot_fills_qq() {
        let bot = NotifyBotTarget {
            bot_id: "12345".into(),
            qq_id: 0,
            bot_name: "n".into(),
            backend: "napcat".into(),
            deployment: "docker".into(),
            container_name: None,
            pid_file: None,
            process_match: None,
            enabled: true,
        };
        let a = build_offline_alert(&bot, OfflineAlertKind::Manual);
        assert_eq!(a.qq_id, 12345);
        assert_eq!(a.event_label(), "offline");
    }
}
