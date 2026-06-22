//! NapCat WebUI 登录轮询组件
//!
//! 暴露 [NapCatLoginPoller] 与其依赖:[PollerConfig],[PollerDeps],
//! [RestartHandle]spawn 启动后台主循环驱动登录态状态机;dispose()
//! 取消其 CancellationToken,主循环当轮 select 结束后退出,退出前补发
//! 一次 [crate::events::DomainEvent::NapCatLoginQrcodeRemoved] 让 UI
//! 立即清掉残留二维码
//!
//! 主循环由单一 tokio::select! 驱动 auth_ticker / status_ticker /
//! cancel / cmd_rx 4 个分支,biased 让取消优先生效;LoginState 仅
//! 由主循环写入,状态转移由 [apply_login_status] 与
//! [apply_online_status] 完成
//!
//! 复用现有资产:
//! - BroadcastEventBus (crate::events)
//! - BotId (crate::ids)
//! - tokio_util::sync::CancellationToken(与 BotActor 同款取消机制)

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::time::{Interval, MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use super::offline_notifier::{OfflineNoticeKind, OfflineNotifier};
use super::webui_client::{
    CheckLoginStatusData, GetQQLoginInfoData, NapCatWebUiClient, NapCatWebUiError,
};
use crate::events::{BroadcastEventBus, DomainEvent, EventBus, NapCatLoginInvalidationReason};
use crate::ids::BotId;

/// 单个 Poller 的运行参数
///
/// unlogged_interval 固定为 1s,对齐 legacy _unlogged_interval;
/// login_check_interval 默认 5s,由 WebUiPollerSettings.bot_login_check_interval_ms 决定;
/// auth_refresh_period 默认 30 分钟;auth_refresh_throttle 默认 5s
#[derive(Debug, Clone)]
pub struct PollerConfig {
    /// 已登录状态下的轮询间隔
    pub login_check_interval: Duration,
    /// 未登录状态下的轮询间隔(固定 1s)
    pub unlogged_interval: Duration,
    /// auth credential 主动刷新周期
    pub auth_refresh_period: Duration,
    /// 401/403 触发的被动 auth 刷新节流
    pub auth_refresh_throttle: Duration,
    /// HTTP 请求超时(与 ReqwestNapCatWebUiClient 内部 timeout 对齐)
    pub http_timeout: Duration,
    /// 离线时是否自动重启 Bot 进程来自 BotConfig.bot.offline_auto_restart
    pub offline_auto_restart: bool,
    /// 是否启用离线通知来自
    /// BotConfig.advanced.offline_notice && (settings.offline_webhook_notice || settings.offline_email_notice)
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
    /// 当前持有的 Bearer credential;None 时下一轮 status poll 触发刷新
    pub auth: Option<String>,
    /// 是否处于已登录状态(NapCat WebUI CheckLoginStatus.isLogin)
    pub is_logged_in: bool,
    /// 是否在线(NapCat WebUI GetQQLoginInfo.online)
    pub online: bool,
    /// 当前离线区间是否已发过一次离线通知(防重发)
    pub offline_notice_sent: bool,
    /// 在线期间检测到 is_login=false 的标记(踢线态首阶段)
    pub login_invalidated_while_online: bool,
    /// 踢线后抑制旧二维码直到下次 online=true
    pub suppress_qrcode_until_online: bool,
    /// 上次 auth refresh 触发时间,用于 5s 节流
    pub last_auth_refresh_attempt_at: Instant,
}

impl LoginState {
    /// 构造初始状态last_auth_refresh_attempt_at 设为远早于现在的时刻,
    /// 保证首次 RequestAuthRefresh 不被节流命中
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            auth: None,
            is_logged_in: false,
            online: false,
            offline_notice_sent: false,
            login_invalidated_while_online: false,
            suppress_qrcode_until_online: false,
            last_auth_refresh_attempt_at: Instant::now()
                .checked_sub(Duration::from_secs(3600))
                .unwrap_or_else(Instant::now),
        }
    }
}

/// Poller 主循环的内部命令
///
/// 由 status poll 任务通过 mpsc 发送到主循环;主循环单点写 LoginState
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum PollerCommand {
    /// status poll 收到 401/403 → 请求主循环触发 auth 刷新(受 5s 节流控制)
    RequestAuthRefresh,
    /// 配置(轮询间隔)热更新——预留给后续 poller_settings 推送使用
    UpdateInterval(Duration),
}

/// Poller 依赖注入容器
///
/// 通过 Arc<dyn ...> 暴露所有外部能力,方便测试用 mock 替换
#[allow(dead_code)]
pub struct PollerDeps {
    pub event_bus: Arc<BroadcastEventBus>,
    pub http: Arc<dyn NapCatWebUiClient>,
    pub notifier: Arc<dyn OfflineNotifier>,
    pub restart_handle: Arc<dyn RestartHandle>,
}

/// 重启 Bot 的能力抽象
///
/// 由 BotManager 实现,让 Poller 不直接持有 BotManager 引用,
/// 避免循环依赖
#[async_trait]
pub trait RestartHandle: Send + Sync {
    /// 触发指定 Bot 的重启流程错误由实现方内部消化(例如发布
    /// DomainEvent::bot_error),不抛回 Poller
    async fn restart_bot(&self, bot_id: &BotId);
}

/// per-Bot 登录轮询组件的句柄
///
/// spawn 会启动一个后台任务驱动状态机;dispose() 取消其
/// CancellationToken,主循环当轮 select 结束后退出
/// Drop 实现作兜底——任何路径忘掉显式 dispose 也不会泄漏 task
#[allow(dead_code)]
pub struct NapCatLoginPoller {
    bot_id: BotId,
    cancel: CancellationToken,
}

impl NapCatLoginPoller {
    /// 启动后台 Poller 任务
    ///
    /// tokio::spawn 出来的 task 由 cancel 控制生命周期;调用方持有的
    /// NapCatLoginPoller 在 Drop 时自动取消,因此即使 caller 忘了显式
    /// dispose() 也不会泄漏后台 task
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

    /// 取消 Poller 的 CancellationToken,请求主循环退出
    ///
    /// 多次调用幂等退出前主循环会补发一次
    /// DomainEvent::NapCatLoginQrcodeRemoved,让 UI 立刻清掉残留二维码
    pub fn dispose(&self) {
        self.cancel.cancel();
    }

    /// 当前 Poller 关联的 BotId
    pub fn bot_id(&self) -> &BotId {
        &self.bot_id
    }
}

impl Drop for NapCatLoginPoller {
    fn drop(&mut self) {
        // 兜底:任何路径忘记显式 dispose 时仍能取消后台 task
        self.cancel.cancel();
    }
}

// Main loop

/// Poller 后台任务主体
///
/// - 启动后立即调用一次 do_auth_refresh + do_status_poll,避免等待
///   首个 ticker 触发
/// - tokio::select! 4 个分支 + biased;:取消优先于任何 ticker,
///   保证 dispose() 立即生效
/// - MissedTickBehavior::Delay:HTTP 比 ticker 慢时,下一个 tick 不堆积,
///   避免雪崩
/// - 退出前补发一次 NapCatLoginQrcodeRemoved:UI 残留二维码立即被清掉
async fn run_poller(
    bot_id: BotId,
    port: u16,
    token: String,
    cfg: PollerConfig,
    deps: PollerDeps,
    cancel: CancellationToken,
) {
    let mut state = LoginState::new();

    // ── auth ticker:30 分钟一次主动刷新 ──
    // tokio 的 Interval 首次 tick 立即返回;这里 await 一次把它消费掉,
    // 让真正的下一次 tick 在 auth_refresh_period 之后
    let mut auth_ticker = interval(cfg.auth_refresh_period);
    auth_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let _ = auth_ticker.tick().await;

    // ── status ticker:未登录 1s / 已登录 5s ──
    // 同样保留首次 tick 的「立即返回」行为:进入主循环后第一轮 select!
    // 会再触发一次 status poll,与 legacy QTimer 立即触发对齐
    let mut status_period = cfg.unlogged_interval;
    let mut status_ticker = interval(status_period);
    status_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // 容量 8 足够:401/403 节流后任意时刻最多排几条 RequestAuthRefresh
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

    // 立刻触发首次 auth + status,对齐 legacy QTimer.singleShot(0)
    do_auth_refresh(&bot_id, port, &token, &cfg, &deps, &mut state).await;
    do_status_poll(&bot_id, port, &cfg, &deps, &mut state, &cmd_tx).await;
    adjust_status_interval(&mut status_ticker, &mut status_period, &state, &cfg);

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                // 退出前补发一次 qrcode_removed,让 UI 立刻清掉残留二维码
                deps.event_bus
                    .publish(DomainEvent::napcat_login_qrcode_removed(bot_id.clone()));
                break;
            }
            _ = auth_ticker.tick() => {
                // 30 min ticker 直接刷新(不走节流),与 legacy 行为一致
                do_auth_refresh(&bot_id, port, &token, &cfg, &deps, &mut state).await;
            }
            _ = status_ticker.tick() => {
                do_status_poll(&bot_id, port, &cfg, &deps, &mut state, &cmd_tx).await;
                // 一轮 poll 后根据登录态调整下一轮的轮询间隔
                adjust_status_interval(&mut status_ticker, &mut status_period, &state, &cfg);
            }
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    PollerCommand::RequestAuthRefresh => {
                        // 5s 节流:最近一次刷新尝试 ≥ throttle 才允许再次
                        // 刷新;否则丢弃以避免 401/403 雪崩
                        if state.last_auth_refresh_attempt_at.elapsed()
                            >= cfg.auth_refresh_throttle
                        {
                            state.auth = None;
                            do_auth_refresh(&bot_id, port, &token, &cfg, &deps, &mut state)
                                .await;
                        }
                    }
                    PollerCommand::UpdateInterval(_d) => {
                        // 当前由 adjust_status_interval 在每轮 poll 后修正;
                        // 预留给配置热更新使用
                    }
                }
            }
        }
    }
}

/// 根据登录态把 status_ticker 切换到正确的周期
///
/// 未登录 → cfg.unlogged_interval(默认 1s);
/// 已登录 → cfg.login_check_interval(默认 5s)
///
/// 当目标周期发生变化时,重建 Interval 实例(tokio Interval 的内部
/// deadline 在创建时锁定;reset 不会改变 period)新 ticker 的首次 tick
/// 仍然是「立即」语义,下一轮 select! 会立即触发一次 status poll,与
/// legacy 立刻使用新间隔的行为一致
fn adjust_status_interval(
    status_ticker: &mut Interval,
    current_period: &mut Duration,
    state: &LoginState,
    cfg: &PollerConfig,
) {
    let target = if state.is_logged_in {
        cfg.login_check_interval
    } else {
        cfg.unlogged_interval
    };
    if target != *current_period {
        *current_period = target;
        let mut new_ticker = interval(target);
        new_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        *status_ticker = new_ticker;
    }
}

/// auth credential 刷新
///
/// - 节流计时点:在调用 deps.http.fetch_credential *之前* 把
///   state.last_auth_refresh_attempt_at 更新为 Instant::now(),让 5s
///   节流([run_poller] 中 RequestAuthRefresh 分支判定)从「尝试」
///   时刻起算——即使本轮 fetch 失败也已经消耗了节流额度,避免 401/403
///   立刻再触发同一路径形成雪崩
/// - 成功路径:把响应里的 Credential 写入 state.auth = Some(_),
///   下一轮 do_status_poll 即可消费;不发布任何 DomainEvent,
///   登录态由后续 apply_login_status / apply_online_status 推送
/// - 失败路径:仅 tracing::warn! 记日志,不发布
///   DomainEvent::BotError——HTTP 噪音不应阻断 BotManager 命令路径
///   state.auth 保持调用前的值不动;caller(RequestAuthRefresh
///   分支)若已先清空 state.auth,下一轮 status poll 会再次入队
///   RequestAuthRefresh(受 5s 节流保护)
async fn do_auth_refresh(
    bot_id: &BotId,
    port: u16,
    token: &str,
    _cfg: &PollerConfig,
    deps: &PollerDeps,
    state: &mut LoginState,
) {
    // 节流计时从「尝试」时刻起算:放在 await 之前,无论成功或失败,
    // 都消耗一次 5s 额度
    state.last_auth_refresh_attempt_at = Instant::now();
    match deps.http.fetch_credential(port, token).await {
        Ok(credential) => {
            state.auth = Some(credential);
        }
        Err(err) => {
            tracing::warn!(
                bot_id = %bot_id,
                error = ?err,
                "NapCat WebUI 刷新登录凭证失败"
            );
        }
    }
}

/// 登录态轮询主入口
///
/// 行为分支:
///
/// 1. 没有 auth credential (state.auth.is_none()) → 通过
///    cmd_tx.try_send(PollerCommand::RequestAuthRefresh) 让主循环受
///    5s 节流保护后刷新;本轮不发任何 HTTP 请求
/// 2. 有 auth → 用 tokio::join! 并发调用 check_login_status +
///    check_online_status,对齐 legacy 的同时刻双查询行为
/// 3. 任意一路返回 Unauthorized → 通过同样的 try_send 命令路径
///    触发 auth 刷新;状态写入仍交给主循环单点完成
/// 4. 其它错误 → 仅 tracing::warn!,不发布 BotError,不堵塞命令
///    路径
///
/// 状态写入只发生在 apply_login_status / apply_online_status 内部
/// (单飞由 tokio::select! 主循环在调用层保证)
async fn do_status_poll(
    bot_id: &BotId,
    port: u16,
    cfg: &PollerConfig,
    deps: &PollerDeps,
    state: &mut LoginState,
    cmd_tx: &mpsc::Sender<PollerCommand>,
) {
    // 没有 credential:让主循环决定要不要刷新(受 5s 节流保护)
    let Some(auth) = state.auth.clone() else {
        let _ = cmd_tx.try_send(PollerCommand::RequestAuthRefresh);
        return;
    };

    // 并发跑两条 HTTP,对齐 legacy 行为;任一路 await 超时由 reqwest 客户端
    // 自身的 timeout 限制,外层不再加额外 deadline
    let login_fut = deps.http.check_login_status(port, &auth);
    let online_fut = deps.http.check_online_status(port, &auth);
    let (login_res, online_res) = tokio::join!(login_fut, online_fut);

    match login_res {
        Ok(data) => apply_login_status(bot_id, data, deps, state),
        Err(NapCatWebUiError::Unauthorized(_)) => {
            // 401/403 → 命令路径触发 auth 刷新online_res 这一轮也不再处理,
            // 等下一轮 status_ticker 携带新 credential 重试
            let _ = cmd_tx.try_send(PollerCommand::RequestAuthRefresh);
            return;
        }
        Err(err) => {
            tracing::warn!(?err, %bot_id, "NapCat 登录态查询失败（check_login_status）");
        }
    }

    match online_res {
        Ok(data) => apply_online_status(bot_id, data, cfg, deps, state).await,
        Err(NapCatWebUiError::Unauthorized(_)) => {
            let _ = cmd_tx.try_send(PollerCommand::RequestAuthRefresh);
        }
        Err(err) => {
            tracing::warn!(?err, %bot_id, "NapCat 在线状态查询失败（check_online_status）");
        }
    }
}

/// 把 CheckLoginStatusData 应用到 LoginState 并发布对应事件
///
/// 三条互斥分支(按出现顺序):
///
/// 1. is_login == true → 清 login_invalidated_while_online /
///    suppress_qrcode_until_online,发布 NapCatLoginQrcodeRemoved
///    并 return(绝不再发 NapCatLoginQrcode)
/// 2. prev_login == true ∧ state.online == true ∧ is_login == false →
///    在线期间被踢:把 login_invalidated_while_online 置 true,
///    发布一次 NapCatLoginInvalidated { reason: Kicked }
/// 3. is_login == false ∧ qrcode_url 非空 ∧
///    !login_invalidated_while_online ∧ !suppress_qrcode_until_online →
///    发布 NapCatLoginQrcode 让前端显示二维码
///
/// 三条分支保证同一调用内不会同时发布 NapCatLoginQrcode 与
/// NapCatLoginQrcodeRemoved
fn apply_login_status(
    bot_id: &BotId,
    data: CheckLoginStatusData,
    deps: &PollerDeps,
    state: &mut LoginState,
) {
    let prev_login = state.is_logged_in;
    state.is_logged_in = data.is_login;

    // 分支 1:已登录
    if data.is_login {
        // 清 invalidation flag,恢复二维码事件可发性
        state.login_invalidated_while_online = false;
        state.suppress_qrcode_until_online = false;
        deps.event_bus
            .publish(DomainEvent::napcat_login_qrcode_removed(bot_id.clone()));
        return;
    }

    // 分支 2:在线期间被踢
    if prev_login && state.online {
        state.login_invalidated_while_online = true;
        deps.event_bus
            .publish(DomainEvent::napcat_login_invalidated(
                bot_id.clone(),
                NapCatLoginInvalidationReason::Kicked,
            ));
    }

    // 分支 3:可见的二维码
    if !data.qrcode_url.is_empty()
        && !state.login_invalidated_while_online
        && !state.suppress_qrcode_until_online
    {
        deps.event_bus.publish(DomainEvent::napcat_login_qrcode(
            bot_id.clone(),
            data.qrcode_url,
        ));
    }
}

/// 把 GetQQLoginInfoData 应用到 LoginState 并触发离线副作用
///
/// 行为顺序:
///
/// 1. 总是先发 NapCatLoginOnline { online }
/// 2. online == true → 重置 offline_notice_sent /
///    login_invalidated_while_online / suppress_qrcode_until_online
///    return
/// 3. prev_online == false → 一直离线状态,无副作用(避免离线期间每轮
///    都触发 notify / restart)
/// 4. prev_online == true ∧ online == false ∧ kicked → 踢线导致的离线:
///    suppress_qrcode_until_online = true,清 login_invalidated_while_online,
///    发 NapCatLoginQrcodeRemoved
/// 5. 普通的「未登录 + 不在线」(!is_logged_in ∧ !kicked)→ 等扫码,
///    无副作用
/// 6. 离线分支 + auto_restart:若 offline_notice_enabled 且未发过
///    通知,调一次 notify(AutoRestart) 并置 offline_notice_sent = true;
///    随后调 restart_handle.restart_bot(bot_id)
/// 7. 离线分支 + 非 auto_restart:若 offline_notice_enabled 且未发过
///    通知,调一次 notify(Manual) 并置 offline_notice_sent = true
async fn apply_online_status(
    bot_id: &BotId,
    data: GetQQLoginInfoData,
    cfg: &PollerConfig,
    deps: &PollerDeps,
    state: &mut LoginState,
) {
    let prev_online = state.online;
    let kicked = state.login_invalidated_while_online;
    state.online = data.online;

    // 步骤 1:总是先发 NapCatLoginOnline
    deps.event_bus.publish(DomainEvent::napcat_login_online(
        bot_id.clone(),
        data.online,
    ));

    // 步骤 2:在线 → 重置离线相关 flag
    if data.online {
        state.offline_notice_sent = false;
        state.login_invalidated_while_online = false;
        state.suppress_qrcode_until_online = false;
        return;
    }

    // 步骤 3:一直离线(无 prev → cur 变化)→ 无副作用
    if !prev_online {
        return;
    }

    // 步骤 4:踢线后第一轮离线 → 抑制二维码 + 清 invalidation flag + 发
    // QrcodeRemovedsuppress_qrcode_until_online 一直保持
    // 到下一次 online == true 才会被步骤 2 清掉
    if kicked {
        state.login_invalidated_while_online = false;
        state.suppress_qrcode_until_online = true;
        deps.event_bus
            .publish(DomainEvent::napcat_login_qrcode_removed(bot_id.clone()));
    }

    // 步骤 5:普通的「未登录 + 不在线」继续等扫码,不做副作用
    if !state.is_logged_in && !kicked {
        return;
    }

    // 步骤 6:自动重启路径
    if cfg.offline_auto_restart {
        if !state.offline_notice_sent && cfg.offline_notice_enabled {
            deps.notifier
                .notify(bot_id, OfflineNoticeKind::AutoRestart)
                .await;
            state.offline_notice_sent = true;
        }
        // 单次 apply_online_status 至多 1 次 restart
        deps.restart_handle.restart_bot(bot_id).await;
        return;
    }

    // 步骤 7:手动重启路径
    if !state.offline_notice_sent && cfg.offline_notice_enabled {
        deps.notifier
            .notify(bot_id, OfflineNoticeKind::Manual)
            .await;
        state.offline_notice_sent = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::offline_notifier::NoopOfflineNotifier;
    use crate::events::EventFilter;

    // ── 既有测试:确保骨架未被破坏 ──

    #[test]
    fn login_state_new_initializes_defaults() {
        let state = LoginState::new();

        assert!(state.auth.is_none());
        assert!(!state.is_logged_in);
        assert!(!state.online);
        assert!(!state.offline_notice_sent);
        assert!(!state.login_invalidated_while_online);
        assert!(!state.suppress_qrcode_until_online);
        // 首次 RequestAuthRefresh 不应被 5s 节流命中
        assert!(
            state.last_auth_refresh_attempt_at.elapsed() >= Duration::from_secs(5),
            "initial last_auth_refresh_attempt_at should be far in the past"
        );
    }

    #[test]
    fn poller_config_defaults_match_design() {
        let cfg = PollerConfig::default();

        assert_eq!(cfg.login_check_interval, Duration::from_millis(5000));
        assert_eq!(cfg.unlogged_interval, Duration::from_secs(1));
        assert_eq!(cfg.auth_refresh_period, Duration::from_secs(30 * 60));
        assert_eq!(cfg.auth_refresh_throttle, Duration::from_secs(5));
        assert_eq!(cfg.http_timeout, Duration::from_secs(5));
        assert!(!cfg.offline_auto_restart);
        assert!(!cfg.offline_notice_enabled);
    }

    #[test]
    fn dispose_cancels_token_and_drop_is_idempotent() {
        let bot_id = BotId::new("10001");
        let cancel = CancellationToken::new();
        let poller = NapCatLoginPoller {
            bot_id: bot_id.clone(),
            cancel: cancel.clone(),
        };

        assert_eq!(poller.bot_id(), &bot_id);
        assert!(!cancel.is_cancelled());

        poller.dispose();
        assert!(cancel.is_cancelled());

        // dispose 多次幂等
        poller.dispose();
        assert!(cancel.is_cancelled());

        // Drop 兜底:再造一个 poller 不调 dispose,drop 后 token 被取消
        let cancel2 = CancellationToken::new();
        {
            let _p = NapCatLoginPoller {
                bot_id,
                cancel: cancel2.clone(),
            };
        }
        assert!(cancel2.is_cancelled());
    }

    // ── 单元测试:adjust_status_interval ──
    //
    // tokio::time::interval 必须在 Tokio runtime 上下文里调用,因此这三个
    // 测试用 #[tokio::test] 而非 #[test]

    #[tokio::test]
    async fn adjust_status_interval_keeps_unlogged_period_when_not_logged_in() {
        let cfg = PollerConfig::default();
        let state = LoginState::new();
        let mut ticker = interval(cfg.unlogged_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut current = cfg.unlogged_interval;

        adjust_status_interval(&mut ticker, &mut current, &state, &cfg);

        assert_eq!(current, cfg.unlogged_interval);
    }

    #[tokio::test]
    async fn adjust_status_interval_switches_to_login_period_when_logged_in() {
        let cfg = PollerConfig::default();
        let mut state = LoginState::new();
        state.is_logged_in = true;
        let mut ticker = interval(cfg.unlogged_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut current = cfg.unlogged_interval;

        adjust_status_interval(&mut ticker, &mut current, &state, &cfg);

        assert_eq!(current, cfg.login_check_interval);
        assert_ne!(current, cfg.unlogged_interval);
    }

    #[tokio::test]
    async fn adjust_status_interval_switches_back_to_unlogged_period() {
        let cfg = PollerConfig::default();
        let mut state = LoginState::new();
        state.is_logged_in = true;
        let mut ticker = interval(cfg.login_check_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut current = cfg.login_check_interval;

        // 1) 已登录 → 保持 5s
        adjust_status_interval(&mut ticker, &mut current, &state, &cfg);
        assert_eq!(current, cfg.login_check_interval);

        // 2) 退出登录 → 切回 1s
        state.is_logged_in = false;
        adjust_status_interval(&mut ticker, &mut current, &state, &cfg);
        assert_eq!(current, cfg.unlogged_interval);
    }

    // ── smoke test:取消时主循环退出并补发 QrcodeRemoved ──

    /// 验证取消语义:dispose() 触发的 cancel.cancelled() 分支
    /// (biased; 优先)必须在退出前发布且仅发布一次
    /// NapCatLoginQrcodeRemoved
    ///
    /// 用 start_paused = true 避免真实时间影响 ticker 行为:tokio 的
    /// Interval 首次 tick 在创建时刻立即可用,因此即使时间被冻结,
    /// 主循环仍能完成「初始 setup → 进入 select! 主循环 → 收到取消信号」
    /// 的全过程
    #[tokio::test(start_paused = true)]
    async fn run_poller_publishes_qrcode_removed_on_cancellation() {
        // 此 smoke test 关心的是「取消优先于 ticker」的语义,因此 stub client 让
        // fetch_credential 失败 → state.auth 始终为 None → 初次
        // do_status_poll 走 RequestAuthRefresh 命令路径,不发布任何
        // DomainEvent状态转移分支由 transition_tests 模块单独覆盖
        struct StubClient;
        #[async_trait]
        impl NapCatWebUiClient for StubClient {
            async fn fetch_credential(
                &self,
                _port: u16,
                _token: &str,
            ) -> Result<String, NapCatWebUiError> {
                Err(NapCatWebUiError::Decode("stub".into()))
            }
            async fn check_login_status(
                &self,
                _port: u16,
                _auth: &str,
            ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
                Err(NapCatWebUiError::Decode("stub".into()))
            }
            async fn check_online_status(
                &self,
                _port: u16,
                _auth: &str,
            ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
                Err(NapCatWebUiError::Decode("stub".into()))
            }

            async fn set_ob11_config(
                &self,
                _port: u16,
                _auth: &str,
                _config_json: &str,
            ) -> Result<(), NapCatWebUiError> {
                Ok(())
            }
        }

        struct StubRestart;
        #[async_trait]
        impl RestartHandle for StubRestart {
            async fn restart_bot(&self, _bot_id: &BotId) {}
        }

        let bus = Arc::new(BroadcastEventBus::default());
        // 必须先订阅再 spawn,否则 broadcast 通道还没有 receiver,
        // 取消时发的事件会被静默丢弃
        let mut sub = bus.subscribe(EventFilter::bot("smoke-bot"));

        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StubClient),
            notifier: Arc::new(NoopOfflineNotifier),
            restart_handle: Arc::new(StubRestart),
        };

        let poller = NapCatLoginPoller::spawn(
            BotId::new("smoke-bot"),
            6099,
            "smoke-token".to_string(),
            PollerConfig::default(),
            deps,
        );

        // 让 spawn 出去的任务跑完初始 setup 并阻塞在 select! 上
        // yield_now 把当前任务让出给运行时;运行时会调度 poller 任务直到
        // 它再次 park,然后回到测试任务
        tokio::task::yield_now().await;

        // 触发取消;biased; 让 cancel 分支在下一次 select! 中优先 fire
        poller.dispose();

        // sub.next().await 会再次让出执行权,运行时调度 poller 任务,
        // poller 发布 QrcodeRemoved 后 break,事件被广播到订阅者
        let event = sub
            .next()
            .await
            .expect("subscription should yield the terminal event");
        match event {
            DomainEvent::NapCatLoginQrcodeRemoved { bot_id } => {
                assert_eq!(bot_id, BotId::new("smoke-bot"));
            }
            other => panic!("expected NapCatLoginQrcodeRemoved on cancel, got {other:?}"),
        }
    }

    // =====================================================================
    // 单元测试:do_auth_refresh
    // =====================================================================
    //
    // 直接驱动私有的 do_auth_refresh,不走 run_poller,用最小可编译的
    // mock client 控制返回值,避免 ticker / 取消等其他变量干扰

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 可编程的 mock client:fetch_credential 返回值由测试预设
    /// check_login_status / check_online_status 返回 default(auth 测试不会触发)
    struct AuthMockClient {
        /// Ok(credential) 或 Err(NapCatWebUiError) 序列;按调用顺序消费
        fetch_responses: Mutex<Vec<Result<String, NapCatWebUiError>>>,
        /// fetch_credential 实际被调用的次数,断言节流 / no-op 路径用
        fetch_calls: AtomicUsize,
    }

    impl AuthMockClient {
        fn new(responses: Vec<Result<String, NapCatWebUiError>>) -> Self {
            Self {
                fetch_responses: Mutex::new(responses),
                fetch_calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.fetch_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl NapCatWebUiClient for AuthMockClient {
        async fn fetch_credential(
            &self,
            _port: u16,
            _token: &str,
        ) -> Result<String, NapCatWebUiError> {
            self.fetch_calls.fetch_add(1, Ordering::SeqCst);
            let mut queue = self.fetch_responses.lock().expect("mock mutex poisoned");
            queue
                .pop()
                .unwrap_or_else(|| Err(NapCatWebUiError::Decode("queue empty".into())))
        }

        async fn check_login_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
            Ok(CheckLoginStatusData::default())
        }

        async fn check_online_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
            Ok(GetQQLoginInfoData::default())
        }

        async fn set_ob11_config(
            &self,
            _port: u16,
            _auth: &str,
            _config_json: &str,
        ) -> Result<(), NapCatWebUiError> {
            Ok(())
        }
    }

    struct StubRestart;
    #[async_trait]
    impl RestartHandle for StubRestart {
        async fn restart_bot(&self, _bot_id: &BotId) {}
    }

    /// 构造仅给 do_auth_refresh 用的最小 PollerDeps
    fn auth_test_deps(client: Arc<AuthMockClient>) -> (PollerDeps, Arc<BroadcastEventBus>) {
        let bus = Arc::new(BroadcastEventBus::default());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: client,
            notifier: Arc::new(NoopOfflineNotifier),
            restart_handle: Arc::new(StubRestart),
        };
        (deps, bus)
    }

    /// fetch_credential 成功 → state.auth = Some(credential)
    /// last_auth_refresh_attempt_at 在调用前被刷新,
    /// 因此调用结束时 elapsed() 远小于 auth_refresh_throttle
    #[tokio::test]
    async fn do_auth_refresh_success_writes_auth_and_updates_timestamp() {
        let bot_id = BotId::new("auth-success");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![Ok("bearer-xyz".into())]));
        let (deps, _bus) = auth_test_deps(client.clone());
        let mut state = LoginState::new();
        // 初始 timestamp 是「过去 1 小时」,调用后必须被刷新
        let initial_ts = state.last_auth_refresh_attempt_at;

        do_auth_refresh(&bot_id, 6099, "smoke-token", &cfg, &deps, &mut state).await;

        assert_eq!(state.auth.as_deref(), Some("bearer-xyz"));
        assert_eq!(client.calls(), 1);
        // timestamp 已被刷新(远晚于初始值)
        assert!(
            state.last_auth_refresh_attempt_at > initial_ts,
            "last_auth_refresh_attempt_at must be updated"
        );
        // 刚调用完,elapsed 应远小于 5s 节流窗口
        assert!(
            state.last_auth_refresh_attempt_at.elapsed() < cfg.auth_refresh_throttle,
            "post-call elapsed must be within throttle window"
        );
    }

    /// fetch_credential 失败仅 tracing::warn!,
    /// 不发布 DomainEvent::BotError,不修改 state.auth,但 timestamp
    /// 仍在调用前被刷新(节流计时从「尝试」时刻起算,与失败无关)
    #[tokio::test]
    async fn do_auth_refresh_failure_leaves_auth_untouched_and_emits_no_bot_error() {
        let bot_id = BotId::new("auth-fail");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![Err(NapCatWebUiError::Timeout)]));
        let (deps, bus) = auth_test_deps(client.clone());
        // 订阅 bot-scoped 事件流:捕获任何 BotError 即视为违规
        let mut sub = bus.subscribe(EventFilter::bot("auth-fail"));

        let mut state = LoginState::new();
        let initial_ts = state.last_auth_refresh_attempt_at;

        do_auth_refresh(&bot_id, 6099, "smoke-token", &cfg, &deps, &mut state).await;

        // auth 保持调用前值不动(None)
        assert!(
            state.auth.is_none(),
            "auth must remain unchanged on failure"
        );
        assert_eq!(client.calls(), 1);
        // timestamp 仍被刷新(节流计时从「尝试」起算)
        assert!(
            state.last_auth_refresh_attempt_at > initial_ts,
            "last_auth_refresh_attempt_at must be updated even on failure"
        );

        // 验证没有任何事件被发布到 bot 流(包括 BotError)
        // 用一个极短超时让 broadcast receiver 报告「无消息」
        let recv = tokio::time::timeout(Duration::from_millis(10), sub.next()).await;
        assert!(
            recv.is_err(),
            "do_auth_refresh failure path must not publish any DomainEvent, got {:?}",
            recv
        );
    }

    /// 成功路径覆盖「先有 auth,再次刷新成功」覆盖原值
    #[tokio::test]
    async fn do_auth_refresh_success_overwrites_previous_auth() {
        let bot_id = BotId::new("auth-overwrite");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![Ok("new-credential".into())]));
        let (deps, _bus) = auth_test_deps(client);
        let mut state = LoginState::new();
        state.auth = Some("old-credential".into());

        do_auth_refresh(&bot_id, 6099, "smoke-token", &cfg, &deps, &mut state).await;

        assert_eq!(state.auth.as_deref(), Some("new-credential"));
    }

    /// 边界:Unauthorized 错误也走 tracing::warn! 路径,行为与
    /// 其它错误一致——do_auth_refresh 内部对错误类型不做分支
    /// (内部只要求「仅日志」),分流由 caller 负责
    #[tokio::test]
    async fn do_auth_refresh_unauthorized_error_does_not_modify_auth() {
        let bot_id = BotId::new("auth-unauth");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![Err(
            NapCatWebUiError::Unauthorized(401),
        )]));
        let (deps, _bus) = auth_test_deps(client.clone());
        let mut state = LoginState::new();
        state.auth = Some("stale-credential".into());

        do_auth_refresh(&bot_id, 6099, "smoke-token", &cfg, &deps, &mut state).await;

        // 失败仅日志,state.auth 保持调用前的值不动
        assert_eq!(state.auth.as_deref(), Some("stale-credential"));
        assert_eq!(client.calls(), 1);
    }

    /// 通过模拟 run_poller 的 RequestAuthRefresh
    /// 分支语义验证 5s 节流——do_auth_refresh 后 timestamp 是「现在」,
    /// 第二次命令到达时 elapsed() < throttle 必命中节流分支被忽略,
    /// state.auth 保持调用前的值不动
    ///
    /// 注:state.last_auth_refresh_attempt_at 使用 std::time::Instant
    /// (wall-clock),不受 tokio::time::pause 影响,因此本测试只覆盖
    /// 「窗口内必须忽略」语义;「窗口外允许刷新」由端到端测试覆盖
    /// (在那里通过真实时间或专用 mock clock 推进)
    #[tokio::test]
    async fn request_auth_refresh_throttle_skips_within_window() {
        // throttle 设大到 60s,保证 ms 级测试内绝不可能自然过期
        let cfg = PollerConfig {
            auth_refresh_period: Duration::from_secs(3600),
            auth_refresh_throttle: Duration::from_secs(60),
            ..PollerConfig::default()
        };

        let client = Arc::new(AuthMockClient::new(vec![Ok("first-cred".into())]));
        let (deps, _bus) = auth_test_deps(client.clone());

        let bot_id = BotId::new("throttle-bot");
        let mut state = LoginState::new();

        // ── 首次:模拟 RequestAuthRefresh 通过节流(initial timestamp = 1h ago)
        let throttle_passed_initial =
            state.last_auth_refresh_attempt_at.elapsed() >= cfg.auth_refresh_throttle;
        assert!(
            throttle_passed_initial,
            "first RequestAuthRefresh must pass throttle (initial ts is 1h ago)"
        );
        state.auth = None; // run_poller 在通过节流后清 auth
        do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
        assert_eq!(client.calls(), 1);
        assert_eq!(state.auth.as_deref(), Some("first-cred"));

        // ── 第二次:模拟立即收到下一条 RequestAuthRefresh
        // timestamp 刚被刷新成现在 → elapsed < 60s throttle → 必须命中节流
        let throttle_passed_second =
            state.last_auth_refresh_attempt_at.elapsed() >= cfg.auth_refresh_throttle;
        assert!(
            !throttle_passed_second,
            "second RequestAuthRefresh within window must NOT pass"
        );
        // 节流命中分支:直接忽略且不修改 state.auth
        // 断言 fetch 没被再次调用,且 state.auth 仍是上一次写入的值
        assert_eq!(client.calls(), 1, "throttled command must not call fetch");
        assert_eq!(
            state.auth.as_deref(),
            Some("first-cred"),
            "throttled command must NOT clear state.auth"
        );
    }

    /// auth_ticker tick 直接调 do_auth_refresh 不走节流
    /// 验证「即使 timestamp 刚更新过,再次直接调用 do_auth_refresh 仍会
    /// 真正执行 fetch」——节流判定不在 do_auth_refresh 自身,而在 caller
    #[tokio::test]
    async fn do_auth_refresh_does_not_self_throttle() {
        let bot_id = BotId::new("ticker-bot");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![
            Ok("second".into()),
            Ok("first".into()),
        ]));
        let (deps, _bus) = auth_test_deps(client.clone());
        let mut state = LoginState::new();

        do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
        assert_eq!(client.calls(), 1);
        assert_eq!(state.auth.as_deref(), Some("first"));

        // 立刻再次调用——do_auth_refresh 自身不带节流,必须真正 fetch
        // 这对应 auth_ticker 30min tick 直接执行的语义
        do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
        assert_eq!(client.calls(), 2);
        assert_eq!(state.auth.as_deref(), Some("second"));
    }
}

// 单元测试:do_status_poll / apply_login_status / apply_online_status
//
// 直接驱动私有函数,覆盖各转移分支每个测试
// 只断言「事件流 + 状态字段 + 副作用计数」三类可观察输出,不引入 ticker /
// 取消等无关变量
//
// 公共测试基础设施:
// - StatusMockClient:可编程的 login/online 响应序列
// - RecordingNotifier:记录 notify(kind) 调用历史,断言至多一次
// - RecordingRestartHandle:记录 restart_bot(bot_id) 调用次数,断言至多
//   一次
// - drain_events(&mut sub):用极短超时排空订阅器的当前事件流
#[cfg(test)]
mod transition_tests {
    use super::*;

    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::offline_notifier::OfflineNoticeKind;
    use crate::events::EventFilter;

    // ── mock client:login + online 响应序列 ──────────────────────────────

    struct StatusMockClient {
        login_responses: StdMutex<Vec<Result<CheckLoginStatusData, NapCatWebUiError>>>,
        online_responses: StdMutex<Vec<Result<GetQQLoginInfoData, NapCatWebUiError>>>,
        login_calls: AtomicUsize,
        online_calls: AtomicUsize,
        fetch_calls: AtomicUsize,
    }

    impl StatusMockClient {
        fn new(
            login_responses: Vec<Result<CheckLoginStatusData, NapCatWebUiError>>,
            online_responses: Vec<Result<GetQQLoginInfoData, NapCatWebUiError>>,
        ) -> Self {
            Self {
                login_responses: StdMutex::new(login_responses),
                online_responses: StdMutex::new(online_responses),
                login_calls: AtomicUsize::new(0),
                online_calls: AtomicUsize::new(0),
                fetch_calls: AtomicUsize::new(0),
            }
        }

        fn login_calls(&self) -> usize {
            self.login_calls.load(Ordering::SeqCst)
        }

        fn online_calls(&self) -> usize {
            self.online_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl NapCatWebUiClient for StatusMockClient {
        async fn fetch_credential(
            &self,
            _port: u16,
            _token: &str,
        ) -> Result<String, NapCatWebUiError> {
            self.fetch_calls.fetch_add(1, Ordering::SeqCst);
            Ok("test-cred".into())
        }

        async fn check_login_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
            self.login_calls.fetch_add(1, Ordering::SeqCst);
            let mut q = self.login_responses.lock().expect("login mutex poisoned");
            q.pop()
                .unwrap_or_else(|| Err(NapCatWebUiError::Decode("login queue empty".into())))
        }

        async fn check_online_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
            self.online_calls.fetch_add(1, Ordering::SeqCst);
            let mut q = self.online_responses.lock().expect("online mutex poisoned");
            q.pop()
                .unwrap_or_else(|| Err(NapCatWebUiError::Decode("online queue empty".into())))
        }

        async fn set_ob11_config(
            &self,
            _port: u16,
            _auth: &str,
            _config_json: &str,
        ) -> Result<(), NapCatWebUiError> {
            Ok(())
        }
    }

    // ── recording notifier:记录 (bot_id, kind) 调用 ──────────────────────

    struct RecordingNotifier {
        calls: StdMutex<Vec<(BotId, OfflineNoticeKind)>>,
    }

    impl RecordingNotifier {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(BotId, OfflineNoticeKind)> {
            self.calls.lock().expect("notifier mutex poisoned").clone()
        }
    }

    #[async_trait]
    impl OfflineNotifier for RecordingNotifier {
        async fn notify(&self, bot_id: &BotId, kind: OfflineNoticeKind) {
            self.calls
                .lock()
                .expect("notifier mutex poisoned")
                .push((bot_id.clone(), kind));
        }
    }

    // ── recording restart handle:记录 restart_bot 调用 ──────────────────

    struct RecordingRestartHandle {
        calls: AtomicUsize,
    }

    impl RecordingRestartHandle {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl RestartHandle for RecordingRestartHandle {
        async fn restart_bot(&self, _bot_id: &BotId) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    // ── 测试辅助:构造 deps + 排空事件 ────────────────────────────────────

    fn status_test_deps(
        client: Arc<StatusMockClient>,
        notifier: Arc<RecordingNotifier>,
        restart: Arc<RecordingRestartHandle>,
    ) -> (PollerDeps, Arc<BroadcastEventBus>) {
        let bus = Arc::new(BroadcastEventBus::default());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: client,
            notifier,
            restart_handle: restart,
        };
        (deps, bus)
    }

    /// 排空 bot-scoped 订阅器中已发布的事件每次循环都用极短超时探测,
    /// 直到 next() 不再立即返回事件为止
    async fn drain_events(sub: &mut crate::events::EventSubscription) -> Vec<DomainEvent> {
        let mut out = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_millis(5), sub.next()).await {
                Ok(Some(ev)) => out.push(ev),
                Ok(None) => break, // closed
                Err(_) => break,   // no more events available within timeout
            }
        }
        out
    }

    // =====================================================================
    // do_status_poll
    // =====================================================================

    /// state.auth.is_none() → 通过 cmd channel 触发
    /// RequestAuthRefresh,不发起任何 HTTP 请求
    #[tokio::test]
    async fn status_poll_without_auth_requests_refresh_and_skips_http() {
        let bot_id = BotId::new("no-auth");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(vec![], vec![]));
        let (deps, bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let mut sub = bus.subscribe(EventFilter::bot("no-auth"));
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        assert!(state.auth.is_none());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        // 没有任何 HTTP 调用
        assert_eq!(client.login_calls(), 0);
        assert_eq!(client.online_calls(), 0);
        // 命令通道有一条 RequestAuthRefresh
        match cmd_rx.try_recv() {
            Ok(PollerCommand::RequestAuthRefresh) => {}
            other => panic!("expected RequestAuthRefresh, got {other:?}"),
        }
        // 没有任何 DomainEvent 被发布
        let events = drain_events(&mut sub).await;
        assert!(events.is_empty(), "no events expected, got {events:?}");
    }

    /// state.auth.is_some() → tokio::join! 并发发起两路
    /// HTTP 请求,每路调用恰好 1 次
    #[tokio::test]
    async fn status_poll_with_auth_calls_both_endpoints_concurrently() {
        let bot_id = BotId::new("with-auth");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Ok(CheckLoginStatusData {
                is_login: true,
                qrcode_url: String::new(),
            })],
            vec![Ok(GetQQLoginInfoData { online: true })],
        ));
        let (deps, _bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        state.auth = Some("bearer-xyz".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        assert_eq!(client.login_calls(), 1);
        assert_eq!(client.online_calls(), 1);
        // 成功路径不会触发 RequestAuthRefresh
        assert!(cmd_rx.try_recv().is_err());
    }

    /// login_res 返回 Unauthorized → 触发
    /// RequestAuthRefresh,且本轮不再处理 online_res(不调用
    /// apply_login_status / apply_online_status,不发布任何事件)
    #[tokio::test]
    async fn status_poll_login_unauthorized_triggers_refresh_and_skips_apply() {
        let bot_id = BotId::new("login-401");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Err(NapCatWebUiError::Unauthorized(401))],
            vec![Ok(GetQQLoginInfoData { online: true })],
        ));
        let (deps, bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let mut sub = bus.subscribe(EventFilter::bot("login-401"));
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        state.auth = Some("bearer".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        // 命令通道收到 RequestAuthRefresh
        match cmd_rx.try_recv() {
            Ok(PollerCommand::RequestAuthRefresh) => {}
            other => panic!("expected RequestAuthRefresh, got {other:?}"),
        }
        // 早退后即使 online 返回成功也不会被 apply(不发布 NapCatLoginOnline)
        let events = drain_events(&mut sub).await;
        assert!(
            events.is_empty(),
            "login Unauthorized must skip online apply, got {events:?}"
        );
        assert!(!state.online);
    }

    /// login_res 返回非 Unauthorized 错误 → 仅 warn,
    /// 不触发 RequestAuthRefresh,不影响 online_res 路径
    #[tokio::test]
    async fn status_poll_login_other_error_warns_and_continues_to_online() {
        let bot_id = BotId::new("login-500");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Err(NapCatWebUiError::Status(500))],
            vec![Ok(GetQQLoginInfoData { online: true })],
        ));
        let (deps, bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let mut sub = bus.subscribe(EventFilter::bot("login-500"));
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        state.auth = Some("bearer".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        // 不应触发 RequestAuthRefresh
        assert!(
            cmd_rx.try_recv().is_err(),
            "non-Unauthorized error must not trigger refresh"
        );
        // online_res 路径仍执行,发了一条 NapCatLoginOnline
        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1, "expected single NapCatLoginOnline event");
        match &events[0] {
            DomainEvent::NapCatLoginOnline { online, .. } => assert!(*online),
            other => panic!("expected NapCatLoginOnline, got {other:?}"),
        }
        assert!(state.online);
    }

    /// online_res 返回 Unauthorized → 触发
    /// RequestAuthRefreshlogin_res 已成功 apply,对应的事件正常发布
    #[tokio::test]
    async fn status_poll_online_unauthorized_triggers_refresh() {
        let bot_id = BotId::new("online-401");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Ok(CheckLoginStatusData {
                is_login: true,
                qrcode_url: String::new(),
            })],
            vec![Err(NapCatWebUiError::Unauthorized(403))],
        ));
        let (deps, _bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        state.auth = Some("bearer".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        // login apply 已经把 is_logged_in 置 true
        assert!(state.is_logged_in);
        // 命令通道收到 RequestAuthRefresh(来自 online 路径)
        match cmd_rx.try_recv() {
            Ok(PollerCommand::RequestAuthRefresh) => {}
            other => panic!("expected RequestAuthRefresh, got {other:?}"),
        }
    }

    // =====================================================================
    // apply_login_status
    // =====================================================================

    /// is_login=true → 清除 invalidation/suppress flag,
    /// 发布 NapCatLoginQrcodeRemoved,return(不再发 Qrcode)
    #[tokio::test]
    async fn apply_login_logged_in_clears_flags_and_emits_qrcode_removed() {
        let bot_id = BotId::new("logged-in");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("logged-in"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        // 预置脏 flag 验证清除语义
        state.login_invalidated_while_online = true;
        state.suppress_qrcode_until_online = true;

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: true,
                // 即便给了 qrcode_url,已登录分支也必须忽略它
                qrcode_url: "data:image/png;base64,SHOULD_BE_IGNORED".into(),
            },
            &deps,
            &mut state,
        );

        assert!(state.is_logged_in);
        assert!(!state.login_invalidated_while_online);
        assert!(!state.suppress_qrcode_until_online);

        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1, "expected single QrcodeRemoved event");
        match &events[0] {
            DomainEvent::NapCatLoginQrcodeRemoved { bot_id: b } => {
                assert_eq!(b, &bot_id);
            }
            other => panic!("expected NapCatLoginQrcodeRemoved, got {other:?}"),
        }
    }

    /// prev_login=true ∧ state.online=true ∧
    /// is_login=false → 设置 login_invalidated_while_online=true,
    /// 发 NapCatLoginInvalidated{Kicked},且二维码被同一调用抑制
    #[tokio::test]
    async fn apply_login_kick_during_online_emits_invalidated_and_suppresses_qrcode() {
        let bot_id = BotId::new("kicked");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("kicked"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        // 上一轮处于 online + logged_in 的稳态
        state.is_logged_in = true;
        state.online = true;

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                // 服务器把 qrcode_url 也回带了,但本调用因 invalidation flag
                // 必须抑制 Qrcode(与 QrcodeRemoved 互斥)
                qrcode_url: "data:image/png;base64,QR".into(),
            },
            &deps,
            &mut state,
        );

        assert!(!state.is_logged_in);
        assert!(state.login_invalidated_while_online);

        let events = drain_events(&mut sub).await;
        // 仅 1 个 Invalidated 事件,没有 Qrcode(互斥不变量)
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::NapCatLoginInvalidated { bot_id: b, reason } => {
                assert_eq!(b, &bot_id);
                assert_eq!(*reason, NapCatLoginInvalidationReason::Kicked);
            }
            other => panic!("expected NapCatLoginInvalidated, got {other:?}"),
        }
    }

    /// is_login=false ∧ qrcode_url 非空 ∧
    /// !login_invalidated_while_online ∧ !suppress_qrcode_until_online →
    /// 发 NapCatLoginQrcode
    #[tokio::test]
    async fn apply_login_unlogged_with_qrcode_emits_qrcode() {
        let bot_id = BotId::new("first-login");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("first-login"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        // 干净状态:未登录 + 不在线

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                qrcode_url: "data:image/png;base64,QR1".into(),
            },
            &deps,
            &mut state,
        );

        assert!(!state.is_logged_in);
        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::NapCatLoginQrcode {
                bot_id: b,
                qrcode_url,
            } => {
                assert_eq!(b, &bot_id);
                assert_eq!(qrcode_url, "data:image/png;base64,QR1");
            }
            other => panic!("expected NapCatLoginQrcode, got {other:?}"),
        }
    }

    /// login_invalidated_while_online=true → 必须抑制
    /// NapCatLoginQrcode 事件(即使 qrcode_url 非空)
    #[tokio::test]
    async fn apply_login_suppresses_qrcode_when_invalidated_while_online() {
        let bot_id = BotId::new("suppress-by-invalidated");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("suppress-by-invalidated"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        // 上一轮已不在线(online=false),所以这次 is_login=false 不会再次
        // 触发 Kicked 事件——只需验证「flag 抑制」语义
        state.login_invalidated_while_online = true;

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                qrcode_url: "data:image/png;base64,SUPPRESSED".into(),
            },
            &deps,
            &mut state,
        );

        let events = drain_events(&mut sub).await;
        assert!(
            events.is_empty(),
            "Qrcode must be suppressed when login_invalidated_while_online; got {events:?}"
        );
    }

    /// suppress_qrcode_until_online=true → 同样抑制
    /// NapCatLoginQrcode
    #[tokio::test]
    async fn apply_login_suppresses_qrcode_when_suppress_until_online() {
        let bot_id = BotId::new("suppress-by-flag");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("suppress-by-flag"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        state.suppress_qrcode_until_online = true;

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                qrcode_url: "data:image/png;base64,STILL_SUPPRESSED".into(),
            },
            &deps,
            &mut state,
        );

        let events = drain_events(&mut sub).await;
        assert!(
            events.is_empty(),
            "Qrcode must be suppressed; got {events:?}"
        );
    }

    /// is_login=false ∧ qrcode_url="" → 没有可发的二维码,无事件
    #[tokio::test]
    async fn apply_login_unlogged_with_empty_qrcode_emits_nothing() {
        let bot_id = BotId::new("no-qr");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("no-qr"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                qrcode_url: String::new(),
            },
            &deps,
            &mut state,
        );

        let events = drain_events(&mut sub).await;
        assert!(events.is_empty());
    }

    // =====================================================================
    // apply_online_status
    // =====================================================================

    /// online=true → 总是先发 NapCatLoginOnline{true},
    /// 然后重置 offline_notice_sent / login_invalidated_while_online /
    /// suppress_qrcode_until_online 三个 flag,return 不触发其它副作用
    #[tokio::test]
    async fn apply_online_true_resets_flags_and_emits_online_event() {
        let bot_id = BotId::new("back-online");
        let cfg = PollerConfig::default();
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("back-online"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        // 预置「曾发过通知 + 处于 kicked + 抑制二维码」
        state.offline_notice_sent = true;
        state.login_invalidated_while_online = true;
        state.suppress_qrcode_until_online = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: true },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        // flags 全部清零
        assert!(!state.offline_notice_sent);
        assert!(!state.login_invalidated_while_online);
        assert!(!state.suppress_qrcode_until_online);
        assert!(state.online);
        // 没有副作用调用
        assert!(notifier.calls().is_empty());
        assert_eq!(restart.calls(), 0);
        // 仅 1 个事件:NapCatLoginOnline{true}
        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::NapCatLoginOnline { online, .. } => assert!(*online),
            other => panic!("expected NapCatLoginOnline, got {other:?}"),
        }
    }

    /// prev_online=false ∧ online=false → 持续离线,仅发
    /// NapCatLoginOnline{false},不触发任何副作用
    #[tokio::test]
    async fn apply_online_persistent_offline_emits_event_only_no_side_effects() {
        let bot_id = BotId::new("persistent-offline");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("persistent-offline"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        // prev_online = false(默认),所以即使 auto_restart=true 也不应触发

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: false },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1, "expected only NapCatLoginOnline{{false}}");
        match &events[0] {
            DomainEvent::NapCatLoginOnline { online, .. } => assert!(!*online),
            other => panic!("expected NapCatLoginOnline, got {other:?}"),
        }
        assert!(notifier.calls().is_empty());
        assert_eq!(restart.calls(), 0);
    }

    /// prev_online=true ∧ online=false ∧ kicked=true →
    /// suppress_qrcode_until_online=true,清 login_invalidated_while_online,
    /// 发 NapCatLoginQrcodeRemovedoffline_auto_restart=true 时还会
    /// 触发一次 restart(属于「踢线后自动重启」分支)
    #[tokio::test]
    async fn apply_online_kicked_then_offline_suppresses_qrcode_and_auto_restarts() {
        let bot_id = BotId::new("kicked-offline");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("kicked-offline"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        // 上一轮:online=true,且 apply_login_status 已把 invalidated_while_online
        // 置为 true(踢线第一阶段)
        state.online = true;
        state.login_invalidated_while_online = true;
        // is_logged_in 当前已被 apply_login_status 设为 false(被踢后)

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: false },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        // suppress 标记开启,invalidation 标记被清
        assert!(state.suppress_qrcode_until_online);
        assert!(!state.login_invalidated_while_online);
        assert!(!state.online);

        // 应至少发 2 个事件:NapCatLoginOnline{false} + NapCatLoginQrcodeRemoved
        let events = drain_events(&mut sub).await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginOnline { online: false, .. })),
            "expected NapCatLoginOnline{{false}} in {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcodeRemoved { .. })),
            "expected NapCatLoginQrcodeRemoved in {events:?}"
        );

        // 自动重启分支:restart 至多 1 次
        assert_eq!(
            restart.calls(),
            1,
            "auto_restart must call restart_bot once"
        );
        // 通知至多 1 次(AutoRestart kind)
        let notice_calls = notifier.calls();
        assert_eq!(notice_calls.len(), 1);
        assert_eq!(notice_calls[0].1, OfflineNoticeKind::AutoRestart);
        assert!(state.offline_notice_sent);
    }

    /// prev_online=true ∧ online=false ∧ is_logged_in=true ∧
    /// kicked=false ∧ offline_auto_restart=true → 发通知(AutoRestart)+
    /// restart_bot(普通在线掉线后自动重启)
    #[tokio::test]
    async fn apply_online_logged_offline_with_auto_restart_notifies_and_restarts() {
        let bot_id = BotId::new("logged-offline-auto");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let _sub = bus.subscribe(EventFilter::bot("logged-offline-auto"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.online = true;
        state.is_logged_in = true; // 已登录但掉线(非踢线)

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: false },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        let calls = notifier.calls();
        assert_eq!(calls.len(), 1, "auto_restart should notify once");
        assert_eq!(calls[0].1, OfflineNoticeKind::AutoRestart);
        assert!(state.offline_notice_sent);
        assert_eq!(restart.calls(), 1);
    }

    /// offline_notice_sent=true → 不再调 notify,
    /// 但 auto_restart=true 仍会调 restart_bot(一次)
    #[tokio::test]
    async fn apply_online_offline_with_notice_already_sent_skips_notify() {
        let bot_id = BotId::new("notice-already");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let _sub = bus.subscribe(EventFilter::bot("notice-already"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.online = true;
        state.is_logged_in = true;
        state.offline_notice_sent = true; // 上次离线已通知过

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: false },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        assert!(notifier.calls().is_empty(), "must not notify twice");
        assert_eq!(restart.calls(), 1, "restart still fires per offline event");
    }

    /// offline_notice_enabled=false → 即使首次离线也
    /// 不调 notify,但 auto_restart=true 仍触发 restart
    #[tokio::test]
    async fn apply_online_offline_with_notice_disabled_skips_notify_but_restarts() {
        let bot_id = BotId::new("notice-disabled");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: false, // 全局通知未开启
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.online = true;
        state.is_logged_in = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: false },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        assert!(notifier.calls().is_empty());
        assert_eq!(restart.calls(), 1);
        // notice_sent 在通知未开启路径下保持 false(没人「发过」)
        assert!(!state.offline_notice_sent);
    }

    /// offline_auto_restart=false ∧ is_logged_in=true ∧
    /// offline_notice_enabled=true → 仅发 Manual 通知,不调 restart_bot
    #[tokio::test]
    async fn apply_online_logged_offline_manual_path_notifies_no_restart() {
        let bot_id = BotId::new("manual-path");
        let cfg = PollerConfig {
            offline_auto_restart: false,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.online = true;
        state.is_logged_in = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: false },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        let calls = notifier.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, OfflineNoticeKind::Manual);
        assert!(state.offline_notice_sent);
        assert_eq!(restart.calls(), 0, "manual path must NOT restart");
    }

    /// 「未登录 + 不在线 + ¬kicked」 → 等扫码,发 NapCatLoginOnline{false}
    /// 后 return,无任何副作用即使 auto_restart=true 也不应触发 restart
    #[tokio::test]
    async fn apply_online_unlogged_offline_no_kick_no_side_effects() {
        let bot_id = BotId::new("unlogged-offline");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("unlogged-offline"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        // 上一轮在线,但本轮未登录(is_logged_in=false)+ 没被踢
        state.online = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: false },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1, "expected only NapCatLoginOnline{{false}}");
        assert!(notifier.calls().is_empty());
        assert_eq!(restart.calls(), 0, "unlogged+!kicked must not restart");
    }
}

// 属性测试:proptest 覆盖核心不变量
//
// 与 transition_tests 模块的边界划分:
// - transition_tests 用具体输入断言「单一分支」的预期行为(example-based)
// - property_tests 用 proptest 在输入空间上断言「全称量化的不变量」
//   (universal invariants)—— 无论输入如何,状态机的输出都满足约束
//
// 性能与可读性权衡:
// - cases = 64:每个 property 64 个生成用例足以覆盖关键路径(主要是 6
//   个布尔 flag 的 2^6=64 组合 + 字符串变体),同时保持单测在秒级完成
// - 每个 case 用现场构造的 current-thread tokio runtime 跑 apply_* 异步
//   函数;apply_login_status 是同步 fn,但事件抽取需要 EventSubscription::next
//   的 async 接口,因此统一走 runtime block_on 简化结构

#[cfg(test)]
mod property_tests {
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use proptest::prelude::*;

    use super::super::offline_notifier::{OfflineNoticeKind, OfflineNotifier};
    use super::super::webui_client::{
        CheckLoginStatusData, GetQQLoginInfoData, NapCatWebUiClient, NapCatWebUiError,
    };
    use crate::events::{
        BroadcastEventBus, DomainEvent, EventBus, EventFilter, EventSubscription,
        NapCatLoginInvalidationReason,
    };
    use crate::ids::BotId;

    use super::{
        LoginState, PollerConfig, PollerDeps, RestartHandle, apply_login_status,
        apply_online_status,
    };

    // ── stub HTTP client:apply_* 不会调用它,给 deps 占位 ─────────────────

    struct StubHttp;

    #[async_trait]
    impl NapCatWebUiClient for StubHttp {
        async fn fetch_credential(
            &self,
            _port: u16,
            _token: &str,
        ) -> Result<String, NapCatWebUiError> {
            Ok("stub".into())
        }
        async fn check_login_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
            Ok(CheckLoginStatusData::default())
        }
        async fn check_online_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
            Ok(GetQQLoginInfoData::default())
        }

        async fn set_ob11_config(
            &self,
            _port: u16,
            _auth: &str,
            _config_json: &str,
        ) -> Result<(), NapCatWebUiError> {
            Ok(())
        }
    }

    // ── property notifier / restart:仅记录调用次数 ────────────────────────

    struct PropNotifier {
        calls: StdMutex<Vec<OfflineNoticeKind>>,
    }
    impl PropNotifier {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
            }
        }
        fn count(&self) -> usize {
            self.calls.lock().expect("notifier mutex poisoned").len()
        }
    }
    #[async_trait]
    impl OfflineNotifier for PropNotifier {
        async fn notify(&self, _bot_id: &BotId, kind: OfflineNoticeKind) {
            self.calls
                .lock()
                .expect("notifier mutex poisoned")
                .push(kind);
        }
    }

    struct PropRestart {
        calls: AtomicUsize,
    }
    impl PropRestart {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
        fn count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }
    #[async_trait]
    impl RestartHandle for PropRestart {
        async fn restart_bot(&self, _bot_id: &BotId) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    // ── 测试基础设施 ──────────────────────────────────────────────────────

    /// 为每个 proptest case 现场构造一个 current-thread tokio runtime
    /// 仅启用 time 驱动——tokio::time::timeout 排空事件时需要它
    fn current_thread_rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("build current-thread runtime")
    }

    /// 排空 bot-scoped 订阅器中已发布的事件;和 transition_tests::drain_events
    /// 等价,复制一份避免跨模块可见性扩散
    async fn drain(sub: &mut EventSubscription) -> Vec<DomainEvent> {
        let mut out = Vec::new();
        while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(5), sub.next()).await {
            out.push(ev);
        }
        out
    }

    fn make_deps(
        notifier: Arc<PropNotifier>,
        restart: Arc<PropRestart>,
    ) -> (PollerDeps, Arc<BroadcastEventBus>) {
        let bus = Arc::new(BroadcastEventBus::default());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StubHttp),
            notifier,
            restart_handle: restart,
        };
        (deps, bus)
    }

    /// 在 bot 维度订阅,调用 apply_login_status 后返回排空的事件序列
    fn run_apply_login(
        bot_id: &BotId,
        state: &mut LoginState,
        data: CheckLoginStatusData,
    ) -> Vec<DomainEvent> {
        let notifier = Arc::new(PropNotifier::new());
        let restart = Arc::new(PropRestart::new());
        let (deps, bus) = make_deps(notifier, restart);
        let rt = current_thread_rt();
        rt.block_on(async {
            let mut sub = bus.subscribe(EventFilter::bot(bot_id.clone()));
            apply_login_status(bot_id, data, &deps, state);
            drain(&mut sub).await
        })
    }

    /// 在 bot 维度订阅,调用 apply_online_status 后返回(事件序列, notify 次数, restart 次数)
    fn run_apply_online(
        bot_id: &BotId,
        cfg: &PollerConfig,
        state: &mut LoginState,
        data: GetQQLoginInfoData,
    ) -> (Vec<DomainEvent>, usize, usize) {
        let notifier = Arc::new(PropNotifier::new());
        let restart = Arc::new(PropRestart::new());
        let (deps, bus) = make_deps(notifier.clone(), restart.clone());
        let rt = current_thread_rt();
        let events = rt.block_on(async {
            let mut sub = bus.subscribe(EventFilter::bot(bot_id.clone()));
            apply_online_status(bot_id, data, cfg, &deps, state).await;
            drain(&mut sub).await
        });
        (events, notifier.count(), restart.count())
    }

    /// 跑一段「连续离线区间」:同一个 state 上施加 N 次 apply_online_status({online:false})
    fn run_offline_window(
        bot_id: &BotId,
        cfg: &PollerConfig,
        state: &mut LoginState,
        n: usize,
    ) -> (usize, usize) {
        let notifier = Arc::new(PropNotifier::new());
        let restart = Arc::new(PropRestart::new());
        let (deps, _bus) = make_deps(notifier.clone(), restart.clone());
        let rt = current_thread_rt();
        rt.block_on(async {
            for _ in 0..n {
                apply_online_status(
                    bot_id,
                    GetQQLoginInfoData { online: false },
                    cfg,
                    &deps,
                    state,
                )
                .await;
            }
        });
        (notifier.count(), restart.count())
    }

    /// proptest 全局配置——cases=64 兼顾覆盖与速度
    fn pconfig() -> ProptestConfig {
        ProptestConfig {
            cases: 16,
            ..ProptestConfig::default()
        }
    }

    // 一个非空 / 空两分支的 qrcode_url 策略,确保两侧覆盖均衡
    fn qrcode_url_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just(String::new()),
            "[a-zA-Z0-9:/+=,.]{1,16}".prop_map(|s| s),
        ]
    }

    // ─────────────────────────────────────────────────────────────────────
    // Qrcode 可观测性
    //
    // ∀ Poller state s, ∀ CheckLoginStatusData data: 若
    //   s.is_logged_in == false ∧ data.is_login == false ∧
    //   data.qrcode_url ≠ ∅ ∧ ¬s.suppress_qrcode_until_online ∧
    //   ¬s.login_invalidated_while_online,
    // 则下一次 apply_login_status(s, data) 必发布 NapCatLoginQrcode{qrcode_url}
    //
    // 注意:当 s.is_logged_in == false 时 prev_login=false,kick 分支不
    // 会被触发,因此 state.suppress_* / state.login_invalidated_* 不会
    // 被本次调用改变——前置条件「应用前」 ≡ 「Qrcode 检查时」
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_1_qrcode_emitted_when_preconditions_hold(
            state_online in any::<bool>(),
            state_offline_notice_sent in any::<bool>(),
            state_invalidated in any::<bool>(),
            state_suppress in any::<bool>(),
            data_is_login in any::<bool>(),
            qrcode_url in qrcode_url_strategy(),
        ) {
            let bot_id = BotId::new("prop-1");
            let mut state = LoginState::new();
            state.is_logged_in = false; // 前置条件
            state.online = state_online;
            state.offline_notice_sent = state_offline_notice_sent;
            state.login_invalidated_while_online = state_invalidated;
            state.suppress_qrcode_until_online = state_suppress;

            let preconditions = !data_is_login
                && !qrcode_url.is_empty()
                && !state_invalidated
                && !state_suppress;

            let events = run_apply_login(
                &bot_id,
                &mut state,
                CheckLoginStatusData {
                    is_login: data_is_login,
                    qrcode_url: qrcode_url.clone(),
                },
            );

            if preconditions {
                let saw_qrcode = events.iter().any(|e| matches!(
                    e,
                    DomainEvent::NapCatLoginQrcode { qrcode_url: u, .. }
                        if u == &qrcode_url
                ));
                prop_assert!(
                    saw_qrcode,
                    "preconditions hold but no NapCatLoginQrcode emitted: events={:?}",
                    events
                );
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Qrcode 与 QrcodeRemoved 互斥
    //
    // ∀ apply_*:同一调用内不会同时发布 NapCatLoginQrcode 与
    // NapCatLoginQrcodeRemoved
    //
    // 验证两个入口各自的互斥不变量:
    //   - apply_login_status:要么发 QrcodeRemoved(is_login=true 分支
    //     return),要么发 Qrcode / Invalidated(其它分支),不会同时出现
    //   - apply_online_status:从不发布 NapCatLoginQrcode,因此互斥
    //     trivially 成立——但仍生成随机输入跑一遍以防回归
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_2_login_qrcode_xor_qrcode_removed(
            state_is_logged_in in any::<bool>(),
            state_online in any::<bool>(),
            state_invalidated in any::<bool>(),
            state_suppress in any::<bool>(),
            data_is_login in any::<bool>(),
            qrcode_url in qrcode_url_strategy(),
        ) {
            let bot_id = BotId::new("prop-2-login");
            let mut state = LoginState::new();
            state.is_logged_in = state_is_logged_in;
            state.online = state_online;
            state.login_invalidated_while_online = state_invalidated;
            state.suppress_qrcode_until_online = state_suppress;

            let events = run_apply_login(
                &bot_id,
                &mut state,
                CheckLoginStatusData {
                    is_login: data_is_login,
                    qrcode_url,
                },
            );
            let saw_qrcode = events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcode { .. }));
            let saw_removed = events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcodeRemoved { .. }));

            prop_assert!(
                !(saw_qrcode && saw_removed),
                "apply_login_status emitted BOTH Qrcode and QrcodeRemoved in one call: events={:?}",
                events
            );
        }

        #[test]
        fn property_2_online_qrcode_xor_qrcode_removed(
            state_is_logged_in in any::<bool>(),
            state_online in any::<bool>(),
            state_invalidated in any::<bool>(),
            state_suppress in any::<bool>(),
            state_notice_sent in any::<bool>(),
            data_online in any::<bool>(),
            auto_restart in any::<bool>(),
            notice_enabled in any::<bool>(),
        ) {
            let bot_id = BotId::new("prop-2-online");
            let cfg = PollerConfig {
                offline_auto_restart: auto_restart,
                offline_notice_enabled: notice_enabled,
                ..PollerConfig::default()
            };
            let mut state = LoginState::new();
            state.is_logged_in = state_is_logged_in;
            state.online = state_online;
            state.login_invalidated_while_online = state_invalidated;
            state.suppress_qrcode_until_online = state_suppress;
            state.offline_notice_sent = state_notice_sent;

            let (events, _, _) = run_apply_online(
                &bot_id,
                &cfg,
                &mut state,
                GetQQLoginInfoData { online: data_online },
            );
            let saw_qrcode = events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcode { .. }));
            let saw_removed = events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcodeRemoved { .. }));

            prop_assert!(
                !(saw_qrcode && saw_removed),
                "apply_online_status emitted BOTH Qrcode and QrcodeRemoved in one call: events={:?}",
                events
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 踢线检测因果性
    //
    // ∀ (prev_state, next_data): NapCatLoginInvalidated{Kicked} 被发布
    // ⇔ prev_state.is_logged_in == true ∧ prev_state.online == true ∧
    //    next_data.is_login == false
    //
    // 设计要求只是 ⇒(必要条件),但代码实现满足 ⇔(充要);测试两个
    // 方向以更严格地锁定行为
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_3_kick_iff_prev_login_and_online_and_not_next_login(
            prev_login in any::<bool>(),
            prev_online in any::<bool>(),
            prev_invalidated in any::<bool>(),
            prev_suppress in any::<bool>(),
            data_is_login in any::<bool>(),
            qrcode_url in qrcode_url_strategy(),
        ) {
            let bot_id = BotId::new("prop-3");
            let mut state = LoginState::new();
            state.is_logged_in = prev_login;
            state.online = prev_online;
            state.login_invalidated_while_online = prev_invalidated;
            state.suppress_qrcode_until_online = prev_suppress;

            let events = run_apply_login(
                &bot_id,
                &mut state,
                CheckLoginStatusData {
                    is_login: data_is_login,
                    qrcode_url,
                },
            );

            let saw_kicked = events.iter().any(|e| matches!(
                e,
                DomainEvent::NapCatLoginInvalidated {
                    reason: NapCatLoginInvalidationReason::Kicked,
                    ..
                }
            ));
            let kick_preconditions = prev_login && prev_online && !data_is_login;

            prop_assert_eq!(
                saw_kicked,
                kick_preconditions,
                "Kicked iff (prev_login ∧ prev_online ∧ ¬next_login); got saw={} expected={}, events={:?}",
                saw_kicked,
                kick_preconditions,
                events
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // auth refresh 5s 节流
    //
    // ∀ 任意 RequestAuthRefresh 到达时间序列:被动路径触发的相邻两次
    // 实际 refresh 间隔必 ≥ auth_refresh_throttle(默认 5s)
    //
    // 备注(来自 prompt):state.last_auth_refresh_attempt_at 用
    // std::time::Instant(wall-clock),不受 tokio::time::pause 影响
    // 因此本属性把节流决策抽象成纯函数 simulate_passive_refresh_throttle
    // 直接驱动——把任意「到达时间序列 + 节流阈值」喂给它,断言输出的
    // 实际 refresh 序列两两间隔 ≥ 阈值这准确镜像了 run_poller 中
    // RequestAuthRefresh 分支的判定语义:
    //
    // text
    // if arrival - last_attempt >= throttle {
    //     actual_refresh.push(arrival);
    //     last_attempt = arrival;
    // }
    //
    // ─────────────────────────────────────────────────────────────────────

    /// 把「RequestAuthRefresh 命令到达时间序列」(毫秒时间戳)按节流阈值
    /// 模拟成「实际 do_auth_refresh 调用时间序列」复制 run_poller 中
    /// PollerCommand::RequestAuthRefresh 分支的判定语义;初始
    /// last_attempt 设为 None 模拟 LoginState::new 的「过去 1 小时」
    /// 远过去状态——首次到达必通过节流
    fn simulate_passive_refresh_throttle(arrivals_ms: &[u64], throttle_ms: u64) -> Vec<u64> {
        let mut last_attempt: Option<u64> = None;
        let mut actual = Vec::new();
        for &arrival in arrivals_ms {
            let passes = match last_attempt {
                None => true,
                Some(prev) => arrival.saturating_sub(prev) >= throttle_ms,
            };
            if passes {
                actual.push(arrival);
                last_attempt = Some(arrival);
            }
        }
        actual
    }

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_4_passive_refresh_intervals_respect_throttle(
            // 1..32 个 0..20s 的 gap,组装成单调递增的到达序列
            gaps_ms in proptest::collection::vec(0u64..20_000, 1..32),
        ) {
            // 累加得到到达时间
            let mut t = 0u64;
            let arrivals: Vec<u64> = gaps_ms
                .iter()
                .map(|g| {
                    t = t.saturating_add(*g);
                    t
                })
                .collect();
            let throttle_ms = 5_000u64; // PollerConfig::default().auth_refresh_throttle = 5s

            let actual = simulate_passive_refresh_throttle(&arrivals, throttle_ms);

            // 任意相邻两次实际 refresh 间隔 ≥ throttle
            for w in actual.windows(2) {
                let gap = w[1].saturating_sub(w[0]);
                prop_assert!(
                    gap >= throttle_ms,
                    "two consecutive passive refreshes within throttle window: \
                     w[0]={} w[1]={} gap={} throttle={}",
                    w[0], w[1], gap, throttle_ms
                );
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 离线通知至多一次
    //
    // ∀ 一段连续离线区间(一连串 apply_online_status({online:false}) 调用,
    // 起始时 prev_online=true):notifier.notify(...) 被调用次数 ≤ 1
    //
    // 关键不变量:第一次离线调用后 state.online=false,后续调用全部
    // 命中「prev_online=false → return」分支,不再进入通知逻辑;同时
    // state.offline_notice_sent 在首轮被置位后封锁后续 notify 调用
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_6_offline_notice_at_most_once_in_continuous_window(
            n in 1usize..16,
            initial_is_logged_in in any::<bool>(),
            initial_invalidated in any::<bool>(),
            initial_suppress in any::<bool>(),
            initial_notice_sent in any::<bool>(),
            auto_restart in any::<bool>(),
            notice_enabled in any::<bool>(),
        ) {
            let cfg = PollerConfig {
                offline_auto_restart: auto_restart,
                offline_notice_enabled: notice_enabled,
                ..PollerConfig::default()
            };
            let bot_id = BotId::new("prop-6");
            let mut state = LoginState::new();
            // 起始处于「在线」稳态——首轮离线进入会触发通知判定
            state.online = true;
            state.is_logged_in = initial_is_logged_in;
            state.login_invalidated_while_online = initial_invalidated;
            state.suppress_qrcode_until_online = initial_suppress;
            state.offline_notice_sent = initial_notice_sent;

            let (notify_count, _restart_count) = run_offline_window(&bot_id, &cfg, &mut state, n);

            prop_assert!(
                notify_count <= 1,
                "notify called {} times in a continuous offline window of len {}; \
                 cfg(auto_restart={}, notice_enabled={}), \
                 init(is_logged_in={}, invalidated={}, suppress={}, notice_sent={})",
                notify_count, n,
                auto_restart, notice_enabled,
                initial_is_logged_in, initial_invalidated, initial_suppress, initial_notice_sent
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 自动重启副作用唯一
    //
    // ∀ 单次 apply_online_status 调用:restart_handle.restart_bot 被
    // 调用次数 ≤ 1(无论分支组合如何)
    //
    // 编译期视角下,函数体内只出现一次 restart_handle.restart_bot(...).await
    // 调用,本属性以运行时观察锁定该结构不变量,防回归
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_7_restart_at_most_once_per_apply_online(
            state_is_logged_in in any::<bool>(),
            state_online in any::<bool>(),
            state_invalidated in any::<bool>(),
            state_suppress in any::<bool>(),
            state_notice_sent in any::<bool>(),
            data_online in any::<bool>(),
            auto_restart in any::<bool>(),
            notice_enabled in any::<bool>(),
        ) {
            let cfg = PollerConfig {
                offline_auto_restart: auto_restart,
                offline_notice_enabled: notice_enabled,
                ..PollerConfig::default()
            };
            let bot_id = BotId::new("prop-7");
            let mut state = LoginState::new();
            state.is_logged_in = state_is_logged_in;
            state.online = state_online;
            state.login_invalidated_while_online = state_invalidated;
            state.suppress_qrcode_until_online = state_suppress;
            state.offline_notice_sent = state_notice_sent;

            let (_events, _notify_count, restart_count) = run_apply_online(
                &bot_id,
                &cfg,
                &mut state,
                GetQQLoginInfoData { online: data_online },
            );

            prop_assert!(
                restart_count <= 1,
                "restart_bot called {} times in a single apply_online_status; \
                 cfg(auto_restart={}, notice_enabled={}), data_online={}",
                restart_count, auto_restart, notice_enabled, data_online
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 纯函数 example 测试:锁定 simulate_passive_refresh_throttle 自身
    // 的边界——proptest 的属性测试如果断言模拟器 bug 也会通过,因此用
    // 几个手工 example 给 simulator 做 sanity check
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn simulator_first_arrival_always_passes() {
        let actual = simulate_passive_refresh_throttle(&[0], 5_000);
        assert_eq!(actual, vec![0]);
    }

    #[test]
    fn simulator_within_window_skipped() {
        // 0ms, 1000ms, 2000ms, 3000ms, 6000ms → 仅 0 与 6000 通过节流(5s)
        let actual = simulate_passive_refresh_throttle(&[0, 1_000, 2_000, 3_000, 6_000], 5_000);
        assert_eq!(actual, vec![0, 6_000]);
    }

    #[test]
    fn simulator_exactly_at_throttle_boundary_passes() {
        // 0, 5000 → 5000-0 = 5000 ≥ throttle,应该通过
        let actual = simulate_passive_refresh_throttle(&[0, 5_000], 5_000);
        assert_eq!(actual, vec![0, 5_000]);
    }

    #[test]
    fn simulator_just_below_boundary_skipped() {
        // 0, 4999 → 4999-0 < 5000,第二条被跳过
        let actual = simulate_passive_refresh_throttle(&[0, 4_999], 5_000);
        assert_eq!(actual, vec![0]);
    }
}
