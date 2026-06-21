use serde::{Deserialize, Serialize};

use crate::kinds::SchemaVersion;
use crate::models::{MigrationOutcome, MigrationStage};
use crate::report::MigrationReport;
use crate::version_snapshot::LocalVersionSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BootstrapStatus {
    #[default]
    Ready,
    Migrating,
    RepairRequired,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    OpenDataDir,
    ExportMigrationReport,
    RestoreBackup,
    Reauthenticate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapSnapshot {
    pub status: BootstrapStatus,
    pub schema_version: SchemaVersion,
    pub report: MigrationReport,
    /// 当前数据根绝对路径(已 to_string_lossy)装配方在
    /// src-tauri/src/bootstrap.rs::build_snapshot_for_data_root,由
    /// resolve_data_root() 决定来源#[serde(default)] 让历史快照缓存
    /// 缺失时反序列化回空字符串,向后兼容
    #[serde(default)]
    pub data_root: String,
    /// 本地 core 安装版本快照
    #[serde(default)]
    pub local_versions: LocalVersionSnapshot,
}

impl BootstrapSnapshot {
    pub fn new(
        status: BootstrapStatus,
        schema_version: SchemaVersion,
        report: MigrationReport,
    ) -> Self {
        Self {
            status,
            schema_version,
            report,
            data_root: String::new(),
            local_versions: LocalVersionSnapshot::default(),
        }
    }

    pub fn from_report(report: MigrationReport) -> Self {
        let status = match (report.stage, report.outcome) {
            (MigrationStage::Failed, _) => BootstrapStatus::Failed,
            (MigrationStage::RepairRequired, _) | (_, MigrationOutcome::NeedsRepair) => {
                BootstrapStatus::RepairRequired
            }
            (MigrationStage::Pending | MigrationStage::Running, _) => BootstrapStatus::Migrating,
            (MigrationStage::Completed, _) => BootstrapStatus::Ready,
        };

        Self::new(status, SchemaVersion::V3, report)
    }

    pub fn ready() -> Self {
        Self::from_report(MigrationReport::clean())
    }
}

impl From<MigrationReport> for BootstrapSnapshot {
    fn from(report: MigrationReport) -> Self {
        Self::from_report(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_clean_report_to_ready_snapshot() {
        let snapshot = BootstrapSnapshot::ready();

        assert_eq!(snapshot.status, BootstrapStatus::Ready);
        assert_eq!(snapshot.schema_version, SchemaVersion::V3);
        assert_eq!(snapshot.report, MigrationReport::clean());
    }

    #[test]
    fn maps_failed_report_to_failed_snapshot() {
        let snapshot = BootstrapSnapshot::from_report(MigrationReport::failed("boom"));

        assert_eq!(snapshot.status, BootstrapStatus::Failed);
    }

    /// 历史快照缓存(schema 演进前写入,没有 data_root / local_versions
    /// 两个字段)必须可以反序列化回 BootstrapSnapshot:data_root 回落为空
    /// 字符串,local_versions 回落为默认值
    #[test]
    fn legacy_snapshot_without_new_fields_deserializes() {
        let legacy_json = serde_json::json!({
            "status": "ready",
            "schema_version": 3,
            "report": MigrationReport::clean(),
        })
        .to_string();

        let decoded: BootstrapSnapshot =
            serde_json::from_str(&legacy_json).expect("legacy snapshot deserialize");

        assert_eq!(decoded.status, BootstrapStatus::Ready);
        assert!(decoded.data_root.is_empty());
        assert_eq!(decoded.local_versions, LocalVersionSnapshot::default());
    }
}
