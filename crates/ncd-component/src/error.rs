//! ActionError:Component / Action 操作的统一错误
//!
//! 设计原则:
//! - 所有 Component 操作的错误都映射到本枚举,避免每个 component 一套自己的 error
//! - 与 [ncd_host::HostError] 正交:Host 错误是"机器无法完成操作",
//!   ActionError 是"业务流程出错"(版本不匹配 / SHA256 不一致 / 部署步骤失败 / ...)

use ncd_host::HostError;

#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    /// 主机层错误(Host trait 抛出的,直接透传)
    #[error("host error: {0}")]
    Host(#[from] HostError),

    /// 网络下载失败(HTTP 状态码异常 / 连接断开)
    #[error("download failed: {url}: {reason}")]
    DownloadFailed { url: String, reason: String },

    /// SHA256 校验不通过
    #[error("checksum mismatch: expected={expected} actual={actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// 探测失败(无法判断版本号 / 关键文件缺失)
    #[error("detect failed: {component}: {reason}")]
    DetectFailed { component: String, reason: String },

    /// 安装步骤失败(具体步骤的语义错误)
    #[error("install step failed: {step}: {reason}")]
    InstallStepFailed { step: String, reason: String },

    /// 当前 Host 不支持该 Component(如 LinuxQQ 不能装 Windows)
    #[error("component {component} not supported on {os:?} {locality:?}")]
    UnsupportedTarget {
        component: String,
        os: ncd_host::Os,
        locality: ncd_host::Locality,
    },

    /// 用户主动取消
    #[error("operation cancelled by user")]
    Cancelled,

    /// 配置错误(URL 无效 / 期望 SHA256 缺失等)
    #[error("invalid configuration: {reason}")]
    InvalidConfig { reason: String },

    /// 其他不便归类的语义错误
    #[error("{0}")]
    Other(String),
}

impl ActionError {
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }

    pub fn install_step(step: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InstallStepFailed {
            step: step.into(),
            reason: reason.into(),
        }
    }

    pub fn detect_failed(component: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::DetectFailed {
            component: component.into(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_mismatch_renders_both_hashes() {
        let err = ActionError::ChecksumMismatch {
            expected: "abc123".into(),
            actual: "def456".into(),
        };
        let text = err.to_string();
        assert!(text.contains("abc123"));
        assert!(text.contains("def456"));
    }

    #[test]
    fn host_error_auto_converts() {
        let host_err = HostError::Unsupported { operation: "x" };
        let action_err: ActionError = host_err.into();
        assert!(matches!(action_err, ActionError::Host(_)));
    }
}
