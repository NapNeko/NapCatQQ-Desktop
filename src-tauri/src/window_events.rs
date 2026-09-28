// 主窗和托盘面板之间几条不走 DomainEvent 总线的窗口通知：名字和信封只在这里定义一次。
// 前端的名字集中在 desktop.service.ts，下面的测试核对两边一致

use serde::Serialize;
use ts_rs::TS;

/// 关窗动作是「退出」时，后端请主窗走退出闸门
pub const DESKTOP_REQUEST_CLOSE: &str = "desktop-request-close";
/// 托盘退出被本机 Bot 拦下，主窗要弹出闸门说明原因
pub const DESKTOP_EXIT_BLOCKED: &str = "desktop-exit-blocked";
/// 托盘面板每次被右键展开前发给面板窗口，面板据此重新量高度
pub const TRAY_PANEL_SHOW: &str = "tray_panel_show";

/// 这几条窗口通知的信封版本；前端自己发 desktop-request-close 时也填这个值
pub const WINDOW_EVENT_VERSION: u32 = 1;

/// 只通知一声、没有其它数据的窗口事件
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct WindowSignal {
    pub v: u32,
}

impl WindowSignal {
    pub const V1: Self = Self {
        v: WINDOW_EVENT_VERSION,
    };
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct DesktopExitBlocked {
    pub v: u32,
    pub local_active: u32,
}

impl DesktopExitBlocked {
    pub fn new(local_active: usize) -> Self {
        Self {
            v: WINDOW_EVENT_VERSION,
            local_active: u32::try_from(local_active).unwrap_or(u32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRONTEND_WINDOW_EVENTS_TS: &str =
        include_str!("../../src-ui/core/services/desktop.service.ts");

    #[test]
    fn payloads_carry_version() {
        assert_eq!(serde_json::to_value(WindowSignal::V1).unwrap(), serde_json::json!({ "v": 1 }));
        assert_eq!(
            serde_json::to_value(DesktopExitBlocked::new(2)).unwrap(),
            serde_json::json!({ "v": 1, "local_active": 2 })
        );
    }

    #[test]
    fn frontend_uses_same_event_names_and_version() {
        for name in [DESKTOP_REQUEST_CLOSE, DESKTOP_EXIT_BLOCKED, TRAY_PANEL_SHOW] {
            assert!(
                FRONTEND_WINDOW_EVENTS_TS.contains(&format!("'{name}'")),
                "window event {name:?} missing from desktop.service.ts"
            );
        }
        assert!(
            FRONTEND_WINDOW_EVENTS_TS.contains(&format!("{{ v: {WINDOW_EVENT_VERSION} }}")),
            "desktop.service.ts must emit WindowSignal with v = {WINDOW_EVENT_VERSION}"
        );
    }
}
