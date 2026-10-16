//! Windows 远端监督协议；传输为限长 NDJSON，不携带桌面登录凭据。
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

pub const WINDOWS_AGENT_PROTOCOL: u32 = 1;
pub const WINDOWS_AGENT_MAX_FRAME: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct RemoteWindowsInfo {
    pub v: u32,
    pub os_version: String,
    pub architecture: String,
    pub user_sid: String,
    pub username: String,
    pub session_id: u32,
    pub elevated: bool,
    pub home_dir: String,
    pub program_data: String,
    pub temp_dir: String,
    pub managed_root: String,
    pub user_root: String,
    pub agent: Option<WindowsAgentStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsAgentStatus {
    pub v: u32,
    pub version: String,
    pub service_running: bool,
    pub service_identity: String,
    pub capabilities: Vec<String>,
    pub sessions: Vec<WindowsSessionInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsSessionInfo {
    pub user_sid: String,
    pub session_id: u32,
    pub worker_ready: bool,
    #[serde(default)]
    pub worker_update_pending: bool,
    pub locked: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsCommandSpec {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    pub stdin: Option<String>,
    #[serde(default)]
    pub elevated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsWorkloadSpec {
    pub instance_id: String,
    pub command: WindowsCommandSpec,
    #[serde(default)]
    pub interactive: bool,
    pub session_id: Option<u32>,
    #[serde(default)]
    pub restart_on_failure: bool,
    /// 显式选择隔离的受限服务身份；默认保留安装用户身份。
    #[serde(default)]
    pub service_owned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum WindowsWorkloadState {
    Starting,
    Running,
    Stopping,
    Stopped,
    WaitingForUserSession,
    Unknown,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsWorkloadStatus {
    pub instance_id: String,
    pub generation: String,
    pub pid: Option<u32>,
    pub session_id: Option<u32>,
    pub state: WindowsWorkloadState,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

impl WindowsWorkloadStatus {
    pub fn is_running(&self) -> bool {
        matches!(
            self.state,
            WindowsWorkloadState::Running | WindowsWorkloadState::Starting
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsAgentRequest {
    pub v: u32,
    pub request_id: String,
    pub action: WindowsAgentAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum WindowsAgentAction {
    Hello,
    Launch(WindowsWorkloadSpec),
    Status {
        instance_id: String,
    },
    Stop {
        instance_id: String,
        generation: String,
    },
    List,
    Tail {
        instance_id: String,
        #[ts(type = "number")]
        offset: u64,
        max_bytes: u32,
    },
    BotStart(crate::windows_bot::WindowsBotSpec),
    BotConfigure(crate::windows_bot::WindowsBotSpec),
    BotStop {
        bot_id: String,
    },
    BotStatus {
        bot_id: String,
    },
    BotQr {
        bot_id: String,
        refresh: bool,
    },
    BotCloseQr {
        bot_id: String,
    },
    BotTail {
        bot_id: String,
        lines: u32,
    },
    BotEvents {
        bot_id: String,
        #[ts(type = "number")]
        after: u64,
        #[serde(default)]
        generation: Option<String>,
    },
    BotEndpoint {
        bot_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsAgentResponse {
    pub v: u32,
    pub request_id: String,
    pub result: WindowsAgentResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum WindowsAgentResult {
    Hello(WindowsAgentStatus),
    Workload(WindowsWorkloadStatus),
    Workloads(Vec<WindowsWorkloadStatus>),
    Log(WindowsLogChunk),
    Error(WindowsAgentError),
    BotStatus(crate::BotStatus),
    BotLog(crate::LogSnapshot),
    QrLogin(crate::SnowLumaQrLoginSnapshot),
    BotEvents(crate::windows_bot::WindowsBotEventBatch),
    BotEndpoint(crate::windows_bot::WindowsBotEndpoint),
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsLogChunk {
    pub instance_id: String,
    pub generation: String,
    #[ts(type = "number")]
    pub offset: u64,
    #[ts(type = "number")]
    pub next_offset: u64,
    pub data: String,
    #[serde(default)]
    pub encoding_lossy: bool,
    pub eof: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WindowsAgentError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_session_defaults_to_no_pending_worker_update_and_round_trips_camel_case() {
        let legacy =
            r#"{"userSid":"S-1-5-21-owner","sessionId":1,"workerReady":true,"locked":false}"#;
        let mut session: WindowsSessionInfo = serde_json::from_str(legacy).unwrap();
        assert!(!session.worker_update_pending);
        session.worker_update_pending = true;
        let json = serde_json::to_value(&session).unwrap();
        assert_eq!(json["workerUpdatePending"], true);
        assert_eq!(
            serde_json::from_value::<WindowsSessionInfo>(json).unwrap(),
            session
        );
    }

    #[test]
    fn request_wire_uses_explicit_action_discriminator() {
        let request = WindowsAgentRequest {
            v: 1,
            request_id: "id".into(),
            action: WindowsAgentAction::Stop {
                instance_id: "bot-1".into(),
                generation: "g1".into(),
            },
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["action"]["kind"], "stop");
        let restored = serde_json::from_value::<WindowsAgentRequest>(json).unwrap();
        assert_eq!(restored.request_id, request.request_id);
    }
}
