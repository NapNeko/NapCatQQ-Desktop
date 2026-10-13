//! Docker 管理页使用的详情、采样和资源契约。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerContainerDetails {
    pub id: String,
    pub name: String,
    pub image: String,
    pub created: String,
    pub state: super::ContainerState,
    pub started_at: String,
    pub finished_at: String,
    pub exit_code: i32,
    pub oom_killed: bool,
    pub error: String,
    pub restart_count: u32,
    pub restart_policy: String,
    pub health: Option<String>,
    pub network_mode: String,
    pub networks: Vec<String>,
    pub ports: Vec<String>,
    pub mounts: Vec<DockerMountInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerMountInfo {
    pub kind: String,
    pub source: String,
    pub destination: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerContainerStats {
    pub cpu_percent: String,
    pub memory_usage: String,
    pub memory_percent: String,
    pub network_io: String,
    pub block_io: String,
    pub pids: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerNetworkInfo {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerVolumeInfo {
    pub name: String,
    pub driver: String,
    pub scope: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DockerResourceKind {
    Network,
    Volume,
}

impl DockerResourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Volume => "volume",
        }
    }
}
