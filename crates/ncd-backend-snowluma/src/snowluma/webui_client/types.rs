//! SnowLuma WebUI 强类型 payload（跨边界 + 内部）

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// SnowLuma WebUI /api/processes 单条 PID 的 hook 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum HookProcessStatus {
    Available,
    Loading,
    Connecting,
    Loaded,
    Online,
    Error,
    Disconnected,
}

/// SnowLuma WebUI /api/processes 单条记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct HookProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: String,
    pub uin: String,
    pub status: HookProcessStatus,
    #[serde(default)]
    pub error: String,
}

/// SnowLuma WebUI /api/qq-list 单条记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct OneBotInstanceInfo {
    pub uin: String,
    pub nickname: String,
}

/// SnowLuma WebUI /api/processes/:pid/probe-login 返回的 QQ 端口探测结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QqPortLoginInfo {
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub uin: String,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default, rename = "nickName")]
    pub nickname: Option<String>,
    #[serde(default, rename = "loggedIn")]
    pub logged_in: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListProcessesResponse {
    #[serde(default)]
    pub list: Vec<HookProcessInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListQqInstancesResponse {
    #[serde(default)]
    pub list: Vec<OneBotInstanceInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProbeProcessLoginResponse {
    pub info: Option<QqPortLoginInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessActionResponse {
    pub success: bool,
    pub process: Option<HookProcessInfo>,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthState {
    #[serde(default, rename = "mustChangePassword")]
    pub must_change_password: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgreementDoc {
    pub id: String,
    pub title: String,
    pub declared_version: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgreementsPayload {
    pub version: String,
    #[serde(default)]
    pub consent_required: bool,
    #[serde(default)]
    pub documents: Vec<AgreementDoc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordConsentRequest {
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordConsentResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub message: String,
    #[serde(default, rename = "currentVersion")]
    pub current_version: Option<String>,
}
