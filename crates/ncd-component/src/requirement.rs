//! 组件依赖声明与解析结果的数据类型
//!
//! 依赖图只在各 Component::requirements() 里声明一次;这里只放边和节点的形状,
//! 递归 / 探测 / 排任务在 ncd-runtime。被依赖方(如 Node)不知道谁在用它,
//! 版本约束由消费方写在自己的边上。

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::types::ComponentId;

/// 依赖在哪个阶段生效
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum RequirementPhase {
    /// 只在安装 / 更新本组件时需要(解压工具一类)
    Install,
    /// 只在运行时需要
    Run,
    /// 装和跑都要
    Both,
}

impl RequirementPhase {
    /// 本条边在 `query`(Install 或 Run)阶段是否生效;`query` 传 Both 表示不过滤
    pub fn applies_to(self, query: RequirementPhase) -> bool {
        matches!(
            (self, query),
            (RequirementPhase::Both, _)
                | (_, RequirementPhase::Both)
                | (RequirementPhase::Install, RequirementPhase::Install)
                | (RequirementPhase::Run, RequirementPhase::Run)
        )
    }
}

/// 消费方对依赖组件的版本约束
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum VersionReq {
    Any,
    /// node-semver 写法,`||` 分隔多段任一满足即可;每段交给 semver crate
    Semver { range: String },
}

impl VersionReq {
    pub fn semver(range: impl Into<String>) -> Self {
        Self::Semver {
            range: range.into(),
        }
    }

    pub fn is_any(&self) -> bool {
        matches!(self, Self::Any)
    }

    /// `raw` 可带 v 前缀,可缺 minor / patch(Node 实际输出总是三段)
    pub fn matches(&self, raw: &str) -> bool {
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

    /// 多个消费方的约束同时满足才算可用
    pub fn all_match(reqs: &[VersionReq], raw: &str) -> bool {
        reqs.iter().all(|r| r.matches(raw))
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => f.write_str("任意版本"),
            Self::Semver { range } => f.write_str(range),
        }
    }
}

fn parse_loose_version(raw: &str) -> Option<semver::Version> {
    let core = raw.trim().trim_start_matches(['v', 'V']);
    let mut parts = core.split('.').map(str::trim);
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().map_or(Some(0), |s| s.parse::<u64>().ok())?;
    let patch = parts.next().map_or(Some(0), |s| {
        // 允许 "22.13.0-rc1" 这种尾巴,只取数字前缀
        let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u64>().ok()
    })?;
    Some(semver::Version::new(major, minor, patch))
}

/// 主机级系统包组(不是 Component,由包管理器整组安装)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum HostPackageGroup {
    /// Linux QQ 运行所需动态库
    QqDependencies,
}

impl HostPackageGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::QqDependencies => "qq_dependencies",
        }
    }
}

/// 一条依赖边:某组件在某 (os, locality) 下需要什么
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum Requirement {
    /// 依赖另一个组件;版本约束由消费方声明
    Component {
        id: ComponentId,
        version: VersionReq,
        phase: RequirementPhase,
    },
    /// 主机 PATH 上要有某个命令;缺时用包管理器装 `package`。只在安装期生效
    HostCommand { command: String, package: String },
    /// 主机上要有一组系统包。装和跑都要
    HostPackages { group: HostPackageGroup },
}

impl Requirement {
    pub fn component(id: ComponentId) -> Self {
        Self::Component {
            id,
            version: VersionReq::Any,
            phase: RequirementPhase::Both,
        }
    }

    pub fn component_version(id: ComponentId, range: impl Into<String>) -> Self {
        Self::Component {
            id,
            version: VersionReq::semver(range),
            phase: RequirementPhase::Both,
        }
    }

    pub fn host_command(command: impl Into<String>, package: impl Into<String>) -> Self {
        Self::HostCommand {
            command: command.into(),
            package: package.into(),
        }
    }

    pub fn host_packages(group: HostPackageGroup) -> Self {
        Self::HostPackages { group }
    }

    pub fn phase(&self) -> RequirementPhase {
        match self {
            Self::Component { phase, .. } => *phase,
            Self::HostCommand { .. } => RequirementPhase::Install,
            Self::HostPackages { .. } => RequirementPhase::Both,
        }
    }

    pub fn applies_to(&self, query: RequirementPhase) -> bool {
        self.phase().applies_to(query)
    }

    /// 依赖目标的身份;同一目标被多个消费方要到时按此合并
    pub fn target(&self) -> DependencyTarget {
        match self {
            Self::Component { id, .. } => DependencyTarget::Component { id: *id },
            Self::HostCommand { command, package } => DependencyTarget::HostCommand {
                command: command.clone(),
                package: package.clone(),
            },
            Self::HostPackages { group } => DependencyTarget::HostPackages { group: *group },
        }
    }

    pub fn component_id(&self) -> Option<ComponentId> {
        match self {
            Self::Component { id, .. } => Some(*id),
            _ => None,
        }
    }

    pub fn version_req(&self) -> Option<&VersionReq> {
        match self {
            Self::Component { version, .. } if !version.is_any() => Some(version),
            _ => None,
        }
    }
}

/// 解析后的依赖目标(边去掉版本 / 阶段后的身份)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DependencyTarget {
    Component { id: ComponentId },
    HostCommand { command: String, package: String },
    HostPackages { group: HostPackageGroup },
}

impl DependencyTarget {
    pub fn component_id(&self) -> Option<ComponentId> {
        match self {
            Self::Component { id } => Some(*id),
            _ => None,
        }
    }

    /// 给 UI / 日志看的短名
    pub fn label(&self) -> String {
        match self {
            Self::Component { id } => id.as_str().to_string(),
            Self::HostCommand { command, .. } => command.clone(),
            Self::HostPackages { group } => group.as_str().to_string(),
        }
    }
}

/// 某个依赖目标在某台主机上的状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum RequirementStatus {
    Satisfied {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    /// 完全没装
    Missing,
    /// 在,但版本不符 / 跑不起来 / 系统包只装了一部分
    Unsatisfied {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        found: Option<String>,
        reason: String,
    },
    /// 这台主机装不了这个目标
    Unsupported,
    /// 探测失败(连接抖动等),不知道装没装
    Unknown { error: String },
}

impl RequirementStatus {
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Self::Satisfied { .. })
    }

    /// 需要排一个安装任务去补(Unsupported 补不了,不算)
    pub fn needs_action(&self) -> bool {
        matches!(
            self,
            Self::Missing | Self::Unsatisfied { .. } | Self::Unknown { .. }
        )
    }
}

/// 解析后的一个节点:目标 + 谁要它 + 合并后的版本约束 + 当前状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DependencyNode {
    pub target: DependencyTarget,
    /// 直接要到这个目标的组件(含 root),按发现顺序
    pub required_by: Vec<ComponentId>,
    /// 各消费方的版本约束去重后并列;全满足才算 Satisfied
    pub version_reqs: Vec<VersionReq>,
    pub status: RequirementStatus,
}

/// 一次解析的完整结果:root 在 host 上按 phase 需要的全部依赖,拓扑序(被依赖者在前)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DependencyPlan {
    pub root: ComponentId,
    pub host_id: String,
    pub phase: RequirementPhase,
    /// 不含 root 自己
    pub nodes: Vec<DependencyNode>,
}

impl DependencyPlan {
    pub fn ready(&self) -> bool {
        self.nodes.iter().all(|n| n.status.is_satisfied())
    }

    pub fn blocking(&self) -> impl Iterator<Item = &DependencyNode> {
        self.nodes.iter().filter(|n| !n.status.is_satisfied())
    }

    /// 缺失 / 不可用的组件依赖(不含主机命令与系统包)
    pub fn blocking_components(&self) -> Vec<ComponentId> {
        self.blocking()
            .filter_map(|n| n.target.component_id())
            .collect()
    }
}

/// 「root 现在能不能跑」:root 自己的状态 + 它 Run 阶段的依赖。Bot 启动门禁消费
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct RuntimeReadiness {
    /// target 一定是 Component { root };required_by 为空
    pub root: DependencyNode,
    pub plan: DependencyPlan,
}

impl RuntimeReadiness {
    pub fn ready(&self) -> bool {
        self.root.status.is_satisfied() && self.plan.ready()
    }

    /// 阻断节点,root 在前
    pub fn blocking(&self) -> Vec<&DependencyNode> {
        std::iter::once(&self.root)
            .filter(|n| !n.status.is_satisfied())
            .chain(self.plan.blocking())
            .collect()
    }
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
        assert!(VersionReq::all_match(&reqs, "22.0.0"));
        assert!(!VersionReq::all_match(&reqs, "23.0.0"));
        assert!(VersionReq::all_match(&[], "anything"));
    }

    #[test]
    fn phase_filtering() {
        assert!(RequirementPhase::Both.applies_to(RequirementPhase::Install));
        assert!(RequirementPhase::Both.applies_to(RequirementPhase::Run));
        assert!(RequirementPhase::Install.applies_to(RequirementPhase::Install));
        assert!(!RequirementPhase::Install.applies_to(RequirementPhase::Run));
        assert!(!RequirementPhase::Run.applies_to(RequirementPhase::Install));
        assert!(RequirementPhase::Run.applies_to(RequirementPhase::Both));

        let tar = Requirement::host_command("tar", "tar");
        assert!(tar.applies_to(RequirementPhase::Install));
        assert!(!tar.applies_to(RequirementPhase::Run));
        let libs = Requirement::host_packages(HostPackageGroup::QqDependencies);
        assert!(libs.applies_to(RequirementPhase::Run));
    }

    #[test]
    fn wire_literals_are_snake_case_tagged() {
        let edge = Requirement::component_version(ComponentId::NodeJs, "^22.13.0");
        let json = serde_json::to_string(&edge).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"component","id":"nodejs","version":{"kind":"semver","range":"^22.13.0"},"phase":"both"}"#
        );
        let back: Requirement = serde_json::from_str(&json).unwrap();
        assert_eq!(back, edge);

        let status = RequirementStatus::Unsatisfied {
            found: Some("18.19.1".into()),
            reason: "too old".into(),
        };
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            r#"{"state":"unsatisfied","found":"18.19.1","reason":"too old"}"#
        );
        assert_eq!(
            serde_json::to_string(&HostPackageGroup::QqDependencies).unwrap(),
            "\"qq_dependencies\""
        );
        assert_eq!(
            serde_json::to_string(&RequirementStatus::Missing).unwrap(),
            r#"{"state":"missing"}"#
        );
    }

    #[test]
    fn plan_ready_and_blocking() {
        let plan = DependencyPlan {
            root: ComponentId::SnowLuma,
            host_id: "local".into(),
            phase: RequirementPhase::Run,
            nodes: vec![
                DependencyNode {
                    target: DependencyTarget::Component { id: ComponentId::Qq },
                    required_by: vec![ComponentId::SnowLuma],
                    version_reqs: vec![],
                    status: RequirementStatus::Satisfied {
                        version: Some("9.9".into()),
                        source: None,
                    },
                },
                DependencyNode {
                    target: DependencyTarget::Component {
                        id: ComponentId::NodeJs,
                    },
                    required_by: vec![ComponentId::SnowLuma],
                    version_reqs: vec![VersionReq::semver("^22.13.0")],
                    status: RequirementStatus::Missing,
                },
                DependencyNode {
                    target: DependencyTarget::HostCommand {
                        command: "tar".into(),
                        package: "tar".into(),
                    },
                    required_by: vec![ComponentId::SnowLuma],
                    version_reqs: vec![],
                    status: RequirementStatus::Missing,
                },
            ],
        };
        assert!(!plan.ready());
        assert_eq!(plan.blocking().count(), 2);
        assert_eq!(plan.blocking_components(), vec![ComponentId::NodeJs]);
        assert!(!RequirementStatus::Unsupported.needs_action());
        assert!(RequirementStatus::Unknown { error: "x".into() }.needs_action());
    }
}
