//! 数据根整树迁移(换盘)的跨边界类型。
//!
//! 与 layout consolidate 不同:这里是换权威 data_root 路径,不是同根洗布局。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 预检展示用的源树顶层条目(不递归全盘,避免 UI 爆炸)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DataRootTreeEntry {
    pub name: String,
    /// dir | file | skip
    pub kind: String,
    /// 目录/文件体积;skip 可为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// 如「不复制(可重建)」
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 迁移预检结果(不写盘、不改指针)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DataRootMigratePreview {
    pub source_root: String,
    pub target_root: String,
    /// 预估将复制的字节数(跳过可重建 tmp 内容时与 execute 规则一致)
    pub bytes_estimate: u64,
    /// 本机 active Bot 数量(>0 时 start 会先尝试停止)
    pub local_active_bots: u32,
    /// 源根顶层结构预览(将复制/跳过)
    #[serde(default)]
    pub tree_entries: Vec<DataRootTreeEntry>,
    /// 硬挡原因;非空则 ok=false
    #[serde(default)]
    pub blocking_reasons: Vec<String>,
    /// 软提示(体积大、将重启等)
    #[serde(default)]
    pub warnings: Vec<String>,
    pub ok: bool,
}

/// 迁移阶段(进度事件 / UI)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DataRootMigratePhase {
    Freezing,
    Copying,
    Verifying,
    Promoting,
    RewritingPaths,
    WritingPointer,
    Done,
    Failed,
    Cancelled,
}

/// 迁移进度(可经 DomainEvent 或专用 tauri 事件推送)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DataRootMigrateProgress {
    pub phase: DataRootMigratePhase,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// 相对路径提示;勿含密钥内容
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_rel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 迁移成功结果(指针已写;调用方应重启进程)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DataRootMigrateResult {
    pub old_root: String,
    pub new_root: String,
    /// 旧根 retired marker 路径
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_marker_path: Option<String>,
    pub restart_required: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// 旧根 retired marker 文件内容(JSON)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataRootRetiredMarker {
    pub v: u32,
    pub retired_at: String,
    pub moved_to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,
}

impl DataRootRetiredMarker {
    pub const CURRENT_V: u32 = 1;
    pub const FILE_NAME: &'static str = ".ncd-data-root-retired.json";
}
