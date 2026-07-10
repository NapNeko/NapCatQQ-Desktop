//! 组合投递:桌面 Toast + Webhook + Email + OneBot
//!
//! 单渠道失败只记日志,不阻断其它渠道与检测路径。
//! 支持 recovered 门控、offline 边沿防抖、内存投递历史。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ncd_domain::ids::BotId;
use ncd_domain::{
    DesktopNotifySettings, OfflineAlert, OfflineAlertKind, OfflineDeliveryChannelResult,
    OfflineDeliveryRecord, OfflineEmailSettings, OfflineOneBotSettings, OfflineWebhookSettings,
    WebUiPollerSettings,
};
use tokio::sync::RwLock;

use crate::napcat::offline_notifier::{OfflineNoticeKind, OfflineNotifier};

use super::email::send_offline_email;
use super::onebot::OneBotMessenger;
use super::webhook::send_offline_webhook;

/// 解析 messenger bot 的 HTTP 端点(由 wiring 注入)
#[async_trait]
pub trait OneBotEndpointResolver: Send + Sync {
    async fn resolve(
        &self,
        messenger_bot_id: &str,
        exclude_bot_id: &BotId,
    ) -> Option<OneBotMessenger>;
}

pub struct NoopOneBotEndpointResolver;

#[async_trait]
impl OneBotEndpointResolver for NoopOneBotEndpointResolver {
    async fn resolve(
        &self,
        _messenger_bot_id: &str,
        _exclude_bot_id: &BotId,
    ) -> Option<OneBotMessenger> {
        None
    }
}

/// 可在 BotManager 建好后再注入真实 resolver
pub struct SwappableOneBotEndpointResolver {
    inner: RwLock<Arc<dyn OneBotEndpointResolver>>,
}

impl SwappableOneBotEndpointResolver {
    pub fn new(initial: Arc<dyn OneBotEndpointResolver>) -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(initial),
        })
    }

    pub async fn set(&self, next: Arc<dyn OneBotEndpointResolver>) {
        *self.inner.write().await = next;
    }
}

#[async_trait]
impl OneBotEndpointResolver for SwappableOneBotEndpointResolver {
    async fn resolve(
        &self,
        messenger_bot_id: &str,
        exclude_bot_id: &BotId,
    ) -> Option<OneBotMessenger> {
        let guard = self.inner.read().await;
        guard.resolve(messenger_bot_id, exclude_bot_id).await
    }
}

/// 桌面 Toast 适配:把 OfflineAlert 转成既有 OfflineNoticeKind 语义
pub struct DesktopToastSink {
    inner: Arc<dyn OfflineNotifier>,
    flags: Arc<RwLock<DesktopNotifySettings>>,
}

impl DesktopToastSink {
    pub fn new(inner: Arc<dyn OfflineNotifier>, flags: Arc<RwLock<DesktopNotifySettings>>) -> Self {
        Self { inner, flags }
    }
}

struct DebounceState {
    last_offline_at: HashMap<String, Instant>,
}

impl DebounceState {
    fn new() -> Self {
        Self {
            last_offline_at: HashMap::new(),
        }
    }
}

pub struct CompositeOfflineNotifier {
    toast: Option<DesktopToastSink>,
    poller: Arc<RwLock<WebUiPollerSettings>>,
    webhook: Arc<RwLock<OfflineWebhookSettings>>,
    email: Arc<RwLock<OfflineEmailSettings>>,
    onebot: Arc<RwLock<OfflineOneBotSettings>>,
    onebot_resolver: Arc<dyn OneBotEndpointResolver>,
    debounce: RwLock<DebounceState>,
    history: RwLock<VecDeque<OfflineDeliveryRecord>>,
}

impl CompositeOfflineNotifier {
    pub fn new(
        toast: Option<DesktopToastSink>,
        poller: Arc<RwLock<WebUiPollerSettings>>,
        webhook: Arc<RwLock<OfflineWebhookSettings>>,
        email: Arc<RwLock<OfflineEmailSettings>>,
        onebot: Arc<RwLock<OfflineOneBotSettings>>,
        onebot_resolver: Arc<dyn OneBotEndpointResolver>,
    ) -> Arc<Self> {
        Arc::new(Self {
            toast,
            poller,
            webhook,
            email,
            onebot,
            onebot_resolver,
            debounce: RwLock::new(DebounceState::new()),
            history: RwLock::new(VecDeque::new()),
        })
    }

    pub async fn update_from_app_settings(
        &self,
        poller: WebUiPollerSettings,
        webhook: OfflineWebhookSettings,
        email: OfflineEmailSettings,
        onebot: OfflineOneBotSettings,
        desktop: DesktopNotifySettings,
    ) {
        *self.poller.write().await = poller;
        *self.webhook.write().await = webhook;
        *self.email.write().await = email;
        *self.onebot.write().await = onebot;
        if let Some(toast) = &self.toast {
            *toast.flags.write().await = desktop;
        }
    }

    /// 最近投递记录(新→旧)
    pub async fn delivery_history(&self) -> Vec<OfflineDeliveryRecord> {
        self.history.read().await.iter().cloned().collect()
    }

    pub async fn clear_delivery_history(&self) {
        self.history.write().await.clear();
    }

    /// 投递完整告警(推荐入口)
    pub async fn deliver(&self, alert: OfflineAlert) {
        let poller = self.poller.read().await.clone();
        let behavior = poller.offline_notify_behavior.clone();

        if matches!(alert.kind, OfflineAlertKind::Recovered) && !behavior.notify_on_recovered {
            tracing::debug!(bot_id = %alert.bot_id, "offline recovered skipped: disabled");
            return;
        }

        if alert.is_offline_edge() && behavior.debounce_seconds > 0 {
            let window = Duration::from_secs(behavior.debounce_seconds as u64);
            let mut deb = self.debounce.write().await;
            let key = alert.bot_id.as_str().to_string();
            if let Some(prev) = deb.last_offline_at.get(&key) {
                if prev.elapsed() < window {
                    let rec = OfflineDeliveryRecord {
                        bot_id: alert.bot_id.clone(),
                        bot_name: alert.bot_name.clone(),
                        kind: alert.kind,
                        source: alert.source,
                        at: alert.at.clone(),
                        toast: OfflineDeliveryChannelResult::Skipped,
                        webhook: OfflineDeliveryChannelResult::Skipped,
                        email: OfflineDeliveryChannelResult::Skipped,
                        onebot: OfflineDeliveryChannelResult::Skipped,
                        debounced: true,
                        note: format!("debounced within {}s", behavior.debounce_seconds),
                    };
                    drop(deb);
                    self.push_history(rec, behavior.delivery_history_limit)
                        .await;
                    tracing::debug!(bot_id = %alert.bot_id, "offline alert debounced");
                    return;
                }
            }
            deb.last_offline_at.insert(key, Instant::now());
        }

        if matches!(alert.kind, OfflineAlertKind::Recovered) {
            // 恢复后清防抖,允许下次 offline 立即报
            self.debounce
                .write()
                .await
                .last_offline_at
                .remove(alert.bot_id.as_str());
        }

        let mut toast_r = OfflineDeliveryChannelResult::Skipped;
        let mut webhook_r = OfflineDeliveryChannelResult::Skipped;
        let mut email_r = OfflineDeliveryChannelResult::Skipped;
        let mut onebot_r = OfflineDeliveryChannelResult::Skipped;
        let mut note = String::new();

        if let Some(toast) = &self.toast {
            match alert.kind {
                OfflineAlertKind::AutoRestart => {
                    toast
                        .inner
                        .notify(&alert.bot_id, OfflineNoticeKind::AutoRestart)
                        .await;
                    toast_r = OfflineDeliveryChannelResult::Ok;
                }
                OfflineAlertKind::Manual => {
                    toast
                        .inner
                        .notify(&alert.bot_id, OfflineNoticeKind::Manual)
                        .await;
                    toast_r = OfflineDeliveryChannelResult::Ok;
                }
                OfflineAlertKind::Recovered => {
                    toast
                        .inner
                        .notify(&alert.bot_id, OfflineNoticeKind::Recovered)
                        .await;
                    toast_r = OfflineDeliveryChannelResult::Ok;
                }
                OfflineAlertKind::Kicked | OfflineAlertKind::ProcessCrashed => {}
            }
        }

        let webhook_cfg = self.webhook.read().await.clone();
        let email_cfg = self.email.read().await.clone();
        let onebot_cfg = self.onebot.read().await.clone();

        if poller.offline_webhook_notice {
            match send_offline_webhook(&webhook_cfg, &alert).await {
                Ok(()) => {
                    tracing::info!(bot_id = %alert.bot_id, "offline webhook sent");
                    webhook_r = OfflineDeliveryChannelResult::Ok;
                }
                Err(err) => {
                    tracing::warn!(bot_id = %alert.bot_id, %err, "offline webhook failed");
                    webhook_r = OfflineDeliveryChannelResult::Failed;
                    push_note(&mut note, &format!("webhook: {err}"));
                }
            }
        }

        if poller.offline_email_notice {
            let alert_clone = alert.clone();
            let email_cfg = email_cfg.clone();
            match tokio::task::spawn_blocking(move || send_offline_email(&email_cfg, &alert_clone))
                .await
            {
                Ok(Ok(())) => {
                    tracing::info!(bot_id = %alert.bot_id, "offline email sent");
                    email_r = OfflineDeliveryChannelResult::Ok;
                }
                Ok(Err(err)) => {
                    tracing::warn!(bot_id = %alert.bot_id, %err, "offline email failed");
                    email_r = OfflineDeliveryChannelResult::Failed;
                    push_note(&mut note, &format!("email: {err}"));
                }
                Err(err) => {
                    tracing::warn!(bot_id = %alert.bot_id, %err, "offline email task join failed");
                    email_r = OfflineDeliveryChannelResult::Failed;
                    push_note(&mut note, "email: join failed");
                }
            }
        }

        if onebot_cfg.enabled {
            let messenger_ids = onebot_cfg.effective_messenger_ids();
            if messenger_ids.is_empty() {
                tracing::debug!(
                    bot_id = %alert.bot_id,
                    "offline onebot skipped: no messenger configured"
                );
                onebot_r = OfflineDeliveryChannelResult::Skipped;
            } else {
                let mut resolved = None;
                for messenger_id in &messenger_ids {
                    if let Some(messenger) = self
                        .onebot_resolver
                        .resolve(messenger_id, &alert.bot_id)
                        .await
                    {
                        resolved = Some((messenger_id.clone(), messenger));
                        break;
                    }
                }
                if let Some((messenger_id, messenger)) = resolved {
                    match messenger.send_alert(&onebot_cfg, &alert).await {
                        Ok(()) => {
                            tracing::info!(
                                bot_id = %alert.bot_id,
                                messenger = %messenger_id,
                                "offline onebot sent"
                            );
                            onebot_r = OfflineDeliveryChannelResult::Ok;
                        }
                        Err(err) => {
                            tracing::warn!(
                                bot_id = %alert.bot_id,
                                messenger = %messenger_id,
                                %err,
                                "offline onebot failed"
                            );
                            onebot_r = OfflineDeliveryChannelResult::Failed;
                            push_note(&mut note, &format!("onebot: {err}"));
                        }
                    }
                } else {
                    tracing::debug!(
                        bot_id = %alert.bot_id,
                        candidates = messenger_ids.len(),
                        "offline onebot skipped: no messenger endpoint available"
                    );
                    onebot_r = OfflineDeliveryChannelResult::Skipped;
                }
            }
        }

        let rec = OfflineDeliveryRecord {
            bot_id: alert.bot_id,
            bot_name: alert.bot_name,
            kind: alert.kind,
            source: alert.source,
            at: alert.at,
            toast: toast_r,
            webhook: webhook_r,
            email: email_r,
            onebot: onebot_r,
            debounced: false,
            note,
        };
        self.push_history(rec, behavior.delivery_history_limit)
            .await;
    }

    async fn push_history(&self, rec: OfflineDeliveryRecord, limit: u32) {
        if limit == 0 {
            return;
        }
        let mut hist = self.history.write().await;
        hist.push_front(rec);
        while hist.len() > limit as usize {
            hist.pop_back();
        }
    }
}

fn push_note(note: &mut String, part: &str) {
    if note.is_empty() {
        *note = part.to_string();
    } else {
        note.push_str("; ");
        note.push_str(part);
    }
}

#[async_trait]
impl OfflineNotifier for CompositeOfflineNotifier {
    async fn notify(&self, bot_id: &BotId, kind: OfflineNoticeKind) {
        let qq_id: u64 = bot_id.as_str().parse().unwrap_or(0);
        let alert_kind = match kind {
            OfflineNoticeKind::AutoRestart => OfflineAlertKind::AutoRestart,
            OfflineNoticeKind::Manual => OfflineAlertKind::Manual,
            OfflineNoticeKind::Recovered => OfflineAlertKind::Recovered,
        };
        let alert = OfflineAlert {
            bot_id: bot_id.clone(),
            qq_id,
            bot_name: String::new(),
            kind: alert_kind,
            source: ncd_domain::OfflineAlertSource::NapCat,
            at: chrono::Local::now().format("%Y/%m/%d %H:%M:%S").to_string(),
        };
        self.deliver(alert).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::OfflineAlertSource;

    fn sample_alert(kind: OfflineAlertKind) -> OfflineAlert {
        OfflineAlert {
            bot_id: BotId::new("10001"),
            qq_id: 10001,
            bot_name: "t".into(),
            kind,
            source: OfflineAlertSource::NapCat,
            at: "now".into(),
        }
    }

    #[tokio::test]
    async fn recovered_skipped_when_disabled() {
        let poller = Arc::new(RwLock::new(WebUiPollerSettings::default()));
        let n = CompositeOfflineNotifier::new(
            None,
            Arc::clone(&poller),
            Arc::new(RwLock::new(OfflineWebhookSettings::default())),
            Arc::new(RwLock::new(OfflineEmailSettings::default())),
            Arc::new(RwLock::new(OfflineOneBotSettings::default())),
            Arc::new(NoopOneBotEndpointResolver),
        );
        n.deliver(sample_alert(OfflineAlertKind::Recovered)).await;
        assert!(n.delivery_history().await.is_empty());
    }

    #[tokio::test]
    async fn debounce_swallows_second_offline() {
        let mut poller_cfg = WebUiPollerSettings::default();
        poller_cfg.offline_notify_behavior.debounce_seconds = 60;
        poller_cfg.offline_notify_behavior.delivery_history_limit = 10;
        let n = CompositeOfflineNotifier::new(
            None,
            Arc::new(RwLock::new(poller_cfg)),
            Arc::new(RwLock::new(OfflineWebhookSettings::default())),
            Arc::new(RwLock::new(OfflineEmailSettings::default())),
            Arc::new(RwLock::new(OfflineOneBotSettings::default())),
            Arc::new(NoopOneBotEndpointResolver),
        );
        n.deliver(sample_alert(OfflineAlertKind::Manual)).await;
        n.deliver(sample_alert(OfflineAlertKind::Manual)).await;
        let hist = n.delivery_history().await;
        assert_eq!(hist.len(), 2);
        assert!(hist[0].debounced);
        assert!(!hist[1].debounced);
    }

    #[tokio::test]
    async fn history_respects_limit() {
        let mut poller_cfg = WebUiPollerSettings::default();
        poller_cfg.offline_notify_behavior.delivery_history_limit = 2;
        let n = CompositeOfflineNotifier::new(
            None,
            Arc::new(RwLock::new(poller_cfg)),
            Arc::new(RwLock::new(OfflineWebhookSettings::default())),
            Arc::new(RwLock::new(OfflineEmailSettings::default())),
            Arc::new(RwLock::new(OfflineOneBotSettings::default())),
            Arc::new(NoopOneBotEndpointResolver),
        );
        for _ in 0..3 {
            n.deliver(sample_alert(OfflineAlertKind::Manual)).await;
        }
        assert_eq!(n.delivery_history().await.len(), 2);
    }
}
