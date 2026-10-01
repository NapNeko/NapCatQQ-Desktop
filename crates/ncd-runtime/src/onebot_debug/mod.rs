//! OneBot 调试台的编排层：列出能调试的 Bot 和它们的通道、打通道（必要时开 SSH 隧道）、
//! 发起可取消 / 有时限的动作调用、合并动作目录、按 Bot 收事件流，以及工作区 / 收藏夹 / 调用历史的落盘。
//!
//! 调试台的可变状态都挂在 [`DebugManager`] 实例上：每个 Bot 一份会话（客户端、隧道、
//! 通道状态、目录缓存）、在途调用的取消令牌、最近几次超大回包的全文。会话表的锁只在
//! 读写表项时短暂持有，建连、登录、开隧道这些网络操作都先把需要的 `Arc` 克隆出来再做。
//!
//! 因为网络操作不持锁，`close_all` 时可能还有探测 / 调用 / 取目录在半路。每个操作开始时
//! 记下当前的「一轮」（[`Epoch`]）：`close_all` 先换新一轮并取消旧一轮的令牌，半路的操作
//! 立刻收手；万一已经拿到了连接或隧道，回来写会话表时也会发现自己属于旧一轮，把东西丢掉。
//! 事件接收器的停止令牌是「这一轮」令牌的子令牌，同样随 `close_all` 收工。

mod calls;
mod catalog;
mod errors;
mod lifecycle;
mod params;
mod persist;
pub mod plan;
mod port;
mod receiver;
mod session;
mod storage;
mod stream;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, PoisonError};

use ncd_domain::RuntimeScenario;
use ncd_domain::bot_config::{BackendType, BotConfig, DeploymentType};
use ncd_domain::ids::BotId;
use ncd_domain::onebot_debug::{
    DebugChannelId, DebugChannelInfo, DebugChannelStatus, DebugChannels, DebugError, DebugHost,
    DebugTarget, mask_token,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::host_resolver::HostResolver;
use catalog::SnapshotCatalogs;
use persist::{HistoryQueue, LazyStore};
use receiver::{OwnCalls, REASON_CLOSED, ReceiverTable};
use session::BotSession;

pub use errors::error_text;
pub use plan::{ChannelPlan, Reach, pick_auto, plan_channels};
pub use port::{DebugBotPort, DebugBotView, DebugEventSink};
pub use stream::DebugStreamSink;

/// 只保留最近这么多次超大回包的全文，够「另存为」用，又不至于把内存吃满
const LARGE_RESPONSES_KEPT: usize = 3;
const NO_CALL_CHANNEL: &str = "没有能用的调用通道";
const NO_SUCH_CHANNEL: &str = "这个 Bot 没有这条通道（可能已在连接配置里删掉或停用）";

/// 会话表的「一轮」：`close_all` 换新一轮，并取消旧一轮的令牌
#[derive(Clone)]
pub(crate) struct Epoch {
    generation: u64,
    token: CancellationToken,
    /// 操作开始时「Bot 停止」计数走到了几。某个 Bot 在这之后停过，这个操作迟到的写入
    /// 就不能再给它建会话（见 [`DebugManager::live_session`]）
    bot_stops: u64,
}

impl Epoch {
    fn first() -> Self {
        Self::next_after(None)
    }

    fn next_after(previous: Option<&Epoch>) -> Self {
        Self {
            generation: previous.map_or(0, |e| e.generation.wrapping_add(1)),
            token: CancellationToken::new(),
            bot_stops: 0,
        }
    }

    /// 这一轮被 `close_all` 结束时完成
    pub(crate) async fn ended(&self) {
        self.token.cancelled().await;
    }
}

pub struct DebugManager {
    bots: Arc<dyn DebugBotPort>,
    host_resolver: Arc<dyn HostResolver>,
    enabled: AtomicBool,
    /// 当前这一轮。只在很短的临界区里读或换，从不跨 await
    epoch: StdMutex<Epoch>,
    sessions: Mutex<HashMap<BotId, BotSession>>,
    /// 在途调用的取消令牌，键是前端给的 request_id
    inflight: StdMutex<HashMap<String, CancellationToken>>,
    /// (request_id, 回包全文)，只存被截断过的
    large_responses: StdMutex<VecDeque<(String, String)>>,
    /// 纯快照的目录（已标注另一个后端的情况），第一次用到时算一次
    snapshot_catalogs: SnapshotCatalogs,
    /// 每个 Bot 的事件接收器，以及各 Bot 下一条事件的编号
    receivers: StdMutex<ReceiverTable>,
    /// 自己发起的 SnowLuma 调用的指纹，事件流回放时据此跳过
    own_calls: Arc<OwnCalls>,
    /// 工作区 / 收藏夹 / 历史，第一次用到时读盘
    store: Arc<LazyStore>,
    history_queue: HistoryQueue,
    /// 管理器被丢弃时取消：常驻的监听和清扫任务据此退出
    shutdown: CancellationToken,
    /// Bot 停止（停机、崩溃、删除）的累计次数，每次停止加一
    bot_stops: AtomicU64,
    /// 每个 Bot 最近一次停止时 `bot_stops` 的值。只在持有会话表锁时写
    stopped_at: StdMutex<HashMap<BotId, u64>>,
    /// 当前开着的 SSH 隧道条数：隧道只由会话持有，会话丢掉时随之关闭
    open_tunnels: Arc<AtomicUsize>,
}

impl DebugManager {
    /// `data_root` 是应用数据根目录，落盘文件放在它下面的 `onebot-debug/`。构造不碰磁盘
    pub fn new(
        bots: Arc<dyn DebugBotPort>,
        host_resolver: Arc<dyn HostResolver>,
        data_root: PathBuf,
    ) -> Self {
        Self {
            bots,
            host_resolver,
            // 功能开关默认开
            enabled: AtomicBool::new(true),
            epoch: StdMutex::new(Epoch::first()),
            sessions: Mutex::new(HashMap::new()),
            inflight: StdMutex::new(HashMap::new()),
            large_responses: StdMutex::new(VecDeque::new()),
            snapshot_catalogs: SnapshotCatalogs::default(),
            receivers: StdMutex::new(ReceiverTable::default()),
            own_calls: Arc::new(OwnCalls::default()),
            store: Arc::new(LazyStore::new(data_root)),
            history_queue: HistoryQueue::default(),
            shutdown: CancellationToken::new(),
            bot_stops: AtomicU64::new(0),
            stopped_at: StdMutex::new(HashMap::new()),
            open_tunnels: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// 关掉时停掉所有接收器、断开全部会话（隧道随之关闭）、取消在途操作；之后的调用返回 `FeatureDisabled`
    pub async fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
        if !on {
            self.close_all().await;
        }
    }

    /// 停掉所有接收器（各推一条「已停止」）、丢掉所有会话（隧道 Drop 即关、WS 主动关）、
    /// 取消所有在途操作、清掉大回包缓存。各 Bot 的事件编号保留，重新打开后接着往后编
    pub async fn close_all(&self) {
        // 先换一轮、取消令牌，再清表：清表之后才回来写的操作一定看得到新一轮，
        // 它们手里新建的连接和隧道会被丢掉
        let previous = {
            let mut epoch = lock_std(&self.epoch);
            let next = Epoch::next_after(Some(&epoch));
            std::mem::replace(&mut *epoch, next)
        };
        previous.token.cancel();
        lock_std(&self.receivers).stop_all(REASON_CLOSED);
        for token in lock_std(&self.inflight).values() {
            token.cancel();
        }
        let drained: Vec<BotSession> = {
            let mut sessions = self.sessions.lock().await;
            sessions.drain().map(|(_, session)| session).collect()
        };
        // 关连接放在锁外：关 WS 会唤醒在等回包的调用，别让它们回头抢锁时还被挡着
        for mut session in drained {
            session.shutdown();
        }
        lock_std(&self.large_responses).clear();
    }

    pub async fn list_targets(&self) -> Vec<DebugTarget> {
        self.bots
            .list_bots()
            .await
            .into_iter()
            .map(|view| DebugTarget {
                bot_id: view.bot_id().as_str().to_owned(),
                name: view.config.bot.name.clone(),
                qq_id: view.config.bot.qq_id,
                backend: view.config.bot.backend_type,
                host: debug_host(&view.config),
                running: view.running(),
                online: view.online,
            })
            .collect()
    }

    pub async fn list_channels(&self, bot_id: &str) -> Result<DebugChannels, DebugError> {
        let id = BotId::new(bot_id);
        let view = self.bots.bot(&id).await.ok_or(DebugError::BotNotFound)?;
        let plans = plan_channels(&view.config);
        let too_old = self.refresh_session(&view).await;
        let channels: Vec<DebugChannelInfo> = {
            let sessions = self.sessions.lock().await;
            let session = sessions.get(&id);
            plans
                .iter()
                .map(|plan| channel_info(&view, plan, session))
                .collect()
        };
        let excluded = auto_exclusions(too_old);
        Ok(DebugChannels {
            bot_id: id.as_str().to_owned(),
            channels,
            auto_call: pick_auto(&plans, true, &excluded),
            auto_events: pick_auto(&plans, false, &excluded),
        })
    }

    /// 测一次连通并记下结果。内部通道探调试接口在不在（404 = 上游太老），
    /// HTTP / WS 通道调一次只读的 `get_status`。`Auto` 测的是「自动」当前会用来调用的那条
    pub async fn test_channel(
        &self,
        bot_id: &str,
        channel: DebugChannelId,
    ) -> Result<DebugChannelInfo, DebugError> {
        let epoch = self.epoch();
        if !self.is_enabled() {
            return Err(DebugError::FeatureDisabled);
        }
        let id = BotId::new(bot_id);
        let view = self.bots.bot(&id).await.ok_or(DebugError::BotNotFound)?;
        let plans = plan_channels(&view.config);
        let excluded = auto_exclusions(self.refresh_session(&view).await);
        let target = match channel {
            DebugChannelId::Auto => pick_auto(&plans, true, &excluded).ok_or_else(|| {
                DebugError::ChannelUnavailable {
                    reason: NO_CALL_CHANNEL.to_owned(),
                }
            })?,
            other => other,
        };
        let plan = plans.iter().find(|p| p.id == target).ok_or_else(|| {
            DebugError::ChannelUnavailable {
                reason: NO_SUCH_CHANNEL.to_owned(),
            }
        })?;
        // Bot 没在跑、或者注定连不上的通道，探了也白探：状态由 channel_info 直接给出
        if view.running() && !matches!(plan.reach, Reach::Unsupported { .. }) {
            self.probe(&view, plan, &epoch).await?;
        }
        let sessions = self.sessions.lock().await;
        Ok(channel_info(&view, plan, sessions.get(&id)))
    }

    /// 不再等这次调用（上游可能已经执行了，这里管不了）
    pub fn cancel(&self, request_id: &str) {
        if let Some(token) = lock_std(&self.inflight).get(request_id) {
            token.cancel();
        }
    }

    /// 把被截断的超大回包全文写到用户选的文件
    pub async fn save_response(&self, request_id: &str, path: &Path) -> Result<(), String> {
        let text = lock_std(&self.large_responses)
            .iter()
            .find(|(id, _)| id == request_id)
            .map(|(_, text)| text.clone())
            .ok_or_else(|| {
                format!("没有这次调用的完整回包：只保留最近 {LARGE_RESPONSES_KEPT} 次被截断的回包")
            })?;
        tokio::fs::write(path, text)
            .await
            .map_err(|e| format!("保存回包失败：{e}"))
    }

    /// 超大回包的全文留着给「另存为」，超出上限挤掉最早的。旧一轮的迟到结果不留
    fn keep_large_response(&self, request_id: &str, text: String, epoch: &Epoch) {
        let mut kept = lock_std(&self.large_responses);
        // 在锁里判断：close_all 先换轮再在这把锁里清空，判断和写入之间不会插进一次清空
        if !self.is_current(epoch) {
            return;
        }
        kept.retain(|(id, _)| id != request_id);
        kept.push_back((request_id.to_owned(), text));
        while kept.len() > LARGE_RESPONSES_KEPT {
            kept.pop_front();
        }
    }

    /// 当前这一轮。每个会写会话表的操作开始时取一份，顺带记下此刻的「Bot 停止」计数
    fn epoch(&self) -> Epoch {
        let mut epoch = lock_std(&self.epoch).clone();
        epoch.bot_stops = self.bot_stops.load(Ordering::SeqCst);
        epoch
    }

    /// 这个 Bot 在操作开始之后停过。停过就一直算停过，所以在哪儿查都不会误判；
    /// 要和「建会话」互斥时（`live_session`）在会话表锁里查，记停止也在那把锁里
    fn stopped_since(&self, bot_id: &BotId, epoch: &Epoch) -> bool {
        lock_std(&self.stopped_at)
            .get(bot_id)
            .is_some_and(|stop| *stop > epoch.bot_stops)
    }

    /// 记一次 Bot 停止：之前开始的操作不能再给它建会话。要在持有会话表锁时调用
    fn note_bot_stopped(&self, bot_id: &BotId) {
        let stop = self
            .bot_stops
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        lock_std(&self.stopped_at).insert(bot_id.clone(), stop);
    }

    /// 当前开着的隧道条数
    #[cfg(test)]
    fn open_tunnels(&self) -> usize {
        self.open_tunnels.load(Ordering::SeqCst)
    }

    /// 操作所属的那一轮是否还有效：开关没关、也没被 `close_all` 换掉
    fn is_current(&self, epoch: &Epoch) -> bool {
        self.is_enabled() && lock_std(&self.epoch).generation == epoch.generation
    }

    /// 旧一轮的操作该报的错：开关关了是 `FeatureDisabled`，只是被 `close_all` 收掉就是 `Cancelled`
    fn stale_error(&self) -> DebugError {
        if self.is_enabled() {
            DebugError::Cancelled
        } else {
            DebugError::FeatureDisabled
        }
    }

    /// 取（必要时建）这个 Bot 的会话，但只给还属于当前一轮、且开始之后 Bot 没停过的操作：
    /// Bot 停了会话就被丢掉，半路的探测 / 调用回来时不能再把它（连同刚开的隧道）建回来。
    /// 拿到 None 的调用方要丢掉手里新建的连接 / 隧道，不写表。要在持有会话表锁时调用。
    /// 取到的会话记一次「有人在用」，空闲回收按它判断
    fn live_session<'a>(
        &self,
        sessions: &'a mut HashMap<BotId, BotSession>,
        view: &DebugBotView,
        epoch: &Epoch,
    ) -> Option<&'a mut BotSession> {
        let bot_id = view.bot_id();
        if !self.is_current(epoch) || self.stopped_since(&bot_id, epoch) {
            return None;
        }
        let session = sessions
            .entry(bot_id)
            .or_insert_with(|| BotSession::new(view.snapshot.revision));
        session.touch();
        Some(session)
    }

    /// 内部通道当前的 WebUI 端点（端口, 口令）；还没就绪时给出给人看的原因
    async fn internal_endpoint(&self, view: &DebugBotView) -> Result<(u16, String), String> {
        let bot_id = view.bot_id();
        match view.config.bot.backend_type {
            BackendType::NapCat => self.bots.napcat_webui(&bot_id).await.ok_or_else(|| {
                "NapCat WebUI 还没就绪：Bot 刚启动时要等它打印出 WebUI 地址".to_owned()
            }),
            BackendType::SnowLuma => self.bots.snowluma_webui(&bot_id).await,
        }
    }

    /// 按 Bot 最新的快照和 WebUI 端点校正已有会话：重启过、换过端点的，
    /// 之前得出的「上游太老」和目录缓存作废。返回校正后内部通道是否仍算太老
    async fn refresh_session(&self, view: &DebugBotView) -> bool {
        let bot_id = view.bot_id();
        if !self.sessions.lock().await.contains_key(&bot_id) {
            return false;
        }
        // 端点查询不持锁：SnowLuma 本机端点要读两三个小文件
        let endpoint = self.internal_endpoint(view).await.ok();
        let mut sessions = self.sessions.lock().await;
        let Some(session) = sessions.get_mut(&bot_id) else {
            return false;
        };
        session.sync_bot_revision(view.snapshot.revision);
        if let Some((port, secret)) = &endpoint {
            session.sync_internal_endpoint(*port, secret);
        }
        session.internal_too_old
    }
}

impl Drop for DebugManager {
    /// 常驻任务和接收器都只拿弱引用，这里取消令牌让它们马上退出，而不是等到下一次醒来
    fn drop(&mut self) {
        self.shutdown.cancel();
        lock_std(&self.epoch).token.cancel();
    }
}

fn auto_exclusions(internal_too_old: bool) -> HashSet<DebugChannelId> {
    if internal_too_old {
        HashSet::from([DebugChannelId::Internal])
    } else {
        HashSet::new()
    }
}

/// 标准库锁的取用：临界区里只有表的增删查，不会 panic；万一被毒化，数据也仍然可用
fn lock_std<T>(mutex: &StdMutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn debug_host(config: &BotConfig) -> DebugHost {
    match RuntimeScenario::from_config(config) {
        Ok(RuntimeScenario::LocalNative { .. }) => DebugHost::Local,
        Ok(RuntimeScenario::RemoteNative { server_id, .. }) => DebugHost::Remote { server_id },
        Ok(RuntimeScenario::RemoteDocker { server_id, .. }) => DebugHost::Docker { server_id },
        // 配置组合不合法时也得在列表里显示出来，按字面上的目标和部署方式归类
        Err(_) => match config.bot.runtime_target.server_id() {
            None => DebugHost::Local,
            Some(id) if config.bot.deployment_type == DeploymentType::Docker => DebugHost::Docker {
                server_id: id.to_owned(),
            },
            Some(id) => DebugHost::Remote {
                server_id: id.to_owned(),
            },
        },
    }
}

/// 一条通道给界面看的样子。状态的优先级：注定连不上 > Bot 没在跑 > 内部通道太老 > 最近一次结果
fn channel_info(
    view: &DebugBotView,
    plan: &ChannelPlan,
    session: Option<&BotSession>,
) -> DebugChannelInfo {
    let status = match &plan.reach {
        Reach::Unsupported { reason } => DebugChannelStatus::Unsupported {
            reason: reason.clone(),
        },
        _ if !view.running() => DebugChannelStatus::BotNotRunning,
        Reach::Internal if session.is_some_and(|s| s.internal_too_old) => {
            DebugChannelStatus::UpstreamTooOld
        }
        _ => session
            .and_then(|s| s.status_for(plan))
            .cloned()
            .unwrap_or(DebugChannelStatus::Unknown),
    };
    DebugChannelInfo {
        id: plan.id.clone(),
        label: plan.label.clone(),
        can_call: plan.can_call,
        can_receive: plan.can_receive,
        status,
        endpoint: endpoint_text(&plan.reach),
        token_hint: plan.token.as_deref().and_then(mask_token),
    }
}

fn endpoint_text(reach: &Reach) -> Option<String> {
    // 根路径不写出来，地址短一点；配了子路径的照实显示
    let suffix = |path: &str| {
        if path == "/" {
            String::new()
        } else {
            path.to_owned()
        }
    };
    match reach {
        Reach::Internal => Some("WebUI".to_owned()),
        Reach::Direct { host, port, path } => {
            Some(format!("{}:{port}{}", calls::url_host(host), suffix(path)))
        }
        Reach::Tunnel {
            remote_host,
            remote_port,
            path,
        } => Some(format!(
            "隧道 → 远端 {}:{remote_port}{}",
            calls::url_host(remote_host),
            suffix(path)
        )),
        Reach::Unsupported { .. } => None,
    }
}
