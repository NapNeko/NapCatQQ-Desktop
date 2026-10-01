//! 动作调用与连通探测：解析通道、按需建客户端和隧道、发请求、把结果折算成界面上的样子，
//! 并把这次结果说明的通道状态记回会话。
//!
//! 所有会写会话表的步骤都带着操作开始时的 [`Epoch`]：`close_all` 之后迟到的写入会被丢掉，
//! 半路新建的连接和隧道随之关闭，不会在关掉的调试台背后留下一条 SSH 隧道。

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use ncd_backend_napcat::NapCatDebugClient;
use ncd_backend_snowluma::SnowLumaDebugClient;
use ncd_domain::bot_config::BackendType;
use ncd_domain::ids::BotId;
use ncd_domain::onebot_debug::{
    DebugCallOrigin, DebugCallOutcome, DebugCallRecord, DebugCallRequest, DebugCallResponse,
    DebugCallResult, DebugChannelId, DebugChannelStatus, DebugError,
};
use ncd_host::remote::TunnelSpec;
use ncd_onebot::client::{
    ClientError, HttpActionClient, RawCall, WsClient, connect_ws, outcome_from,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::errors::{
    Failure, duration_ms, error_text, from_client, from_napcat, from_snowluma,
    napcat_call_failure_reply,
};
use super::params::cap_record_params;
use super::plan::{ChannelPlan, Reach, pick_auto, plan_channels};
use super::port::DebugBotView;
use super::session::{BotSession, INTERNAL_KEY, OpenTunnel};
use super::storage::history_entry_from_call;
use super::{DebugManager, Epoch, NO_CALL_CHANNEL, NO_SUCH_CHANNEL, auto_exclusions, lock_std};

/// 没给超时时等一分钟：上传文件、拉长列表之类的动作确实会慢
const DEFAULT_TIMEOUT_MS: u32 = 60_000;
const MIN_TIMEOUT_MS: u32 = 1_000;
const MAX_TIMEOUT_MS: u32 = 600_000;
/// 「测试连通」的总时限（含登录、开隧道、建连）
pub(super) const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// 已经就绪的内部通道客户端
pub(super) enum InternalClient {
    NapCat(Arc<NapCatDebugClient>),
    SnowLuma(Arc<SnowLumaDebugClient>),
}

/// 一次调用 / 探测的结果对通道状态意味着什么
pub(super) enum ChannelResult {
    /// 通了：直连记可用，走隧道的记隧道本机口
    Worked,
    /// 失败本身说明了通道的状况（连不上、鉴权没过……）。连不上的顺带丢掉连接和隧道
    Failed(DebugChannelStatus),
    /// 只记下来给界面看，不动连接和隧道：超时、上游回了 5xx 只说明「这一次」不行，
    /// 隧道和连接本身多半是好的，拆了反而要重新开一遍 SSH
    Noted(DebugChannelStatus),
}

/// 桌面端实际要连的地址
struct Endpoint {
    host: String,
    port: u16,
    path: String,
}

/// 一次调用的边界：属于哪一轮、自己的取消令牌、总的截止时间
struct CallScope {
    epoch: Epoch,
    token: CancellationToken,
    deadline: Instant,
    timeout: Duration,
}

impl DebugManager {
    pub async fn call(&self, req: DebugCallRequest) -> DebugCallResponse {
        // 取消令牌在第一个 await 之前登记：前端紧跟着发来的「取消」不会因为还没登记而落空
        let token = CancellationToken::new();
        let result = match InflightGuard::register(self, &req.request_id, token.clone()) {
            Err(error) => DebugCallResult::Err { error },
            Ok(_inflight) => {
                let timeout = effective_timeout(req.timeout_ms);
                let scope = CallScope {
                    epoch: self.epoch(),
                    token,
                    deadline: Instant::now() + timeout,
                    timeout,
                };
                self.call_in_scope(&req, &scope).await
            }
        };
        DebugCallResponse {
            request_id: req.request_id,
            result,
        }
    }

    /// SnowLuma 的事件流会把我们自己发的调用也原样回放一遍（不带来源）。
    /// 在这里登记这次调用的指纹，回放到达时事件接收器认出来跳过
    pub(super) fn note_own_call(&self, bot: &BotId, action: &str, params: &Value) {
        self.own_calls.note(bot, action, params);
    }

    /// 每次调用结束后：把调用写进事件流（聊天视图里显示「我发了什么」），测试台、输入框和
    /// MCP 发起的再记进历史。不等历史落盘，调用结果马上返回。
    ///
    /// 调用所属的一轮已经结束（关了调试台 / close_all）时什么都不记：界面那头已经关了，
    /// 这时候再冒出一条记录只会让重新打开的调试台看到一次「没发过」的调用
    pub(super) fn record_call(
        &self,
        view: &DebugBotView,
        req: &DebugCallRequest,
        channel: &DebugChannelId,
        result: &DebugCallResult,
        epoch: &Epoch,
    ) {
        if !self.is_current(epoch) {
            return;
        }
        let bot_id = view.bot_id();
        // 命令面板挑参数时的查询不算用户的调用，不进聊天
        if req.origin != DebugCallOrigin::Picker {
            self.push_call_record(&bot_id, own_call_record(req, channel, result));
        }
        if let Some(entry) = history_entry_from_call(
            bot_id.as_str(),
            &view.config.bot.name,
            view.config.bot.backend_type,
            req,
            channel,
            result,
        ) {
            self.queue_history(entry);
        }
    }

    async fn call_in_scope(&self, req: &DebugCallRequest, scope: &CallScope) -> DebugCallResult {
        // 找 Bot 这一步也可能慢（读配置），同样受取消和超时约束；在这里被打断就没有可记的 Bot
        let view = match self.guarded(scope, self.bot_for_call(req)).await {
            Ok(Ok(view)) => view,
            Ok(Err(error)) | Err(error) => return DebugCallResult::Err { error },
        };
        let (channel, result) = self.call_with_view(&view, req, scope).await;
        self.record_call(&view, req, &channel, &result, &scope.epoch);
        result
    }

    /// 让 `work` 跑到完成，或者先被关调试台 / 取消 / 超时打断
    async fn guarded<T>(
        &self,
        scope: &CallScope,
        work: impl Future<Output = T>,
    ) -> Result<T, DebugError> {
        tokio::select! {
            biased;
            _ = scope.epoch.ended() => Err(self.stale_error()),
            _ = scope.token.cancelled() => Err(DebugError::Cancelled),
            out = work => Ok(out),
            _ = tokio::time::sleep_until(scope.deadline) => Err(DebugError::Timeout {
                ms: duration_ms(scope.timeout),
            }),
        }
    }

    async fn bot_for_call(&self, req: &DebugCallRequest) -> Result<DebugBotView, DebugError> {
        if !self.is_enabled() {
            return Err(DebugError::FeatureDisabled);
        }
        self.bots
            .bot(&BotId::new(req.bot_id.as_str()))
            .await
            .ok_or(DebugError::BotNotFound)
    }

    /// 返回实际用到的通道（没走到解析通道那一步时是请求里的原样）和结果
    async fn call_with_view(
        &self,
        view: &DebugBotView,
        req: &DebugCallRequest,
        scope: &CallScope,
    ) -> (DebugChannelId, DebugCallResult) {
        let fail = |error| DebugCallResult::Err { error };
        if !view.running() {
            return (req.channel.clone(), fail(DebugError::BotNotRunning));
        }
        let params = match normalize_params(&req.params) {
            Ok(params) => params,
            Err(error) => return (req.channel.clone(), fail(error)),
        };
        let plans = plan_channels(&view.config);
        // 被取消 / 超时打断时要知道当时落在哪条通道上（可能已经降级到下一条）
        let current = StdMutex::new(req.channel.clone());

        let work = async {
            let excluded = auto_exclusions(self.refresh_session(view).await);
            let first =
                resolve_call_channel(&plans, &req.channel, &excluded).map_err(Failure::new)?;
            *lock_std(&current) = first.id.clone();
            // SnowLuma 的事件流会回放这个账号执行的每个动作，不管是从调试接口还是从 OneBot
            // HTTP / WS 进来的。先登记再发（回放可能比回包先到），一次调用只登记一次：
            // 降级重试时第一条通道并没有真的执行
            if view.config.bot.backend_type == BackendType::SnowLuma {
                self.note_own_call(&view.bot_id(), &req.action, &params);
            }
            let outcome = self.dispatch(view, first, req, &params, scope).await;
            let fell_back = req.channel == DebugChannelId::Auto
                && first.id == DebugChannelId::Internal
                && matches!(&outcome, Err(f) if f.error == DebugError::UpstreamTooOld);
            if !fell_back {
                return outcome;
            }
            // 「自动」选中的内部通道太老：换下一条自动通道再试一次
            let mut excluded = excluded;
            excluded.insert(DebugChannelId::Internal);
            let next =
                pick_auto(&plans, true, &excluded).and_then(|id| plans.iter().find(|p| p.id == id));
            match next {
                Some(next) => {
                    *lock_std(&current) = next.id.clone();
                    self.dispatch(view, next, req, &params, scope).await
                }
                // 没有别的通道可换：「上游太老」比「没有通道」更能说明该怎么办
                None => outcome,
            }
        };

        let result = match self.guarded(scope, work).await {
            Ok(Ok(outcome)) => DebugCallResult::Ok { outcome },
            Ok(Err(failure)) => DebugCallResult::Err {
                error: failure.error,
            },
            Err(error) => DebugCallResult::Err { error },
        };
        let channel = lock_std(&current).clone();
        (channel, result)
    }

    /// 在一条已解析的通道上调一次，并把结果说明的通道状态记回会话
    async fn dispatch(
        &self,
        view: &DebugBotView,
        plan: &ChannelPlan,
        req: &DebugCallRequest,
        params: &Value,
        scope: &CallScope,
    ) -> Result<DebugCallOutcome, Failure> {
        let epoch = &scope.epoch;
        let remaining = scope.deadline.saturating_duration_since(Instant::now());
        let timeout = scope.timeout;
        let raw = match &plan.id {
            DebugChannelId::Internal => {
                self.call_internal(view, &req.action, params, remaining, timeout, epoch)
                    .await
            }
            DebugChannelId::Http { .. } => match self.ensure_http(view, plan, epoch).await {
                Ok(client) => {
                    let started = Instant::now();
                    client
                        .call(&req.action, params, remaining)
                        .await
                        .map(|raw| (raw, started.elapsed()))
                        .map_err(|e| from_client(e, timeout))
                }
                Err(failure) => Err(failure),
            },
            DebugChannelId::Ws { .. } => match self.ensure_ws(view, plan, None, epoch).await {
                Ok(client) => {
                    let started = Instant::now();
                    self.call_on_ws(
                        client,
                        view,
                        plan,
                        &req.action,
                        params,
                        scope.deadline,
                        epoch,
                    )
                    .await
                    .map(|raw| (raw, started.elapsed()))
                    .map_err(|e| from_client(e, timeout))
                }
                Err(failure) => Err(failure),
            },
            DebugChannelId::Auto => Err(Failure::new(DebugError::Internal {
                message: "「自动」通道没有解析成具体通道".to_owned(),
            })),
        };

        let key = plan.id.label_key();
        match raw {
            Ok((raw, elapsed)) => {
                self.apply_status(view, &key, ChannelResult::Worked, epoch)
                    .await;
                let outcome = outcome_from(&raw.text, raw.value, elapsed, plan.id.clone());
                if outcome.truncated {
                    self.keep_large_response(&req.request_id, raw.text, epoch);
                }
                Ok(outcome)
            }
            Err(failure) => {
                if let Some(status) = &failure.status {
                    self.apply_status(view, &key, ChannelResult::Failed(status.clone()), epoch)
                        .await;
                }
                Err(failure)
            }
        }
    }

    async fn call_internal(
        &self,
        view: &DebugBotView,
        action: &str,
        params: &Value,
        remaining: Duration,
        timeout: Duration,
        epoch: &Epoch,
    ) -> Result<(RawCall, Duration), Failure> {
        match self.ensure_internal(view, epoch).await? {
            InternalClient::NapCat(client) => {
                let started = Instant::now();
                match client.call(action, params, remaining).await {
                    Ok(raw) => Ok((raw, started.elapsed())),
                    Err(err) => match napcat_call_failure_reply(&err) {
                        Some(value) => Ok((
                            RawCall {
                                text: value.to_string(),
                                value,
                            },
                            started.elapsed(),
                        )),
                        None => Err(from_napcat(err, timeout)),
                    },
                }
            }
            InternalClient::SnowLuma(client) => {
                let uin = view.config.bot.qq_id.to_string();
                let started = Instant::now();
                client
                    .invoke(&uin, action, params, remaining)
                    .await
                    .map(|raw| (raw, started.elapsed()))
                    .map_err(|e| from_snowluma(e, timeout))
            }
        }
    }

    /// 测一次连通并记下状态。只有这次探测所属的一轮已经结束（关了调试台）时才返回错误
    pub(super) async fn probe(
        &self,
        view: &DebugBotView,
        plan: &ChannelPlan,
        epoch: &Epoch,
    ) -> Result<(), DebugError> {
        let probe = async {
            match &plan.id {
                DebugChannelId::Internal => self.probe_internal(view, epoch).await,
                DebugChannelId::Http { .. } => {
                    let client = self.ensure_http(view, plan, epoch).await?;
                    client
                        .call("get_status", &json!({}), PROBE_TIMEOUT)
                        .await
                        .map(drop)
                        .map_err(|e| from_client(e, PROBE_TIMEOUT))
                }
                // 只推事件的 WS 服务（role = Event）不回动作，发 get_status 只会干等到超时：
                // 握手成功就算通了
                DebugChannelId::Ws { .. } if !plan.can_call => {
                    self.probe_ws_handshake(view, plan, epoch).await
                }
                DebugChannelId::Ws { .. } => {
                    let client = self.ensure_ws(view, plan, None, epoch).await?;
                    let deadline = Instant::now() + PROBE_TIMEOUT;
                    self.call_on_ws(
                        client,
                        view,
                        plan,
                        "get_status",
                        &json!({}),
                        deadline,
                        epoch,
                    )
                    .await
                    .map(drop)
                    .map_err(|e| from_client(e, PROBE_TIMEOUT))
                }
                DebugChannelId::Auto => Err(Failure::new(DebugError::Internal {
                    message: "「自动」通道没有解析成具体通道".to_owned(),
                })),
            }
        };
        let result = tokio::select! {
            biased;
            _ = epoch.ended() => return Err(self.stale_error()),
            result = tokio::time::timeout(PROBE_TIMEOUT, probe) => match result {
                Ok(result) => result,
                Err(_) => Err(Failure::new(DebugError::Timeout {
                    ms: duration_ms(PROBE_TIMEOUT),
                })),
            },
        };
        let update = match result {
            Ok(()) => ChannelResult::Worked,
            // 失败本身说明了通道状况（连不上、鉴权没过）才按失败处理；超时、5xx 只记给界面看
            Err(failure) if failure.status.is_some() => {
                ChannelResult::Failed(failure.probe_status())
            }
            Err(failure) => ChannelResult::Noted(failure.probe_status()),
        };
        self.apply_status(view, &plan.id.label_key(), update, epoch)
            .await;
        if self.is_current(epoch) {
            Ok(())
        } else {
            Err(self.stale_error())
        }
    }

    /// 在一条已经拿到手的 WS 连接上调一次。拿到之后、请求写出去之前连接被关掉了（接收器停下时
    /// 关了这条共用连接，恰好撞上）：上游根本没收到，换一条新连接重发一次。只重发这一种，
    /// 请求写出去之后才断的不重发 —— `send_msg` 之类有副作用，不能让上游执行两遍
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn call_on_ws(
        &self,
        client: Arc<WsClient>,
        view: &DebugBotView,
        plan: &ChannelPlan,
        action: &str,
        params: &Value,
        deadline: Instant,
        epoch: &Epoch,
    ) -> Result<RawCall, ClientError> {
        let remaining = || deadline.saturating_duration_since(Instant::now());
        match client.call(action, params, remaining()).await {
            Err(ClientError::NotSent) => {
                // 关掉的那条会被 ensure_ws 跳过，这里拿到的是新连接；连不上就照原样报没发出去
                let Ok(fresh) = self.ensure_ws(view, plan, None, epoch).await else {
                    return Err(ClientError::NotSent);
                };
                fresh.call(action, params, remaining()).await
            }
            other => other,
        }
    }

    /// 只握一次手就关：这条连接收不到回包，留在会话里也没人用
    async fn probe_ws_handshake(
        &self,
        view: &DebugBotView,
        plan: &ChannelPlan,
        epoch: &Epoch,
    ) -> Result<(), Failure> {
        {
            // 和 ensure_ws 一样先按当前规划校一遍：记下的状态要对得上这份规划才算数
            let mut sessions = self.sessions.lock().await;
            self.live_session(&mut sessions, view, epoch)
                .ok_or_else(|| Failure::new(self.stale_error()))?
                .sync_plan(plan);
        }
        let endpoint = self.resolve_endpoint(view, plan, epoch).await?;
        let url = format!(
            "ws://{}:{}{}",
            url_host(&endpoint.host),
            endpoint.port,
            endpoint.path
        );
        // 事件接收端当场丢掉：连接收到的事件直接跳过
        let (events, _) = mpsc::channel(1);
        let ws = connect_ws(&url, plan.token.as_deref(), events)
            .await
            .map_err(|e| from_client(e, PROBE_TIMEOUT))?;
        ws.close();
        Ok(())
    }

    /// 内部通道的探测就是取一次动作目录：接口在不在一试便知，取到的目录顺手缓存
    async fn probe_internal(&self, view: &DebugBotView, epoch: &Epoch) -> Result<(), Failure> {
        let client = self.ensure_internal(view, epoch).await?;
        let data = self.fetch_catalog_data(&client).await?;
        self.cache_live_catalog(view, &data, epoch).await;
        Ok(())
    }

    /// 取上游的原始目录数据
    pub(super) async fn fetch_catalog_data(
        &self,
        client: &InternalClient,
    ) -> Result<Value, Failure> {
        match client {
            InternalClient::NapCat(c) => {
                c.schemas().await.map_err(|e| from_napcat(e, PROBE_TIMEOUT))
            }
            InternalClient::SnowLuma(c) => c
                .actions()
                .await
                .map_err(|e| from_snowluma(e, PROBE_TIMEOUT)),
        }
    }

    /// 把结果记成通道状态。旧一轮的迟到结果直接丢掉
    pub(super) async fn apply_status(
        &self,
        view: &DebugBotView,
        key: &str,
        result: ChannelResult,
        epoch: &Epoch,
    ) {
        let mut sessions = self.sessions.lock().await;
        let Some(session) = self.live_session(&mut sessions, view, epoch) else {
            return;
        };
        session.touch();
        let (status, teardown) = match result {
            ChannelResult::Worked => (session.ok_status(key), false),
            ChannelResult::Failed(status) => (status, true),
            ChannelResult::Noted(status) => (status, false),
        };
        if key == INTERNAL_KEY {
            match status {
                DebugChannelStatus::UpstreamTooOld => session.internal_too_old = true,
                // 接口又能用了（上游升级过）
                DebugChannelStatus::Available => session.internal_too_old = false,
                _ => {}
            }
        } else if teardown && matches!(status, DebugChannelStatus::Unreachable { .. }) {
            // 连不上时丢掉这条通道的连接和隧道：远端 SSH 断过的话旧隧道已经没用，下次重建
            session.http.remove(key);
            session.ws.remove(key);
            session.tunnels.remove(key);
        }
        session.status.insert(key.to_owned(), status);
    }

    /// 内部通道的客户端。WebUI 端点每次都重新问一遍（很便宜），变了就换新客户端
    pub(super) async fn ensure_internal(
        &self,
        view: &DebugBotView,
        epoch: &Epoch,
    ) -> Result<InternalClient, Failure> {
        let (port, secret) = self
            .internal_endpoint(view)
            .await
            .map_err(Failure::unreachable)?;
        let mut sessions = self.sessions.lock().await;
        let session = self
            .live_session(&mut sessions, view, epoch)
            .ok_or_else(|| Failure::new(self.stale_error()))?;
        session.sync_internal_endpoint(port, &secret);
        // 客户端的构造只建 reqwest 客户端、不碰网络，可以在锁里做
        match view.config.bot.backend_type {
            BackendType::NapCat => {
                if let Some(client) = &session.napcat {
                    return Ok(InternalClient::NapCat(Arc::clone(client)));
                }
                let client = Arc::new(
                    NapCatDebugClient::new(port, secret)
                        .map_err(|e| from_napcat(e, Duration::ZERO))?,
                );
                session.napcat = Some(Arc::clone(&client));
                Ok(InternalClient::NapCat(client))
            }
            BackendType::SnowLuma => {
                if let Some(client) = &session.snowluma {
                    return Ok(InternalClient::SnowLuma(Arc::clone(client)));
                }
                let client = Arc::new(
                    SnowLumaDebugClient::new(port, secret)
                        .map_err(|e| from_snowluma(e, Duration::ZERO))?,
                );
                session.snowluma = Some(Arc::clone(&client));
                Ok(InternalClient::SnowLuma(client))
            }
        }
    }

    pub(super) async fn ensure_http(
        &self,
        view: &DebugBotView,
        plan: &ChannelPlan,
        epoch: &Epoch,
    ) -> Result<Arc<HttpActionClient>, Failure> {
        let key = plan.id.label_key();
        {
            let mut sessions = self.sessions.lock().await;
            let session = self
                .live_session(&mut sessions, view, epoch)
                .ok_or_else(|| Failure::new(self.stale_error()))?;
            session.sync_plan(plan);
            if let Some(client) = session.http.get(&key) {
                return Ok(Arc::clone(client));
            }
        }
        let endpoint = self.resolve_endpoint(view, plan, epoch).await?;
        let base = format!(
            "http://{}:{}{}",
            url_host(&endpoint.host),
            endpoint.port,
            endpoint.path
        );
        let client = Arc::new(
            HttpActionClient::new(&base, plan.token.clone())
                .map_err(|e| from_client(e, Duration::ZERO))?,
        );
        let mut sessions = self.sessions.lock().await;
        let session = self
            .live_session(&mut sessions, view, epoch)
            .ok_or_else(|| Failure::new(self.stale_error()))?;
        // 并发时别人可能先建好了：用先到的那个
        Ok(Arc::clone(session.http.entry(key).or_insert(client)))
    }

    /// WS 通道的连接。`events` 为空时是只为调用建的连接：有活着的就复用，推来的事件直接丢；
    /// 给了 `events` 表示要收这条通道的事件，总是新建一条带事件出口的连接顶替掉旧的
    /// （旧连接上还在等回包的调用拿着自己的克隆，不受影响）
    pub(super) async fn ensure_ws(
        &self,
        view: &DebugBotView,
        plan: &ChannelPlan,
        events: Option<mpsc::Sender<Value>>,
        epoch: &Epoch,
    ) -> Result<Arc<WsClient>, Failure> {
        let key = plan.id.label_key();
        let share = events.is_none();
        {
            let mut sessions = self.sessions.lock().await;
            let session = self
                .live_session(&mut sessions, view, epoch)
                .ok_or_else(|| Failure::new(self.stale_error()))?;
            session.sync_plan(plan);
            if share {
                if let Some(ws) = live_ws(session, &key) {
                    return Ok(ws);
                }
            }
        }
        let endpoint = self.resolve_endpoint(view, plan, epoch).await?;
        let url = format!(
            "ws://{}:{}{}",
            url_host(&endpoint.host),
            endpoint.port,
            endpoint.path
        );
        let events = events.unwrap_or_else(|| {
            // 接收端当场丢掉：连接收到的事件 try_send 失败即丢，不会堵住回包
            let (tx, _rx) = mpsc::channel(1);
            tx
        });
        let ws = Arc::new(
            connect_ws(&url, plan.token.as_deref(), events)
                .await
                .map_err(|e| from_client(e, PROBE_TIMEOUT))?,
        );
        let mut sessions = self.sessions.lock().await;
        let Some(session) = self.live_session(&mut sessions, view, epoch) else {
            // 这一轮已经结束：刚连上的也不留
            ws.close();
            return Err(Failure::new(self.stale_error()));
        };
        if share {
            if let Some(existing) = live_ws(session, &key) {
                ws.close();
                return Ok(existing);
            }
        }
        session.ws.insert(key, Arc::clone(&ws));
        Ok(ws)
    }

    /// 通道规划 → 桌面端要连的地址；远端的要先开（或复用）隧道
    async fn resolve_endpoint(
        &self,
        view: &DebugBotView,
        plan: &ChannelPlan,
        epoch: &Epoch,
    ) -> Result<Endpoint, Failure> {
        match &plan.reach {
            Reach::Direct { host, port, path } => Ok(Endpoint {
                host: host.clone(),
                port: *port,
                path: path.clone(),
            }),
            Reach::Tunnel {
                remote_host,
                remote_port,
                path,
            } => {
                let local_port = self
                    .ensure_tunnel(view, &plan.id.label_key(), remote_host, *remote_port, epoch)
                    .await?;
                Ok(Endpoint {
                    host: "127.0.0.1".to_owned(),
                    port: local_port,
                    path: path.clone(),
                })
            }
            Reach::Unsupported { reason } => Err(Failure::unsupported(reason.clone())),
            Reach::Internal => Err(Failure::new(DebugError::Internal {
                message: "内部通道没有独立的地址".to_owned(),
            })),
        }
    }

    /// 到远端 `remote_host:remote_port` 的隧道，返回本机口。已有就复用
    pub(super) async fn ensure_tunnel(
        &self,
        view: &DebugBotView,
        key: &str,
        remote_host: &str,
        remote_port: u16,
        epoch: &Epoch,
    ) -> Result<u16, Failure> {
        {
            let mut sessions = self.sessions.lock().await;
            // 这一轮已经结束、或者 Bot 已经停了：别再去开一条注定要被丢掉的隧道
            let session = self
                .live_session(&mut sessions, view, epoch)
                .ok_or_else(|| Failure::new(self.stale_error()))?;
            if let Some(tunnel) = session.tunnels.get(key) {
                return Ok(tunnel.local_port());
            }
        }
        let host = self
            .host_resolver
            .resolve(&view.config.bot.runtime_target)
            .await
            .map_err(|e| Failure::unreachable(format!("连不上远端主机：{e}")))?;
        let spec = TunnelSpec {
            remote_host: remote_host.to_owned(),
            ..TunnelSpec::local_to_remote(0, remote_port)
        };
        let handle = host
            .open_tunnel(spec)
            .await
            .map_err(|e| Failure::unreachable(format!("建立 SSH 隧道失败：{e}")))?;
        let handle = OpenTunnel::new(handle, &self.open_tunnels);
        let mut sessions = self.sessions.lock().await;
        // 这一轮已经结束：隧道句柄在这里被丢弃，随即关闭
        let session = self
            .live_session(&mut sessions, view, epoch)
            .ok_or_else(|| Failure::new(self.stale_error()))?;
        // 并发时别人可能先开好了：用先到的那条，自己这条在 or_insert 里被丢弃，随即关闭
        let local_port = session
            .tunnels
            .entry(key.to_owned())
            .or_insert(handle)
            .local_port();
        session
            .status
            .insert(key.to_owned(), DebugChannelStatus::Tunneled { local_port });
        Ok(local_port)
    }
}

/// 我们自己发起的一次调用在事件流里的样子
fn own_call_record(
    req: &DebugCallRequest,
    channel: &DebugChannelId,
    result: &DebugCallResult,
) -> DebugCallRecord {
    let mut params = normalize_params(&req.params).unwrap_or_else(|_| req.params.clone());
    // 事件缓冲按条数封顶，发图的 base64 不瘦身的话几十条就是几百 MB
    cap_record_params(&mut params);
    let base = DebugCallRecord {
        request_id: Some(req.request_id.clone()),
        origin: req.origin.clone(),
        action: req.action.clone(),
        params,
        ok: Some(false),
        retcode: None,
        elapsed_ms: None,
        message_id: None,
        error: None,
        channel: Some(channel.clone()),
    };
    match result {
        DebugCallResult::Ok { outcome } => DebugCallRecord {
            ok: Some(outcome.ok),
            retcode: Some(outcome.retcode),
            elapsed_ms: Some(outcome.elapsed_ms),
            // 聊天视图拿它把「我发的」和随后到的 message_sent 合成一条
            message_id: outcome.data.get("message_id").and_then(Value::as_i64),
            channel: Some(outcome.channel.clone()),
            ..base
        },
        DebugCallResult::Err { error } => DebugCallRecord {
            // 超时的耗时就是时限；其它失败没测耗时
            elapsed_ms: match error {
                DebugError::Timeout { ms } => Some(*ms),
                _ => None,
            },
            error: Some(error_text(error)),
            ..base
        },
    }
}

fn live_ws(session: &BotSession, key: &str) -> Option<Arc<WsClient>> {
    session
        .ws
        .get(key)
        .filter(|ws| !ws.is_closed())
        .map(Arc::clone)
}

/// URL 里的主机部分：IPv6 字面量要加方括号
pub(super) fn url_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    }
}

/// OneBot 的参数必须是对象；`null` 当作没有参数
fn normalize_params(params: &Value) -> Result<Value, DebugError> {
    match params {
        Value::Null => Ok(json!({})),
        Value::Object(_) => Ok(params.clone()),
        _ => Err(DebugError::InvalidParams {
            message: "参数必须是 JSON 对象".to_owned(),
        }),
    }
}

fn effective_timeout(timeout_ms: Option<u32>) -> Duration {
    let ms = timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS);
    Duration::from_millis(u64::from(ms))
}

fn resolve_call_channel<'a>(
    plans: &'a [ChannelPlan],
    requested: &DebugChannelId,
    excluded: &HashSet<DebugChannelId>,
) -> Result<&'a ChannelPlan, DebugError> {
    let unavailable = |reason: &str| DebugError::ChannelUnavailable {
        reason: reason.to_owned(),
    };
    let id = match requested {
        DebugChannelId::Auto => {
            pick_auto(plans, true, excluded).ok_or_else(|| unavailable(NO_CALL_CHANNEL))?
        }
        other => other.clone(),
    };
    let plan = plans
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| unavailable(NO_SUCH_CHANNEL))?;
    if !plan.can_call {
        return Err(unavailable("这条通道只收事件，不能调用"));
    }
    if let Reach::Unsupported { reason } = &plan.reach {
        return Err(unavailable(reason));
    }
    Ok(plan)
}

/// 在途调用的登记：离开 `call`（正常返回、取消、超时）时摘掉，免得表里留下死项
struct InflightGuard<'a> {
    manager: &'a DebugManager,
    request_id: &'a str,
}

impl<'a> InflightGuard<'a> {
    fn register(
        manager: &'a DebugManager,
        request_id: &'a str,
        token: CancellationToken,
    ) -> Result<Self, DebugError> {
        let mut inflight = lock_std(&manager.inflight);
        // 同一个 request_id 同时有两次调用时，取消和对账都分不清是哪次
        if inflight.contains_key(request_id) {
            return Err(DebugError::InvalidParams {
                message: "request_id 与一次还没结束的调用重复".to_owned(),
            });
        }
        inflight.insert(request_id.to_owned(), token);
        Ok(Self {
            manager,
            request_id,
        })
    }
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        lock_std(&self.manager.inflight).remove(self.request_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_is_clamped() {
        assert_eq!(effective_timeout(None), Duration::from_secs(60));
        assert_eq!(effective_timeout(Some(200)), Duration::from_secs(1));
        assert_eq!(effective_timeout(Some(5_000)), Duration::from_secs(5));
        assert_eq!(effective_timeout(Some(u32::MAX)), Duration::from_secs(600));
    }

    #[test]
    fn params_must_be_an_object() {
        assert_eq!(normalize_params(&Value::Null).unwrap(), json!({}));
        assert_eq!(normalize_params(&json!({"a": 1})).unwrap(), json!({"a": 1}));
        for bad in [json!([1]), json!("x"), json!(3), json!(true)] {
            assert!(matches!(
                normalize_params(&bad),
                Err(DebugError::InvalidParams { .. })
            ));
        }
    }

    #[test]
    fn ipv6_hosts_are_bracketed() {
        assert_eq!(url_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(url_host("fe80::1"), "[fe80::1]");
        assert_eq!(url_host("[::1]"), "[::1]");
    }
}
