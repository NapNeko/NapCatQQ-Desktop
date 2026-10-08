//! 原生截图的调用参数与待发送附件。
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatScreenshotRequest {
    pub hide_window: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatScreenshotAttachment {
    pub path: String,
    pub preview_path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub clipboard_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatScreenshotShortcut {
    pub enabled: bool,
    #[serde(default = "default_global_shortcut")]
    pub global: bool,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub release_owner: bool,
    #[serde(default = "default_global_shortcut")]
    pub hide_window: bool,
}

fn default_global_shortcut() -> bool {
    true
}

impl Default for ChatScreenshotShortcut {
    fn default() -> Self {
        Self {
            enabled: false,
            global: true,
            control: false,
            alt: false,
            shift: false,
            key: String::new(),
            context: String::new(),
            release_owner: false,
            hide_window: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatScreenshotShortcutEvent {
    pub v: u32,
    pub capture_id: String,
    pub context: String,
    pub result: ChatScreenshotShortcutResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub enum ChatScreenshotShortcutResult {
    Started,
    Finished {
        file: Option<ChatScreenshotAttachment>,
    },
    Failed {
        message: String,
    },
}

impl ChatScreenshotShortcut {
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if !self.control && !self.alt {
            return Err("截图快捷键需要 Ctrl 或 Alt".into());
        }
        let bytes = self.key.as_bytes();
        let character =
            bytes.len() == 1 && (bytes[0].is_ascii_uppercase() || bytes[0].is_ascii_digit());
        let function = matches!(
            self.key.as_str(),
            "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12"
        );
        if bytes.len() > 3 || (!character && !function) {
            return Err("截图快捷键只支持字母、数字或 F1–F12".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_accepts_only_representable_native_combinations() {
        let mut request = ChatScreenshotShortcut {
            enabled: true,
            global: true,
            control: true,
            alt: true,
            shift: false,
            key: "S".into(),
            ..Default::default()
        };
        for key in ["A", "Z", "0", "9", "F1", "F12"] {
            request.key = key.into();
            assert!(request.validate().is_ok(), "{key}");
        }
        for key in ["", "s", " F1", "F01", "F13", "Ctrl+S", "Delete", "é"] {
            request.key = key.into();
            assert!(request.validate().is_err(), "{key}");
        }
        request.key = "S".into();
        request.control = false;
        request.alt = false;
        request.shift = true;
        assert!(request.validate().is_err());
        request.enabled = false;
        request.key = "unsupported".into();
        assert!(request.validate().is_ok());
    }

    #[test]
    fn older_shortcut_requests_keep_global_scope_and_can_switch_to_focus_only() {
        let old = r#"{"enabled":true,"control":true,"alt":true,"shift":false,"key":"S"}"#;
        let mut request: ChatScreenshotShortcut = serde_json::from_str(old).unwrap();
        assert!(request.global);
        assert!(request.hide_window);
        request.global = false;
        assert!(request.validate().is_ok());
        let restored: ChatScreenshotShortcut =
            serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
        assert!(!restored.global);
    }

    #[test]
    fn screenshot_contracts_round_trip_without_pixel_payloads() {
        let shortcut = ChatScreenshotShortcut {
            enabled: true,
            control: true,
            alt: true,
            key: "S".into(),
            ..Default::default()
        };
        let encoded = serde_json::to_string(&shortcut).unwrap();
        assert_eq!(
            serde_json::from_str::<ChatScreenshotShortcut>(&encoded).unwrap(),
            shortcut
        );
        let attachment = ChatScreenshotAttachment {
            path: "full.png".into(),
            preview_path: "preview.png".into(),
            name: "截图.png".into(),
            width: 1920,
            height: 1080,
            clipboard_error: None,
        };
        let encoded = serde_json::to_value(&attachment).unwrap();
        assert_eq!(encoded["previewPath"], "preview.png");
        assert_eq!(
            serde_json::from_value::<ChatScreenshotAttachment>(encoded).unwrap(),
            attachment
        );
    }

    #[test]
    fn native_shortcut_result_keeps_its_capture_and_draft_context() {
        let event = ChatScreenshotShortcutEvent {
            v: 1,
            capture_id: "capture-1".into(),
            context: "original-draft".into(),
            result: ChatScreenshotShortcutResult::Finished { file: None },
        };
        let encoded = serde_json::to_string(&event).unwrap();
        assert_eq!(
            serde_json::from_str::<ChatScreenshotShortcutEvent>(&encoded).unwrap(),
            event
        );
        assert!(encoded.contains("\"kind\":\"finished\""));
    }
}
