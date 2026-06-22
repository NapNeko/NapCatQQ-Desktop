// ProgressEvent / ProgressKind / ProgressLogLevel: 进度上报数据类型
//
// 纯 serde + ts-rs,零运行时依赖。ActionCtx (tokio mpsc) 保留在 ncd-component。

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::DockerPullLayerSnapshot;

/// 进度事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ProgressKind {
    Started { total_steps: u32 },
    StepBegin { step: u32, message: String },
    StepProgress {
        step: u32,
        percent: u8,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speed_bps: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        downloaded_bytes: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_bytes: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        download_stage: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        docker_layers: Option<Vec<DockerPullLayerSnapshot>>,
    },
    StepEnd { step: u32, ok: bool },
    Finished { ok: bool },
    Log {
        level: ProgressLogLevel,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ProgressLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ProgressEvent {
    #[serde(default = "default_version")]
    pub v: u32,
    pub timestamp_ms: u64,
    #[serde(flatten)]
    pub kind: ProgressKind,
}

fn default_version() -> u32 {
    1
}

impl ProgressEvent {
    pub fn new(kind: ProgressKind) -> Self {
        Self {
            v: 1,
            timestamp_ms: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            kind,
        }
    }
}
