// 只有 SL 明确确认目标进程尚未登录且已连接时，扫码辅助才允许取图。
use ncd_domain::{SnowLumaQrLoginSnapshot, SnowLumaQrLoginState};

use crate::HookProcessInfo;

pub fn process_gate(
    process: Option<&HookProcessInfo>,
    pid: u32,
    expected_uin: &str,
) -> Option<SnowLumaQrLoginSnapshot> {
    let process = process.filter(|process| process.pid == pid);
    let mut snapshot = SnowLumaQrLoginSnapshot {
        v: 1,
        state: SnowLumaQrLoginState::Starting,
        qq_pid: Some(pid),
        qrcode_payload: None,
        uin: None,
        observer_removed: true,
        message: Some("等待 SL 确认登录状态".into()),
    };
    if let Some(process) = process {
        if process.logged_in == Some(true) {
            snapshot.state = if process.uin.is_empty() {
                SnowLumaQrLoginState::LoginDetected
            } else if process.uin == expected_uin {
                SnowLumaQrLoginState::LoggedIn
            } else {
                SnowLumaQrLoginState::AccountMismatch
            };
            snapshot.uin = (!process.uin.is_empty()).then(|| process.uin.clone());
            snapshot.message = None;
        } else if process.logged_in == Some(false)
            && process.connected == Some(true)
            && process.injected == Some(true)
        {
            return None;
        }
    }
    Some(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process() -> HookProcessInfo {
        serde_json::from_str(
            r#"{"pid":7,"uin":"","name":"qq","path":"/opt/QQ/qq","status":"loaded"}"#,
        )
        .expect("process fixture")
    }

    #[test]
    fn only_explicit_connected_logged_out_process_can_capture() {
        let mut process = process();
        assert!(process_gate(Some(&process), 7, "123").is_some());
        process.logged_in = Some(false);
        process.connected = Some(true);
        assert!(process_gate(Some(&process), 7, "123").is_some());
        process.injected = Some(true);
        assert!(process_gate(Some(&process), 7, "123").is_none());
        assert!(process_gate(Some(&process), 8, "123").is_some());
        assert!(process_gate(None, 7, "123").is_some());
    }

    #[test]
    fn login_requires_matching_account_and_never_returns_material() {
        let mut process = process();
        process.logged_in = Some(true);
        let detected = process_gate(Some(&process), 7, "123").expect("login disposition");
        assert_eq!(detected.state, SnowLumaQrLoginState::LoginDetected);
        process.uin = "999".into();
        let mismatch = process_gate(Some(&process), 7, "123").expect("login disposition");
        assert_eq!(mismatch.state, SnowLumaQrLoginState::AccountMismatch);
        assert!(mismatch.qrcode_payload.is_none());
        process.uin = "123".into();
        assert_eq!(
            process_gate(Some(&process), 7, "123")
                .expect("login disposition")
                .state,
            SnowLumaQrLoginState::LoggedIn
        );
    }
}
