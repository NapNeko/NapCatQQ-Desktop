// 本文件只写 `SnowLumaLoginState` enum + `ProcessTreeProbe` async trait 占位
// `SnowLumaStatusPoller` 主体（构造、主循环、UIN 锁定、状态合成、dispose）
// 由 写。
//
// 严格红线：`SnowLumaLoginState` 跨 Tauri 边界（DomainEvent::SnowLumaLoginStateChanged
// 的 `state` 字段）必须 ts-rs 派生 + 导出，避免前后端类型漂移
// 。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ---------------------------------------------------------------------------
// 跨边界（Tauri / 前端）类型 —— ts-rs 派生 + 导出
// ---------------------------------------------------------------------------

/// SnowLuma 单个 Bot 在 status poller 视角下合成出来的登录状态。
/// 4 档语义（与 状态合成表对齐，通过 `snake_case` 序列化跨 Tauri 边界）：
/// - `Starting`：QQ 进程已起，processes 还未出现自身候选 PID 的条目（注入未生效）。
/// - `WaitingForQrScan`：processes 命中且 `status == Loaded`，等待用户扫码 / 输密码。
/// - `LoggedIn`：processes 命中且 `status == Online`，OneBot pipe 已连。
/// - `Disconnected`：processes 命中但 `status ∈ {Disconnected, Error}`
/// 或 dispose / 连续探测失败兜底。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum SnowLumaLoginState {
    Starting,
    WaitingForQrScan,
    LoggedIn,
    Disconnected,
}

// ---------------------------------------------------------------------------
// 进程树枚举抽象 —— async trait + Mock 边界
// ---------------------------------------------------------------------------

/// 给定起始 PID，返回该进程及其所有后代 PID 的集合（含自身）。
/// 设计目的：把 sysinfo / Windows 进程枚举从 `SnowLumaStatusPoller` 内剥离
/// 让 poller 单测可以注入 `MockProcessTreeProbe`，避免真实系统调用。
/// 实现合约（`SysinfoProcessTreeProbe` 落地时复核）：
/// - 失败（PID 不存在 / 权限不足）必须返回 `BTreeSet::from([initial_pid])`
/// 不得 panic。
/// - 非 Windows 平台亦须返回 `BTreeSet::from([initial_pid])`，保持类型签名一致。
#[async_trait::async_trait]
pub trait ProcessTreeProbe: Send + Sync {
    async fn collect_descendants(&self, initial_pid: u32) -> BTreeSet<u32>;
}

// ===========================================================================
// ：实装主循环 + UIN 锁定 + 状态合成
// ===========================================================================

use std::sync::Arc;
use std::time::Duration;

use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep};
use tokio_util::sync::CancellationToken;

use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::ids::BotId;
use crate::snowluma::webui_client::{
    HookProcessInfo, HookProcessStatus, OneBotInstanceInfo, SnowLumaWebUiClient,
};

/// 启动延迟：让 daemon 完成首启注入再开始 poll，避免抢在 daemon Ready 之前
/// 打到没就绪的 WebUI。
const START_DELAY: Duration = Duration::from_millis(500);

/// 主循环 tick 周期。
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// 连续 HTTP 失败门限：达到时最多发一次
/// `Disconnected`，恢复前不再发新状态。
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// per-Bot 主循环依赖（注入边界，便于单测换 mock）。
pub struct PollerDeps {
    pub event_bus: Arc<BroadcastEventBus>,
    pub http: Arc<dyn SnowLumaWebUiClient>,
    pub proc_tree: Arc<dyn ProcessTreeProbe>,
}

/// per-Bot 状态轮询组件句柄。
/// `spawn` 启动后台 task 驱动主循环；`dispose` 取消其 `CancellationToken`
/// 主循环当轮 select 结束后退出。`Drop` 兜底——任何路径忘掉显式 `dispose`
/// 也不会泄漏 task。
#[allow(dead_code)]
pub struct SnowLumaStatusPoller {
    bot_id: BotId,
    cancel: CancellationToken,
}

impl SnowLumaStatusPoller {
    /// 启动后台 task。
    pub fn spawn(bot_id: BotId, initial_qq_pid: u32, deps: PollerDeps) -> Self {
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        let bot_id_for_task = bot_id.clone();
        tokio::spawn(async move {
            run_poller(bot_id_for_task, initial_qq_pid, deps, cancel_for_task).await;
        });
        Self { bot_id, cancel }
    }

    /// 请求主循环退出；多次调用幂等。退出前主循环若 last_state ≠ `Disconnected`
    /// 会补发一次终止性 `SnowLumaLoginStateChanged{Disconnected}`
    /// 。
    pub fn dispose(&self) {
        self.cancel.cancel();
    }

    /// 当前 Poller 关联的 `BotId`。
    pub fn bot_id(&self) -> &BotId {
        &self.bot_id
    }
}

impl Drop for SnowLumaStatusPoller {
    fn drop(&mut self) {
        // 兜底：忘记显式 dispose 时仍取消后台 task。
        self.cancel.cancel();
    }
}

/// 主循环私有可变状态。
#[derive(Debug, Default)]
struct PollerState {
    initial_qq_pid: u32,
    uin: Option<String>,
    last_state: Option<SnowLumaLoginState>,
    last_pid_set: BTreeSet<u32>,
    consecutive_failures: u32,
}

impl PollerState {
    fn new(initial_qq_pid: u32) -> Self {
        Self {
            initial_qq_pid,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// 主循环 run_poller
// ---------------------------------------------------------------------------

async fn run_poller(
    bot_id: BotId,
    initial_qq_pid: u32,
    deps: PollerDeps,
    cancel: CancellationToken,
) {
    let mut state = PollerState::new(initial_qq_pid);

    // 启动延迟：避免抢在 daemon Ready 之前发请求。期间允许 cancel 直接退出。
    tokio::select! {
    biased;
    _ = cancel.cancelled() => {
    emit_terminal_disconnected_if_needed(&bot_id, &deps, &mut state);
    return;
    }
    _ = sleep(START_DELAY) => {}
    }

    // ticker：第一轮在 START_DELAY 之后立刻触发，与 legacy QTimer 对齐。
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

/// 退出前补发一次终止性 `Disconnected`（仅当 last_state ≠ Disconnected）。
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

/// 单轮 tick：并发拉 `/api/processes` + `/api/qq-list` → UIN 锁定 → 状态合成 →
/// PID 集合变化通知。
async fn tick_once(bot_id: &BotId, deps: &PollerDeps, state: &mut PollerState) {
    let (proc_res, qq_res) =
        tokio::join!(deps.http.list_processes(), deps.http.list_qq_instances());

    let (processes, qq_instances) = match (proc_res, qq_res) {
        (Ok(p), Ok(q)) => (p, q),
        _ => {
            // 任一失败：累计失败，达到门限发一次 Disconnected（仅一次）
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

    // 全部成功：恢复失败计数。
    state.consecutive_failures = 0;

    // === UIN 锁定（仅 uin == None 时尝试）===
    if state.uin.is_none() {
        let candidates = deps
            .proc_tree
            .collect_descendants(state.initial_qq_pid)
            .await;
        if let Some(locked) = try_lock_uin(&processes, &qq_instances, &candidates) {
            deps.event_bus.publish(DomainEvent::snowluma_uin_detected(
                bot_id.clone(),
                locked.clone(),
            ));
            state.uin = Some(locked);
        }
    }

    // 还没锁定 → 本轮不发布状态 / PID 集合事件。
    let Some(ref locked_uin) = state.uin else {
        return;
    };

    // === 状态合成 ===
    let matched: Vec<&HookProcessInfo> =
        processes.iter().filter(|p| p.uin == *locked_uin).collect();
    let qq_has_uin = qq_instances.iter().any(|i| i.uin == *locked_uin);
    let synthesized = synthesize_state(&matched, qq_has_uin);

    if let Some(new_state) = synthesized
        && state.last_state != Some(new_state)
    {
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

// ---------------------------------------------------------------------------
// 纯函数：UIN 锁定 + 状态合成 + 真实性校验
// ---------------------------------------------------------------------------

/// `is_real_uin`：非空 + 非 "0" + 全 ASCII 数字 + 长度 ≥ 5。
fn is_real_uin(s: &str) -> bool {
    !s.is_empty() && s != "0" && s.len() >= 5 && s.bytes().all(|b| b.is_ascii_digit())
}

/// 严格 UIN 锁定策略：
/// - 策略 A：任一 process.pid ∈ candidate set 且 `is_real_uin(process.uin)`
/// → 锁该 uin。
/// - 策略 B（fallback，仅当 processes 完全空时）：qq_instances 恰好 1 条 +
/// `is_real_uin(qq_instances[0].uin)` → 锁该 uin。
/// - 否则：返回 `None`，等下一轮重试。
/// 多 instance（≥ 2）显式拒绝，避免 cross-Bot 误匹配（legacy 复现过）。
fn try_lock_uin(
    processes: &[HookProcessInfo],
    qq_instances: &[OneBotInstanceInfo],
    candidates: &BTreeSet<u32>,
) -> Option<String> {
    // 策略 A
    for p in processes {
        if candidates.contains(&p.pid) && is_real_uin(&p.uin) {
            return Some(p.uin.clone());
        }
    }
    // 策略 B：仅当 processes 完全空 + qq_instances 恰好 1 条
    if processes.is_empty() && qq_instances.len() == 1 && is_real_uin(&qq_instances[0].uin) {
        return Some(qq_instances[0].uin.clone());
    }
    None
}

/// 状态合成：
/// 1. 任一 matched.status == Online → `LoggedIn`
/// 2. 否则任一 matched.status == Loaded → `WaitingForQrScan`
/// 3. 否则任一 matched.status ∈ {Available, Loading, Connecting} → `Starting`
/// 4. 否则 matched 非空 + 全部 ∈ {Error, Disconnected} → `Disconnected`
/// 5. 否则 matched 空 + qq_instances 含已锁 uin → fallback `LoggedIn`
/// （Windows `getAllMainProcess` bug 兜底）
/// 6. 否则 → `None`，本轮不发布
fn synthesize_state(matched: &[&HookProcessInfo], qq_has_uin: bool) -> Option<SnowLumaLoginState> {
    if matched
        .iter()
        .any(|p| p.status == HookProcessStatus::Online)
    {
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
    if matched.is_empty() && qq_has_uin {
        return Some(SnowLumaLoginState::LoggedIn);
    }
    None
}

// ===========================================================================
// ：单元测试 + 属性测试
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::VecDeque;

    use async_trait::async_trait;
    use tokio::sync::Mutex as TokioMutex;

    use crate::events::{DomainEventKind, EventFilter};
    use crate::snowluma::error::SnowLumaWebUiError;
    use crate::snowluma::proc_tree::MockProcessTreeProbe;
    use crate::snowluma::webui_client::AuthState;

    // --- is_real_uin ---

    #[test]
    fn is_real_uin_handles_edge_cases() {
        assert!(!is_real_uin(""));
        assert!(!is_real_uin("0"));
        assert!(!is_real_uin("abc12345"));
        assert!(!is_real_uin("1234")); // len < 5
        assert!(is_real_uin("12345"));
        assert!(is_real_uin("100200"));
    }

    // --- 测试用 Mock client ---

    #[derive(Default)]
    struct MockBehavior {
        processes_responses: VecDeque<Result<Vec<HookProcessInfo>, SnowLumaWebUiError>>,
        qq_responses: VecDeque<Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError>>,
        last_processes: Option<Result<Vec<HookProcessInfo>, SnowLumaWebUiError>>,
        last_qq: Option<Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError>>,
    }

    fn clone_webui_error(e: &SnowLumaWebUiError) -> SnowLumaWebUiError {
        match e {
            SnowLumaWebUiError::Status {
                endpoint,
                status,
                message,
            } => SnowLumaWebUiError::Status {
                endpoint: endpoint.clone(),
                status: *status,
                message: message.clone(),
            },
            SnowLumaWebUiError::Timeout { endpoint } => SnowLumaWebUiError::Timeout {
                endpoint: endpoint.clone(),
            },
            SnowLumaWebUiError::Http { endpoint, cause } => SnowLumaWebUiError::Http {
                endpoint: endpoint.clone(),
                cause: cause.clone(),
            },
            SnowLumaWebUiError::Decode { endpoint, message } => SnowLumaWebUiError::Decode {
                endpoint: endpoint.clone(),
                message: message.clone(),
            },
            SnowLumaWebUiError::NotReady(d, errs) => SnowLumaWebUiError::NotReady(*d, errs.clone()),
            SnowLumaWebUiError::LoginFailed(msg) => SnowLumaWebUiError::LoginFailed(msg.clone()),
            SnowLumaWebUiError::ServerRejected { endpoint, message } => {
                SnowLumaWebUiError::ServerRejected {
                    endpoint: endpoint.clone(),
                    message: message.clone(),
                }
            }
        }
    }

    fn clone_proc_result(
        r: &Result<Vec<HookProcessInfo>, SnowLumaWebUiError>,
    ) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
        match r {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(clone_webui_error(e)),
        }
    }

    fn clone_qq_result(
        r: &Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError>,
    ) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
        match r {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(clone_webui_error(e)),
        }
    }

    struct MockClient {
        behavior: Arc<TokioMutex<MockBehavior>>,
    }

    impl MockClient {
        fn new() -> (Arc<Self>, Arc<TokioMutex<MockBehavior>>) {
            let behavior = Arc::new(TokioMutex::new(MockBehavior::default()));
            (
                Arc::new(Self {
                    behavior: Arc::clone(&behavior),
                }),
                behavior,
            )
        }
    }

    #[async_trait]
    impl SnowLumaWebUiClient for MockClient {
        async fn wait_ready(
            &self,
            _timeout: Duration,
            _dead_check: Box<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<(), SnowLumaWebUiError> {
            Ok(())
        }
        async fn login(&self) -> Result<(), SnowLumaWebUiError> {
            Ok(())
        }
        async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
            Ok(())
        }
        async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
            let mut behavior = self.behavior.lock().await;
            if let Some(front) = behavior.processes_responses.pop_front() {
                behavior.last_processes = Some(clone_proc_result(&front));
                front
            } else if let Some(last) = &behavior.last_processes {
                clone_proc_result(last)
            } else {
                Ok(Vec::new())
            }
        }
        async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
            let mut behavior = self.behavior.lock().await;
            if let Some(front) = behavior.qq_responses.pop_front() {
                behavior.last_qq = Some(clone_qq_result(&front));
                front
            } else if let Some(last) = &behavior.last_qq {
                clone_qq_result(last)
            } else {
                Ok(Vec::new())
            }
        }
        async fn load_process(&self, _pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::ServerRejected {
                endpoint: "<mock>".into(),
                message: "n/a".into(),
            })
        }
        async fn unload_process(&self, _pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::ServerRejected {
                endpoint: "<mock>".into(),
                message: "n/a".into(),
            })
        }
        async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError> {
            Ok(AuthState::default())
        }

            async fn update_onebot_config(&self, _uin: &str, _config: &serde_json::Value) -> Result<bool, SnowLumaWebUiError> {
                Ok(true)
            }
    }

    fn proc(pid: u32, uin: &str, status: HookProcessStatus) -> HookProcessInfo {
        HookProcessInfo {
            pid,
            name: "QQ.exe".into(),
            path: "C:/qq".into(),
            uin: uin.into(),
            status,
            error: String::new(),
        }
    }

    fn instance(uin: &str) -> OneBotInstanceInfo {
        OneBotInstanceInfo {
            uin: uin.into(),
            nickname: "test".into(),
        }
    }

    // 占位：后续追加测试用例

    // ----- UIN 锁定行为（纯函数 + tick_once 集成） -----

    #[test]
    fn try_lock_uin_strategy_a_proc_tree_match() {
        let processes = vec![
            proc(99999, "100200", HookProcessStatus::Loaded),
            proc(12346, "100200", HookProcessStatus::Loaded),
        ];
        let qq_instances = vec![];
        let candidates: BTreeSet<u32> = [12345u32, 12346u32].into_iter().collect();
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates),
            Some("100200".to_string())
        );
    }

    #[test]
    fn try_lock_uin_strategy_b_processes_empty_single_qq_instance() {
        let processes: Vec<HookProcessInfo> = vec![];
        let qq_instances = vec![instance("100200")];
        let candidates = BTreeSet::from([12345u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates),
            Some("100200".to_string())
        );
    }

    #[test]
    fn try_lock_uin_refuses_with_multiple_qq_instances() {
        let processes: Vec<HookProcessInfo> = vec![];
        let qq_instances = vec![instance("100200"), instance("999999")];
        let candidates = BTreeSet::from([12345u32]);
        assert_eq!(try_lock_uin(&processes, &qq_instances, &candidates), None);
    }

    #[test]
    fn try_lock_uin_refuses_when_processes_non_empty_no_candidate_match() {
        // processes 非空但无 candidate 命中 → 不应走 fallback B
        let processes = vec![proc(77777, "100200", HookProcessStatus::Loaded)];
        let qq_instances = vec![instance("100200")];
        let candidates = BTreeSet::from([12345u32]);
        assert_eq!(try_lock_uin(&processes, &qq_instances, &candidates), None);
    }

    #[test]
    fn try_lock_uin_rejects_invalid_uin_field() {
        let processes = vec![proc(12346, "0", HookProcessStatus::Loaded)];
        let qq_instances = vec![];
        let candidates = BTreeSet::from([12346u32]);
        assert_eq!(try_lock_uin(&processes, &qq_instances, &candidates), None);
    }

    // ----- 状态合成 6 分支 -----

    #[test]
    fn synthesize_state_logged_in_when_any_online() {
        let p1 = proc(1, "100200", HookProcessStatus::Loaded);
        let p2 = proc(2, "100200", HookProcessStatus::Online);
        let matched = vec![&p1, &p2];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::LoggedIn)
        );
    }

    #[test]
    fn synthesize_state_waiting_for_qr_scan_when_any_loaded() {
        let p1 = proc(1, "100200", HookProcessStatus::Connecting);
        let p2 = proc(2, "100200", HookProcessStatus::Loaded);
        let matched = vec![&p1, &p2];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::WaitingForQrScan)
        );
    }

    #[test]
    fn synthesize_state_starting_when_any_in_progress() {
        let p1 = proc(1, "100200", HookProcessStatus::Available);
        let matched = vec![&p1];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::Starting)
        );

        let p2 = proc(2, "100200", HookProcessStatus::Loading);
        let matched = vec![&p2];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::Starting)
        );

        let p3 = proc(3, "100200", HookProcessStatus::Connecting);
        let matched = vec![&p3];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::Starting)
        );
    }

    #[test]
    fn synthesize_state_disconnected_when_all_failed() {
        let p1 = proc(1, "100200", HookProcessStatus::Error);
        let p2 = proc(2, "100200", HookProcessStatus::Disconnected);
        let matched = vec![&p1, &p2];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::Disconnected)
        );
    }

    #[test]
    fn synthesize_state_fallback_logged_in_when_matched_empty_but_qq_has_uin() {
        let matched: Vec<&HookProcessInfo> = vec![];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::LoggedIn)
        );
    }

    #[test]
    fn synthesize_state_none_when_matched_empty_and_qq_lacks_uin() {
        let matched: Vec<&HookProcessInfo> = vec![];
        assert_eq!(synthesize_state(&matched, false), None);
    }

    // ----- 主循环 tick_once 行为：用 deps 注入 mock 直接触发单轮 -----

    fn build_test_deps(
        client: Arc<dyn SnowLumaWebUiClient>,
        proc_tree: Arc<dyn ProcessTreeProbe>,
    ) -> (PollerDeps, Arc<BroadcastEventBus>) {
        let bus = Arc::new(BroadcastEventBus::default());
        let deps = PollerDeps {
            event_bus: Arc::clone(&bus),
            http: client,
            proc_tree,
        };
        (deps, bus)
    }

    #[tokio::test]
    async fn tick_once_locks_uin_and_emits_logged_in() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            b.processes_responses.push_back(Ok(vec![proc(
                12346,
                "100200",
                HookProcessStatus::Online,
            )]));
            b.qq_responses.push_back(Ok(vec![instance("100200")]));
        }
        let probe: Arc<dyn ProcessTreeProbe> =
            Arc::new(MockProcessTreeProbe::with_set([12345u32, 12346u32]));
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::all());
        let mut state = PollerState::new(12345);
        tick_once(&bot_id, &deps, &mut state).await;

        assert_eq!(state.uin.as_deref(), Some("100200"));
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));

        // 收事件：UinDetected → LoginStateChanged{LoggedIn} → PidSetChanged
        let mut got_uin = false;
        let mut got_state = false;
        let mut got_pids = false;
        for _ in 0..3 {
            let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
                .await
                .expect("event in 1s")
                .expect("subscription open");
            match evt {
                DomainEvent::SnowLumaUinDetected { uin, .. } => {
                    assert_eq!(uin, "100200");
                    got_uin = true;
                }
                DomainEvent::SnowLumaLoginStateChanged { state, .. } => {
                    assert_eq!(state, SnowLumaLoginState::LoggedIn);
                    got_state = true;
                }
                DomainEvent::SnowLumaPidSetChanged { pids, .. } => {
                    assert_eq!(pids, vec![12346]);
                    got_pids = true;
                }
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert!(got_uin && got_state && got_pids);
    }

    #[tokio::test]
    async fn tick_once_consecutive_failures_emit_disconnected_only_once() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            // 让两端都失败，复用最后一条
            b.processes_responses
                .push_back(Err(SnowLumaWebUiError::Timeout {
                    endpoint: "/api/processes".into(),
                }));
            b.qq_responses.push_back(Err(SnowLumaWebUiError::Timeout {
                endpoint: "/api/qq-list".into(),
            }));
        }
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::new());
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaLoginStateChanged,
        ));
        let mut state = PollerState::new(12345);

        // 两次失败：未达门限，不应发任何事件。
        tick_once(&bot_id, &deps, &mut state).await;
        tick_once(&bot_id, &deps, &mut state).await;
        let r = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
        assert!(r.is_err(), "no event before threshold");

        // 第 3 次失败：达到门限，发一次 Disconnected。
        tick_once(&bot_id, &deps, &mut state).await;
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("disconnected within 1s")
            .expect("subscription open");
        match evt {
            DomainEvent::SnowLumaLoginStateChanged { state: s, .. } => {
                assert_eq!(s, SnowLumaLoginState::Disconnected);
            }
            other => panic!("expected LoginStateChanged, got {other:?}"),
        }

        // 第 4 / 5 次失败：last_state == Disconnected，不应再发。
        tick_once(&bot_id, &deps, &mut state).await;
        tick_once(&bot_id, &deps, &mut state).await;
        let r = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
        assert!(r.is_err(), "no duplicate Disconnected after threshold");
    }

    #[tokio::test]
    async fn tick_once_pid_set_change_emits_event() {
        let (client, behavior) = MockClient::new();
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::with_set([
            12345u32, 12346u32, 12347u32,
        ]));
        let (deps, bus) = build_test_deps(client, probe);
        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::kind(DomainEventKind::SnowLumaPidSetChanged));
        let mut state = PollerState::new(12345);

        // 第 1 轮：matched 集合 {12346}
        {
            let mut b = behavior.lock().await;
            b.processes_responses.push_back(Ok(vec![proc(
                12346,
                "100200",
                HookProcessStatus::Online,
            )]));
            b.qq_responses.push_back(Ok(vec![]));
        }
        tick_once(&bot_id, &deps, &mut state).await;
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("first PidSet event")
            .expect("open");
        match evt {
            DomainEvent::SnowLumaPidSetChanged { pids, .. } => {
                assert_eq!(pids, vec![12346]);
            }
            o => panic!("{o:?}"),
        }

        // 第 2 轮：matched 集合变成 {12346, 12347}
        {
            let mut b = behavior.lock().await;
            b.processes_responses.push_back(Ok(vec![
                proc(12346, "100200", HookProcessStatus::Online),
                proc(12347, "100200", HookProcessStatus::Online),
            ]));
            b.qq_responses.push_back(Ok(vec![]));
        }
        tick_once(&bot_id, &deps, &mut state).await;
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("second PidSet event")
            .expect("open");
        match evt {
            DomainEvent::SnowLumaPidSetChanged { pids, .. } => {
                assert_eq!(pids, vec![12346, 12347]);
            }
            o => panic!("{o:?}"),
        }
    }

    #[tokio::test]
    async fn dispose_emits_terminal_disconnected_once() {
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaLoginStateChanged,
        ));
        let mut state = PollerState::new(12345);
        let (client, _b) = MockClient::new();
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::new());
        let deps = PollerDeps {
            event_bus: Arc::clone(&bus),
            http: client,
            proc_tree: probe,
        };
        let bot_id = BotId::new("10001");

        emit_terminal_disconnected_if_needed(&bot_id, &deps, &mut state);
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("terminal Disconnected")
            .expect("open");
        match evt {
            DomainEvent::SnowLumaLoginStateChanged { state: s, .. } => {
                assert_eq!(s, SnowLumaLoginState::Disconnected);
            }
            o => panic!("{o:?}"),
        }

        // 再调一次：last_state == Disconnected，不应重复发。
        emit_terminal_disconnected_if_needed(&bot_id, &deps, &mut state);
        let r = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
        assert!(r.is_err());
    }
}
