//! Poller 配置、状态、依赖与句柄

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::loop_::run_poller;
use crate::napcat::offline_notifier::OfflineNotifier;
use crate::napcat::webui_client::NapCatWebUiClient;
use ncd_domain::ids::BotId;
use ncd_traits::events::BroadcastEventBus;

/// 单个 Poller 的运行参数
#[derive(Debug, Clone)]
pub struct PollerConfig {
    pub login_check_interval: Duration,
    pub unlogged_interval: Duration,
    pub auth_refresh_period: Duration,
    pub auth_refresh_throttle: Duration,
    pub http_timeout: Duration,
    pub offline_auto_restart: bool,
    pub offline_notice_enabled: bool,
}

impl Default for PollerConfig {
    fn default() -> Self {
        Self {
            login_check_interval: Duration::from_millis(5000),
            unlogged_interval: Duration::from_secs(1),
            auth_refresh_period: Duration::from_secs(30 * 60),
            auth_refresh_throttle: Duration::from_secs(5),
            http_timeout: Duration::from_secs(5),
            offline_auto_restart: false,
            offline_notice_enabled: false,
        }
    }
}

/// Poller 主循环维护的登录子状态
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct LoginState {
    pub auth: Option<String>,
    pub is_logged_in: bool,
    pub online: bool,
    pub offline_notice_sent: bool,
    pub login_invalidated_while_online: bool,
    pub suppress_qrcode_until_online: bool,
    /// 连续离线样本；在线→离线需两个样本确认，避免瞬时 selfInfo 抖动。
    pub consecutive_offline_observations: u32,
    /// 连续没有拿到任何在线信号的轮数。
    pub consecutive_probe_failures: u32,
    /// 避免探测持续失败时重复广播 unknown。
    pub probe_unavailable_published: bool,
    /// None = 从未尝试,首次必过节流(避免 Instant::checked_sub 短 uptime 失败)
    pub last_auth_refresh_attempt_at: Option<Instant>,
}

impl LoginState {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            auth: None,
            is_logged_in: false,
            online: false,
            offline_notice_sent: false,
            login_invalidated_while_online: false,
            suppress_qrcode_until_online: false,
            consecutive_offline_observations: 0,
            consecutive_probe_failures: 0,
            probe_unavailable_published: false,
            last_auth_refresh_attempt_at: None,
        }
    }

    pub(crate) fn auth_refresh_throttle_elapsed(&self, throttle: Duration) -> bool {
        match self.last_auth_refresh_attempt_at {
            None => true,
            Some(at) => at.elapsed() >= throttle,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum PollerCommand {
    RequestAuthRefresh,
    UpdateInterval(Duration),
}

#[allow(dead_code)]
pub struct PollerDeps {
    pub event_bus: Arc<BroadcastEventBus>,
    pub http: Arc<dyn NapCatWebUiClient>,
    pub notifier: Arc<dyn OfflineNotifier>,
    pub restart_handle: Arc<dyn RestartHandle>,
}

#[async_trait]
pub trait RestartHandle: Send + Sync {
    async fn restart_bot(&self, bot_id: &BotId);
}

#[allow(dead_code)]
pub struct NapCatLoginPoller {
    // pub(crate): 单测可直接构造句柄验证 dispose/Drop
    pub(crate) bot_id: BotId,
    pub(crate) cancel: CancellationToken,
}

impl NapCatLoginPoller {
    pub fn spawn(
        bot_id: BotId,
        port: u16,
        token: String,
        config: PollerConfig,
        deps: PollerDeps,
    ) -> Self {
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        let bot_id_for_task = bot_id.clone();
        tokio::spawn(async move {
            run_poller(bot_id_for_task, port, token, config, deps, cancel_for_task).await;
        });
        Self { bot_id, cancel }
    }

    pub fn dispose(&self) {
        self.cancel.cancel();
    }

    pub fn bot_id(&self) -> &BotId {
        &self.bot_id
    }
}

impl Drop for NapCatLoginPoller {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
