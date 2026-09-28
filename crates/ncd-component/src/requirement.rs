//! 组件依赖的版本匹配
//!
//! 依赖边和解析结果的数据类型在 ncd-domain(要跨 IPC),这里原样转出,老路径不变;
//! 匹配版本号要 semver,ncd-domain 不带这个依赖,所以这部分留在组件层。

use semver::Version;

pub use ncd_domain::component::{
    DependencyNode, DependencyPlan, DependencyTarget, HostPackageGroup, Requirement,
    RequirementPhase, RequirementStatus, RuntimeReadiness, VersionReq,
};

/// 按 node-semver 的口径拿版本号去比 VersionReq
pub trait VersionMatch {
    /// `raw` 可带 v 前缀,可缺 minor / patch(Node 实际输出总是三段)
    fn matches(&self, raw: &str) -> bool;
}

impl VersionMatch for VersionReq {
    fn matches(&self, raw: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Semver { range } => {
                let Some(version) = parse_loose_version(raw) else {
                    return false;
                };
                range.split("||").any(|part| {
                    semver::VersionReq::parse(part.trim())
                        .map(|req| req.matches(&version))
                        .unwrap_or(false)
                })
            }
        }
    }
}

/// 多个消费方的约束同时满足才算可用
pub fn all_versions_match(reqs: &[VersionReq], raw: &str) -> bool {
    reqs.iter().all(|r| r.matches(raw))
}

fn parse_loose_version(raw: &str) -> Option<Version> {
    let core = raw.trim().trim_start_matches(['v', 'V']);
    let mut parts = core.split('.').map(str::trim);
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().map_or(Some(0), |s| s.parse::<u64>().ok())?;
    let patch = parts.next().map_or(Some(0), |s| {
        // 允许 "22.13.0-rc1" 这种尾巴,只取数字前缀
        let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u64>().ok()
    })?;
    Some(Version::new(major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_range_with_or_matches_like_node_semver() {
        let req = VersionReq::semver("^22.13.0 || >=23.4.0");
        for ok in ["22.13.0", "v22.14.1", "23.4.0", "24.0.0", "22.13", "v24"] {
            assert!(req.matches(ok), "{ok} should match");
        }
        for bad in ["22.12.0", "18.19.1", "23.3.0", "", "abc"] {
            assert!(!req.matches(bad), "{bad} should not match");
        }
    }

    #[test]
    fn any_matches_everything_and_all_match_requires_every_req() {
        assert!(VersionReq::Any.matches("garbage"));
        let reqs = [VersionReq::semver(">=20"), VersionReq::semver("<23")];
        assert!(all_versions_match(&reqs, "22.0.0"));
        assert!(!all_versions_match(&reqs, "23.0.0"));
        assert!(all_versions_match(&[], "anything"));
    }
}
