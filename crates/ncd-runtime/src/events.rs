use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::bot_actor::BotActorSnapshot;
use crate::ids::BotId;
use crate::runtime_backend::BotStatus;
use crate::snowluma::daemon::DaemonState;
use crate::snowluma::status_poller::SnowLumaLoginState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainEventKind {
    BotStateChanged,
    BotStatusChanged,
    BotLogAppended,
    BotError,
    TaskProgress,
    // serde 默认 snake_case 会把 `NapCat...` 切成 `nap_cat_...`；这里
    // 显式 rename，与 `DomainEvent::tauri_event_name` 单一来源对齐。
    #[serde(rename = "napcat_webui_available")]
    NapCatWebuiAvailable,
    BotProcessExited,
    #[serde(rename = "napcat_login_qrcode")]
    NapCatLoginQrcode,
    #[serde(rename = "napcat_login_qrcode_removed")]
    NapCatLoginQrcodeRemoved,
    #[serde(rename = "napcat_login_online")]
    NapCatLoginOnline,
    #[serde(rename = "napcat_login_invalidated")]
    NapCatLoginInvalidated,
    // SnowLuma 系列：避免 `rename_all = "snake_case"` 把 `SnowLuma...` 切成
    // `snow_luma_...`，每个 variant 显式 rename。
    #[serde(rename = "snowluma_daemon_state_changed")]
    SnowLumaDaemonStateChanged,
    #[serde(rename = "snowluma_bot_injected")]
    SnowLumaBotInjected,
    #[serde(rename = "snowluma_uin_detected")]
    SnowLumaUinDetected,
    #[serde(rename = "snowluma_login_state_changed")]
    SnowLumaLoginStateChanged,
    #[serde(rename = "snowluma_pid_set_changed")]
    SnowLumaPidSetChanged,
    #[serde(rename = "snowluma_daemon_log")]
    SnowLumaDaemonLog,
}

/// 描述 NapCat WebUI 登录失效的原因。
/// - `Kicked`: 在线状态下账号被踢下线（在线 → 离线 + `is_login=false`）。
/// - `LoggedOut`: 用户主动登出或会话过期，从未达到 `online=true` 即失效。
/// `#[serde(rename_all = "snake_case")]` 与前端 `NapCatLoginInvalidationReason`
/// 字面量类型 (`'kicked' | 'logged_out'`) 保持字面量一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NapCatLoginInvalidationReason {
    Kicked,
    LoggedOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DomainEvent {
    BotStateChanged {
        snapshot: BotActorSnapshot,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    BotStatusChanged {
        status: BotStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    BotLogAppended {
        bot_id: BotId,
        line: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    BotError {
        bot_id: BotId,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
    },
    TaskProgress {
        task_id: String,
        progress: u8,
        message: String,
    },
    /// NapCat WebUI 已就绪：从 NapCat stdout 解析得到的登录入口。
    //
    // 注意：serde 默认 snake_case 会把 `NapCatWebuiAvailable` 切成
    // `nap_cat_webui_available`（连续大写字母都算单词边界）。这里显式
    // rename，与 `tauri_event_name` 保持单一字面量来源。
    #[serde(rename = "napcat_webui_available")]
    NapCatWebuiAvailable {
        bot_id: BotId,
        port: u16,
        token: String,
    },
    /// Bot 进程退出（包括正常退出、崩溃、被信号终止）。
    BotProcessExited {
        bot_id: BotId,
        /// 进程退出码；被信号终止或 wait 失败时可能为 None。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// NapCat WebUI 登录二维码可用：通常是 `data:image/png;base64,...`
    /// 也可能是普通 URL；后端透传，不做解析。
    #[serde(rename = "napcat_login_qrcode")]
    NapCatLoginQrcode { bot_id: BotId, qrcode_url: String },
    /// NapCat WebUI 登录二维码应当从 UI 上移除（已扫码登录、被踢、Poller dispose 等场景）。
    #[serde(rename = "napcat_login_qrcode_removed")]
    NapCatLoginQrcodeRemoved { bot_id: BotId },
    /// NapCat WebUI 在线状态变化（来自 `GetQQLoginInfo.online`）。
    #[serde(rename = "napcat_login_online")]
    NapCatLoginOnline { bot_id: BotId, online: bool },
    /// NapCat WebUI 登录失效（被踢 / 主动登出）。
    #[serde(rename = "napcat_login_invalidated")]
    NapCatLoginInvalidated {
        bot_id: BotId,
        reason: NapCatLoginInvalidationReason,
    },
    // ------------------------------------------------------------------
    // SnowLuma 系列 6 个 variant
    //
    // 每个 variant 显式 `#[serde(rename = "snowluma_xxx")]`：避免顶层
    // `rename_all = "snake_case"` 把 `SnowLuma...` 切成 `snow_luma_...`
    // 。
    // ------------------------------------------------------------------
    /// SnowLuma daemon 状态机切换。
    /// `state` 复用 `crates/ncd-core/src/snowluma/daemon.rs` 中的 ts-rs
    /// 派生 enum；`ref_count` 仅作监控信号；`reason` 在 Crashed 时携带
    /// daemon 最近一次错误描述。
    #[serde(rename = "snowluma_daemon_state_changed")]
    SnowLumaDaemonStateChanged {
        state: DaemonState,
        ref_count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// `/api/processes/:pid/load` 注入成功后发布（物理就绪）。
    /// 不等同于 QQ 已登录在线（业务就绪由 `SnowLumaLoginStateChanged` 表达）。
    #[serde(rename = "snowluma_bot_injected")]
    SnowLumaBotInjected { bot_id: BotId, qq_pid: u32 },
    /// SnowLumaStatusPoller 首次锁定 UIN 时发布。
    /// UIN 为字符串（与 `/api/qq-list` / `/api/processes` payload 字段类型对齐）。
    #[serde(rename = "snowluma_uin_detected")]
    SnowLumaUinDetected { bot_id: BotId, uin: String },
    /// SnowLumaStatusPoller 合成出的 4 档登录态变化事件。
    #[serde(rename = "snowluma_login_state_changed")]
    SnowLumaLoginStateChanged {
        bot_id: BotId,
        state: SnowLumaLoginState,
    },
    /// 已锁定 UIN 关联的 PID 集合发生变化（升序），由 manager 据此回写
    /// `ancillary_pids`。
    #[serde(rename = "snowluma_pid_set_changed")]
    SnowLumaPidSetChanged { bot_id: BotId, pids: Vec<u32> },
    /// SnowLuma daemon 共享的 node.exe stdout 单行（已经过 ANSI / 控制
    /// 字符清洗）。多 SL Bot 共享同一份 daemon stdout，故本 variant 不
    /// 携带 `bot_id`，订阅方根据需要广播给所有 SL flavor BotLogPage。
    #[serde(rename = "snowluma_daemon_log")]
    SnowLumaDaemonLog { line: String },
}

impl DomainEvent {
    pub fn kind(&self) -> DomainEventKind {
        match self {
            Self::BotStateChanged { .. } => DomainEventKind::BotStateChanged,
            Self::BotStatusChanged { .. } => DomainEventKind::BotStatusChanged,
            Self::BotLogAppended { .. } => DomainEventKind::BotLogAppended,
            Self::BotError { .. } => DomainEventKind::BotError,
            Self::TaskProgress { .. } => DomainEventKind::TaskProgress,
            Self::NapCatWebuiAvailable { .. } => DomainEventKind::NapCatWebuiAvailable,
            Self::BotProcessExited { .. } => DomainEventKind::BotProcessExited,
            Self::NapCatLoginQrcode { .. } => DomainEventKind::NapCatLoginQrcode,
            Self::NapCatLoginQrcodeRemoved { .. } => DomainEventKind::NapCatLoginQrcodeRemoved,
            Self::NapCatLoginOnline { .. } => DomainEventKind::NapCatLoginOnline,
            Self::NapCatLoginInvalidated { .. } => DomainEventKind::NapCatLoginInvalidated,
            Self::SnowLumaDaemonStateChanged { .. } => DomainEventKind::SnowLumaDaemonStateChanged,
            Self::SnowLumaBotInjected { .. } => DomainEventKind::SnowLumaBotInjected,
            Self::SnowLumaUinDetected { .. } => DomainEventKind::SnowLumaUinDetected,
            Self::SnowLumaLoginStateChanged { .. } => DomainEventKind::SnowLumaLoginStateChanged,
            Self::SnowLumaPidSetChanged { .. } => DomainEventKind::SnowLumaPidSetChanged,
            Self::SnowLumaDaemonLog { .. } => DomainEventKind::SnowLumaDaemonLog,
        }
    }

    pub fn tauri_event_name(&self) -> &'static str {
        match self {
            Self::BotStateChanged { .. } => "bot_state_changed",
            Self::BotStatusChanged { .. } => "bot_status_changed",
            Self::BotLogAppended { .. } => "bot_log_appended",
            Self::BotError { .. } => "bot_error",
            Self::TaskProgress { .. } => "task_progress",
            Self::NapCatWebuiAvailable { .. } => "napcat_webui_available",
            Self::BotProcessExited { .. } => "bot_process_exited",
            Self::NapCatLoginQrcode { .. } => "napcat_login_qrcode",
            Self::NapCatLoginQrcodeRemoved { .. } => "napcat_login_qrcode_removed",
            Self::NapCatLoginOnline { .. } => "napcat_login_online",
            Self::NapCatLoginInvalidated { .. } => "napcat_login_invalidated",
            Self::SnowLumaDaemonStateChanged { .. } => "snowluma_daemon_state_changed",
            Self::SnowLumaBotInjected { .. } => "snowluma_bot_injected",
            Self::SnowLumaUinDetected { .. } => "snowluma_uin_detected",
            Self::SnowLumaLoginStateChanged { .. } => "snowluma_login_state_changed",
            Self::SnowLumaPidSetChanged { .. } => "snowluma_pid_set_changed",
            Self::SnowLumaDaemonLog { .. } => "snowluma_daemon_log",
        }
    }

    pub fn bot_id(&self) -> Option<&BotId> {
        match self {
            Self::BotStateChanged { snapshot, .. } => Some(&snapshot.bot_id),
            Self::BotStatusChanged { status, .. } => Some(&status.bot_id),
            Self::BotLogAppended { bot_id, .. } => Some(bot_id),
            Self::BotError { bot_id, .. } => Some(bot_id),
            Self::TaskProgress { .. } => None,
            Self::NapCatWebuiAvailable { bot_id, .. } => Some(bot_id),
            Self::BotProcessExited { bot_id, .. } => Some(bot_id),
            Self::NapCatLoginQrcode { bot_id, .. } => Some(bot_id),
            Self::NapCatLoginQrcodeRemoved { bot_id, .. } => Some(bot_id),
            Self::NapCatLoginOnline { bot_id, .. } => Some(bot_id),
            Self::NapCatLoginInvalidated { bot_id, .. } => Some(bot_id),
            // SnowLuma 系列：daemon 级事件 / 仅 daemon log 不携带 bot_id
            // 其他 5 个 per-Bot 事件返回 Some。
            Self::SnowLumaDaemonStateChanged { .. } => None,
            Self::SnowLumaBotInjected { bot_id, .. } => Some(bot_id),
            Self::SnowLumaUinDetected { bot_id, .. } => Some(bot_id),
            Self::SnowLumaLoginStateChanged { bot_id, .. } => Some(bot_id),
            Self::SnowLumaPidSetChanged { bot_id, .. } => Some(bot_id),
            Self::SnowLumaDaemonLog { .. } => None,
        }
    }

    pub fn bot_state_changed(snapshot: BotActorSnapshot, reason: impl Into<String>) -> Self {
        Self::BotStateChanged {
            snapshot,
            reason: Some(reason.into()),
        }
    }

    pub fn bot_log(bot_id: impl Into<BotId>, line: impl Into<String>) -> Self {
        Self::BotLogAppended {
            bot_id: bot_id.into(),
            line: line.into(),
            channel: None,
        }
    }

    pub fn bot_status_changed(status: BotStatus, source: impl Into<String>) -> Self {
        Self::BotStatusChanged {
            status,
            source: Some(source.into()),
        }
    }

    pub fn bot_error(
        bot_id: impl Into<BotId>,
        message: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self::BotError {
            bot_id: bot_id.into(),
            message: message.into(),
            hint,
        }
    }

    pub fn task_progress(
        task_id: impl Into<String>,
        progress: u8,
        message: impl Into<String>,
    ) -> Self {
        Self::TaskProgress {
            task_id: task_id.into(),
            progress,
            message: message.into(),
        }
    }

    pub fn napcat_webui_available(
        bot_id: impl Into<BotId>,
        port: u16,
        token: impl Into<String>,
    ) -> Self {
        Self::NapCatWebuiAvailable {
            bot_id: bot_id.into(),
            port,
            token: token.into(),
        }
    }

    pub fn bot_process_exited(
        bot_id: impl Into<BotId>,
        exit_code: Option<i32>,
        reason: Option<String>,
    ) -> Self {
        Self::BotProcessExited {
            bot_id: bot_id.into(),
            exit_code,
            reason,
        }
    }

    pub fn napcat_login_qrcode(bot_id: impl Into<BotId>, qrcode_url: impl Into<String>) -> Self {
        Self::NapCatLoginQrcode {
            bot_id: bot_id.into(),
            qrcode_url: qrcode_url.into(),
        }
    }

    pub fn napcat_login_qrcode_removed(bot_id: impl Into<BotId>) -> Self {
        Self::NapCatLoginQrcodeRemoved {
            bot_id: bot_id.into(),
        }
    }

    pub fn napcat_login_online(bot_id: impl Into<BotId>, online: bool) -> Self {
        Self::NapCatLoginOnline {
            bot_id: bot_id.into(),
            online,
        }
    }

    pub fn napcat_login_invalidated(
        bot_id: impl Into<BotId>,
        reason: NapCatLoginInvalidationReason,
    ) -> Self {
        Self::NapCatLoginInvalidated {
            bot_id: bot_id.into(),
            reason,
        }
    }

    // ------------------------------------------------------------------
    // SnowLuma 系列 helper 构造器
    // ------------------------------------------------------------------

    pub fn snowluma_daemon_state_changed(
        state: DaemonState,
        ref_count: u32,
        reason: Option<String>,
    ) -> Self {
        Self::SnowLumaDaemonStateChanged {
            state,
            ref_count,
            reason,
        }
    }

    pub fn snowluma_bot_injected(bot_id: impl Into<BotId>, qq_pid: u32) -> Self {
        Self::SnowLumaBotInjected {
            bot_id: bot_id.into(),
            qq_pid,
        }
    }

    pub fn snowluma_uin_detected(bot_id: impl Into<BotId>, uin: impl Into<String>) -> Self {
        Self::SnowLumaUinDetected {
            bot_id: bot_id.into(),
            uin: uin.into(),
        }
    }

    pub fn snowluma_login_state_changed(
        bot_id: impl Into<BotId>,
        state: SnowLumaLoginState,
    ) -> Self {
        Self::SnowLumaLoginStateChanged {
            bot_id: bot_id.into(),
            state,
        }
    }

    pub fn snowluma_pid_set_changed(bot_id: impl Into<BotId>, pids: Vec<u32>) -> Self {
        Self::SnowLumaPidSetChanged {
            bot_id: bot_id.into(),
            pids,
        }
    }

    pub fn snowluma_daemon_log(line: impl Into<String>) -> Self {
        Self::SnowLumaDaemonLog { line: line.into() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventFilter {
    pub bot_id: Option<BotId>,
    pub kind: Option<DomainEventKind>,
}

impl EventFilter {
    pub fn all() -> Self {
        Self::default()
    }

    pub fn bot(bot_id: impl Into<BotId>) -> Self {
        Self {
            bot_id: Some(bot_id.into()),
            kind: None,
        }
    }

    pub fn kind(kind: DomainEventKind) -> Self {
        Self {
            bot_id: None,
            kind: Some(kind),
        }
    }

    pub fn matches(&self, event: &DomainEvent) -> bool {
        if let Some(kind) = self.kind
            && event.kind() != kind
        {
            return false;
        }
        if let Some(bot_id) = &self.bot_id
            && event.bot_id() != Some(bot_id)
        {
            return false;
        }
        true
    }
}

pub struct EventSubscription {
    receiver: broadcast::Receiver<DomainEvent>,
    filter: EventFilter,
}

impl EventSubscription {
    pub async fn next(&mut self) -> Option<DomainEvent> {
        loop {
            match self.receiver.recv().await {
                Ok(event) if self.filter.matches(&event) => return Some(event),
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

pub trait EventBus: Send + Sync {
    fn publish(&self, event: DomainEvent);
    fn subscribe(&self, filter: EventFilter) -> EventSubscription;
}

#[derive(Debug, Clone)]
pub struct BroadcastEventBus {
    sender: broadcast::Sender<DomainEvent>,
}

impl BroadcastEventBus {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
}

impl Default for BroadcastEventBus {
    fn default() -> Self {
        Self::new(128)
    }
}

impl EventBus for BroadcastEventBus {
    fn publish(&self, event: DomainEvent) {
        let _ = self.sender.send(event);
    }

    fn subscribe(&self, filter: EventFilter) -> EventSubscription {
        EventSubscription {
            receiver: self.sender.subscribe(),
            filter,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_actor::BotActorSnapshot;

    #[test]
    fn event_name_mapping_matches_frontend_contract() {
        let event = DomainEvent::bot_log("10001", "hello");
        assert_eq!(event.tauri_event_name(), "bot_log_appended");
        assert_eq!(event.kind(), DomainEventKind::BotLogAppended);
    }

    #[tokio::test]
    async fn broadcast_event_bus_filters_by_bot_id() {
        let bus = BroadcastEventBus::default();
        let mut subscription = bus.subscribe(EventFilter::bot("10001"));

        bus.publish(DomainEvent::bot_log("10002", "skip"));
        bus.publish(DomainEvent::bot_log("10001", "hit"));

        let event = subscription.next().await.expect("expected matching event");
        match event {
            DomainEvent::BotLogAppended { bot_id, line, .. } => {
                assert_eq!(bot_id.as_str(), "10001");
                assert_eq!(line, "hit");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn bot_status_changed_event_serializes() {
        let status = BotStatus::running("10004", 1234, 5678);
        let event = DomainEvent::bot_status_changed(status, "runtime_poll");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("bot_status_changed"));
        assert!(json.contains("runtime_poll"));
    }

    #[test]
    fn bot_state_changed_event_serializes() {
        let snapshot = BotActorSnapshot::new("10003");
        let event = DomainEvent::bot_state_changed(snapshot, "start_requested");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("bot_state_changed"));
        assert!(json.contains("start_requested"));
    }

    // ------------------------------------------------------------------
    // 事件名稳定性测试
    //
    // 1) 4 个新 variant 字节级 round-trip：序列化后再反序列化必须等价。
    // 2) 4 个新 variant 的 `tauri_event_name` 字面量值锁定。
    // 3) 跨文件契约：4 个 tauri_event_name 必须全部出现在前端
    // `event-stream.service.ts` 的 `DOMAIN_EVENT_NAMES` 数组中
    // （编译期 `include_str!` 取出文本后 grep）。
    // ------------------------------------------------------------------

    /// 编译期把前端事件清单嵌入测试二进制，避免运行时 IO 与路径漂移。
    /// 路径相对于本文件 (`crates/ncd-runtime/src/events.rs`) → 仓库根
    /// → `src-ui/core/services/event-stream.service.ts`。
    const FRONTEND_EVENTS_TS: &str =
        include_str!("../../../src-ui/core/services/event-stream.service.ts");

    fn assert_round_trip(event: DomainEvent) {
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: DomainEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, event, "round-trip must preserve equality");
        // 二次序列化后字节也应该等价（serde_json 的输出对同一结构是稳定的）。
        let json2 = serde_json::to_string(&decoded).expect("re-serialize");
        assert_eq!(json, json2, "byte-level round-trip must be stable");
    }

    #[test]
    fn napcat_login_qrcode_round_trips() {
        assert_round_trip(DomainEvent::napcat_login_qrcode(
            "10001",
            "data:image/png;base64,AAAA",
        ));
    }

    #[test]
    fn napcat_login_qrcode_removed_round_trips() {
        assert_round_trip(DomainEvent::napcat_login_qrcode_removed("10001"));
    }

    #[test]
    fn napcat_login_online_round_trips() {
        assert_round_trip(DomainEvent::napcat_login_online("10001", true));
        assert_round_trip(DomainEvent::napcat_login_online("10001", false));
    }

    #[test]
    fn napcat_login_invalidated_round_trips() {
        assert_round_trip(DomainEvent::napcat_login_invalidated(
            "10001",
            NapCatLoginInvalidationReason::Kicked,
        ));
        assert_round_trip(DomainEvent::napcat_login_invalidated(
            "10001",
            NapCatLoginInvalidationReason::LoggedOut,
        ));
    }

    #[test]
    fn napcat_login_invalidation_reason_serializes_snake_case() {
        // 字面量锁定：前端 TS 字面量类型为 'kicked' | 'logged_out'。
        assert_eq!(
            serde_json::to_string(&NapCatLoginInvalidationReason::Kicked).unwrap(),
            "\"kicked\""
        );
        assert_eq!(
            serde_json::to_string(&NapCatLoginInvalidationReason::LoggedOut).unwrap(),
            "\"logged_out\""
        );
    }

    /// 4 个新 variant 的 `tauri_event_name` 字面量值锁定
    /// 任何一处 typo 都会让此测试失败。
    #[test]
    fn napcat_login_event_name_literals_are_stable() {
        let cases: [(DomainEvent, &str); 4] = [
            (
                DomainEvent::napcat_login_qrcode("10001", "url"),
                "napcat_login_qrcode",
            ),
            (
                DomainEvent::napcat_login_qrcode_removed("10001"),
                "napcat_login_qrcode_removed",
            ),
            (
                DomainEvent::napcat_login_online("10001", true),
                "napcat_login_online",
            ),
            (
                DomainEvent::napcat_login_invalidated(
                    "10001",
                    NapCatLoginInvalidationReason::Kicked,
                ),
                "napcat_login_invalidated",
            ),
        ];
        for (event, expected) in &cases {
            assert_eq!(event.tauri_event_name(), *expected);
        }
    }

    /// 前后端事件契约一一对应。
    /// 4 个新 `tauri_event_name` 必须在前端 `event-stream.service.ts` 的
    /// `DOMAIN_EVENT_NAMES` 数组中出现为单/双引号字符串字面量。这样可避免
    /// 误把出现在注释或别的标识符中的子串当作匹配。
    #[test]
    fn napcat_login_event_names_are_present_in_frontend_events_ts() {
        let names = [
            "napcat_login_qrcode",
            "napcat_login_qrcode_removed",
            "napcat_login_online",
            "napcat_login_invalidated",
        ];
        for name in &names {
            let needle_single = format!("'{name}'");
            let needle_double = format!("\"{name}\"");
            assert!(
                FRONTEND_EVENTS_TS.contains(&needle_single)
                    || FRONTEND_EVENTS_TS.contains(&needle_double),
                "frontend event-stream.service.ts must contain literal {name:?} \
 as a quoted string (检查 src-ui/core/services/event-stream.service.ts 的 \
 DOMAIN_EVENT_NAMES 数组)",
            );
        }
    }

    /// 反向防呆：所有 DomainEvent variant 的 `tauri_event_name` 都必须
    /// 出现在前端 `event-stream.service.ts` 中，否则前端无法订阅到对应事件。
    /// 这条断言锁定了「Rust → DOMAIN_EVENT_NAMES」单向覆盖，但允许
    /// DOMAIN_EVENT_NAMES 含 DomainEvent 之外的额外通道（按任务说明）。
    #[test]
    fn every_domain_event_variant_is_listed_in_frontend_events_ts() {
        // 用每种 variant 的代表实例覆盖全部分支。
        let snapshot = BotActorSnapshot::new("10001");
        let status = BotStatus::running("10001", 1, 0);
        let all: Vec<DomainEvent> = vec![
            DomainEvent::bot_state_changed(snapshot, "init"),
            DomainEvent::bot_status_changed(status, "init"),
            DomainEvent::bot_log("10001", "x"),
            DomainEvent::bot_error("10001", "x", None),
            DomainEvent::task_progress("t", 0, "x"),
            DomainEvent::napcat_webui_available("10001", 6099, "tk"),
            DomainEvent::bot_process_exited("10001", Some(0), None),
            DomainEvent::napcat_login_qrcode("10001", "url"),
            DomainEvent::napcat_login_qrcode_removed("10001"),
            DomainEvent::napcat_login_online("10001", true),
            DomainEvent::napcat_login_invalidated("10001", NapCatLoginInvalidationReason::Kicked),
            // SnowLuma 系列 6 个 variant
            // ，与前端 event-stream.service.ts DOMAIN_EVENT_NAMES 一一对应。
            DomainEvent::snowluma_daemon_state_changed(DaemonState::Ready, 1, None),
            DomainEvent::snowluma_bot_injected("10001", 12345),
            DomainEvent::snowluma_uin_detected("10001", "100200"),
            DomainEvent::snowluma_login_state_changed("10001", SnowLumaLoginState::LoggedIn),
            DomainEvent::snowluma_pid_set_changed("10001", vec![1234, 5678]),
            DomainEvent::snowluma_daemon_log("hello world"),
        ];
        for event in &all {
            let name = event.tauri_event_name();
            let needle_single = format!("'{name}'");
            let needle_double = format!("\"{name}\"");
            assert!(
                FRONTEND_EVENTS_TS.contains(&needle_single)
                    || FRONTEND_EVENTS_TS.contains(&needle_double),
                "DomainEvent::{:?} 的 tauri_event_name {name:?} 未出现在 \
 src-ui/core/services/event-stream.service.ts 的 DOMAIN_EVENT_NAMES，\
 前端将无法订阅",
                event.kind(),
            );
        }
    }

    // ------------------------------------------------------------------
    // SnowLuma 系列 6 个 variant 的稳定性测试
    //
    //
    // 1) 6 个 variant 字节级 round-trip：序列化后再反序列化必须等价。
    // 2) 6 个 variant `tauri_event_name` 字面量值锁定，防 typo / 防
    // `rename_all = "snake_case"` 把 `SnowLuma` 切成 `snow_luma`。
    // 3) 跨文件契约：6 个 tauri_event_name 必须全部出现在前端
    // event-stream.service.ts 的 DOMAIN_EVENT_NAMES 数组。
    // ------------------------------------------------------------------

    #[test]
    fn snowluma_daemon_state_changed_round_trips() {
        assert_round_trip(DomainEvent::snowluma_daemon_state_changed(
            DaemonState::Ready,
            1,
            None,
        ));
        assert_round_trip(DomainEvent::snowluma_daemon_state_changed(
            DaemonState::Crashed,
            0,
            Some("node child exited unexpectedly".into()),
        ));
    }

    #[test]
    fn snowluma_bot_injected_round_trips() {
        assert_round_trip(DomainEvent::snowluma_bot_injected("10001", 12345));
    }

    #[test]
    fn snowluma_uin_detected_round_trips() {
        assert_round_trip(DomainEvent::snowluma_uin_detected("10001", "100200"));
    }

    #[test]
    fn snowluma_login_state_changed_round_trips() {
        assert_round_trip(DomainEvent::snowluma_login_state_changed(
            "10001",
            SnowLumaLoginState::LoggedIn,
        ));
        assert_round_trip(DomainEvent::snowluma_login_state_changed(
            "10001",
            SnowLumaLoginState::WaitingForQrScan,
        ));
    }

    #[test]
    fn snowluma_pid_set_changed_round_trips() {
        assert_round_trip(DomainEvent::snowluma_pid_set_changed(
            "10001",
            vec![1234, 5678],
        ));
        // 空集合也必须可 round-trip（poller dispose 时可能下发空集合）。
        assert_round_trip(DomainEvent::snowluma_pid_set_changed("10001", vec![]));
    }

    #[test]
    fn snowluma_daemon_log_round_trips() {
        assert_round_trip(DomainEvent::snowluma_daemon_log("hello world"));
    }

    /// 6 个 SL variant 的 `tauri_event_name` 字面量值锁定
    /// 任何一处 typo（包括 `snow_luma_xxx` 这种 snake_case 误切）都会失败。
    #[test]
    fn snowluma_event_name_literals_are_stable() {
        let cases: [(DomainEvent, &str); 6] = [
            (
                DomainEvent::snowluma_daemon_state_changed(DaemonState::Ready, 1, None),
                "snowluma_daemon_state_changed",
            ),
            (
                DomainEvent::snowluma_bot_injected("10001", 12345),
                "snowluma_bot_injected",
            ),
            (
                DomainEvent::snowluma_uin_detected("10001", "100200"),
                "snowluma_uin_detected",
            ),
            (
                DomainEvent::snowluma_login_state_changed("10001", SnowLumaLoginState::LoggedIn),
                "snowluma_login_state_changed",
            ),
            (
                DomainEvent::snowluma_pid_set_changed("10001", vec![1234, 5678]),
                "snowluma_pid_set_changed",
            ),
            (
                DomainEvent::snowluma_daemon_log("hello world"),
                "snowluma_daemon_log",
            ),
        ];
        for (event, expected) in &cases {
            assert_eq!(event.tauri_event_name(), *expected);
        }
    }

    /// 前后端事件契约一一对应（SnowLuma 系列）。
    /// 6 个新 `tauri_event_name` 必须在前端 `event-stream.service.ts` 的
    /// `DOMAIN_EVENT_NAMES` 中出现为单/双引号字符串字面量。
    #[test]
    fn snowluma_event_names_are_present_in_frontend_events_ts() {
        let names = [
            "snowluma_daemon_state_changed",
            "snowluma_bot_injected",
            "snowluma_uin_detected",
            "snowluma_login_state_changed",
            "snowluma_pid_set_changed",
            "snowluma_daemon_log",
        ];
        for name in &names {
            let needle_single = format!("'{name}'");
            let needle_double = format!("\"{name}\"");
            assert!(
                FRONTEND_EVENTS_TS.contains(&needle_single)
                    || FRONTEND_EVENTS_TS.contains(&needle_double),
                "frontend event-stream.service.ts must contain literal {name:?} \
 as a quoted string (检查 src-ui/core/services/event-stream.service.ts 的 \
 DOMAIN_EVENT_NAMES 数组)",
            );
        }
    }
}
