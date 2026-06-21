//! Update 共享数据类型

use chrono::{DateTime, Utc};
use ncd_domain::SchemaVersion;
use semver::Version;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 一次可用的更新信息(由 [UpdateProvider::check] 返回)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/update/")]
pub struct AvailableUpdate {
    /// 协议版本(envelope,与前端契约同步)
    #[serde(default = "default_v")]
    pub v: u32,
    /// 新版本号(SemVer)
    #[ts(type = "string")]
    pub version: Version,
    /// 新版要求的最低数据 schema(用于预检)
    #[ts(type = "number")]
    pub schema_version: SchemaVersion,
    /// 发行说明(Markdown)
    pub notes: String,
    /// 发布时间
    #[ts(type = "string")]
    pub pub_date: DateTime<Utc>,
    /// 下载 URL(交给 tauri-plugin-updater 处理签名验证 + 下载)
    pub download_url: String,
    /// 签名(base64),由 tauri-plugin-updater 自动验证 Ed25519
    pub signature: String,
}

fn default_v() -> u32 {
    1
}

/// schema 兼容预检报告
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/update/")]
pub struct PrecheckReport {
    /// 协议版本(envelope,与前端契约同步)
    #[serde(default = "default_v")]
    pub v: u32,
    pub can_upgrade: bool,
    /// 阻塞性问题(数据 schema 跨度太大 / 不兼容字段等)
    pub blocking: Vec<String>,
    /// 警告(某些字段会被丢弃 / 行为变化)
    pub warnings: Vec<String>,
    /// 估算迁移耗时(毫秒)
    pub estimated_migration_time_ms: u64,
}

impl PrecheckReport {
    pub fn ok() -> Self {
        Self {
            v: 1,
            can_upgrade: true,
            blocking: Vec::new(),
            warnings: Vec::new(),
            estimated_migration_time_ms: 0,
        }
    }

    pub fn blocked(reason: impl Into<String>) -> Self {
        Self {
            v: 1,
            can_upgrade: false,
            blocking: vec![reason.into()],
            warnings: Vec::new(),
            estimated_migration_time_ms: 0,
        }
    }

    pub fn add_warning(mut self, msg: impl Into<String>) -> Self {
        self.warnings.push(msg.into());
        self
    }

    pub fn add_blocking(mut self, msg: impl Into<String>) -> Self {
        self.blocking.push(msg.into());
        self.can_upgrade = false;
        self
    }
}

/// 更新失败 telemetry 记录(写到 data_root/update-failures.jsonl)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedFailure {
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 失败的更新版本(SemVer 字符串,失败可能在拿到版本前)
    pub target_version: Option<String>,
    /// 失败阶段(check / precheck / shutdown / install / resume)
    pub phase: String,
    /// 错误描述
    pub error: String,
}

impl RecordedFailure {
    pub fn new(phase: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            target_version: None,
            phase: phase.into(),
            error: error.into(),
        }
    }

    pub fn with_target_version(mut self, version: impl Into<String>) -> Self {
        self.target_version = Some(version.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precheck_ok_starts_clean() {
        let r = PrecheckReport::ok();
        assert!(r.can_upgrade);
        assert!(r.blocking.is_empty());
        assert!(r.warnings.is_empty());
        assert_eq!(r.v, 1);
    }

    #[test]
    fn precheck_blocked_carries_reason() {
        let r = PrecheckReport::blocked("schema too old");
        assert!(!r.can_upgrade);
        assert_eq!(r.blocking, vec!["schema too old"]);
    }

    #[test]
    fn add_blocking_flips_can_upgrade() {
        let r = PrecheckReport::ok().add_blocking("config breaking change");
        assert!(!r.can_upgrade);
        assert_eq!(r.blocking.len(), 1);
    }

    #[test]
    fn add_warning_keeps_can_upgrade_true() {
        let r = PrecheckReport::ok().add_warning("field X deprecated");
        assert!(r.can_upgrade);
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn precheck_serializes_with_v_field() {
        let r = PrecheckReport::ok();
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"v\":1"));
    }

    #[test]
    fn available_update_serializes_with_v_field() {
        let u = AvailableUpdate {
            v: 1,
            version: Version::new(0, 2, 0),
            schema_version: SchemaVersion::V3,
            notes: "test".into(),
            pub_date: Utc::now(),
            download_url: "https://example.com/update.msi".into(),
            signature: "fake".into(),
        };
        let json = serde_json::to_string(&u).unwrap();
        assert!(json.contains("\"v\":1"));
        assert!(json.contains("0.2.0"));
    }

    #[test]
    fn recorded_failure_with_phase() {
        let f = RecordedFailure::new("install", "msi exit 1603");
        assert_eq!(f.phase, "install");
        assert_eq!(f.error, "msi exit 1603");
        assert!(f.target_version.is_none());
    }

    #[test]
    fn recorded_failure_with_target_version_chain() {
        let f = RecordedFailure::new("check", "404")
            .with_target_version("1.2.3");
        assert_eq!(f.target_version.as_deref(), Some("1.2.3"));
    }
}
