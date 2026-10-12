// SL 扫码状态；Debug 不包含短期登录材料。
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum SnowLumaQrLoginState {
    Disabled,
    Unavailable,
    Starting,
    WaitingForQr,
    Scanned,
    Expired,
    LoggedIn,
    LoginDetected,
    Cancelled,
    AccountMismatch,
    CleanupFailed,
}

impl SnowLumaQrLoginState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::LoggedIn
                | Self::LoginDetected
                | Self::Cancelled
                | Self::AccountMismatch
                | Self::CleanupFailed
                | Self::Unavailable
                | Self::Disabled
        )
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct SnowLumaQrLoginSnapshot {
    pub v: u32,
    pub state: SnowLumaQrLoginState,
    pub qq_pid: Option<u32>,
    pub qrcode_payload: Option<String>,
    pub uin: Option<String>,
    pub observer_removed: bool,
    pub message: Option<String>,
}

impl SnowLumaQrLoginSnapshot {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            v: 1,
            state: SnowLumaQrLoginState::Unavailable,
            qq_pid: None,
            qrcode_payload: None,
            uin: None,
            observer_removed: true,
            message: Some(message.into()),
        }
    }
}

impl std::fmt::Debug for SnowLumaQrLoginSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnowLumaQrLoginSnapshot")
            .field("v", &self.v)
            .field("state", &self.state)
            .field("qq_pid", &self.qq_pid)
            .field("has_qrcode", &self.qrcode_payload.is_some())
            .field("observer_removed", &self.observer_removed)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_contains_qrcode_material() {
        let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("test");
        snapshot.qrcode_payload = Some("https://txz.qq.com/p?k=private-login-material".into());
        assert!(!format!("{snapshot:?}").contains("private-login-material"));
    }

    #[test]
    fn missing_qr_login_flags_are_enabled() {
        let cfg: crate::SnowLumaAppConfig =
            serde_json::from_str(r#"{"snowlumaWebuiPort":5099}"#).unwrap();
        assert!(cfg.remote_qr_login_enabled);
        assert!(cfg.local_qr_login_enabled);
        assert!(crate::SnowLumaAppConfig::default().remote_qr_login_enabled);
        assert!(crate::SnowLumaAppConfig::default().local_qr_login_enabled);
    }

    #[test]
    fn explicit_disabled_qr_login_flags_survive_round_trip() {
        let cfg: crate::SnowLumaAppConfig = serde_json::from_str(
            r#"{"snowlumaRemoteQrLoginEnabled":false,"snowlumaLocalQrLoginEnabled":false}"#,
        )
        .unwrap();
        let restored: crate::SnowLumaAppConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert!(!restored.remote_qr_login_enabled);
        assert!(!restored.local_qr_login_enabled);
    }

    #[test]
    fn missing_local_flag_does_not_override_explicit_remote_disable() {
        let cfg: crate::SnowLumaAppConfig =
            serde_json::from_str(r#"{"snowlumaRemoteQrLoginEnabled":false}"#).unwrap();
        assert!(!cfg.remote_qr_login_enabled);
        assert!(cfg.local_qr_login_enabled);
    }
}
