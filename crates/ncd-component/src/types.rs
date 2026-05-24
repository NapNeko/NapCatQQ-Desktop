//! Component / Action 共享数据类型。

use serde::{Deserialize, Serialize};

/// Component 标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentId {
    NapCat,
    SnowLumaFramework,
    LinuxQq,
    NodeJs,
    NoVnc,
    DesktopSelf,
}

impl ComponentId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NapCat => "napcat",
            Self::SnowLumaFramework => "snowluma_framework",
            Self::LinuxQq => "linuxqq",
            Self::NodeJs => "nodejs",
            Self::NoVnc => "novnc",
            Self::DesktopSelf => "desktop_self",
        }
    }
}

/// 探测结果(Component::detect 返回值)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedVersion {
    /// 探测到的版本号(如 "v20.10.0" / "3.2.25-45758")
    pub version: String,
    /// 探测来源(如 "package.json" / "qq --version" / "node -v")
    pub source: String,
}

/// 校验报告(Component::verify 返回值)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub ok: bool,
    /// 已验证项(如 "binary exists" / "sha256 matches" / "manifest version")
    pub checks: Vec<VerifyCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyCheck {
    pub name: String,
    pub passed: bool,
    pub detail: Option<String>,
}

impl VerifyReport {
    pub fn ok() -> Self {
        Self {
            ok: true,
            checks: Vec::new(),
        }
    }

    pub fn with_check(mut self, name: impl Into<String>, passed: bool, detail: Option<String>) -> Self {
        self.checks.push(VerifyCheck {
            name: name.into(),
            passed,
            detail,
        });
        if !passed {
            self.ok = false;
        }
        self
    }
}

/// 启动参数(Component::launch_command 入参)。
#[derive(Debug, Clone, Default)]
pub struct LaunchArgs {
    /// 额外环境变量
    pub extra_env: Vec<(String, String)>,
    /// 额外命令行参数
    pub extra_args: Vec<String>,
    /// 工作目录(若 None,由 Component 决定默认值)
    pub working_dir: Option<ncd_host::HostPath>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_id_as_str_matches_snake_case() {
        assert_eq!(ComponentId::NapCat.as_str(), "napcat");
        assert_eq!(ComponentId::SnowLumaFramework.as_str(), "snowluma_framework");
        assert_eq!(ComponentId::LinuxQq.as_str(), "linuxqq");
    }

    #[test]
    fn component_id_serializes_snake_case() {
        let s = serde_json::to_string(&ComponentId::DesktopSelf).unwrap();
        assert_eq!(s, "\"desktop_self\"");
    }

    #[test]
    fn verify_report_ok_starts_empty() {
        let r = VerifyReport::ok();
        assert!(r.ok);
        assert!(r.checks.is_empty());
    }

    #[test]
    fn verify_report_failed_check_flips_ok() {
        let r = VerifyReport::ok()
            .with_check("a", true, None)
            .with_check("b", false, Some("missing".into()));
        assert!(!r.ok);
        assert_eq!(r.checks.len(), 2);
    }
}
