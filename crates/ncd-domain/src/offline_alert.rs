//! 离线告警与通知通道配置
//!
//! 检测边沿归一化为 OfflineAlert,再由投递层 fan-out。
//! 字段 rename 对齐旧 Desktop 配置键,便于迁移。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::BotId;

/// 告警语义分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum OfflineAlertKind {
    /// 掉线且配置了自动重启
    AutoRestart,
    /// 掉线需手动处理
    Manual,
    /// 账号被踢 / 登录失效
    Kicked,
    /// 进程异常退出
    ProcessCrashed,
    /// 掉线后重新上线(默认不投递,由设置开关控制)
    Recovered,
}

/// 告警来源,便于日志与模板区分
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum OfflineAlertSource {
    NapCat,
    SnowLuma,
    Process,
}

/// 归一化的离线告警(跨渠道共用)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OfflineAlert {
    #[ts(type = "string")]
    pub bot_id: BotId,
    #[serde(rename = "qqId")]
    #[ts(type = "number")]
    pub qq_id: u64,
    #[serde(rename = "botName")]
    pub bot_name: String,
    pub kind: OfflineAlertKind,
    pub source: OfflineAlertSource,
    /// RFC3339 或本地可读时间,由投递层填入
    pub at: String,
}

impl OfflineAlert {
    pub fn event_label(&self) -> &'static str {
        match self.kind {
            OfflineAlertKind::AutoRestart | OfflineAlertKind::Manual => "offline",
            OfflineAlertKind::Kicked => "kicked",
            OfflineAlertKind::ProcessCrashed => "crashed",
            OfflineAlertKind::Recovered => "online",
        }
    }

    /// 模板变量表:同时覆盖旧 Desktop 与 Server酱风格占位符
    pub fn template_vars(&self) -> Vec<(&'static str, String)> {
        let nickname = if self.bot_name.is_empty() {
            self.qq_id.to_string()
        } else {
            self.bot_name.clone()
        };
        let event = self.event_label().to_string();
        vec![
            ("bot_name", nickname.clone()),
            ("nickname", nickname),
            ("bot_qq_id", self.qq_id.to_string()),
            ("uin", self.qq_id.to_string()),
            ("disconnect_time", self.at.clone()),
            ("time", self.at.clone()),
            ("event", event),
            ("kind", format!("{:?}", self.kind)),
            ("bot_id", self.bot_id.as_str().to_string()),
        ]
    }

    pub fn is_offline_edge(&self) -> bool {
        matches!(
            self.kind,
            OfflineAlertKind::AutoRestart
                | OfflineAlertKind::Manual
                | OfflineAlertKind::Kicked
                | OfflineAlertKind::ProcessCrashed
        )
    }
}

/// 机械 `{key}` 替换;模板是合法 JSON 时对插入值做 JSON 字符串转义
pub fn render_template(template: &str, vars: &[(&str, String)]) -> String {
    let is_json = serde_json::from_str::<serde_json::Value>(template).is_ok();
    let mut out = template.to_string();
    for (key, value) in vars {
        let needle = format!("{{{key}}}");
        let replacement = if is_json {
            value.replace('\\', "\\\\").replace('"', "\\\"")
        } else {
            value.clone()
        };
        out = out.replace(&needle, &replacement);
    }
    out
}

/// Server-chan style default webhook body (placeholders only; UI may ship localized presets)
pub fn default_webhook_body_template() -> String {
    String::from(
        "{\n  \"title\": \"Account status: {event}\",\n  \"desp\": \"Account status changed.\\n\\n**name**: {nickname}\\n**qq**: {uin}\\n**status**: {event}\\n**time**: {time}\"\n}",
    )
}

/// DingTalk robot markdown
pub fn dingtalk_webhook_body_template() -> String {
    String::from(
        "{\n  \"msgtype\": \"markdown\",\n  \"markdown\": {\n    \"title\": \"Account status: {event}\",\n    \"text\": \"Account status: {event}\\n\\n- **name**: {nickname}\\n- **qq**: {uin}\\n- **status**: {event}\\n- **time**: {time}\"\n  }\n}",
    )
}

/// Feishu text
pub fn feishu_webhook_body_template() -> String {
    String::from(
        "{\n  \"msg_type\": \"text\",\n  \"content\": {\n    \"text\": \"Account status: {event}\\nname: {nickname}\\nqq: {uin}\\ntime: {time}\"\n  }\n}",
    )
}

/// Discord inbound webhook
pub fn discord_webhook_body_template() -> String {
    String::from(
        "{\n  \"content\": null,\n  \"embeds\": [\n    {\n      \"title\": \"Account status: {event}\",\n      \"description\": \"**name**: {nickname}\\n**qq**: {uin}\\n**status**: {event}\\n**time**: {time}\",\n      \"color\": 15158332\n    }\n  ]\n}",
    )
}

/// Bark
pub fn bark_webhook_body_template() -> String {
    String::from(
        "{\n  \"title\": \"Account status: {event}\",\n  \"body\": \"name: {nickname}\\nqq: {uin}\\nstatus: {event}\\ntime: {time}\",\n  \"group\": \"NapCatQQ Desktop\"\n}",
    )
}

fn default_webhook_method() -> String {
    "POST".to_string()
}

fn default_true() -> bool {
    true
}

fn default_email_port() -> u16 {
    465
}

fn default_email_encryption() -> String {
    "SSL".to_string()
}

/// 单个 Webhook 通道
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OfflineWebhookChannel {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default = "default_webhook_body_template")]
    pub body_template: String,
    #[serde(default = "default_webhook_method")]
    pub method: String,
}

impl OfflineWebhookChannel {
    pub fn new_blank(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            enabled: true,
            url: String::new(),
            secret: String::new(),
            body_template: default_webhook_body_template(),
            method: default_webhook_method(),
        }
    }

    pub fn from_legacy(url: String, secret: String, body_template: String, method: String) -> Self {
        Self {
            id: "legacy".to_string(),
            name: "default".to_string(),
            enabled: true,
            url,
            secret,
            body_template: if body_template.trim().is_empty() {
                default_webhook_body_template()
            } else {
                body_template
            },
            method: if method.trim().is_empty() {
                default_webhook_method()
            } else {
                method
            },
        }
    }
}

/// 离线 Webhook 配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OfflineWebhookSettings {
    #[serde(rename = "WebHookUrl", default)]
    pub url: String,
    #[serde(rename = "WebHookSecret", default)]
    pub secret: String,
    #[serde(rename = "WebHookJson", default = "default_webhook_body_template")]
    pub body_template: String,
    #[serde(rename = "WebHookMethod", default = "default_webhook_method")]
    pub method: String,
    #[serde(default, rename = "channels")]
    pub channels: Vec<OfflineWebhookChannel>,
}

impl Default for OfflineWebhookSettings {
    fn default() -> Self {
        Self {
            url: String::new(),
            secret: String::new(),
            body_template: default_webhook_body_template(),
            method: default_webhook_method(),
            channels: Vec::new(),
        }
    }
}

impl OfflineWebhookSettings {
    pub fn effective_channels(&self) -> Vec<OfflineWebhookChannel> {
        if !self.channels.is_empty() {
            return self.channels.clone();
        }
        if self.url.trim().is_empty() {
            return Vec::new();
        }
        vec![OfflineWebhookChannel::from_legacy(
            self.url.clone(),
            self.secret.clone(),
            self.body_template.clone(),
            self.method.clone(),
        )]
    }

    pub fn normalize(&mut self) {
        for (i, ch) in self.channels.iter_mut().enumerate() {
            if ch.id.trim().is_empty() {
                ch.id = format!("channel-{}", i + 1);
            }
            if ch.body_template.trim().is_empty() {
                ch.body_template = default_webhook_body_template();
            }
            if ch.method.trim().is_empty() {
                ch.method = default_webhook_method();
            }
            ch.method = ch.method.to_ascii_uppercase();
            if ch.method != "GET" {
                ch.method = "POST".to_string();
            }
        }
        if self.channels.is_empty() && !self.url.trim().is_empty() {
            self.channels.push(OfflineWebhookChannel::from_legacy(
                self.url.clone(),
                self.secret.clone(),
                self.body_template.clone(),
                self.method.clone(),
            ));
            if let Some(ch) = self.channels.last_mut() {
                ch.method = ch.method.to_ascii_uppercase();
                if ch.method != "GET" {
                    ch.method = "POST".to_string();
                }
                if ch.body_template.trim().is_empty() {
                    ch.body_template = default_webhook_body_template();
                }
            }
        }
        if let Some(ch) = self
            .channels
            .iter()
            .find(|c| c.enabled && !c.url.trim().is_empty())
            .or_else(|| self.channels.first())
        {
            self.url = ch.url.clone();
            self.secret = ch.secret.clone();
            self.body_template = ch.body_template.clone();
            self.method = ch.method.clone();
        } else {
            self.method = self.method.to_ascii_uppercase();
            if self.method != "GET" {
                self.method = "POST".to_string();
            }
            if self.body_template.trim().is_empty() {
                self.body_template = default_webhook_body_template();
            }
        }
    }

    pub fn channel_by_id(&self, id: &str) -> Option<OfflineWebhookChannel> {
        self.effective_channels().into_iter().find(|c| c.id == id)
    }
}

/// 离线邮件通道(对齐旧 Email 组)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OfflineEmailSettings {
    #[serde(rename = "EmailSender", default)]
    pub sender: String,
    #[serde(rename = "EmailReceiver", default)]
    pub receiver: String,
    #[serde(rename = "EmailToken", default)]
    pub token: String,
    #[serde(rename = "EmailStmpServer", default)]
    pub smtp_server: String,
    #[serde(rename = "EmailStmpPort", default = "default_email_port")]
    pub smtp_port: u16,
    #[serde(rename = "EmailEncryption", default = "default_email_encryption")]
    pub encryption: String,
}

impl Default for OfflineEmailSettings {
    fn default() -> Self {
        Self {
            sender: String::new(),
            receiver: String::new(),
            token: String::new(),
            smtp_server: String::new(),
            smtp_port: default_email_port(),
            encryption: default_email_encryption(),
        }
    }
}

/// 用其它 Bot 的 OneBot HTTP 发私聊/群告警
///
/// 多发送方 / 多目标是主字段;单值字段只作旧配置兼容镜像。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OfflineOneBotSettings {
    #[serde(rename = "onebotNoticeEnabled", default)]
    pub enabled: bool,
    /// 旧单发送方字段;normalize 后与 messenger_bot_ids[0] 对齐
    #[serde(rename = "onebotMessengerBotId", default)]
    pub messenger_bot_id: String,
    /// 可配置多个本机 messenger,投递时按顺序取第一个可用
    #[serde(rename = "onebotMessengerBotIds", default)]
    pub messenger_bot_ids: Vec<String>,
    #[serde(rename = "onebotTargetType", default = "default_onebot_target_type")]
    pub target_type: String,
    /// 旧单目标字段;normalize 后与 target_ids[0] 对齐
    #[serde(rename = "onebotTargetId", default)]
    #[ts(type = "number")]
    pub target_id: u64,
    /// 可配置多个私聊/群目标,全部投递
    #[serde(rename = "onebotTargetIds", default)]
    #[ts(type = "Array<number>")]
    pub target_ids: Vec<u64>,
    #[serde(
        rename = "onebotMessageTemplate",
        default = "default_onebot_message_template"
    )]
    pub message_template: String,
}

fn default_onebot_target_type() -> String {
    "private".to_string()
}

fn default_onebot_message_template() -> String {
    "【掉线通知】{nickname}({uin}) 状态={event} 时间={time}".to_string()
}

impl Default for OfflineOneBotSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            messenger_bot_id: String::new(),
            messenger_bot_ids: Vec::new(),
            target_type: default_onebot_target_type(),
            target_id: 0,
            target_ids: Vec::new(),
            message_template: default_onebot_message_template(),
        }
    }
}

impl OfflineOneBotSettings {
    /// 合并旧单值字段、去重、镜像回单值,保证读写 round-trip 稳定
    pub fn normalize(&mut self) {
        let mut messengers = self
            .messenger_bot_ids
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if messengers.is_empty() {
            let legacy = self.messenger_bot_id.trim();
            if !legacy.is_empty() {
                // 旧配置偶发写成逗号分隔,一并拆开
                for part in legacy.split([',', '，', ';', ' ']) {
                    let id = part.trim();
                    if !id.is_empty() {
                        messengers.push(id.to_string());
                    }
                }
            }
        }
        messengers = dedupe_preserve_order(messengers);
        self.messenger_bot_ids = messengers;
        self.messenger_bot_id = self.messenger_bot_ids.first().cloned().unwrap_or_default();

        let mut targets = self
            .target_ids
            .iter()
            .copied()
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
        if targets.is_empty() && self.target_id > 0 {
            targets.push(self.target_id);
        }
        targets = dedupe_preserve_order(targets);
        self.target_ids = targets;
        self.target_id = self.target_ids.first().copied().unwrap_or(0);

        if !self.target_type.eq_ignore_ascii_case("group") {
            self.target_type = default_onebot_target_type();
        } else {
            self.target_type = "group".to_string();
        }
        if self.message_template.trim().is_empty() {
            self.message_template = default_onebot_message_template();
        }
    }

    pub fn effective_messenger_ids(&self) -> Vec<String> {
        let mut ids = self
            .messenger_bot_ids
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            let legacy = self.messenger_bot_id.trim();
            if !legacy.is_empty() {
                ids.push(legacy.to_string());
            }
        }
        dedupe_preserve_order(ids)
    }

    pub fn effective_target_ids(&self) -> Vec<u64> {
        let mut ids = self
            .target_ids
            .iter()
            .copied()
            .filter(|id| *id > 0)
            .collect::<Vec<_>>();
        if ids.is_empty() && self.target_id > 0 {
            ids.push(self.target_id);
        }
        dedupe_preserve_order(ids)
    }
}

fn dedupe_preserve_order<T>(items: Vec<T>) -> Vec<T>
where
    T: Eq + std::hash::Hash + Clone,
{
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}

/// 设置页可选的 OneBot 发送方候选(本机 Bot + 是否具备环回 HTTP)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OneBotMessengerCandidate {
    pub bot_id: String,
    pub name: String,
    /// running / stopped / ...
    pub state: String,
    /// napcat / snowluma
    pub backend_type: String,
    pub has_local_http: bool,
    /// 已有可用本机 HTTP 时的端口;没有则为 0
    #[ts(type = "number")]
    pub http_port: u16,
    /// Running 且具备本机 HTTP 端点时为 true
    pub eligible: bool,
    /// 缺本机 HTTP 时可一键补齐
    pub can_enable_http: bool,
}

/// 为发送方自动补齐本机 HTTP 后的结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct EnsureOneBotMessengerHttpResult {
    pub bot_id: String,
    /// created / enabled / already_ready
    pub action: String,
    #[ts(type = "number")]
    pub port: u16,
    pub candidate: OneBotMessengerCandidate,
}

fn default_notify_debounce_secs() -> u32 {
    0
}

fn default_delivery_history_limit() -> u32 {
    50
}

/// 离线通知行为:恢复通知 / 防抖 / 内存历史容量
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OfflineNotifyBehavior {
    /// 掉线后重新上线时是否投递 recovered(默认关)
    #[serde(rename = "notifyOnRecovered", default)]
    pub notify_on_recovered: bool,
    /// 同一 bot 重复 offline 边沿合并窗口(秒);0 = 关闭
    #[serde(rename = "debounceSeconds", default = "default_notify_debounce_secs")]
    pub debounce_seconds: u32,
    /// 内存投递历史条数上限;0 = 不记录
    #[serde(
        rename = "deliveryHistoryLimit",
        default = "default_delivery_history_limit"
    )]
    pub delivery_history_limit: u32,
}

impl Default for OfflineNotifyBehavior {
    fn default() -> Self {
        Self {
            notify_on_recovered: false,
            debounce_seconds: default_notify_debounce_secs(),
            delivery_history_limit: default_delivery_history_limit(),
        }
    }
}

impl OfflineNotifyBehavior {
    pub fn normalize(&mut self) {
        if self.debounce_seconds > 600 {
            self.debounce_seconds = 600;
        }
        if self.delivery_history_limit > 200 {
            self.delivery_history_limit = 200;
        }
    }
}

/// 单次渠道投递结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum OfflineDeliveryChannelResult {
    Ok,
    Failed,
    Skipped,
}

/// 内存中的一条投递历史(不落盘)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OfflineDeliveryRecord {
    #[ts(type = "string")]
    pub bot_id: BotId,
    #[serde(rename = "botName")]
    pub bot_name: String,
    pub kind: OfflineAlertKind,
    pub source: OfflineAlertSource,
    pub at: String,
    pub toast: OfflineDeliveryChannelResult,
    pub webhook: OfflineDeliveryChannelResult,
    pub email: OfflineDeliveryChannelResult,
    pub onebot: OfflineDeliveryChannelResult,
    /// 是否被防抖吞掉(整条未 fan-out)
    #[serde(default)]
    pub debounced: bool,
    /// 简短说明(失败原因摘要等)
    #[serde(default)]
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_serverchan_template_escapes_json() {
        let alert = OfflineAlert {
            bot_id: BotId::new("10001"),
            qq_id: 10001,
            bot_name: r#"foo"bar"#.to_string(),
            kind: OfflineAlertKind::Manual,
            source: OfflineAlertSource::NapCat,
            at: "2026-07-09T12:00:00Z".to_string(),
        };
        let body = render_template(&default_webhook_body_template(), &alert.template_vars());
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid json");
        assert!(v["title"].as_str().unwrap().contains("offline"));
        assert!(v["desp"].as_str().unwrap().contains("10001"));
    }

    #[test]
    fn recovered_event_label_is_online() {
        let alert = OfflineAlert {
            bot_id: BotId::new("1"),
            qq_id: 1,
            bot_name: "n".into(),
            kind: OfflineAlertKind::Recovered,
            source: OfflineAlertSource::NapCat,
            at: "t".into(),
        };
        assert_eq!(alert.event_label(), "online");
    }

    #[test]
    fn behavior_defaults_and_clamp() {
        let mut b = OfflineNotifyBehavior::default();
        assert!(!b.notify_on_recovered);
        assert_eq!(b.debounce_seconds, 0);
        assert_eq!(b.delivery_history_limit, 50);
        b.debounce_seconds = 9999;
        b.delivery_history_limit = 9999;
        b.normalize();
        assert_eq!(b.debounce_seconds, 600);
        assert_eq!(b.delivery_history_limit, 200);
    }

    #[test]
    fn webhook_settings_default_roundtrip() {
        let raw = serde_json::to_string(&OfflineWebhookSettings::default()).unwrap();
        let parsed: OfflineWebhookSettings = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed, OfflineWebhookSettings::default());
    }

    #[test]
    fn onebot_legacy_single_fields_expand_on_normalize() {
        let mut settings = OfflineOneBotSettings {
            enabled: true,
            messenger_bot_id: "10002, 10003".into(),
            messenger_bot_ids: Vec::new(),
            target_type: "group".into(),
            target_id: 20001,
            target_ids: Vec::new(),
            message_template: String::new(),
        };
        settings.normalize();
        assert_eq!(
            settings.messenger_bot_ids,
            vec!["10002".to_string(), "10003".to_string()]
        );
        assert_eq!(settings.messenger_bot_id, "10002");
        assert_eq!(settings.target_ids, vec![20001]);
        assert_eq!(settings.target_id, 20001);
        assert_eq!(settings.target_type, "group");
        assert!(!settings.message_template.is_empty());
    }

    #[test]
    fn onebot_multi_fields_dedupe_and_mirror() {
        let mut settings = OfflineOneBotSettings {
            enabled: true,
            messenger_bot_id: "legacy".into(),
            messenger_bot_ids: vec![" 10002 ".into(), "10003".into(), "10002".into(), "".into()],
            target_type: "PRIVATE".into(),
            target_id: 9,
            target_ids: vec![0, 30001, 30002, 30001],
            message_template: "hi {uin}".into(),
        };
        settings.normalize();
        assert_eq!(
            settings.messenger_bot_ids,
            vec!["10002".to_string(), "10003".to_string()]
        );
        assert_eq!(settings.messenger_bot_id, "10002");
        assert_eq!(settings.target_ids, vec![30001, 30002]);
        assert_eq!(settings.target_id, 30001);
        assert_eq!(settings.target_type, "private");
        assert_eq!(settings.message_template, "hi {uin}");
    }
}
