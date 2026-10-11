// SnowLuma 登录探测与隧道的 UI 快照契约。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::daemon_state::{DaemonState, SnowLumaLoginState};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct SnowlumaUiBotDto {
    pub bot_id: String,
    pub injected: bool,
    pub uin: Option<String>,
    pub login_state: Option<SnowLumaLoginState>,
    pub endpoints_ready: bool,
    #[serde(default)]
    pub probe_unavailable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct SnowlumaUiSnapshotDto {
    // 兼容旧前端：仅表示本机 daemon。
    pub daemon_state: Option<DaemonState>,
    pub daemon_states: HashMap<String, DaemonState>,
    pub bots: Vec<SnowlumaUiBotDto>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_probe_survives_snapshot_round_trip() {
        let snapshot = SnowlumaUiSnapshotDto {
            daemon_state: None,
            daemon_states: HashMap::from([("remote".into(), DaemonState::Ready)]),
            bots: vec![SnowlumaUiBotDto {
                bot_id: "10001".into(),
                injected: true,
                uin: Some("10001".into()),
                login_state: None,
                endpoints_ready: true,
                probe_unavailable: true,
            }],
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let decoded: SnowlumaUiSnapshotDto = serde_json::from_str(&json).unwrap();
        assert!(decoded.bots[0].probe_unavailable);
        assert_eq!(decoded.bots[0].login_state, None);
    }

    #[test]
    fn legacy_bot_snapshot_defaults_probe_availability() {
        let decoded: SnowlumaUiBotDto = serde_json::from_str(
            r#"{"bot_id":"10001","injected":true,"uin":null,"login_state":null,"endpoints_ready":true}"#,
        )
        .unwrap();
        assert!(!decoded.probe_unavailable);
    }
}
