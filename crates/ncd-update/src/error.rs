//! Update 错误类型

use ncd_domain::SchemaVersion;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// 检查更新失败(网络 / 签名验证 / 解析)
    #[error("check failed: {reason}")]
    CheckFailed { reason: String },

    /// 没有可用更新
    #[error("no update available")]
    NoUpdateAvailable,

    /// schema 兼容预检不通过
    #[error("precheck blocked: {reason}")]
    PrecheckBlocked { reason: String },

    /// schema 跨度太大,需要中间版本过渡
    #[error("schema gap too large: current={current:?} target={target:?}")]
    SchemaGapTooLarge {
        current: SchemaVersion,
        target: SchemaVersion,
    },

    /// 下载或安装失败
    #[error("install failed: {reason}")]
    InstallFailed { reason: String },

    /// 签名验证失败
    #[error("signature verification failed: {reason}")]
    SignatureFailed { reason: String },

    /// resume snapshot 写入或读取失败
    #[error("resume snapshot error: {reason}")]
    ResumeError { reason: String },

    /// IO 错误透传
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 解析错误
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// 用户取消
    #[error("update cancelled")]
    Cancelled,

    /// 内部状态错误(不应该到达)
    #[error("internal error: {0}")]
    Internal(String),
}

impl UpdateError {
    pub fn check_failed(reason: impl Into<String>) -> Self {
        Self::CheckFailed {
            reason: reason.into(),
        }
    }

    pub fn install_failed(reason: impl Into<String>) -> Self {
        Self::InstallFailed {
            reason: reason.into(),
        }
    }

    pub fn precheck_blocked(reason: impl Into<String>) -> Self {
        Self::PrecheckBlocked {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_failed_helper() {
        let e = UpdateError::check_failed("network down");
        assert!(e.to_string().contains("network down"));
    }

    #[test]
    fn no_update_available_renders_clean() {
        let e = UpdateError::NoUpdateAvailable;
        assert_eq!(e.to_string(), "no update available");
    }

    #[test]
    fn schema_gap_renders_versions() {
        let e = UpdateError::SchemaGapTooLarge {
            current: SchemaVersion::V1,
            target: SchemaVersion::V3,
        };
        let text = e.to_string();
        assert!(text.contains("V1") || text.contains("v1") || text.contains("1"));
        assert!(text.contains("V3") || text.contains("v3") || text.contains("3"));
    }
}
