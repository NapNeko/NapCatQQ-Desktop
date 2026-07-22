use chrono::{DateTime, Utc};
use ncd_domain::SchemaVersion;
use semver::Version;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// IPC envelope 版本, 跨 update 类型共享, 满足 R14 契约
pub const UPDATE_PROTOCOL_VERSION: u32 = 1;

pub(crate) fn default_v() -> u32 {
    UPDATE_PROTOCOL_VERSION
}

/// 启动时消费一次的自更新结果提示（resume / 失败日志）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/update/")]
pub struct DesktopUpdateStartupNotice {
    #[serde(default = "default_v")]
    pub v: u32,
    pub kind: DesktopUpdateNoticeKind,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub message: String,
}

/// 启动提示分类（wire: snake_case，与前端 InfoBar 分支对齐）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/update/")]
pub enum DesktopUpdateNoticeKind {
    Success,
    Incomplete,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/update/")]
pub struct AvailableUpdate {
    /// R14 IPC envelope 版本号
    #[serde(default = "default_v")]
    pub v: u32,
    #[ts(type = "string")]
    pub version: Version,
    /// 新版要求的最低数据 schema, precheck 用这个比较
    #[ts(type = "number")]
    pub schema_version: SchemaVersion,
    /// 发行说明(Markdown)
    pub notes: String,
    #[ts(type = "string")]
    pub pub_date: DateTime<Utc>,
    /// 安装包下载 URL(GitHub MSI 或 updater 资产)
    pub download_url: String,
    /// base64 Ed25519 签名; 仅签名包路径使用, MSI 路径恒为空串
    #[serde(default)]
    pub signature: String,
    /// 下载内容期望 SHA256(64-hex 小写); 无 digest 时为空串。MSI 路径用此字段校验。
    #[serde(default)]
    pub content_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/update/")]
pub struct PrecheckReport {
    /// R14 IPC envelope 版本号
    #[serde(default = "default_v")]
    pub v: u32,
    pub can_upgrade: bool,
    /// 阻塞性问题(schema 跨度太大 / 不兼容字段等)
    pub blocking: Vec<String>,
    /// 非阻塞警告(某些字段会被丢弃 / 行为变化)
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

/// 更新流水线的失败阶段
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    Check,
    Precheck,
    Shutdown,
    Install,
    Resume,
}

impl std::fmt::Display for UpdatePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Check => f.write_str("check"),
            Self::Precheck => f.write_str("precheck"),
            Self::Shutdown => f.write_str("shutdown"),
            Self::Install => f.write_str("install"),
            Self::Resume => f.write_str("resume"),
        }
    }
}

/// 失败 telemetry 记录, 追加到 data_root/update-failures.jsonl
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedFailure {
    pub timestamp: DateTime<Utc>,
    /// 可能在拿到版本号之前就失败了
    pub target_version: Option<String>,
    pub phase: UpdatePhase,
    pub error: String,
}

impl RecordedFailure {
    pub fn new(phase: UpdatePhase, error: impl Into<String>) -> Self {
        Self {
            timestamp: Utc::now(),
            target_version: None,
            phase,
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
            signature: String::new(),
            content_sha256: "abc".into(),
        };
        let json = serde_json::to_string(&u).unwrap();
        assert!(json.contains("\"v\":1"));
        assert!(json.contains("0.2.0"));
    }

    #[test]
    fn recorded_failure_with_phase() {
        let f = RecordedFailure::new(UpdatePhase::Install, "msi exit 1603");
        assert_eq!(f.phase, UpdatePhase::Install);
        assert_eq!(f.error, "msi exit 1603");
        assert!(f.target_version.is_none());
    }

    #[test]
    fn recorded_failure_with_target_version_chain() {
        let f = RecordedFailure::new(UpdatePhase::Check, "404").with_target_version("1.2.3");
        assert_eq!(f.target_version.as_deref(), Some("1.2.3"));
    }
}
