// 本文件只写 SnowLumaLoginState enum + ProcessTreeProbe async trait 占位
// SnowLumaStatusPoller 主体(构造,主循环,UIN 锁定,状态合成,dispose)
// 由 写
//
// 严格红线:SnowLumaLoginState 跨 Tauri 边界(DomainEvent::SnowLumaLoginStateChanged
// 的 state 字段)必须 ts-rs 派生 + 导出,避免前后端类型漂移
//

use std::collections::BTreeSet;

// SnowLumaLoginState 已下沉到 ncd-domain，此处 re-export 保持向后兼容
pub use ncd_domain::daemon_state::SnowLumaLoginState;

// 进程树枚举抽象 -- async trait + Mock 边界

/// 给定起始 PID,返回该进程及其所有后代 PID 的集合(含自身)
/// 设计目的:把 sysinfo / Windows 进程枚举从 SnowLumaStatusPoller 内剥离
/// 让 poller 单测可以注入 MockProcessTreeProbe,避免真实系统调用
/// 实现合约(SysinfoProcessTreeProbe 落地时复核):
/// - 失败(PID 不存在 / 权限不足)必须返回 BTreeSet::from([initial_pid])
/// 不得 panic
/// - 非 Windows 平台亦须返回 BTreeSet::from([initial_pid]),保持类型签名一致
#[async_trait::async_trait]
pub trait ProcessTreeProbe: Send + Sync {
    async fn collect_descendants(&self, initial_pid: u32) -> BTreeSet<u32>;
}

// 实装主循环 + UIN 锁定 + 状态合成

use std::sync::Arc;
use std::time::Duration;

use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep};
use tokio_util::sync::CancellationToken;

use crate::snowluma::webui_client::{
    HookProcessInfo, HookProcessStatus, OneBotInstanceInfo, QqPortLoginInfo, SnowLumaWebUiClient,
};
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::ids::BotId;
use ncd_traits::events::{BroadcastEventBus, EventBus};

/// 启动延迟:让 daemon 完成首启注入再开始 poll,避免抢在 daemon Ready 之前
/// 打到没就绪的 WebUI
const START_DELAY: Duration = Duration::from_millis(500);

/// 主循环 tick 周期
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// 连续 HTTP 失败门限:达到时最多发一次
/// Disconnected,恢复前不再发新状态
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// 曾在线后全信号消失的连续 tick 门限:达到才发 Disconnected。
/// 远端 hook 信号 (process.uin / status) 可能短暂抖动空值,qq-list 也可能
/// 闪断;不防抖会把短暂抖动误报成掉线。2s tick × 3 = 6s 窗口,足够滤掉
/// 单次抖动,又比用户感知"卡住"快
const NO_SIGNAL_THRESHOLD: u32 = 3;

/// per-Bot 主循环依赖(注入边界,便于单测换 mock)
pub struct PollerDeps {
    pub event_bus: Arc<BroadcastEventBus>,
    pub http: Arc<dyn SnowLumaWebUiClient>,
    pub proc_tree: Arc<dyn ProcessTreeProbe>,
    pub expected_uin: Option<String>,
}

/// per-Bot 状态轮询组件句柄
/// spawn 启动后台 task 驱动主循环;dispose 取消其 CancellationToken
/// 主循环当轮 select 结束后退出Drop 兜底——任何路径忘掉显式 dispose
/// 也不会泄漏 task
#[allow(dead_code)]
pub struct SnowLumaStatusPoller {
    bot_id: BotId,
    cancel: CancellationToken,
}

impl SnowLumaStatusPoller {
    /// 启动后台 task
    pub fn spawn(bot_id: BotId, initial_qq_pid: u32, deps: PollerDeps) -> Self {
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        let bot_id_for_task = bot_id.clone();
        tokio::spawn(async move {
            run_poller(bot_id_for_task, initial_qq_pid, deps, cancel_for_task).await;
        });
        Self { bot_id, cancel }
    }

    /// 请求主循环退出;多次调用幂等退出前主循环若 last_state ≠ Disconnected
    /// 会补发一次终止性 SnowLumaLoginStateChanged{Disconnected}
    ///
    pub fn dispose(&self) {
        self.cancel.cancel();
    }

    /// 当前 Poller 关联的 BotId
    pub fn bot_id(&self) -> &BotId {
        &self.bot_id
    }
}

impl Drop for SnowLumaStatusPoller {
    fn drop(&mut self) {
        // 兜底:忘记显式 dispose 时仍取消后台 task
        self.cancel.cancel();
    }
}

/// 主循环私有可变状态
#[derive(Debug, Default)]
struct PollerState {
    initial_qq_pid: u32,
    uin: Option<String>,
    locked_pid: Option<u32>,
    last_state: Option<SnowLumaLoginState>,
    last_pid_set: BTreeSet<u32>,
    consecutive_failures: u32,
    /// 曾在线后连续"全信号消失"的 tick 数,达 NO_SIGNAL_THRESHOLD 才发 Disconnected
    consecutive_no_signal: u32,
}

impl PollerState {
    fn new(initial_qq_pid: u32) -> Self {
        Self {
            initial_qq_pid,
            ..Default::default()
        }
    }
}

// 主循环 run_poller

async fn run_poller(
    bot_id: BotId,
    initial_qq_pid: u32,
    deps: PollerDeps,
    cancel: CancellationToken,
) {
    let mut state = PollerState::new(initial_qq_pid);

    // 启动延迟:避免抢在 daemon Ready 之前发请求期间允许 cancel 直接退出
    tokio::select! {
    biased;
    _ = cancel.cancelled() => {
    emit_terminal_disconnected_if_needed(&bot_id, &deps, &mut state);
    return;
    }
    _ = sleep(START_DELAY) => {}
    }

    // ticker:第一轮在 START_DELAY 之后立刻触发,与 legacy QTimer 对齐
    let mut ticker = interval_at(Instant::now(), POLL_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
        biased;
        _ = cancel.cancelled() => {
        emit_terminal_disconnected_if_needed(&bot_id, &deps, &mut state);
        break;
        }
        _ = ticker.tick() => {
        tick_once(&bot_id, &deps, &mut state).await;
        }
        }
    }
}

/// 退出前补发一次终止性 Disconnected(仅当 last_state ≠ Disconnected)
fn emit_terminal_disconnected_if_needed(
    bot_id: &BotId,
    deps: &PollerDeps,
    state: &mut PollerState,
) {
    if state.last_state != Some(SnowLumaLoginState::Disconnected) {
        deps.event_bus
            .publish(DomainEvent::snowluma_login_state_changed(
                bot_id.clone(),
                SnowLumaLoginState::Disconnected,
            ));
        state.last_state = Some(SnowLumaLoginState::Disconnected);
    }
}

/// 单轮 tick:并发拉 /api/processes + /api/qq-list → UIN 锁定 → 状态合成 →
/// PID 集合变化通知
async fn tick_once(bot_id: &BotId, deps: &PollerDeps, state: &mut PollerState) {
    let (proc_res, qq_res) =
        tokio::join!(deps.http.list_processes(), deps.http.list_qq_instances());

    let (processes, qq_instances) = match (proc_res, qq_res) {
        (Ok(p), Ok(q)) => (p, q),
        _ => {
            // 任一失败:累计失败,达到门限发一次 Disconnected(仅一次)
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
            if state.consecutive_failures >= MAX_CONSECUTIVE_FAILURES
                && state.last_state != Some(SnowLumaLoginState::Disconnected)
            {
                deps.event_bus
                    .publish(DomainEvent::snowluma_login_state_changed(
                        bot_id.clone(),
                        SnowLumaLoginState::Disconnected,
                    ));
                state.last_state = Some(SnowLumaLoginState::Disconnected);
            }
            return;
        }
    };

    // 全部成功:恢复失败计数
    state.consecutive_failures = 0;

    let mut probe_login_evidence: Option<ProbeLoginEvidence> = None;

    // === UIN 锁定(仅 uin == None 时尝试)===
    if state.uin.is_none() {
        let candidates = deps
            .proc_tree
            .collect_descendants(state.initial_qq_pid)
            .await;
        if let Some(locked) = try_lock_uin(
            &processes,
            &qq_instances,
            &candidates,
            deps.expected_uin.as_deref(),
        ) {
            state.locked_pid = find_candidate_pid_for_uin(&processes, &candidates, &locked);
            if state.locked_pid.is_none() {
                state.locked_pid = find_pid_for_uin(&processes, &locked);
            }
            deps.event_bus.publish(DomainEvent::snowluma_uin_detected(
                bot_id.clone(),
                locked.clone(),
            ));
            tracing::info!(
                target: "ncd_runtime::snowluma_status_poller",
                bot_id = %bot_id,
                uin = %locked,
                locked_pid = ?state.locked_pid,
                expected_uin = ?deps.expected_uin,
                "SnowLuma UIN locked"
            );
            state.uin = Some(locked);
        } else if let Some(evidence) = probe_candidate_login_info(
            &deps.http,
            &processes,
            &candidates,
            deps.expected_uin.as_deref(),
        )
        .await
        {
            deps.event_bus.publish(DomainEvent::snowluma_uin_detected(
                bot_id.clone(),
                evidence.uin.clone(),
            ));
            state.locked_pid = Some(evidence.pid);
            state.uin = Some(evidence.uin.clone());
            tracing::info!(
                target: "ncd_runtime::snowluma_status_poller",
                bot_id = %bot_id,
                uin = %evidence.uin,
                locked_pid = evidence.pid,
                expected_uin = ?deps.expected_uin,
                "SnowLuma UIN locked from probe-login"
            );
            probe_login_evidence = Some(evidence);
        }
    }

    // 还没锁定 → 本轮不发布状态 / PID 集合事件
    let Some(locked_uin) = state.uin.clone() else {
        return;
    };

    // UIN 已锁但 locked_pid 仍空(hook 未 login,process.uin 没刷出真实值)
    // → 先 find_pid_for_uin 碰运气,仍空就 probe 候选 pid 补上。没有
    // locked_pid 后续 matched / probe 兜底都拿不到 pid,掉线时无法报 Disconnected
    let mut probe_has_uin = probe_login_evidence
        .as_ref()
        .is_some_and(|e| e.uin == locked_uin);
    if state.locked_pid.is_none() {
        state.locked_pid = find_pid_for_uin(&processes, &locked_uin);
    }
    if state.locked_pid.is_none() && !probe_has_uin {
        if let Some(evidence) = probe_candidate_login_info(
            &deps.http,
            &processes,
            &BTreeSet::new(),
            Some(locked_uin.as_str()),
        )
        .await
        {
            state.locked_pid = Some(evidence.pid);
            // probe_candidate_login_info 要求 info.logged_in,evidence 隐含在线证据
            probe_has_uin = true;
        }
    }

    // === 状态合成 ===
    let locked_pid = state.locked_pid;
    let matched: Vec<&HookProcessInfo> = processes
        .iter()
        .filter(|p| process_matches_locked(p, &locked_uin, locked_pid))
        .collect();
    let qq_has_uin = qq_instances.iter().any(|i| i.uin == locked_uin);
    // 信号缺失时 probe 确认:locked_pid 优先,仍 None 时遍历候选 pid fallback
    if !probe_has_uin
        && !qq_has_uin
        && !matched
            .iter()
            .any(|p| p.status == HookProcessStatus::Online)
    {
        let probe_pids: Vec<u32> = match locked_pid {
            Some(pid) => vec![pid],
            None => {
                let candidates = deps
                    .proc_tree
                    .collect_descendants(state.initial_qq_pid)
                    .await;
                processes
                    .iter()
                    .filter(|p| candidates.contains(&p.pid))
                    .map(|p| p.pid)
                    .collect()
            }
        };
        for pid in probe_pids {
            if let Ok(Some(info)) = deps.http.probe_process_login_info(pid).await {
                if probe_info_matches_uin(&info, &locked_uin) {
                    probe_has_uin = true;
                    break;
                }
            }
        }
    }
    let synthesized = synthesize_state(&matched, qq_has_uin || probe_has_uin);

    // 曾在线 + 全信号消失(matched 空 + qq-list 无 + probe 无)→ 候选 Disconnected。
    // 远端 hook 信号可能短暂抖动空值,连续 NO_SIGNAL_THRESHOLD 轮才采纳,滤掉单次抖动
    let all_signal_lost = matched.is_empty() && !qq_has_uin && !probe_has_uin;
    let was_online = state.last_state == Some(SnowLumaLoginState::LoggedIn);
    let new_state = if all_signal_lost && was_online {
        state.consecutive_no_signal = state.consecutive_no_signal.saturating_add(1);
        if state.consecutive_no_signal >= NO_SIGNAL_THRESHOLD {
            Some(SnowLumaLoginState::Disconnected)
        } else {
            None
        }
    } else {
        state.consecutive_no_signal = 0;
        synthesized
    };

    if let Some(new_state) = new_state
        && state.last_state != Some(new_state)
    {
        tracing::info!(
            target: "ncd_runtime::snowluma_status_poller",
            bot_id = %bot_id,
            uin = %locked_uin,
            locked_pid = ?locked_pid,
            state = ?new_state,
            "SnowLuma login state changed"
        );
        deps.event_bus
            .publish(DomainEvent::snowluma_login_state_changed(
                bot_id.clone(),
                new_state,
            ));
        state.last_state = Some(new_state);
    }

    // === PID 集合变化通知 ===
    let mut pid_set: BTreeSet<u32> = matched.iter().map(|p| p.pid).collect();
    if pid_set != state.last_pid_set {
        let pids: Vec<u32> = pid_set.iter().copied().collect();
        deps.event_bus
            .publish(DomainEvent::snowluma_pid_set_changed(bot_id.clone(), pids));
        std::mem::swap(&mut state.last_pid_set, &mut pid_set);
    }
}

// 纯函数:UIN 锁定 + 状态合成 + 真实性校验

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProbeLoginEvidence {
    pid: u32,
    uin: String,
}

/// is_real_uin:非空 + 非 "0" + 全 ASCII 数字 + 长度 ≥ 5
fn is_real_uin(s: &str) -> bool {
    !s.is_empty() && s != "0" && s.len() >= 5 && s.bytes().all(|b| b.is_ascii_digit())
}

fn uin_matches_expected(uin: &str, expected_uin: Option<&str>) -> bool {
    expected_uin.is_none_or(|expected| expected == uin)
}

fn probe_info_matches_uin(info: &QqPortLoginInfo, uin: &str) -> bool {
    info.logged_in && info.uin == uin && is_real_uin(&info.uin)
}

fn process_matches_locked(
    process: &HookProcessInfo,
    locked_uin: &str,
    locked_pid: Option<u32>,
) -> bool {
    process.uin == locked_uin || locked_pid == Some(process.pid)
}

fn find_pid_for_uin(processes: &[HookProcessInfo], uin: &str) -> Option<u32> {
    processes.iter().find(|p| p.uin == uin).map(|p| p.pid)
}

fn find_candidate_pid_for_uin(
    processes: &[HookProcessInfo],
    candidates: &BTreeSet<u32>,
    uin: &str,
) -> Option<u32> {
    processes
        .iter()
        .find(|p| candidates.contains(&p.pid) && p.uin == uin)
        .map(|p| p.pid)
}

async fn probe_candidate_login_info(
    http: &Arc<dyn SnowLumaWebUiClient>,
    processes: &[HookProcessInfo],
    candidates: &BTreeSet<u32>,
    expected_uin: Option<&str>,
) -> Option<ProbeLoginEvidence> {
    let mut pids: Vec<u32> = processes
        .iter()
        .filter(|p| expected_uin.is_some() || candidates.contains(&p.pid))
        .map(|p| p.pid)
        .collect();
    pids.sort_unstable();
    pids.dedup();

    for pid in pids {
        let Ok(Some(info)) = http.probe_process_login_info(pid).await else {
            continue;
        };
        if info.logged_in && is_real_uin(&info.uin) && uin_matches_expected(&info.uin, expected_uin)
        {
            return Some(ProbeLoginEvidence { pid, uin: info.uin });
        }
    }
    None
}

/// 严格 UIN 锁定策略:
/// - 策略 A:任一 process.pid ∈ candidate set 且 is_real_uin(process.uin)
///   → 锁该 uin
/// - 策略 B(expected):配置里有明确 QQ 号时,允许跨 PID 从 process.uin 或
///   qq-list 精确锁定它。远端 Linux QQ 启动脚本可能返回 wrapper PID,
///   而 SnowLuma WebUI 枚举的是最终 QQ 进程 PID。
/// - 策略 C(fallback,仅当 processes 完全空时):qq_instances 恰好 1 条 +
///   is_real_uin(qq_instances[0].uin) → 锁该 uin
/// - 策略 D(fallback):已看到 candidate 进程,但 processes 里还没有任何真实
///   UIN,同时 qq_instances 恰好 1 条真实 UIN → 锁该 uin。扫码登录后
///   OneBot 会话可能先出现在 qq-list,process.uin/status 下一轮才刷新。
/// - 否则:返回 None,等下一轮重试
///   多 instance(≥ 2)显式拒绝,避免 cross-Bot 误匹配(legacy 复现过)
fn try_lock_uin(
    processes: &[HookProcessInfo],
    qq_instances: &[OneBotInstanceInfo],
    candidates: &BTreeSet<u32>,
    expected_uin: Option<&str>,
) -> Option<String> {
    // 策略 A
    for p in processes {
        if candidates.contains(&p.pid)
            && is_real_uin(&p.uin)
            && uin_matches_expected(&p.uin, expected_uin)
        {
            return Some(p.uin.clone());
        }
    }

    // 策略 B:配置里已有 QQ 号时,用 WebUI 事实精确绑定,不被启动 PID 卡死。
    if let Some(expected) = expected_uin.filter(|v| is_real_uin(v)) {
        if processes
            .iter()
            .any(|p| p.uin == expected && is_real_uin(&p.uin))
        {
            return Some(expected.to_string());
        }
        if qq_instances.iter().any(|i| i.uin == expected) {
            return Some(expected.to_string());
        }
    }

    // 策略 C:仅当 processes 完全空 + qq_instances 恰好 1 条
    if processes.is_empty()
        && qq_instances.len() == 1
        && is_real_uin(&qq_instances[0].uin)
        && uin_matches_expected(&qq_instances[0].uin, expected_uin)
    {
        return Some(qq_instances[0].uin.clone());
    }
    // 策略 D:候选 QQ 进程已被 WebUI 枚举到,但 hook loginState 还没把
    // process.uin 刷成真实账号。只有 processes 里完全没有真实 UIN 时才
    // 信任唯一 qq-list,避免多账号场景把别的 Bot 误锁过来。
    let has_candidate_process = processes.iter().any(|p| candidates.contains(&p.pid));
    let has_any_real_process_uin = processes.iter().any(|p| is_real_uin(&p.uin));
    if has_candidate_process
        && !has_any_real_process_uin
        && qq_instances.len() == 1
        && is_real_uin(&qq_instances[0].uin)
        && uin_matches_expected(&qq_instances[0].uin, expected_uin)
    {
        return Some(qq_instances[0].uin.clone());
    }
    None
}

/// 状态合成:
/// 1. 任一 matched.status == Online → LoggedIn
/// 2. qq_instances 含已锁 uin → LoggedIn
///    OneBotManager 只在 Bridge session close 时移除实例；它比 hook pipe 的
///    短暂 Disconnected/Error 更能代表账号仍在线。
/// 3. 否则任一 matched.status == Loaded → WaitingForQrScan
/// 4. 否则任一 matched.status ∈ {Available, Loading, Connecting} → Starting
/// 5. 否则 matched 非空 + 全部 ∈ {Error, Disconnected} → Disconnected
/// 6. 否则 → None,本轮不发布
fn synthesize_state(matched: &[&HookProcessInfo], qq_has_uin: bool) -> Option<SnowLumaLoginState> {
    if matched
        .iter()
        .any(|p| p.status == HookProcessStatus::Online)
    {
        return Some(SnowLumaLoginState::LoggedIn);
    }
    if qq_has_uin {
        return Some(SnowLumaLoginState::LoggedIn);
    }
    if matched
        .iter()
        .any(|p| p.status == HookProcessStatus::Loaded)
    {
        return Some(SnowLumaLoginState::WaitingForQrScan);
    }
    if matched.iter().any(|p| {
        matches!(
            p.status,
            HookProcessStatus::Available
                | HookProcessStatus::Loading
                | HookProcessStatus::Connecting
        )
    }) {
        return Some(SnowLumaLoginState::Starting);
    }
    if !matched.is_empty()
        && matched.iter().all(|p| {
            matches!(
                p.status,
                HookProcessStatus::Error | HookProcessStatus::Disconnected
            )
        })
    {
        return Some(SnowLumaLoginState::Disconnected);
    }
    None
}

// 单元测试 + 属性测试


#[cfg(test)]
#[path = "tests.rs"]
mod tests;
