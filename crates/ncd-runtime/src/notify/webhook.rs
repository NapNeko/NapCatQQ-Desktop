//! HTTP Webhook 投递(多通道 + 旧扁平字段兼容)

use ncd_domain::{OfflineAlert, OfflineWebhookChannel, OfflineWebhookSettings, render_template};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};

const TIMEOUT_SECS: u64 = 10;

/// 向所有启用且 URL 非空的通道发送
pub async fn send_offline_webhook(
    settings: &OfflineWebhookSettings,
    alert: &OfflineAlert,
) -> Result<(), String> {
    let channels: Vec<_> = settings
        .effective_channels()
        .into_iter()
        .filter(|c| c.enabled && !c.url.trim().is_empty())
        .collect();
    if channels.is_empty() {
        return Err("Webhook 未配置可用通道".to_string());
    }

    let mut errors = Vec::new();
    let mut ok = 0usize;
    for ch in &channels {
        match send_channel(ch, alert).await {
            Ok(()) => ok += 1,
            Err(err) => errors.push(format!("{}: {err}", channel_label(ch))),
        }
    }
    if ok == 0 {
        return Err(errors.join("; "));
    }
    if !errors.is_empty() {
        tracing::warn!(
            ok,
            failed = errors.len(),
            "部分 Webhook 通道发送失败: {}",
            errors.join("; ")
        );
    }
    Ok(())
}

/// 测试指定通道
///
/// 解析优先级:
/// 1. `channel` 直接传入(前端编辑中的草稿,跳过已保存配置,改完即测)
/// 2. `channel_id` 在已保存配置里查
/// 3. 两者都空则取第一条有效通道
pub async fn send_test_webhook(
    settings: &OfflineWebhookSettings,
    channel_id: Option<&str>,
    channel: Option<&OfflineWebhookChannel>,
) -> Result<(), String> {
    let alert = OfflineAlert {
        bot_id: ncd_domain::ids::BotId::new("0"),
        qq_id: 0,
        bot_name: "测试 Bot".to_string(),
        kind: ncd_domain::OfflineAlertKind::Manual,
        source: ncd_domain::OfflineAlertSource::NapCat,
        at: chrono_local_now(),
    };

    let target: OfflineWebhookChannel = if let Some(ch) = channel {
        ch.clone()
    } else if let Some(id) = channel_id.filter(|s| !s.is_empty()) {
        settings
            .channel_by_id(id)
            .ok_or_else(|| format!("未找到 Webhook 通道: {id}"))?
    } else {
        settings
            .effective_channels()
            .into_iter()
            .find(|c| !c.url.trim().is_empty())
            .ok_or_else(|| "Webhook URL 未配置".to_string())?
    };

    send_channel(&target, &alert).await
}

fn channel_label(ch: &OfflineWebhookChannel) -> &str {
    if ch.name.trim().is_empty() {
        ch.id.as_str()
    } else {
        ch.name.as_str()
    }
}

async fn send_channel(channel: &OfflineWebhookChannel, alert: &OfflineAlert) -> Result<(), String> {
    let url = channel.url.trim();
    if url.is_empty() {
        return Err("Webhook URL 未配置".to_string());
    }
    let body = render_template(&channel.body_template, &alert.template_vars());
    post_or_get(url, channel.secret.trim(), channel.method.trim(), &body).await
}

fn chrono_local_now() -> String {
    chrono::Local::now().format("%Y/%m/%d %H:%M:%S").to_string()
}

async fn post_or_get(url: &str, secret: &str, method: &str, body: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if !secret.is_empty() {
        let value = format!("Bearer {secret}");
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
    .map_err(|e| format!("Webhook 请求失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Webhook HTTP {}", response.status().as_u16()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::ids::BotId;
    use ncd_domain::{OfflineAlertKind, OfflineAlertSource};

    #[test]
    fn empty_channels_rejected() {
        let settings = OfflineWebhookSettings::default();
        let alert = OfflineAlert {
            bot_id: BotId::new("1"),
            qq_id: 1,
            bot_name: "a".into(),
            kind: OfflineAlertKind::Manual,
            source: OfflineAlertSource::NapCat,
            at: "t".into(),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(send_offline_webhook(&settings, &alert))
            .unwrap_err();
        assert!(err.contains("未配置"));
    }

    // 传入 channel 时优先用它,即使 channel_id 指向不存在的通道也不报"未找到"
    #[test]
    fn test_webhook_prefers_inlined_channel() {
        let settings = OfflineWebhookSettings::default();
        let inlined = OfflineWebhookChannel {
            url: String::new(),
            ..OfflineWebhookChannel::new_blank("draft")
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(send_test_webhook(
                &settings,
                Some("nonexistent"),
                Some(&inlined),
            ))
            .unwrap_err();
        // 走 send_channel 空 URL 分支,而非"未找到 Webhook 通道"
        assert_eq!(err, "Webhook URL 未配置");
    }

    // 未传 channel 时回退到 channel_id 查已保存配置
    #[test]
    fn test_webhook_falls_back_to_channel_id() {
        let settings = OfflineWebhookSettings::default();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(send_test_webhook(&settings, Some("nonexistent"), None))
            .unwrap_err();
        assert!(err.contains("未找到 Webhook 通道"));
    }
}
