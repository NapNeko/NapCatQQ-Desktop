//! Deploy 执行结果数据

use serde::{Deserialize, Serialize};

use crate::plan::StepKind;

/// 单步执行结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepOutcome {
    pub name: String,
    pub kind: String, // StepKind::as_str() 字符串(序列化稳定)
    pub status: StepStatus,
    /// 执行耗时(毫秒)
    pub duration_ms: u64,
    /// 失败时的错误描述(成功为 None)
    pub error: Option<String>,
    /// 是否被跳过(EnsureInstalled 探测到已装时跳过 install)
    pub skipped: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Ok,
    Failed,
    Skipped,
    Cancelled,
}

impl StepOutcome {
    pub fn ok(name: impl Into<String>, kind: StepKind, duration_ms: u64, skipped: bool) -> Self {
        Self {
            name: name.into(),
            kind: kind.as_str().to_string(),
            status: if skipped {
                StepStatus::Skipped
            } else {
                StepStatus::Ok
            },
            duration_ms,
            error: None,
            skipped,
        }
    }

    pub fn failed(
        name: impl Into<String>,
        kind: StepKind,
        duration_ms: u64,
        error: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            kind: kind.as_str().to_string(),
            status: StepStatus::Failed,
            duration_ms,
            error: Some(error.into()),
            skipped: false,
        }
    }

    pub fn cancelled(name: impl Into<String>, kind: StepKind, duration_ms: u64) -> Self {
        Self {
            name: name.into(),
            kind: kind.as_str().to_string(),
            status: StepStatus::Cancelled,
            duration_ms,
            error: None,
            skipped: false,
        }
    }
}

/// 整个 plan 的执行结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeployOutcome {
    pub ok: bool,
    pub steps: Vec<StepOutcome>,
    /// 总耗时(毫秒)
    pub total_duration_ms: u64,
}

impl DeployOutcome {
    pub fn new(steps: Vec<StepOutcome>, total_duration_ms: u64) -> Self {
        let ok = steps.iter().all(|s| {
            matches!(s.status, StepStatus::Ok | StepStatus::Skipped)
        });
        Self {
            ok,
            steps,
            total_duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_outcome_marks_success() {
        let s = StepOutcome::ok("nodejs", StepKind::EnsureInstalled, 100, false);
        assert_eq!(s.status, StepStatus::Ok);
        assert!(!s.skipped);
        assert!(s.error.is_none());
    }

    #[test]
    fn skipped_outcome_uses_skipped_status() {
        let s = StepOutcome::ok("nodejs", StepKind::EnsureInstalled, 5, true);
        assert_eq!(s.status, StepStatus::Skipped);
        assert!(s.skipped);
    }

    #[test]
    fn failed_outcome_carries_error() {
        let s = StepOutcome::failed(
            "qq",
            StepKind::ForceInstall,
            500,
            "download failed",
        );
        assert_eq!(s.status, StepStatus::Failed);
        assert_eq!(s.error.as_deref(), Some("download failed"));
    }

    #[test]
    fn outcome_ok_when_all_ok_or_skipped() {
        let outcome = DeployOutcome::new(
            vec![
                StepOutcome::ok("a", StepKind::EnsureInstalled, 10, false),
                StepOutcome::ok("b", StepKind::EnsureInstalled, 20, true),
            ],
            30,
        );
        assert!(outcome.ok);
    }

    #[test]
    fn outcome_not_ok_when_any_failed() {
        let outcome = DeployOutcome::new(
            vec![
                StepOutcome::ok("a", StepKind::EnsureInstalled, 10, false),
                StepOutcome::failed("b", StepKind::ForceInstall, 100, "boom"),
            ],
            110,
        );
        assert!(!outcome.ok);
    }

    #[test]
    fn outcome_serializes_with_snake_case_status() {
        let outcome = DeployOutcome::new(
            vec![StepOutcome::ok("a", StepKind::EnsureInstalled, 10, false)],
            10,
        );
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"kind\":\"ensure_installed\""));
    }
}
