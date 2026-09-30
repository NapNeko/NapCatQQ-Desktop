//! 事件接收器：每个 Bot 至多一个，从选定的来源（NapCat / SnowLuma 的内部通道，或者一条
//! OneBot WS 服务）持续收事件，编号后放进环形缓冲，每 50 ms 攒成一批推给所有正在看的窗口。
//!
//! 几个要点：
//! - 编号（`seq`）在管理器的整个生命周期里对同一个 Bot 单调递增，接收器停了又建也接着往后编：
//!   前端按 `seq` 去重，编号回退会让它把新事件当成看过的旧事件丢掉；
//! - 新窗口订阅时先补发缓冲里已有的，再加入实时推送，两步都在接收器的锁里完成，不漏也不重；
//! - 连接断了按退避重连，重新连上时补一条「中间漏了多久」；鉴权失败、上游太老这类重试也没用的，
//!   直接停下并写明原因；
//! - 连接、重连、补发都跟着接收器自己的停止令牌走，而这个令牌是「这一轮」令牌的子令牌：
//!   `close_all` / 关掉调试台时所有接收器随之收工，不会在背后留下连接或定时器。
//!
//! 锁只有标准库锁，临界区里没有 await：推给窗口的 `send` 是同步的（Tauri Channel 只是排队）。
//! 加锁顺序固定为「接收器表 → 单个接收器」，反过来的顺序从不出现。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, Weak};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use ncd_backend_napcat::NapCatDebugError;
use ncd_backend_snowluma::{SlStreamFrame, SnowLumaDebugError, parse_stream_frame};
use ncd_domain::bot_config::BackendType;
use ncd_domain::ids::BotId;
use ncd_domain::onebot_debug::{
    DEBUG_EVENT_VERSION, DebugCallOrigin, DebugCallRecord, DebugChannelId, DebugChannelStatus,
    DebugError, DebugEvent, DebugEventBatch, DebugEventBody, DebugReceiverInfo, DebugReceiverState,
    DebugSubscribeResponse,
};
use ncd_onebot::backoff::Backoff;
use ncd_onebot::client::{ClientError, SseParser, WsClient, connect_ws};
use ncd_onebot::ring::{DEFAULT_RING_CAP, EventRing};
use serde_json::Value;
use tokio::sync::{Notify, mpsc};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing::debug;
use uuid::Uuid;

use super::calls::{ChannelResult, InternalClient, PROBE_TIMEOUT};
use super::errors::{Failure, error_text, from_client, from_napcat, from_snowluma};
use super::params::cap_record_params;
use super::plan::{ChannelPlan, Reach, pick_auto, plan_channels};
use super::port::{DebugBotView, DebugEventSink};
use super::{DebugManager, Epoch, NO_SUCH_CHANNEL, auto_exclusions, lock_std};
use crate::metrics::now_ms;

/// 攒一批再推：逐条推会让前端每来一条事件就重排一次列表
pub(super) const FLUSH_EVERY: Duration = Duration::from_millis(50);
/// 补发缓冲时每批的条数，免得一次 IPC 塞进几千条
const BACKLOG_CHUNK: usize = 500;
/// 协议客户端交给接收器的事件队列容量；接收器只做入缓冲，消费得很快
const EVENT_QUEUE_CAP: usize = 1024;
/// 一次建连（含登录、建适配器、开隧道）的总时限
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// SnowLuma 每 15 秒发一次心跳注释。这么久连一个字节都没收到，多半是隧道那头已经断了
const SSE_IDLE_TIMEOUT: Duration = Duration::from_secs(45);
/// 连接撑过这么久才把退避归零：连上就断的上游（握手成功、随即被踢）不该每次都 1 秒重试
const STABLE_AFTER: Duration = Duration::from_secs(5);
/// 没人看、也没人调用超过这么久的接收器由清扫停掉
pub(super) const IDLE_STOP_AFTER: Duration = Duration::from_secs(30 * 60);
/// 清扫的间隔
pub(super) const SWEEP_EVERY: Duration = Duration::from_secs(60);
/// 登录和「检测到 QQ 号」两条事件几乎同时到，只重连一次
const KICK_COALESCE: Duration = Duration::from_secs(2);
/// 自己发起的调用的指纹最多留这么多条、这么久：SnowLuma 的回放通常几十毫秒内就到
const OWN_CALLS_KEPT: usize = 64;
const OWN_CALL_TTL: Duration = Duration::from_secs(10);

pub(super) const REASON_MANUAL: &str = "已手动停止";
pub(super) const REASON_BOT_STOPPED: &str = "Bot 已停止";
pub(super) const REASON_IDLE: &str = "30 分钟没人看，已停止接收";
pub(super) const REASON_CLOSED: &str = "调试台已关闭";
const NO_EVENT_CHANNEL: &str = "没有能接收事件的通道";

// ─── 接收器表 ────────────────────────────────────────────────────────────────

/// 所有接收器，以及每个 Bot 下一条事件该用的编号
#[derive(Default)]
pub(super) struct ReceiverTable {
    live: HashMap<BotId, Arc<Receiver>>,
    /// 接收器停掉时记下它编到了哪，同一个 Bot 的下一个接收器从这里接着编
    next_seq: HashMap<BotId, u64>,
}

impl ReceiverTable {
    /// 停掉并摘掉一个接收器，记下编号。没有这个接收器时返回 false
    fn stop(&mut self, bot_id: &BotId, reason: &str) -> bool {
        let Some(receiver) = self.live.remove(bot_id) else {
            return false;
        };
        let next = receiver.close(reason);
        self.next_seq.insert(bot_id.clone(), next);
        true
    }

    pub(super) fn stop_all(&mut self, reason: &str) {
        let ids: Vec<BotId> = self.live.keys().cloned().collect();
        for id in ids {
            self.stop(&id, reason);
        }
    }
}

// ─── 单个接收器 ──────────────────────────────────────────────────────────────

pub(super) struct Receiver {
    bot_id: BotId,
    /// 整个接收器的停止令牌，是建它时那一轮令牌的子令牌
    stop: CancellationToken,
    inner: StdMutex<ReceiverInner>,
}

struct ReceiverInner {
    /// 已解析的来源，不会是 Auto
    source: DebugChannelId,
    state: DebugReceiverState,
    ring: EventRing,
    /// 进了缓冲、还没推出去的
    pending: Vec<DebugEvent>,
    sinks: HashMap<String, Arc<dyn DebugEventSink>>,
    /// 最近一次有人看（最后一个窗口离开的时刻也算）或者有调用的时间，清扫按它判断
    last_viewer_or_call: Instant,
    /// 连接断开的时刻（Unix 毫秒），重新连上时据此补一条 Gap
    disconnected_at: Option<u64>,
    /// 当前这台泵的编号和令牌。换来源、重新拉起时换新，旧泵迟到的写入对不上号就丢掉
    pump_seq: u64,
    pump: CancellationToken,
    /// 要求当前这台泵断开重连（SnowLuma 只给连接时已在线的账号挂钩子，登录后得重连）。
    /// 每台泵一个：发给旧泵、没被用掉的通知不会让新泵一上来就白白重连一次
    kick: Arc<Notify>,
    last_kick: Option<Instant>,
    /// 已停止：之后的写入和订阅一律拒绝
    closed: bool,
}

/// 一台新泵要用的东西
pub(super) struct PumpStart {
    pump_id: u64,
    token: CancellationToken,
    source: DebugChannelId,
    kick: Arc<Notify>,
    /// 第一次尝试的「连接中」已经推过了（换来源时在订阅的锁里就推了）
    announced: bool,
}

impl ReceiverInner {
    fn push(&mut self, body: DebugEventBody) {
        let event = self.ring.push(now_ms(), body);
        self.pending.push(event);
        // 正常情况下 50 ms 内攒不到这么多；真攒到了，最老的那些在缓冲里也已经被挤掉了，
        // 前端看到编号跳了会自己按 seq 补拉
        if self.pending.len() > DEFAULT_RING_CAP {
            let excess = self.pending.len() - DEFAULT_RING_CAP;
            self.pending.drain(..excess);
        }
    }

    fn set_state(&mut self, state: DebugReceiverState) {
        self.state = state.clone();
        let source = self.source.clone();
        self.push(DebugEventBody::Receiver { state, source });
    }

    /// 把攒着的推给所有窗口；对面已经走了的摘掉
    fn flush(&mut self, bot_id: &BotId) {
        if self.pending.is_empty() {
            return;
        }
        let batch = DebugEventBatch {
            v: DEBUG_EVENT_VERSION,
            bot_id: bot_id.as_str().to_owned(),
            events: std::mem::take(&mut self.pending),
        };
        let had_viewers = !self.sinks.is_empty();
        self.sinks.retain(|_, sink| sink.send(&batch));
        if had_viewers && self.sinks.is_empty() {
            // 最后一个窗口刚走：空闲从现在算起
            self.last_viewer_or_call = Instant::now();
        }
    }

    fn info(&self, bot_id: &BotId) -> DebugReceiverInfo {
        DebugReceiverInfo {
            bot_id: bot_id.as_str().to_owned(),
            source: self.source.clone(),
            state: self.state.clone(),
            buffered: u32::try_from(self.ring.len()).unwrap_or(u32::MAX),
            dropped_total: self.ring.dropped_total(),
            first_seq: self.ring.first_seq(),
            viewers: u32::try_from(self.sinks.len()).unwrap_or(u32::MAX),
        }
    }

    /// 换一台新泵：旧泵的令牌取消，编号加一，状态当场变成「连接中」。
    /// 订阅的回包和随后补发的缓冲说的是同一件事，不会一个说「已停止」一个说「连接中」
    fn next_pump(&mut self, stop: &CancellationToken, source: DebugChannelId) -> PumpStart {
        self.pump.cancel();
        self.pump_seq = self.pump_seq.wrapping_add(1);
        self.pump = stop.child_token();
        self.kick = Arc::new(Notify::new());
        self.source = source.clone();
        // 换了来源，旧来源的断线时刻和新连接无关
        self.disconnected_at = None;
        self.set_state(DebugReceiverState::Connecting);
        PumpStart {
            pump_id: self.pump_seq,
            token: self.pump.clone(),
            source,
            kick: Arc::clone(&self.kick),
            announced: true,
        }
    }
}

impl Receiver {
    pub(super) fn new(
        bot_id: BotId,
        source: DebugChannelId,
        next_seq: u64,
        stop: CancellationToken,
    ) -> Self {
        let pump = stop.child_token();
        Self {
            bot_id,
            inner: StdMutex::new(ReceiverInner {
                source,
                state: DebugReceiverState::Connecting,
                ring: EventRing::with_start_seq(DEFAULT_RING_CAP, next_seq),
                pending: Vec::new(),
                sinks: HashMap::new(),
                last_viewer_or_call: Instant::now(),
                disconnected_at: None,
                pump_seq: 0,
                pump,
                kick: Arc::new(Notify::new()),
                last_kick: None,
                closed: false,
            }),
            stop,
        }
    }

    fn lock(&self) -> MutexGuard<'_, ReceiverInner> {
        lock_std(&self.inner)
    }

    /// 第一台泵。新接收器的缓冲是空的，「连接中」由泵自己推
    pub(super) fn first_pump(&self) -> PumpStart {
        let inner = self.lock();
        PumpStart {
            pump_id: inner.pump_seq,
            token: inner.pump.clone(),
            source: inner.source.clone(),
            kick: Arc::clone(&inner.kick),
            announced: false,
        }
    }

    /// 订阅时判断要不要换一台泵：显式要了另一个来源，或者上一台泵因为重试也没用而停了
    /// （用户可能已经改好了令牌、升级了上游，重新订阅就是在要求再试一次）
    fn restart_if_needed(
        &self,
        explicit: Option<&DebugChannelId>,
        auto_resolved: &DebugChannelId,
    ) -> Option<PumpStart> {
        let mut inner = self.lock();
        if inner.closed {
            return None;
        }
        let halted = matches!(inner.state, DebugReceiverState::Stopped { .. });
        let target = match explicit {
            Some(source) => source.clone(),
            None if halted => auto_resolved.clone(),
            None => return None,
        };
        if target == inner.source && !halted {
            return None;
        }
        Some(inner.next_pump(&self.stop, target))
    }

    /// 泵推一条事件；旧泵的迟到写入丢掉
    fn push_from(&self, pump_id: u64, body: DebugEventBody) {
        let mut inner = self.lock();
        if inner.closed || inner.pump_seq != pump_id {
            return;
        }
        inner.push(body);
    }

    fn set_state_from(&self, pump_id: u64, state: DebugReceiverState) {
        let mut inner = self.lock();
        if inner.closed || inner.pump_seq != pump_id {
            return;
        }
        inner.set_state(state);
    }

    /// 连上了：断过线的补一条漏掉的时间段
    fn connected(&self, pump_id: u64) {
        let mut inner = self.lock();
        if inner.closed || inner.pump_seq != pump_id {
            return;
        }
        inner.set_state(DebugReceiverState::Connected);
        if let Some(from_ms) = inner.disconnected_at.take() {
            inner.push(DebugEventBody::Gap {
                from_ms,
                to_ms: now_ms(),
            });
        }
    }

    /// 断了（或者没连上），`delay` 之后重试。`since` 是上一条连接断开的时刻（Unix 毫秒）：
    /// 被要求重连、重连又没成功的，漏事件从旧连接关掉那一刻就开始了
    fn disconnected(&self, pump_id: u64, attempt: u32, delay: Duration, since: u64) {
        let mut inner = self.lock();
        if inner.closed || inner.pump_seq != pump_id {
            return;
        }
        if inner.disconnected_at.is_none() {
            inner.disconnected_at = Some(since);
        }
        inner.set_state(DebugReceiverState::Reconnecting {
            attempt,
            retry_in_ms: u32::try_from(delay.as_millis()).unwrap_or(u32::MAX),
        });
    }

    /// 重试也没用（鉴权失败、上游太老……）：写明原因停下，接收器留着，缓冲照样能看，
    /// 再订阅一次会重新拉起
    fn halt(&self, pump_id: u64, reason: String) {
        self.set_state_from(pump_id, DebugReceiverState::Stopped { reason });
    }

    /// 自己发起的调用记进事件流
    fn push_call(&self, record: DebugCallRecord) {
        let mut inner = self.lock();
        if inner.closed {
            return;
        }
        inner.push(DebugEventBody::Call { record });
        inner.last_viewer_or_call = Instant::now();
    }

    fn flush(&self) {
        self.lock().flush(&self.bot_id);
    }

    /// 登记一个窗口：先把还没推出去的推给已有窗口，再给新窗口补发整个缓冲，最后加入实时推送。
    /// 全程持锁，这期间进来的事件只会在下一次推送里出现，新窗口不漏也不重。已停止时返回 None
    fn attach(
        &self,
        subscription_id: &str,
        sink: Arc<dyn DebugEventSink>,
    ) -> Option<DebugReceiverInfo> {
        let mut inner = self.lock();
        if inner.closed {
            return None;
        }
        inner.flush(&self.bot_id);
        let mut backlog = inner.ring.tail(DEFAULT_RING_CAP).into_iter();
        let mut alive = true;
        loop {
            let events: Vec<DebugEvent> = backlog.by_ref().take(BACKLOG_CHUNK).collect();
            if events.is_empty() {
                break;
            }
            let batch = DebugEventBatch {
                v: DEBUG_EVENT_VERSION,
                bot_id: self.bot_id.as_str().to_owned(),
                events,
            };
            if !sink.send(&batch) {
                alive = false;
                break;
            }
        }
        if alive {
            inner.sinks.insert(subscription_id.to_owned(), sink);
        }
        inner.last_viewer_or_call = Instant::now();
        Some(inner.info(&self.bot_id))
    }

    fn detach(&self, subscription_id: &str) -> bool {
        let mut inner = self.lock();
        let removed = inner.sinks.remove(subscription_id).is_some();
        if removed && inner.sinks.is_empty() {
            inner.last_viewer_or_call = Instant::now();
        }
        removed
    }

    /// 窗口 `page` 的页面在 `loading_since` 开始重新加载：它在这之前开的出口都已作废
    fn detach_page(&self, page: &str, loading_since: std::time::Instant) {
        let mut inner = self.lock();
        let before = inner.sinks.len();
        inner.sinks.retain(|_, sink| {
            !matches!(sink.opened_by(), Some((owner, at)) if owner == page && at < loading_since)
        });
        if inner.sinks.len() < before && inner.sinks.is_empty() {
            inner.last_viewer_or_call = Instant::now();
        }
    }

    pub(super) fn info(&self) -> DebugReceiverInfo {
        self.lock().info(&self.bot_id)
    }

    fn source(&self) -> DebugChannelId {
        self.lock().source.clone()
    }

    fn is_idle(&self, now: Instant) -> bool {
        let inner = self.lock();
        inner.sinks.is_empty()
            && now.saturating_duration_since(inner.last_viewer_or_call) > IDLE_STOP_AFTER
    }

    fn read_since(&self, since_seq: u64, limit: usize) -> Vec<DebugEvent> {
        self.lock().ring.read_since(since_seq, limit)
    }

    /// 要求当前这台泵断开重连。连接中收到的会留到连上之后再重连一次（上游按建连那一刻
    /// 在线的账号挂钩子，这次建连可能早于登录）；两秒内的重复要求只算一次
    fn kick(&self) {
        let mut inner = self.lock();
        if inner.closed {
            return;
        }
        let now = Instant::now();
        if inner
            .last_kick
            .is_some_and(|at| now.saturating_duration_since(at) < KICK_COALESCE)
        {
            return;
        }
        inner.last_kick = Some(now);
        inner.kick.notify_one();
    }

    /// 在等重连的话立刻重连（不在等就什么都不做，免得把好好的连接断掉）
    fn kick_if_waiting(&self) {
        let inner = self.lock();
        if matches!(inner.state, DebugReceiverState::Reconnecting { .. }) {
            inner.kick.notify_one();
        }
    }

    /// 给每个窗口发一个空批次，摘掉已经走了的。没有事件的接收器（安静的群、停下的接收器）
    /// 不会触发推送，不这样探一下，关掉的窗口会一直占着「有人在看」
    fn probe_sinks(&self) {
        let mut inner = self.lock();
        if inner.closed || inner.sinks.is_empty() {
            return;
        }
        let batch = DebugEventBatch {
            v: DEBUG_EVENT_VERSION,
            bot_id: self.bot_id.as_str().to_owned(),
            events: Vec::new(),
        };
        inner.sinks.retain(|_, sink| sink.send(&batch));
        if inner.sinks.is_empty() {
            inner.last_viewer_or_call = Instant::now();
        }
    }

    /// 停下：推一条「已停止」并立即推出去，然后取消泵和定时推送。返回下一条该用的编号
    fn close(&self, reason: &str) -> u64 {
        let next = {
            let mut inner = self.lock();
            if !inner.closed {
                inner.set_state(DebugReceiverState::Stopped {
                    reason: reason.to_owned(),
                });
                inner.flush(&self.bot_id);
                inner.closed = true;
                inner.sinks.clear();
                inner.pending.clear();
            }
            inner.ring.next_seq()
        };
        self.stop.cancel();
        next
    }
}

// ─── 自己发起的调用的指纹 ────────────────────────────────────────────────────

/// SnowLuma 的事件流会把调试台自己发的调用也回放一遍且不带来源。调用前记下指纹，
/// 回放到达时认出来跳过，聊天里就不会出现两条一模一样的「我发了什么」
#[derive(Default)]
pub(super) struct OwnCalls {
    calls: StdMutex<HashMap<BotId, VecDeque<OwnCall>>>,
}

struct OwnCall {
    action: String,
    params: String,
    at: Instant,
}

impl OwnCalls {
    pub(super) fn note(&self, bot_id: &BotId, action: &str, params: &Value) {
        let call = OwnCall {
            action: action.to_owned(),
            params: canonical_params(params),
            at: Instant::now(),
        };
        let mut all = lock_std(&self.calls);
        let calls = all.entry(bot_id.clone()).or_default();
        forget_expired(calls, call.at);
        calls.push_back(call);
        while calls.len() > OWN_CALLS_KEPT {
            calls.pop_front();
        }
    }

    /// 是自己发的就摘掉那条指纹并返回 true（同样的调用发了两次，就要认两次）
    pub(super) fn take(&self, bot_id: &BotId, action: &str, params: &Value) -> bool {
        let params = canonical_params(params);
        let mut all = lock_std(&self.calls);
        let Some(calls) = all.get_mut(bot_id) else {
            return false;
        };
        forget_expired(calls, Instant::now());
        let found = calls
            .iter()
            .position(|c| c.action == action && c.params == params)
            .and_then(|index| calls.remove(index))
            .is_some();
        if calls.is_empty() {
            all.remove(bot_id);
        }
        found
    }
}

fn forget_expired(calls: &mut VecDeque<OwnCall>, now: Instant) {
    while calls
        .front()
        .is_some_and(|c| now.saturating_duration_since(c.at) > OWN_CALL_TTL)
    {
        calls.pop_front();
    }
}

/// 参数的规范文本：对象键排好序。发出去和回放回来的键序不一定一样；
/// 没带参数（null）和空对象是一回事，我们发出去的总是对象
fn canonical_params(params: &Value) -> String {
    if params.is_null() {
        return "{}".to_owned();
    }
    canonical_json(params)
}

/// 对象键排序后的紧凑 JSON。不能直接 `to_string`：依赖图里有人开了 serde_json 的
/// `preserve_order`，对象按插入顺序输出
pub(super) fn canonical_json(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut entries: Vec<(&String, &Value)> = map.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(k, v)| (k.clone(), sorted(v)))
                        .collect(),
                )
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    sorted(value).to_string()
}

// ─── 对外的接收器操作 ────────────────────────────────────────────────────────

impl DebugManager {
    /// 订阅一个 Bot 的事件流。`source` 为 Auto 时挑「自动」的收事件通道；已有接收器且
    /// 显式要了另一个来源时换到新来源（缓冲保留）。先补发缓冲，再加入实时推送。
    ///
    /// 要 `Arc<Self>`：接收器的后台任务只拿管理器的弱引用，但得有个强引用才能降级出来
    pub async fn subscribe(
        self: &Arc<Self>,
        bot_id: &str,
        source: DebugChannelId,
        sink: Arc<dyn DebugEventSink>,
    ) -> Result<DebugSubscribeResponse, DebugError> {
        let epoch = self.epoch();
        if !self.is_enabled() {
            return Err(DebugError::FeatureDisabled);
        }
        let id = BotId::new(bot_id);
        let view = self.bots.bot(&id).await.ok_or(DebugError::BotNotFound)?;
        if !view.running() {
            return Err(DebugError::BotNotRunning);
        }
        let plans = plan_channels(&view.config);
        let excluded = auto_exclusions(self.refresh_session(&view).await);
        let resolved = resolve_event_channel(&plans, &source, &excluded)?;
        let explicit = (source != DebugChannelId::Auto).then_some(&resolved);

        let (receiver, pump, fresh) = {
            let mut table = lock_std(&self.receivers);
            // 在表锁里确认这一轮还在：close_all 先换轮再清表，这里建的接收器要么被它清掉，
            // 要么根本建不出来
            if !self.is_current(&epoch) {
                return Err(self.stale_error());
            }
            // 订阅开始之后 Bot 停过（停止事件恰好插在查 Bot 和进表之间）：这时建出来的接收器
            // 没人会去停，泵拿着旧的一轮也永远建不起会话
            if self.stopped_since(&id, &epoch) {
                return Err(DebugError::BotNotRunning);
            }
            match table.live.get(&id).cloned() {
                Some(existing) => {
                    let pump = existing.restart_if_needed(explicit, &resolved);
                    (existing, pump, false)
                }
                None => {
                    let next_seq = table.next_seq.get(&id).copied().unwrap_or(1);
                    let receiver = Arc::new(Receiver::new(
                        id.clone(),
                        resolved.clone(),
                        next_seq,
                        epoch.token.child_token(),
                    ));
                    table.live.insert(id.clone(), Arc::clone(&receiver));
                    let pump = receiver.first_pump();
                    (receiver, Some(pump), true)
                }
            }
        };

        let subscription_id = Uuid::new_v4().to_string();
        // 刚被停掉（Bot 停了、清扫、关调试台）：新建的泵不必再起
        let info = receiver
            .attach(&subscription_id, sink)
            .ok_or_else(|| self.stale_error())?;
        if fresh {
            tokio::spawn(run_flusher(Arc::clone(&receiver)));
        }
        if let Some(pump) = pump {
            tokio::spawn(run_pump(
                Arc::downgrade(self),
                Arc::clone(&receiver),
                pump,
                epoch,
                Arc::clone(&self.own_calls),
            ));
        }
        Ok(DebugSubscribeResponse {
            subscription_id,
            receiver: info,
        })
    }

    /// 窗口不再看了。接收器继续收（重新打开时还有最近的事件），没人看满 30 分钟由清扫停掉
    pub async fn unsubscribe(&self, subscription_id: &str) {
        let receivers: Vec<Arc<Receiver>> =
            lock_std(&self.receivers).live.values().cloned().collect();
        for receiver in receivers {
            if receiver.detach(subscription_id) {
                return;
            }
        }
    }

    /// 窗口 `page` 的页面开始（重新）加载。旧页面没机会退订，它的出口又探不出死活，
    /// 不摘掉的话接收器一直算「有人在看」，30 分钟空闲停不下来（隧道也跟着留着）。
    /// 只摘 `loading_since` 之前开的，新页面随后的订阅不受影响；全在同步锁里，可以直接在页面加载钩子里调
    pub fn page_loading(&self, page: &str, loading_since: std::time::Instant) {
        let receivers: Vec<Arc<Receiver>> =
            lock_std(&self.receivers).live.values().cloned().collect();
        for receiver in &receivers {
            receiver.detach_page(page, loading_since);
        }
    }

    pub async fn receivers(&self) -> Vec<DebugReceiverInfo> {
        let receivers: Vec<Arc<Receiver>> =
            lock_std(&self.receivers).live.values().cloned().collect();
        let mut infos: Vec<DebugReceiverInfo> = receivers.iter().map(|r| r.info()).collect();
        infos.sort_by(|a, b| a.bot_id.cmp(&b.bot_id));
        infos
    }

    /// 手动停掉一个 Bot 的接收器，缓冲随之丢掉（编号不回退）
    pub async fn stop_receiver(&self, bot_id: &str) {
        self.stop_receiver_with(&BotId::new(bot_id), REASON_MANUAL);
    }

    pub(super) fn stop_receiver_with(&self, bot_id: &BotId, reason: &str) -> bool {
        lock_std(&self.receivers).stop(bot_id, reason)
    }

    /// 缓冲里 `seq > since_seq` 的事件，从老到新最多 `limit` 条；没有接收器时为空
    pub async fn read_events(&self, bot_id: &str, since_seq: u64, limit: u32) -> Vec<DebugEvent> {
        let receiver = lock_std(&self.receivers)
            .live
            .get(&BotId::new(bot_id))
            .cloned();
        let limit = usize::try_from(limit)
            .unwrap_or(usize::MAX)
            .min(DEFAULT_RING_CAP);
        receiver
            .map(|r| r.read_since(since_seq, limit))
            .unwrap_or_default()
    }

    /// 每分钟清扫一次没人看、也没人调用超过 30 分钟的接收器。启动时 spawn 一次，
    /// 管理器被丢弃后立刻退出（只拿弱引用，不会反过来吊着管理器）
    pub async fn run_idle_sweeper(self: Arc<Self>) {
        let shutdown = self.shutdown.clone();
        let manager = Arc::downgrade(&self);
        drop(self);
        let mut ticker = tokio::time::interval(SWEEP_EVERY);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        // interval 的第一下立刻就响，跳过它
        ticker.tick().await;
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => return,
                _ = ticker.tick() => {}
            }
            let Some(manager) = manager.upgrade() else {
                return;
            };
            manager.sweep_idle().await;
        }
    }

    /// 一次清扫：先停掉没人看的接收器，再丢掉没有接收器、也 30 分钟没人用的会话
    pub(super) async fn sweep_idle(&self) {
        self.sweep_idle_receivers();
        self.sweep_idle_sessions().await;
    }

    /// 有接收器的 Bot（它的会话由接收器在用）
    pub(super) fn bots_with_receivers(&self) -> std::collections::HashSet<BotId> {
        lock_std(&self.receivers).live.keys().cloned().collect()
    }

    fn sweep_idle_receivers(&self) {
        // 先在表锁外探一遍窗口：关掉的窗口摘掉之后才算「没人看」
        let receivers: Vec<Arc<Receiver>> =
            lock_std(&self.receivers).live.values().cloned().collect();
        for receiver in &receivers {
            receiver.probe_sinks();
        }
        let now = Instant::now();
        let mut table = lock_std(&self.receivers);
        let idle: Vec<BotId> = table
            .live
            .iter()
            .filter(|(_, receiver)| receiver.is_idle(now))
            .map(|(id, _)| id.clone())
            .collect();
        for id in idle {
            debug!(bot_id = %id, "调试台：接收器 30 分钟没人看，停掉");
            table.stop(&id, REASON_IDLE);
        }
    }

    /// 把一次调用记进这个 Bot 的事件流。没有接收器就不记，也不为此建一个
    pub(super) fn push_call_record(&self, bot_id: &BotId, record: DebugCallRecord) {
        let receiver = lock_std(&self.receivers).live.get(bot_id).cloned();
        if let Some(receiver) = receiver {
            receiver.push_call(record);
        }
    }

    /// 让这个 Bot 走内部通道的接收器断开重连（缓冲保留，不算断线，不补 Gap）
    pub(super) fn kick_internal_receiver(&self, bot_id: &BotId) {
        let receiver = lock_std(&self.receivers).live.get(bot_id).cloned();
        if let Some(receiver) = receiver.filter(|r| r.source() == DebugChannelId::Internal) {
            receiver.kick();
        }
    }

    /// 内部通道的接收器正在等重连时让它马上重试（WebUI 刚就绪）
    pub(super) fn hurry_internal_receiver(&self, bot_id: &BotId) {
        let receiver = lock_std(&self.receivers).live.get(bot_id).cloned();
        if let Some(receiver) = receiver.filter(|r| r.source() == DebugChannelId::Internal) {
            receiver.kick_if_waiting();
        }
    }
}

fn resolve_event_channel(
    plans: &[ChannelPlan],
    requested: &DebugChannelId,
    excluded: &std::collections::HashSet<DebugChannelId>,
) -> Result<DebugChannelId, DebugError> {
    let unavailable = |reason: &str| DebugError::ChannelUnavailable {
        reason: reason.to_owned(),
    };
    let id = match requested {
        DebugChannelId::Auto => {
            pick_auto(plans, false, excluded).ok_or_else(|| unavailable(NO_EVENT_CHANNEL))?
        }
        other => other.clone(),
    };
    let plan = plans
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| unavailable(NO_SUCH_CHANNEL))?;
    if !plan.can_receive {
        return Err(unavailable("这条通道不能接收事件"));
    }
    if let Reach::Unsupported { reason } = &plan.reach {
        return Err(unavailable(reason));
    }
    Ok(id)
}

// ─── 后台任务 ────────────────────────────────────────────────────────────────

/// 每 50 ms 把攒着的事件推出去，接收器停了就退出
async fn run_flusher(receiver: Arc<Receiver>) {
    let mut ticker = tokio::time::interval(FLUSH_EVERY);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = receiver.stop.cancelled() => return,
            _ = ticker.tick() => receiver.flush(),
        }
    }
}

/// 一台泵：连上来源、收事件、断了按退避重连，直到令牌取消或者遇到重试也没用的失败
pub(super) async fn run_pump(
    manager: Weak<DebugManager>,
    receiver: Arc<Receiver>,
    pump: PumpStart,
    epoch: Epoch,
    own_calls: Arc<OwnCalls>,
) {
    let PumpStart {
        pump_id,
        token,
        source,
        kick,
        mut announced,
    } = pump;
    let mut backoff = Backoff::new();
    // 上一条连接关掉的时刻。马上重连成功就不算断线；重连没成功的，漏事件从这一刻算起
    let mut lost_at: Option<u64> = None;
    loop {
        if !std::mem::take(&mut announced) {
            receiver.set_state_from(pump_id, DebugReceiverState::Connecting);
        }
        let attempt = async {
            tokio::time::timeout(
                CONNECT_TIMEOUT,
                connect(&manager, &receiver.bot_id, &source, &epoch),
            )
            .await
            .unwrap_or_else(|_| Err(ConnectFailure::retry("连接超时")))
        };
        let connected = tokio::select! {
            biased;
            _ = token.cancelled() => return,
            result = attempt => result,
        };
        match connected {
            Ok(mut conn) => {
                let connected_at = Instant::now();
                // 连上之后 lost_at 不用清：离开这里的每条路都会重新记它（或者直接退出）
                receiver.connected(pump_id);
                let ended = tokio::select! {
                    biased;
                    _ = token.cancelled() => Ended::Stopped,
                    _ = kick.notified() => Ended::Kicked,
                    reason = conn.run(&receiver, pump_id, &own_calls) => Ended::Lost(reason),
                };
                conn.close();
                if connected_at.elapsed() >= STABLE_AFTER {
                    backoff.reset();
                }
                match ended {
                    Ended::Stopped => return,
                    // 要求重连：马上重连，重连成功就不算断线
                    Ended::Kicked => {
                        lost_at = Some(now_ms());
                        continue;
                    }
                    Ended::Lost(reason) => {
                        lost_at = Some(now_ms());
                        debug!(bot_id = %receiver.bot_id, %reason, "调试台：事件连接断开，稍后重连");
                    }
                }
            }
            Err(failure) if failure.fatal => {
                receiver.halt(pump_id, failure.reason);
                return;
            }
            Err(failure) => {
                debug!(bot_id = %receiver.bot_id, reason = %failure.reason, "调试台：事件来源没连上，稍后重试");
            }
        }
        let delay = backoff.next_delay();
        let since = lost_at.take().unwrap_or_else(now_ms);
        receiver.disconnected(pump_id, backoff.attempt(), delay, since);
        tokio::select! {
            biased;
            _ = token.cancelled() => return,
            _ = kick.notified() => {}
            _ = tokio::time::sleep(delay) => {}
        }
    }
}

/// 一次连接为什么结束
enum Ended {
    /// 接收器停了、换了来源或者这一轮结束了
    Stopped,
    /// 被要求重连
    Kicked,
    /// 连接自己断了，原因只进日志
    Lost(String),
}

// ─── 连接各来源 ──────────────────────────────────────────────────────────────

struct ConnectFailure {
    reason: String,
    /// 重试也没用：鉴权失败、上游太老、需要两步验证……
    fatal: bool,
}

impl ConnectFailure {
    fn retry(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            fatal: false,
        }
    }

    fn fatal(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            fatal: true,
        }
    }

    /// 由 `ensure_*` 给出的失败：只有错误本身能看出是不是重试也没用
    fn from_failure(failure: &Failure) -> Self {
        let fatal = matches!(
            failure.error,
            DebugError::AuthFailed { .. } | DebugError::UpstreamTooOld
        ) || matches!(failure.status, Some(DebugChannelStatus::Unsupported { .. }));
        Self {
            reason: error_text(&failure.error),
            fatal,
        }
    }
}

/// 一条已经连上的事件来源
enum Conn {
    /// NapCat 调试适配器的 WS，或者一条 OneBot WS 服务
    Ws {
        ws: Arc<WsClient>,
        events: mpsc::Receiver<Value>,
        /// NapCat 的消息事件带一大块 `raw`（调试适配器总带，OneBot 服务开了 debug 也带），
        /// 对调试没用还占缓冲
        strip_raw: bool,
        /// 连接只归接收器所有（NapCat 调试适配器）。OneBot WS 服务的连接和调用共用，
        /// 停下时只在没有调用等回包时才关
        owned: bool,
    },
    /// SnowLuma 的 SSE 事件流
    Sse {
        body: BoxStream<'static, reqwest::Result<bytes::Bytes>>,
        parser: SseParser,
        /// 这个 Bot 的 QQ 号：一个 SnowLuma 进程里可能挂着好几个账号
        uin: String,
    },
}

impl Conn {
    /// 一直读到连接结束，返回结束的原因（只进日志）
    async fn run(&mut self, receiver: &Receiver, pump_id: u64, own_calls: &OwnCalls) -> String {
        match self {
            Conn::Ws {
                ws,
                events,
                strip_raw,
                ..
            } => {
                let mut seen_dropped = ws.dropped();
                while let Some(mut event) = events.recv().await {
                    if *strip_raw {
                        strip_raw_field(&mut event);
                    }
                    receiver.push_from(pump_id, DebugEventBody::Ob11 { payload: event });
                    let dropped = ws.dropped();
                    if dropped > seen_dropped {
                        let count = u32::try_from(dropped - seen_dropped).unwrap_or(u32::MAX);
                        receiver.push_from(pump_id, DebugEventBody::Dropped { count });
                        seen_dropped = dropped;
                    }
                }
                "WebSocket 连接已断开".to_owned()
            }
            Conn::Sse { body, parser, uin } => loop {
                let chunk = match tokio::time::timeout(SSE_IDLE_TIMEOUT, body.next()).await {
                    Err(_) => {
                        return format!(
                            "{} 秒没有收到任何数据（连心跳都没有）",
                            SSE_IDLE_TIMEOUT.as_secs()
                        );
                    }
                    Ok(None) => return "事件流已结束".to_owned(),
                    Ok(Some(Err(e))) => return format!("读事件流失败：{e}"),
                    Ok(Some(Ok(chunk))) => chunk,
                };
                let frames = match parser.try_push(&chunk) {
                    Ok(frames) => frames,
                    // 一行 / 一帧攒到离谱的量：这条流不要了，重连换个干净的解析器
                    Err(too_large) => return too_large.to_string(),
                };
                for frame in frames {
                    on_snowluma_frame(
                        parse_stream_frame(&frame.data),
                        uin,
                        receiver,
                        pump_id,
                        own_calls,
                    );
                }
            },
        }
    }

    /// 这条连接不再收事件了。NapCat 调试适配器的连接归接收器所有，直接关；OneBot WS 服务的
    /// 连接放在会话里和调用共用，没有调用在等回包时也关掉 —— 否则它会一直开着（远端的还占着
    /// 一条隧道），上游推来的每条事件都白白收一遍。之后的调用发现它关了会自己重连
    fn close(&self) {
        if let Conn::Ws { ws, owned, .. } = self {
            if *owned || ws.in_flight() == 0 {
                ws.close();
            }
        }
    }
}

/// 去掉事件顶层的 `raw` 字段
pub(super) fn strip_raw_field(event: &mut Value) {
    if let Value::Object(map) = event {
        map.remove("raw");
    }
}

fn on_snowluma_frame(
    frame: SlStreamFrame,
    uin: &str,
    receiver: &Receiver,
    pump_id: u64,
    own_calls: &OwnCalls,
) {
    match frame {
        SlStreamFrame::Event { uin: from, event } if from == uin => {
            receiver.push_from(pump_id, DebugEventBody::Ob11 { payload: event });
        }
        SlStreamFrame::Action {
            uin: from,
            action,
            params,
            response,
            ms,
        } if from == uin => {
            // 调试台自己发的：调用那边已经记过一条带 request_id 的了
            if own_calls.take(&receiver.bot_id, &action, &params) {
                return;
            }
            let record = observed_call(action, params, &response, ms);
            receiver.push_from(pump_id, DebugEventBody::Call { record });
        }
        SlStreamFrame::Dropped { count } => {
            receiver.push_from(pump_id, DebugEventBody::Dropped { count });
        }
        // ready 只说明连上了（已经记过）；别的账号的、看不懂的帧不关心
        _ => {}
    }
}

/// SnowLuma 旁路看到的、别人（插件、框架）发起的一次调用
fn observed_call(action: String, mut params: Value, response: &Value, ms: u64) -> DebugCallRecord {
    // 别人发的图片 base64 同样会进事件缓冲
    cap_record_params(&mut params);
    DebugCallRecord {
        request_id: None,
        origin: DebugCallOrigin::Other,
        action,
        params,
        ok: Some(response.get("status").and_then(Value::as_str) == Some("ok")),
        retcode: response.get("retcode").and_then(Value::as_i64),
        elapsed_ms: Some(u32::try_from(ms).unwrap_or(u32::MAX)),
        message_id: response
            .get("data")
            .and_then(|data| data.get("message_id"))
            .and_then(Value::as_i64),
        error: None,
        // 上游不说别人走的是哪条路
        channel: None,
    }
}

/// 连一次事件来源。连上 / 失败都顺手把通道状态记回会话，和调用、探测看到的是同一份状态
async fn connect(
    manager: &Weak<DebugManager>,
    bot_id: &BotId,
    source: &DebugChannelId,
    epoch: &Epoch,
) -> Result<Conn, ConnectFailure> {
    let manager = manager
        .upgrade()
        .ok_or_else(|| ConnectFailure::fatal(REASON_CLOSED))?;
    // 先查一遍：Bot 已经停了的话，下面「Bot 没在运行」会被当成等它启动而一直重试
    if let Some(reason) = stale_reason(&manager, bot_id, epoch) {
        return Err(ConnectFailure::fatal(reason));
    }
    let view = manager
        .bots
        .bot(bot_id)
        .await
        .ok_or_else(|| ConnectFailure::fatal("Bot 已删除"))?;
    if !view.running() {
        return Err(ConnectFailure::retry("Bot 没有在运行"));
    }
    let plans = plan_channels(&view.config);
    let plan = plans
        .iter()
        .find(|p| &p.id == source)
        .ok_or_else(|| ConnectFailure::fatal(NO_SUCH_CHANNEL))?;
    let key = plan.id.label_key();
    let result = match source {
        DebugChannelId::Internal => connect_internal(&manager, &view, epoch).await,
        DebugChannelId::Ws { .. } => {
            let (tx, events) = mpsc::channel(EVENT_QUEUE_CAP);
            manager
                .ensure_ws(&view, plan, Some(tx), epoch)
                .await
                .map(|ws| Conn::Ws {
                    ws,
                    events,
                    // NapCat 的网络配置开了 debug 时，OneBot WS 服务推的事件也带 `raw`
                    strip_raw: view.config.bot.backend_type == BackendType::NapCat,
                    owned: false,
                })
                .map_err(|failure| (ConnectFailure::from_failure(&failure), failure.status))
        }
        DebugChannelId::Http { .. } | DebugChannelId::Auto => {
            Err((ConnectFailure::fatal("这条通道不能接收事件"), None))
        }
    };
    match result {
        Ok(conn) => {
            manager
                .apply_status(&view, &key, ChannelResult::Worked, epoch)
                .await;
            Ok(conn)
        }
        Err((failure, status)) => {
            // 建连期间 Bot 停了、或者这一轮结束了：ensure_* 报的「已取消」重试也不会好
            if let Some(reason) = stale_reason(&manager, bot_id, epoch) {
                return Err(ConnectFailure::fatal(reason));
            }
            if let Some(status) = status {
                manager
                    .apply_status(&view, &key, ChannelResult::Failed(status), epoch)
                    .await;
            }
            Err(failure)
        }
    }
}

/// 泵所属的操作已经过期：Bot 在它开始之后停过，或者这一轮已经结束（关了调试台）。
/// 这种泵永远拿不到会话，重连只会无限退避下去，得写明原因停下
fn stale_reason(manager: &DebugManager, bot_id: &BotId, epoch: &Epoch) -> Option<&'static str> {
    if manager.stopped_since(bot_id, epoch) {
        Some(REASON_BOT_STOPPED)
    } else if !manager.is_current(epoch) {
        Some(REASON_CLOSED)
    } else {
        None
    }
}

type Attempt = Result<Conn, (ConnectFailure, Option<DebugChannelStatus>)>;

async fn connect_internal(manager: &DebugManager, view: &DebugBotView, epoch: &Epoch) -> Attempt {
    let client = manager
        .ensure_internal(view, epoch)
        .await
        .map_err(|failure| (ConnectFailure::from_failure(&failure), failure.status))?;
    match client {
        InternalClient::NapCat(client) => {
            let token = client.create_adapter().await.map_err(napcat_failure)?;
            let (tx, events) = mpsc::channel(EVENT_QUEUE_CAP);
            // 适配器 WS 不走 WebUI 鉴权，只认适配器 token；每次重连都重新要一个
            // （适配器闲置被回收后再建，token 会变）
            let ws = connect_ws(&client.ws_url(&token), None, tx)
                .await
                .map_err(client_failure)?;
            Ok(Conn::Ws {
                ws: Arc::new(ws),
                events,
                strip_raw: true,
                owned: true,
            })
        }
        InternalClient::SnowLuma(client) => {
            let response = client.open_stream().await.map_err(snowluma_failure)?;
            Ok(Conn::Sse {
                body: response.bytes_stream().boxed(),
                parser: SseParser::new(),
                uin: view.config.bot.qq_id.to_string(),
            })
        }
    }
}

fn napcat_failure(err: NapCatDebugError) -> (ConnectFailure, Option<DebugChannelStatus>) {
    let fatal = matches!(
        err,
        NapCatDebugError::TooOld
            | NapCatDebugError::Unauthorized
            | NapCatDebugError::TwoFactorRequired
    );
    let reason = err.to_string();
    let status = from_napcat(err, PROBE_TIMEOUT).status;
    (ConnectFailure { reason, fatal }, status)
}

fn snowluma_failure(err: SnowLumaDebugError) -> (ConnectFailure, Option<DebugChannelStatus>) {
    // 登录被拒（密码错 / 被锁 / 要两步验证）和要先同意协议：客户端已经记住不再去撞登录，
    // 重试只会一遍遍报同样的错
    let fatal = matches!(
        err,
        SnowLumaDebugError::TooOld
            | SnowLumaDebugError::Unauthorized
            | SnowLumaDebugError::LoginBlocked(_)
            | SnowLumaDebugError::NeedsConsent
    );
    let reason = err.to_string();
    let status = from_snowluma(err, PROBE_TIMEOUT).status;
    (ConnectFailure { reason, fatal }, status)
}

fn client_failure(err: ClientError) -> (ConnectFailure, Option<DebugChannelStatus>) {
    let fatal = matches!(err, ClientError::Unauthorized(_));
    let reason = err.to_string();
    let status = from_client(err, PROBE_TIMEOUT).status;
    (ConnectFailure { reason, fatal }, status)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn canonical_json_sorts_keys_at_every_level() {
        let a = json!({"b": 1, "a": {"y": [ {"d": 1, "c": 2} ], "x": null}});
        let b = json!({"a": {"x": null, "y": [ {"c": 2, "d": 1} ]}, "b": 1});
        assert_eq!(canonical_json(&a), canonical_json(&b));
        assert_eq!(
            canonical_json(&a),
            r#"{"a":{"x":null,"y":[{"c":2,"d":1}]},"b":1}"#
        );
    }

    #[test]
    fn raw_is_removed_only_at_the_top_level() {
        let mut event =
            json!({"post_type": "message", "raw": {"huge": true}, "message": [{"raw": 1}]});
        strip_raw_field(&mut event);
        assert_eq!(
            event,
            json!({"post_type": "message", "message": [{"raw": 1}]})
        );
    }

    #[tokio::test(start_paused = true)]
    async fn own_calls_match_once_regardless_of_key_order() {
        let calls = OwnCalls::default();
        let bot = BotId::new("1");
        calls.note(&bot, "send_msg", &json!({"b": 2, "a": 1}));
        calls.note(&bot, "send_msg", &json!({"a": 1, "b": 2}));
        assert!(!calls.take(&bot, "send_msg", &json!({"a": 9})));
        assert!(!calls.take(&BotId::new("2"), "send_msg", &json!({"a": 1, "b": 2})));
        assert!(calls.take(&bot, "send_msg", &json!({"a": 1, "b": 2})));
        assert!(calls.take(&bot, "send_msg", &json!({"b": 2, "a": 1})));
        assert!(
            !calls.take(&bot, "send_msg", &json!({"a": 1, "b": 2})),
            "每条指纹只认一次"
        );

        // 空参数和 null 是一回事
        calls.note(&bot, "get_status", &json!(null));
        assert!(calls.take(&bot, "get_status", &json!({})));
    }

    #[tokio::test(start_paused = true)]
    async fn own_calls_expire_and_are_capped() {
        let calls = OwnCalls::default();
        let bot = BotId::new("1");
        calls.note(&bot, "old", &json!({}));
        tokio::time::advance(OWN_CALL_TTL + Duration::from_millis(1)).await;
        assert!(!calls.take(&bot, "old", &json!({})), "过了 10 秒不再认");

        for n in 0..=OWN_CALLS_KEPT {
            calls.note(&bot, "send_msg", &json!({ "n": n }));
        }
        assert!(
            !calls.take(&bot, "send_msg", &json!({"n": 0})),
            "超出上限的最老一条被挤掉"
        );
        assert!(calls.take(&bot, "send_msg", &json!({"n": 1})));
        assert!(calls.take(&bot, "send_msg", &json!({ "n": OWN_CALLS_KEPT })));
    }

    #[test]
    fn observed_calls_carry_the_upstream_reply() {
        let record = observed_call(
            "send_group_msg".into(),
            json!({"group_id": 1}),
            &json!({"status": "ok", "retcode": 0, "data": {"message_id": 77}}),
            12,
        );
        assert_eq!(record.origin, DebugCallOrigin::Other);
        assert_eq!(record.request_id, None);
        assert_eq!(record.ok, Some(true));
        assert_eq!(record.retcode, Some(0));
        assert_eq!(record.elapsed_ms, Some(12));
        assert_eq!(record.message_id, Some(77));
        assert_eq!(record.channel, None);

        let failed = observed_call(
            "x".into(),
            json!({}),
            &json!({"status": "failed", "retcode": 100}),
            0,
        );
        assert_eq!(failed.ok, Some(false));
        assert_eq!(failed.message_id, None);

        // 别人发图的 base64 也要瘦身，事件缓冲按条数封顶，不按字节
        let big = observed_call(
            "send_group_msg".into(),
            json!({"group_id": 7, "message": [{"type": "image", "data": {"file": "B".repeat(200_000)}}]}),
            &json!({"status": "ok"}),
            1,
        );
        assert_eq!(big.params["group_id"], 7);
        assert_eq!(
            big.params["message"][0]["data"]["file"],
            "<已省略 200000 字节>"
        );
    }

    // ─── 单个接收器 ─────────────────────────────────────────────────────────

    #[derive(Default)]
    struct Collect {
        batches: StdMutex<Vec<DebugEventBatch>>,
        gone: std::sync::atomic::AtomicBool,
    }

    impl DebugEventSink for Collect {
        fn send(&self, batch: &DebugEventBatch) -> bool {
            if self.gone.load(std::sync::atomic::Ordering::SeqCst) {
                return false;
            }
            lock_std(&self.batches).push(batch.clone());
            true
        }
    }

    impl Collect {
        fn seqs(&self) -> Vec<u64> {
            lock_std(&self.batches)
                .iter()
                .flat_map(|b| b.events.iter().map(|e| e.seq))
                .collect()
        }

        fn batch_count(&self) -> usize {
            lock_std(&self.batches).len()
        }
    }

    fn ob11(n: u64) -> DebugEventBody {
        DebugEventBody::Ob11 {
            payload: json!({ "n": n }),
        }
    }

    fn receiver(next_seq: u64) -> Receiver {
        Receiver::new(
            BotId::new("1"),
            DebugChannelId::Internal,
            next_seq,
            CancellationToken::new(),
        )
    }

    #[test]
    fn a_viewer_joining_between_flushes_sees_every_event_exactly_once() {
        let receiver = receiver(7);
        let first = Arc::new(Collect::default());
        receiver.attach("a", Arc::clone(&first) as Arc<dyn DebugEventSink>);
        for n in 0..3 {
            receiver.push_from(0, ob11(n));
        }
        // 三条还攒着没推时来了第二个窗口
        let second = Arc::new(Collect::default());
        let info = receiver
            .attach("b", Arc::clone(&second) as Arc<dyn DebugEventSink>)
            .unwrap();
        assert_eq!(info.viewers, 2);
        assert_eq!(first.seqs(), [7, 8, 9], "攒着的先推给已有的窗口");
        assert_eq!(second.seqs(), [7, 8, 9], "新窗口从补发里拿到它们");
        for n in 3..5 {
            receiver.push_from(0, ob11(n));
        }
        receiver.flush();
        receiver.flush();
        assert_eq!(first.seqs(), [7, 8, 9, 10, 11]);
        assert_eq!(second.seqs(), [7, 8, 9, 10, 11]);

        // 旧泵迟到的写入对不上号，丢掉
        receiver.push_from(99, ob11(9));
        receiver.flush();
        assert_eq!(first.seqs().len(), 5);
    }

    #[test]
    fn restarting_a_halted_receiver_says_connecting_right_away() {
        let receiver = receiver(1);
        receiver.halt(0, "鉴权失败".into());
        assert!(matches!(
            receiver.info().state,
            DebugReceiverState::Stopped { .. }
        ));
        let pump = receiver
            .restart_if_needed(None, &DebugChannelId::Internal)
            .unwrap();
        assert!(pump.announced);
        assert_eq!(pump.pump_id, 1);
        assert_eq!(receiver.info().state, DebugReceiverState::Connecting);
        let last = receiver.read_since(0, 100).pop().unwrap();
        assert!(matches!(
            last.body,
            DebugEventBody::Receiver {
                state: DebugReceiverState::Connecting,
                ..
            }
        ));
        // 已经在用这个来源、也没停：不换泵
        assert!(
            receiver
                .restart_if_needed(Some(&DebugChannelId::Internal), &DebugChannelId::Internal)
                .is_none()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn kicks_are_coalesced_and_do_not_leak_into_the_next_pump() {
        use futures_util::FutureExt;

        let receiver = receiver(1);
        let first = receiver.first_pump();
        receiver.kick();
        // 泵收到第一次、已经去重连了，第二次紧跟着到
        assert!(first.kick.notified().now_or_never().is_some());
        receiver.kick();
        assert!(
            first.kick.notified().now_or_never().is_none(),
            "两秒内的第二次只算一次"
        );

        tokio::time::advance(KICK_COALESCE + Duration::from_millis(1)).await;
        receiver.kick();
        // 这次没被用掉就换了泵：新泵不该一上来就重连
        let next = receiver
            .restart_if_needed(
                Some(&DebugChannelId::Ws { name: "ev".into() }),
                &DebugChannelId::Internal,
            )
            .unwrap();
        assert!(next.kick.notified().now_or_never().is_none());
    }

    #[test]
    fn probing_drops_windows_that_went_away_on_a_quiet_receiver() {
        let receiver = receiver(1);
        let stays = Arc::new(Collect::default());
        let leaves = Arc::new(Collect::default());
        receiver.attach("stays", Arc::clone(&stays) as Arc<dyn DebugEventSink>);
        receiver.attach("leaves", Arc::clone(&leaves) as Arc<dyn DebugEventSink>);
        leaves.gone.store(true, std::sync::atomic::Ordering::SeqCst);

        receiver.probe_sinks();
        assert_eq!(receiver.info().viewers, 1);
        assert_eq!(stays.batch_count(), 1, "探测发的是空批次");
        assert!(stays.seqs().is_empty());
    }
}
