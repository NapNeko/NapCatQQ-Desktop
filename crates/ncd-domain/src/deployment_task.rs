//! Deployment task queue data contract.
//!
//! Runtime scheduling lives in `ncd-runtime`; this module only defines the
//! serialized task snapshot shared with Tauri and the frontend.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::docker::DockerFlavor;
use crate::progress::ProgressEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DeploymentTaskKind {
    ComponentAction {
        component_id: String,
        action: String,
    },
    SystemPackage {
        package_group: String,
    },
    DockerInstall,
    DockerImagePull {
        flavor: DockerFlavor,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DeploymentTaskStatus {
    Queued,
    Running,
    WaitingInput,
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DeploymentTaskResource {
    PackageManager {
        host_id: String,
    },
    InstallTarget {
        host_id: String,
        target: String,
    },
    DockerCapability {
        host_id: String,
    },
    DockerDaemon {
        host_id: String,
    },
    DockerImage {
        host_id: String,
        flavor: DockerFlavor,
    },
    GlobalDownloadSlot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DeploymentTaskSnapshot {
    pub task_id: String,
    pub kind: DeploymentTaskKind,
    pub status: DeploymentTaskStatus,
    pub host_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub resources: Vec<DeploymentTaskResource>,
    #[serde(default)]
    pub progress_events: Vec<ProgressEvent>,
    pub submitted_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub cancellable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DeploymentTaskList {
    pub tasks: Vec<DeploymentTaskSnapshot>,
}

impl DeploymentTaskStatus {
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::WaitingInput)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Cancelled)
    }
}
