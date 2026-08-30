use serde::{Deserialize, Serialize};
use ts_rs::TS;


#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct QrLoginSessionId(String);

impl QrLoginSessionId {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.trim().is_empty()).then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum SnowlumaQrLoginStatus {
    Preparing,
    WaitingForScan,
    Succeeded,
    FallbackNoVnc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum SnowlumaQrFailureCategory {
    UnsupportedVariant,
    CapabilityUnavailable,
    AmbiguousBinding,
    CaptureFailed,
    DecoderUnavailable,
    DecodeFailed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct SnowlumaQrLoginSession {
    pub server_id: String,
    pub bot_id: String,
    pub session_id: QrLoginSessionId,
    pub capture_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum SnowlumaQrLoginResult {
    Payload {
        session: SnowlumaQrLoginSession,
        payload: String,
    },
    FallbackNoVnc {
        session: SnowlumaQrLoginSession,
        reason: SnowlumaQrFailureCategory,
    },
}
