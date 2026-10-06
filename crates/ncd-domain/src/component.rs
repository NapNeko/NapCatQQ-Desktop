//! 组件标识与依赖图的数据类型:依赖边、解析出的节点状态、整张计划
//!
//! 依赖图只在各 Component::requirements() 里声明一次;这里只放边和节点的形状,
//! 递归 / 探测 / 排任务在 ncd-runtime。被依赖方(如 Node)不知道谁在用它,
//! 版本约束由消费方写在自己的边上。版本号匹配要 semver,留在 ncd-component。

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Component 标识
///
/// 跨边界时各 variant 的字面量(serde / ts-rs)锁定为:
/// - NapCat → napcat
/// - SnowLuma → snowluma
/// - Qq → qq
/// - NodeJs → nodejs
/// - NoVnc → novnc
/// - VcRedist → vcredist（Visual C++ 2015-2022 x64 运行库；NapCat Windows 注入器的运行时依赖）
/// - DesktopSelf → desktop_self
/// - NcdWatch → ncd_watch
/// - Karin → karin（应用端框架；按实例目录安装，不进组件页 catalog）
/// - Uv → uv（Python 工具链，单二进制，可自带装 Python；NoneBot2 的运行时依赖）
/// - NoneBot2 → nonebot2（应用端框架；同 Karin 按实例目录安装）
/// - AstrBot → astrbot（应用端框架；按实例目录安装，不进组件页 catalog）
/// - MaiBot → maibot（应用端框架；按实例目录安装，不进组件页 catalog）
/// - Koishi → koishi（应用端框架；按实例目录安装，不进组件页 catalog）
/// - Git → git（版本管理工具；云崽装本体、装插件、群里 #更新 都要）
/// - Redis → redis（键值库；云崽启动时自己拉起，桌面端只装二进制）
/// - Yunzai → yunzai（应用端框架，TRSS-Yunzai；按实例目录安装，不进组件页 catalog）
/// - NeoBot → neobot（应用端框架，PyPI neobot-app；按实例目录安装，不进组件页 catalog）
///
/// 与项目内 napcat_* / snowluma_* 事件名风格保持一致;不直接走 serde
/// 的 rename_all = "snake_case",因为它会把 NapCat 切成 nap_cat,
/// Qq 切成 qq 也算巧合,但 NapCat 不行,所以统一都用显式 rename
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ComponentId {
    #[serde(rename = "napcat")]
    NapCat,
    #[serde(rename = "snowluma")]
    SnowLuma,
    #[serde(rename = "qq")]
    Qq,
    #[serde(rename = "nodejs")]
    NodeJs,
    #[serde(rename = "novnc")]
    NoVnc,
    #[serde(rename = "vcredist")]
    VcRedist,
    #[serde(rename = "desktop_self")]
    DesktopSelf,
    #[serde(rename = "ncd_watch")]
    NcdWatch,
    #[serde(rename = "karin")]
    Karin,
    #[serde(rename = "uv")]
    Uv,
    #[serde(rename = "nonebot2")]
    NoneBot2,
    #[serde(rename = "astrbot")]
    AstrBot,
    #[serde(rename = "maibot")]
    MaiBot,
    #[serde(rename = "koishi")]
    Koishi,
    #[serde(rename = "git")]
    Git,
    #[serde(rename = "redis")]
    Redis,
    #[serde(rename = "yunzai")]
    Yunzai,
    #[serde(rename = "neobot")]
    NeoBot,
}

impl ComponentId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NapCat => "napcat",
            Self::SnowLuma => "snowluma",
            Self::Qq => "qq",
            Self::NodeJs => "nodejs",
            Self::NoVnc => "novnc",
            Self::VcRedist => "vcredist",
            Self::DesktopSelf => "desktop_self",
            Self::NcdWatch => "ncd_watch",
            Self::Karin => "karin",
            Self::Uv => "uv",
            Self::NoneBot2 => "nonebot2",
            Self::AstrBot => "astrbot",
            Self::MaiBot => "maibot",
            Self::Koishi => "koishi",
            Self::Git => "git",
            Self::Redis => "redis",
            Self::Yunzai => "yunzai",
            Self::NeoBot => "neobot",
        }
    }

    /// 应用端框架：按实例目录装，不进组件页 catalog。
    /// 新框架加变体时必须写进这里，factory / 依赖图靠它分流，不再点名。
    pub const fn is_app_framework(&self) -> bool {
        matches!(
            self,
            Self::Karin
                | Self::NoneBot2
                | Self::AstrBot
                | Self::MaiBot
                | Self::Koishi
                | Self::Yunzai
                | Self::NeoBot
        )
    }

    /// 从跨边界字面量还原（与 serde rename 同源）；未知返回 None
    pub fn parse(value: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(value.to_string())).ok()
    }
}

/// 本机 Bot 冷启动会拉起哪份 QQ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum LocalQqSource {
    /// 组件页装的那份(components/QQ),和用户自己的 QQ 互不相干
    Managed,
    /// 用户自己装的系统 QQ,Bot 和日常 QQ 共用一份程序
    System,
    /// 两样都没有,启动会报缺 QQ
    Missing,
}

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
    Semver {
        range: String,
    },
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
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => f.write_str("任意版本"),
            Self::Semver { range } => f.write_str(range),
        }
    }
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
    fn component_id_as_str_matches_snake_case() {
        assert_eq!(ComponentId::NapCat.as_str(), "napcat");
        assert_eq!(ComponentId::SnowLuma.as_str(), "snowluma");
        assert_eq!(ComponentId::Qq.as_str(), "qq");
    }

    #[test]
    fn component_id_serializes_snake_case() {
        let s = serde_json::to_string(&ComponentId::DesktopSelf).unwrap();
        assert_eq!(s, "\"desktop_self\"");
    }

    /// 锁定每个 ComponentId variant 的 wire 字面量与 as_str() 一致;
    /// 同时锁定 round-trip 等价任何 typo(包括误用 serde 默认 snake_case
    /// 把 NapCat 切成 nap_cat)都会让此测试失败
    #[test]
    fn component_id_parse_round_trips_as_str() {
        for id in [
            ComponentId::NapCat,
            ComponentId::SnowLuma,
            ComponentId::Qq,
            ComponentId::NodeJs,
            ComponentId::NoVnc,
            ComponentId::VcRedist,
            ComponentId::DesktopSelf,
            ComponentId::NcdWatch,
            ComponentId::Karin,
            ComponentId::Uv,
            ComponentId::NoneBot2,
            ComponentId::AstrBot,
            ComponentId::MaiBot,
            ComponentId::Koishi,
            ComponentId::Git,
            ComponentId::Redis,
            ComponentId::Yunzai,
            ComponentId::NeoBot,
        ] {
            assert_eq!(ComponentId::parse(id.as_str()), Some(id));
        }
        assert_eq!(ComponentId::parse("nope"), None);
    }

    #[test]
    fn app_framework_flag_marks_only_frameworks() {
        assert!(ComponentId::Karin.is_app_framework());
        assert!(ComponentId::NoneBot2.is_app_framework());
        assert!(ComponentId::AstrBot.is_app_framework());
        assert!(ComponentId::MaiBot.is_app_framework());
        assert!(ComponentId::Koishi.is_app_framework());
        assert!(ComponentId::Yunzai.is_app_framework());
        assert!(ComponentId::NeoBot.is_app_framework());
        assert!(!ComponentId::Uv.is_app_framework());
        assert!(!ComponentId::Git.is_app_framework());
        assert!(!ComponentId::Redis.is_app_framework());
        assert!(!ComponentId::NodeJs.is_app_framework());
    }

    #[test]
    fn component_id_serde_aligns_with_as_str() {
        for id in [
            ComponentId::NapCat,
            ComponentId::SnowLuma,
            ComponentId::Qq,
            ComponentId::NodeJs,
            ComponentId::NoVnc,
            ComponentId::VcRedist,
            ComponentId::DesktopSelf,
            ComponentId::NcdWatch,
            ComponentId::Karin,
            ComponentId::Uv,
            ComponentId::NoneBot2,
            ComponentId::AstrBot,
            ComponentId::MaiBot,
            ComponentId::Koishi,
            ComponentId::Git,
            ComponentId::Redis,
            ComponentId::Yunzai,
            ComponentId::NeoBot,
        ] {
            let s = serde_json::to_string(&id).unwrap();
            let expected = format!("\"{}\"", id.as_str());
            assert_eq!(s, expected);
            let decoded: ComponentId = serde_json::from_str(&s).unwrap();
            assert_eq!(decoded, id);
        }
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
                    target: DependencyTarget::Component {
                        id: ComponentId::Qq,
                    },
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
