//! SMTP 邮件投递(对齐 Desktop notify/email,source=watch 场景)

use lettre::message::{header::ContentType, Mailbox, Message};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{SmtpTransport, Transport};
use ncd_domain::{OfflineAlert, OfflineEmailSettings};

pub fn send_watch_email(settings: &OfflineEmailSettings, alert: &OfflineAlert) -> Result<(), String> {
    validate_settings(settings)?;
    let subject = "NapCatQQ-Desktop 机器人离线通知";
    let body = html_body(alert);
    send_smtp(settings, subject, &body)
}

fn validate_settings(settings: &OfflineEmailSettings) -> Result<(), String> {
    if settings.smtp_server.trim().is_empty() {
        return Err("SMTP 服务器未配置".to_string());
    }
    if settings.sender.trim().is_empty() || settings.receiver.trim().is_empty() {
        return Err("发件人或收件人为空".to_string());
    }
    if settings.token.trim().is_empty() {
        return Err("邮箱授权码未配置".to_string());
    }
    Ok(())
}

fn html_body(alert: &OfflineAlert) -> String {
    let name = if alert.bot_name.is_empty() {
        alert.qq_id.to_string()
    } else {
        format!("{} ({})", alert.bot_name, alert.qq_id)
    };
    format!(
        r#"<!DOCTYPE html><html lang="zh"><body>
<h2>机器人掉线通知</h2>
<p>Bot: {name}</p>
<p>状态: {event}</p>
<p>时间: {at}</p>
<p>来源: ncd-watch</p>
<p>本邮件由远端 ncd-watch 在 Desktop 离线时发送。</p>
</body></html>"#,
        name = name,
        event = alert.event_label(),
        at = alert.at
    )
}

fn send_smtp(settings: &OfflineEmailSettings, subject: &str, html: &str) -> Result<(), String> {
    let sender: Mailbox = settings
        .sender
        .trim()
        .parse()
        .map_err(|e| format!("发件人地址无效: {e}"))?;
    let receiver: Mailbox = settings
        .receiver
        .trim()
        .parse()
        .map_err(|e| format!("收件人地址无效: {e}"))?;

    let email = Message::builder()
        .from(sender)
        .to(receiver)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html.to_string())
        .map_err(|e| format!("构造邮件失败: {e}"))?;

    let creds = Credentials::new(
        settings.sender.trim().to_string(),
        settings.token.trim().to_string(),
    );
    let host = settings.smtp_server.trim();
    let port = settings.smtp_port;
    let encryption = settings.encryption.trim();

    let mailer = match encryption {
        "TLS" => {
            let tls = TlsParameters::new(host.to_string())
                .map_err(|e| format!("TLS 参数失败: {e}"))?;
            SmtpTransport::starttls_relay(host)
                .map_err(|e| format!("SMTP STARTTLS 构建失败: {e}"))?
                .port(port)
                .timeout(Some(std::time::Duration::from_secs(15)))
                .tls(Tls::Required(tls))
                .credentials(creds)
                .build()
        }
        "无加密" | "NONE" | "None" => SmtpTransport::builder_dangerous(host)
            .port(port)
            .timeout(Some(std::time::Duration::from_secs(15)))
            .credentials(creds)
            .build(),
        _ => {
            let tls = TlsParameters::new(host.to_string())
                .map_err(|e| format!("TLS 参数失败: {e}"))?;
            SmtpTransport::relay(host)
                .map_err(|e| format!("SMTP SSL 构建失败: {e}"))?
                .port(port)
                .timeout(Some(std::time::Duration::from_secs(15)))
                .tls(Tls::Wrapper(tls))
                .credentials(creds)
                .build()
        }
    };

    mailer
        .send(&email)
        .map_err(|e| format!("发送邮件失败: {e}"))?;
    Ok(())
}
