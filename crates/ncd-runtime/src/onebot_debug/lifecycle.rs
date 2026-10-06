//! 跟着 Bot 的生命周期收拾调试台：Bot 停了就停掉它的接收器、丢掉会话；SnowLuma 账号登录后
//! 让内部通道的接收器重连（上游只给连接时已在线的账号挂钩子）；NapCat WebUI 换了端口或令牌时
//! 丢掉旧客户端。
//!
//! 监听任务只拿管理器的弱引用，并盯着管理器的关闭令牌：管理器被丢弃时立刻退出，不吊着它。
//!
//! 会话的空闲回收也在这里：没有接收器、30 分钟没人用的会话整个丢掉，连接和隧道随之关闭。

use std::sync::Arc;

use tokio::time::Instant;
use tracing::debug;

use ncd_domain::bot_actor::BotActorState;
use ncd_domain::daemon_state::SnowLumaLoginState;
use ncd_domain::ids::BotId;

use super::DebugManager;
use super::receiver::{IDLE_STOP_AFTER, REASON_BOT_STOPPED};
use super::session::BotSession;
use crate::events::{DomainEvent, EventBus, EventFilter};

/// BotManager 删除 Bot 时发的 BotStateChanged 带的原因
const REASON_BOT_DELETED: &str = "bot_deleted";

impl DebugManager {
    /// 订阅领域事件总线，按 Bot 的状态变化收拾接收器和会话。启动时 spawn 一次
    pub async fn run_bot_event_listener(self: Arc<Self>, bus: Arc<dyn EventBus>) {
        let mut events = bus.subscribe(EventFilter::all());
        let shutdown = self.shutdown.clone();
        let manager = Arc::downgrade(&self);
        drop(self);
        loop {
            let event = tokio::select! {
                biased;
                _ = shutdown.cancelled() => return,
                event = events.next() => event,
            };
            let Some(event) = event else {
                return;
            };
            let Some(manager) = manager.upgrade() else {
                return;
            };
            manager.on_bot_event(&event).await;
        }
    }

    pub(super) async fn on_bot_event(&self, event: &DomainEvent) {
        match event {
            DomainEvent::BotStateChanged { snapshot, reason } => {
                let gone = matches!(
                    snapshot.state,
                    BotActorState::Stopped | BotActorState::Crashed
                ) || reason.as_deref() == Some(REASON_BOT_DELETED);
                if gone {
                    // 先记「停过」、再停接收器：正在订阅的那一方在接收器表锁里查「停过没有」，
                    // 查的时候还没记上，它建的接收器就一定赶在这里停接收器之前进了表，会被一起停掉
                    self.drop_session(&snapshot.bot_id).await;
                    self.stop_receiver_with(&snapshot.bot_id, REASON_BOT_STOPPED);
                }
            }
            DomainEvent::SnowLumaLoginStateChanged {
                bot_id,
                state: SnowLumaLoginState::LoggedIn,
            }
            | DomainEvent::SnowLumaUinDetected { bot_id, .. } => {
                self.kick_internal_receiver(bot_id);
            }
            DomainEvent::NapCatWebuiAvailable { bot_id, .. } => {
                self.forget_napcat_client(bot_id).await;
                // 接收器可能正因为 WebUI 还没就绪在等重连，不必等满退避
                self.hurry_internal_receiver(bot_id);
            }
            DomainEvent::HostConnectionRecovered { server_id, .. } => {
                for view in self.bots.list_bots().await {
                    if matches!(&view.config.bot.runtime_target, ncd_domain::RuntimeTarget::Server(id) if id == server_id)
                    {
                        self.hurry_internal_receiver(&view.bot_id());
                    }
                }
            }
            _ => {}
        }
    }

    /// Bot 停了：它的连接、隧道、通道状态都没用了。关连接放在锁外。
    /// 在同一把锁里记下「停过」：半路的探测 / 调用回来时不会再把会话和隧道建回来
    pub(super) async fn drop_session(&self, bot_id: &BotId) {
        let session: Option<BotSession> = {
            let mut sessions = self.sessions.lock().await;
            self.note_bot_stopped(bot_id);
            sessions.remove(bot_id)
        };
        if let Some(mut session) = session {
            session.shutdown();
        }
    }

    /// 丢掉没有接收器、且超过 30 分钟没人用的会话。关连接放在锁外
    pub(super) async fn sweep_idle_sessions(&self) {
        let watched = self.bots_with_receivers();
        let now = Instant::now();
        let idle: Vec<(BotId, BotSession)> = {
            let mut sessions = self.sessions.lock().await;
            let ids: Vec<BotId> = sessions
                .iter()
                .filter(|(id, session)| {
                    !watched.contains(*id) && session.idle_longer_than(IDLE_STOP_AFTER, now)
                })
                .map(|(id, _)| id.clone())
                .collect();
            ids.into_iter()
                .filter_map(|id| sessions.remove(&id).map(|session| (id, session)))
                .collect()
        };
        for (id, mut session) in idle {
            debug!(bot_id = %id, "调试台：会话 30 分钟没人用，关掉它的连接和隧道");
            session.shutdown();
        }
    }

    /// NapCat 重启后 WebUI 的端口和令牌可能都变了，旧客户端的凭据也跟着作废
    async fn forget_napcat_client(&self, bot_id: &BotId) {
        if let Some(session) = self.sessions.lock().await.get_mut(bot_id) {
            session.napcat = None;
        }
    }
}
