//! SnowLuma UI 会话态内存表
//!
//! 冷启动 bootstrap reconcile 会立刻发 snowluma_* 事件,但 broadcast 无 backlog,
//! 前端 snowlumaStore 又晚于 reconcile 才 subscribe,事件全丢 → 再开软件 Running
//! 但登录态/WebUI 按钮全空。
//!
//! 本表由 BotManager 镜像 SL 相关 DomainEvent,供 list_snowluma_ui_snapshot
//! 给前端 hydrate(对齐 NapCatEndpointTable + list_napcat_webui_bindings)。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use ncd_domain::daemon_state::{DaemonState, SnowLumaLoginState};
use ncd_domain::ids::BotId;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SnowLumaUiBotSnapshot {
    pub injected: bool,
    pub uin: Option<String>,
    pub login_state: Option<SnowLumaLoginState>,
    /// Docker 或远端 Native 隧道就绪后 UI 可开 WebUI
    pub endpoints_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SnowLumaUiSnapshot {
    /// `local` 表示本机；其它 key 为 SSH server_id。不同主机 daemon 互不覆盖。
    pub daemon_states: HashMap<String, DaemonState>,
    pub by_bot: HashMap<BotId, SnowLumaUiBotSnapshot>,
}

#[derive(Debug, Clone, Default)]
pub struct SnowLumaUiStateTable {
    inner: Arc<RwLock<SnowLumaUiSnapshot>>,
}

impl SnowLumaUiStateTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn set_daemon_state(&self, scope: impl Into<String>, state: DaemonState) {
        let mut guard = self.inner.write().await;
        guard.daemon_states.insert(scope.into(), state);
    }

    pub async fn mark_injected(&self, bot_id: &BotId) {
        let mut guard = self.inner.write().await;
        guard.by_bot.entry(bot_id.clone()).or_default().injected = true;
    }

    pub async fn set_uin(&self, bot_id: &BotId, uin: String) {
        let mut guard = self.inner.write().await;
        guard.by_bot.entry(bot_id.clone()).or_default().uin = Some(uin);
    }

    pub async fn set_login_state(&self, bot_id: &BotId, state: SnowLumaLoginState) {
        let mut guard = self.inner.write().await;
        guard.by_bot.entry(bot_id.clone()).or_default().login_state = Some(state);
    }

    pub async fn clear_login_state(&self, bot_id: &BotId) {
        let mut guard = self.inner.write().await;
        guard.by_bot.entry(bot_id.clone()).or_default().login_state = None;
    }

    pub async fn mark_endpoints_ready(&self, bot_id: &BotId) {
        let mut guard = self.inner.write().await;
        guard
            .by_bot
            .entry(bot_id.clone())
            .or_default()
            .endpoints_ready = true;
    }

    pub async fn clear_bot(&self, bot_id: &BotId) {
        self.inner.write().await.by_bot.remove(bot_id);
    }

    pub async fn snapshot(&self) -> SnowLumaUiSnapshot {
        self.inner.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mirror_and_clear_bot() {
        let t = SnowLumaUiStateTable::new();
        let bot = BotId::new("10001");
        t.set_daemon_state("local", DaemonState::Ready).await;
        t.mark_injected(&bot).await;
        t.mark_endpoints_ready(&bot).await;
        t.set_login_state(&bot, SnowLumaLoginState::LoggedIn).await;
        t.set_uin(&bot, "10001".into()).await;

        let snap = t.snapshot().await;
        assert_eq!(snap.daemon_states.get("local"), Some(&DaemonState::Ready));
        let b = snap.by_bot.get(&bot).expect("bot");
        assert!(b.injected);
        assert!(b.endpoints_ready);
        assert_eq!(b.login_state, Some(SnowLumaLoginState::LoggedIn));
        assert_eq!(b.uin.as_deref(), Some("10001"));

        t.clear_bot(&bot).await;
        assert!(!t.snapshot().await.by_bot.contains_key(&bot));
    }

    #[tokio::test]
    async fn daemon_states_are_isolated_by_host_scope() {
        let t = SnowLumaUiStateTable::new();
        let bot = BotId::new("10001");
        t.mark_injected(&bot).await;
        t.mark_endpoints_ready(&bot).await;
        t.set_login_state(&bot, SnowLumaLoginState::LoggedIn).await;
        t.set_daemon_state("server-a", DaemonState::Crashed).await;
        t.set_daemon_state("server-b", DaemonState::Ready).await;
        let b = t.snapshot().await.by_bot.get(&bot).cloned().unwrap();
        assert!(b.injected);
        assert_eq!(b.login_state, Some(SnowLumaLoginState::LoggedIn));
        assert!(b.endpoints_ready);
        let snap = t.snapshot().await;
        assert_eq!(
            snap.daemon_states.get("server-a"),
            Some(&DaemonState::Crashed)
        );
        assert_eq!(
            snap.daemon_states.get("server-b"),
            Some(&DaemonState::Ready)
        );
    }
}
