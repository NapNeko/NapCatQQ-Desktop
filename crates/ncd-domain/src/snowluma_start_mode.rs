// 本文件只放 SnowLuma 启动模式枚举 `SnowLumaStartMode` 与基础 helper。
//
// `SnowLumaLaunchPlan` struct 的完整字段（runtime_root / snowluma_data_root /
// start_mode / qq_install_path / bot_qq_id）由 在
// `crates/ncd-core/src/runtime_launch_plan.rs` 中扩展，避免与本文件耦合。
//
// 严格红线：本枚举跨 Tauri / 前端边界，必须用强类型 serde + ts-rs 派生
// 导出 TypeScript 类型。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// SnowLuma 启动模式：决定 SnowLuma backend 在 Phase A 是否需要自己 spawn QQ.exe。
/// - [`SnowLumaStartMode::ColdStart`]：用户希望 backend 全权 spawn QQ.exe
/// 随后由 SnowLuma daemon 注入。stop 时由 backend 负责终结 QQ.exe 进程树。
/// - [`SnowLumaStartMode::HotStart`]：用户已自己启动了 QQ.exe（典型场景：
/// 想保留人手登录得到的会话），backend 仅 attach 到该 PID；stop 时**绝不**
/// kill 用户的 QQ.exe 进程，只是 unload daemon 的 hook。
/// 序列化形态（`#[serde(tag = "mode", rename_all = "snake_case")]`）：
/// ```json
/// { "mode": "cold_start" }
/// { "mode": "hot_start", "attach_pid": 12345 }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum SnowLumaStartMode {
    /// 由 backend 负责 spawn QQ.exe（cold start 路径）。
    ColdStart,
    /// 由用户自行启动 QQ.exe，backend 仅 attach 到给定 PID（hot start 路径）。
    HotStart {
        /// 用户已启动的 QQ.exe 进程 PID。
        attach_pid: u32,
    },
}

impl SnowLumaStartMode {
    /// 返回 hot start 携带的 `attach_pid`，cold start 返回 `None`。
    /// 用于 backend Phase A 决策与日志打印。
    pub fn attach_pid(&self) -> Option<u32> {
        match self {
            SnowLumaStartMode::ColdStart => None,
            SnowLumaStartMode::HotStart { attach_pid } => Some(*attach_pid),
        }
    }

    /// 是否 cold start 模式（backend spawn QQ.exe）。
    pub fn is_cold(&self) -> bool {
        matches!(self, SnowLumaStartMode::ColdStart)
    }

    /// 是否 hot start 模式（attach 到用户自启的 QQ.exe）。
    pub fn is_hot(&self) -> bool {
        matches!(self, SnowLumaStartMode::HotStart { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 字节级 round-trip：序列化后字面量必须与设计一致，且再反序列化等价。
    /// 锁定 wire format：
    /// - `ColdStart` → `{"mode":"cold_start"}`
    /// - `HotStart { attach_pid: 12345 }` → `{"mode":"hot_start","attach_pid":12345}`
    /// 这两条字面量是前后端契约（ts-rs 导出后由前端 BotConfigPage 直接消费）
    /// 任何字段名 / variant 名变更都会破坏迁移，不允许漂移。
    #[test]
    fn cold_start_round_trip_is_byte_stable() {
        let value = SnowLumaStartMode::ColdStart;

        let serialized = serde_json::to_string(&value).expect("serialize ColdStart");
        assert_eq!(serialized, r#"{"mode":"cold_start"}"#);

        let decoded: SnowLumaStartMode =
            serde_json::from_str(&serialized).expect("deserialize ColdStart");
        assert_eq!(decoded, value);

        // 二次序列化字节等价。
        let serialized_again = serde_json::to_string(&decoded).expect("re-serialize ColdStart");
        assert_eq!(serialized.as_bytes(), serialized_again.as_bytes());
    }

    #[test]
    fn hot_start_round_trip_is_byte_stable() {
        let value = SnowLumaStartMode::HotStart { attach_pid: 12345 };

        let serialized = serde_json::to_string(&value).expect("serialize HotStart");
        assert_eq!(serialized, r#"{"mode":"hot_start","attach_pid":12345}"#);

        let decoded: SnowLumaStartMode =
            serde_json::from_str(&serialized).expect("deserialize HotStart");
        assert_eq!(decoded, value);

        let serialized_again = serde_json::to_string(&decoded).expect("re-serialize HotStart");
        assert_eq!(serialized.as_bytes(), serialized_again.as_bytes());
    }

    #[test]
    fn helpers_report_mode_correctly() {
        let cold = SnowLumaStartMode::ColdStart;
        assert!(cold.is_cold());
        assert!(!cold.is_hot());
        assert_eq!(cold.attach_pid(), None);

        let hot = SnowLumaStartMode::HotStart { attach_pid: 9876 };
        assert!(!hot.is_cold());
        assert!(hot.is_hot());
        assert_eq!(hot.attach_pid(), Some(9876));
    }
}
