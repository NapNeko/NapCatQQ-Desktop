//! Bot 启动前的运行时依赖预检:框架组件 + 它的 Run 依赖是否都在
//!
//! 依赖图与探测在 components::resolver;这里只定义 BotManager 依赖的口子,
//! 因为把 BotConfig 变成 (host, 构建输入) 需要 ServerManager / 库存 / 设置,
//! 那些在 src-tauri wiring 里才齐,由那边实现并注入。

use async_trait::async_trait;
use ncd_component::{ComponentId, DependencyTarget, RequirementStatus, RuntimeReadiness};
use ncd_domain::bot_config::{BackendType, BotConfig};

use crate::components::action_policy::component_catalog;

#[async_trait]
pub trait RuntimeReadinessGate: Send + Sync {
    /// Ok(None):本次启动不适用(如 Docker 部署);Err:探测本身没做成(连不上主机等)
    async fn check(&self, config: &BotConfig) -> Result<Option<RuntimeReadiness>, String>;
}

/// Bot 的框架组件
pub fn framework_component_for(backend: BackendType) -> ComponentId {
    match backend {
        BackendType::NapCat => ComponentId::NapCat,
        BackendType::SnowLuma => ComponentId::SnowLuma,
    }
}

/// 给启动失败 / 门禁提示看的一句话;None 表示就绪
pub fn describe_not_ready(readiness: &RuntimeReadiness, local: bool) -> Option<String> {
    let blocking = readiness.blocking();
    if blocking.is_empty() {
        return None;
    }
    let where_ = if local { "本机" } else { "远程主机" };
    let mut missing = Vec::new();
    let mut unusable = Vec::new();
    let mut unknown = Vec::new();
    for node in blocking {
        let name = target_display_name(&node.target);
        match &node.status {
            RequirementStatus::Missing | RequirementStatus::Unsupported => missing.push(name),
            RequirementStatus::Unsatisfied { reason, .. } => {
                unusable.push(format!("{name}({reason})"))
            }
            RequirementStatus::Unknown { .. } => unknown.push(name),
            RequirementStatus::Satisfied { .. } => {}
        }
    }
    let mut parts = Vec::new();
    if !missing.is_empty() {
        parts.push(format!("缺少 {}", missing.join("、")));
    }
    if !unusable.is_empty() {
        parts.push(format!("不可用:{}", unusable.join("、")));
    }
    if !unknown.is_empty() {
        parts.push(format!("未能确认 {}", unknown.join("、")));
    }
    Some(format!(
        "{where_}{},请到「组件」页安装后再启动",
        parts.join(";")
    ))
}

fn target_display_name(target: &DependencyTarget) -> String {
    match target {
        DependencyTarget::Component { id } => component_catalog()
            .into_iter()
            .find(|info| info.id == *id)
            .map(|info| info.display_name)
            .unwrap_or_else(|| id.as_str().to_string()),
        DependencyTarget::HostCommand { command, .. } => command.clone(),
        DependencyTarget::HostPackages { .. } => "QQ 系统依赖".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_component::{DependencyNode, DependencyPlan, RequirementPhase, VersionReq};

    fn node(target: DependencyTarget, status: RequirementStatus) -> DependencyNode {
        DependencyNode {
            target,
            required_by: vec![],
            version_reqs: vec![],
            status,
        }
    }

    #[test]
    fn not_ready_message_groups_missing_and_unusable() {
        let readiness = RuntimeReadiness {
            root: node(
                DependencyTarget::Component {
                    id: ComponentId::SnowLuma,
                },
                RequirementStatus::Satisfied {
                    version: None,
                    source: None,
                },
            ),
            plan: DependencyPlan {
                root: ComponentId::SnowLuma,
                host_id: "local".into(),
                phase: RequirementPhase::Run,
                nodes: vec![
                    node(
                        DependencyTarget::Component { id: ComponentId::Qq },
                        RequirementStatus::Missing,
                    ),
                    DependencyNode {
                        version_reqs: vec![VersionReq::semver("^22.13.0")],
                        ..node(
                            DependencyTarget::Component {
                                id: ComponentId::NodeJs,
                            },
                            RequirementStatus::Unsatisfied {
                                found: Some("18.19.1".into()),
                                reason: "v18.19.1 不满足 ^22.13.0".into(),
                            },
                        )
                    },
                ],
            },
        };
        assert_eq!(
            describe_not_ready(&readiness, true).unwrap(),
            "本机缺少 QQ;不可用:Node.js(v18.19.1 不满足 ^22.13.0),请到「组件」页安装后再启动"
        );
    }

    #[test]
    fn ready_gives_none_and_root_missing_is_reported_first() {
        let ok = RuntimeReadiness {
            root: node(
                DependencyTarget::Component {
                    id: ComponentId::NapCat,
                },
                RequirementStatus::Satisfied {
                    version: None,
                    source: None,
                },
            ),
            plan: DependencyPlan {
                root: ComponentId::NapCat,
                host_id: "remote:a".into(),
                phase: RequirementPhase::Run,
                nodes: vec![],
            },
        };
        assert!(describe_not_ready(&ok, false).is_none());

        let root_missing = RuntimeReadiness {
            root: node(
                DependencyTarget::Component {
                    id: ComponentId::NapCat,
                },
                RequirementStatus::Missing,
            ),
            plan: DependencyPlan {
                root: ComponentId::NapCat,
                host_id: "remote:a".into(),
                phase: RequirementPhase::Run,
                nodes: vec![node(
                    DependencyTarget::Component { id: ComponentId::Qq },
                    RequirementStatus::Missing,
                )],
            },
        };
        assert_eq!(
            describe_not_ready(&root_missing, false).unwrap(),
            "远程主机缺少 NapCat、QQ,请到「组件」页安装后再启动"
        );
    }
}
