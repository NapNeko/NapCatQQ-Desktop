//! 本机 CPU / 内存占用快照（概览性能监控 IPC 边界）。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 当前时刻的全局 CPU、内存占用百分比（0–100）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct SystemResourceSnapshot {
    #[serde(rename = "cpuPercent")]
    pub cpu_percent: f64,
    #[serde(rename = "ramPercent")]
    pub ram_percent: f64,
}