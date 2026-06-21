//! Deploy 错误类型

use ncd_component::ActionError;

#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    /// Component 操作错误透传
    #[error("action error: {0}")]
    Action(#[from] ActionError),

    /// Plan 配置错误(空 plan / 重复 step / 依赖循环)
    #[error("invalid plan: {reason}")]
    InvalidPlan { reason: String },

    /// 某步失败且 rollback 也失败(双重故障)
    #[error("step '{step}' failed and rollback also failed: orig={orig}; rollback={rollback}")]
    RollbackFailed {
        step: String,
        orig: String,
        rollback: String,
    },

    /// 用户主动取消
    #[error("deploy cancelled")]
    Cancelled,
}

impl DeployError {
    pub fn invalid_plan(reason: impl Into<String>) -> Self {
        Self::InvalidPlan {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_plan_helper() {
        let err = DeployError::invalid_plan("empty step list");
        assert!(err.to_string().contains("empty step list"));
    }

    #[test]
    fn action_error_auto_converts() {
        let action_err = ActionError::other("oops");
        let deploy_err: DeployError = action_err.into();
        assert!(matches!(deploy_err, DeployError::Action(_)));
    }

    #[test]
    fn rollback_failed_carries_both_messages() {
        let err = DeployError::RollbackFailed {
            step: "install_napcat".into(),
            orig: "download timeout".into(),
            rollback: "rm -rf failed".into(),
        };
        let text = err.to_string();
        assert!(text.contains("install_napcat"));
        assert!(text.contains("download timeout"));
        assert!(text.contains("rm -rf failed"));
    }
}
