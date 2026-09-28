//! 依赖图的纯查询:不探测主机,只把各 Component::requirements() 递归成闭包
//!
//! 组件用占位路径实例化,因为 requirements() 只看 (os, locality) 和变体
//! (SnowLuma Full / Lite),不看安装路径。真正的探测在 resolver.rs。

use std::sync::Arc;

use ncd_appframework::{AppComponentSpec, AppFrameworkRegistry};
use ncd_component::{
    Component, ComponentId, DependencyTarget, DesktopSelfComponent, NapCatComponent,
    NcdWatchComponent, NoVncComponent, NodeJsComponent, QQComponent, Requirement,
    RequirementPhase, SnowLumaComponent, UvComponent, VersionReq,
};
use ncd_domain::SnowLumaLinuxPackage;
use ncd_host::{HostPath, Locality, Os};

fn graph_placeholder_spec() -> AppComponentSpec {
    AppComponentSpec {
        install_dir: HostPath::from_posix("/x"),
        port: 0,
        node_bin: None,
        uv_bin: None,
        npm_registry: None,
        install_renderer: false,
        adopt_existing: false,
        instance_id: "x".into(),
        webui_username: None,
        webui_password: None,
    }
}

/// catalog 顺序（与 component_catalog 一致）。应用端不写在这里，由注册表追加。
const HOST_GRAPH_COMPONENT_IDS: [ComponentId; 8] = [
    ComponentId::NapCat,
    ComponentId::SnowLuma,
    ComponentId::NodeJs,
    ComponentId::Uv,
    ComponentId::Qq,
    ComponentId::NoVnc,
    ComponentId::NcdWatch,
    ComponentId::DesktopSelf,
];

/// 主机组件 + 已注册应用端。接新框架只改注册表，不用改这张名单。
pub fn graph_component_ids(registry: &AppFrameworkRegistry) -> Vec<ComponentId> {
    let mut ids = HOST_GRAPH_COMPONENT_IDS.to_vec();
    for m in registry.manifests() {
        if let Some(id) = ComponentId::parse(&m.component_id) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

/// 只为调 requirements() 的占位实例;路径都是假的,别拿去 detect / install。
/// 没注册的应用端 / 漏写的主机组件给 Err,调用方跳过这棵子树,不拖垮整张图
pub fn graph_component(
    registry: &AppFrameworkRegistry,
    id: ComponentId,
    package: SnowLumaLinuxPackage,
) -> Result<Arc<dyn Component>, String> {
    if id.is_app_framework() {
        return registry
            .by_component_id(id.as_str())
            .map(|adapter| adapter.component(&graph_placeholder_spec()))
            .ok_or_else(|| format!("应用端框架未注册: {}", id.as_str()));
    }
    let x = HostPath::from_posix("/x");
    let component: Arc<dyn Component> = match id {
        ComponentId::NapCat => Arc::new(NapCatComponent::new(x)),
        ComponentId::SnowLuma => Arc::new(
            SnowLumaComponent::new(x, "https://example.invalid/x.tar.gz").with_package(package),
        ),
        ComponentId::NodeJs => Arc::new(NodeJsComponent::new("0.0.0", x)),
        ComponentId::Uv => Arc::new(UvComponent::new("0.0.0", x)),
        ComponentId::Qq => Arc::new(QQComponent::default_v3_2_25(x)),
        ComponentId::NoVnc => Arc::new(NoVncComponent::new()),
        ComponentId::NcdWatch => Arc::new(NcdWatchComponent::new(None)),
        ComponentId::DesktopSelf => Arc::new(DesktopSelfComponent::new("0.0.0", x)),
        _ => return Err(format!("依赖图缺 {} 的占位实例", id.as_str())),
    };
    Ok(component)
}

/// 递归闭包里的一个节点(未探测)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureNode {
    pub target: DependencyTarget,
    /// 直接要到它的组件,按发现顺序(root 在内)
    pub required_by: Vec<ComponentId>,
    /// 各消费方的版本约束去重
    pub version_reqs: Vec<VersionReq>,
}

impl ClosureNode {
    fn absorb(&mut self, from: ComponentId, req: &Requirement) {
        if !self.required_by.contains(&from) {
            self.required_by.push(from);
        }
        if let Some(v) = req.version_req() {
            if !self.version_reqs.contains(v) {
                self.version_reqs.push(v.clone());
            }
        }
    }
}

/// root 的依赖闭包,拓扑序(被依赖者在前),同一目标合并。root 自己不在里面。
/// 依赖组件用占位实例展开(依赖方永远不是 SnowLuma,变体只影响 root)
pub fn requirement_closure(
    registry: &AppFrameworkRegistry,
    root: &dyn Component,
    os: Os,
    locality: Locality,
    phase: RequirementPhase,
) -> Vec<ClosureNode> {
    let mut out: Vec<ClosureNode> = Vec::new();
    let mut visiting: Vec<ComponentId> = vec![root.id()];
    walk(registry, root, os, locality, phase, &mut visiting, &mut out);
    out
}

fn walk(
    registry: &AppFrameworkRegistry,
    from: &dyn Component,
    os: Os,
    locality: Locality,
    phase: RequirementPhase,
    visiting: &mut Vec<ComponentId>,
    out: &mut Vec<ClosureNode>,
) {
    for req in from.requirements(os, locality) {
        if !req.applies_to(phase) {
            continue;
        }
        let target = req.target();
        if let Some(existing) = out.iter_mut().find(|n| n.target == target) {
            existing.absorb(from.id(), &req);
            continue;
        }
        if let Some(child_id) = req.component_id() {
            // 环保护:声明出环是 bug,这里只保证不死循环
            if visiting.contains(&child_id) {
                continue;
            }
            // 实例化不了只是不展开它自己的依赖,节点照常入图交给 resolver 探测
            match graph_component(registry, child_id, SnowLumaLinuxPackage::Full) {
                Ok(child) => {
                    visiting.push(child_id);
                    walk(registry, child.as_ref(), os, locality, phase, visiting, out);
                    visiting.pop();
                }
                Err(err) => tracing::warn!(error = %err, "dependency subtree skipped"),
            }
        }
        let mut node = ClosureNode {
            target,
            required_by: Vec::new(),
            version_reqs: Vec::new(),
        };
        node.absorb(from.id(), &req);
        out.push(node);
    }
}

/// 该 (os, locality) 下 catalog 里所有组件(两种 SnowLuma 变体都算)对 `target`
/// 的版本约束。被依赖方(Node)自己不知道范围,单独探测它时用这个注入
pub fn catalog_version_reqs_for(
    registry: &AppFrameworkRegistry,
    target: ComponentId,
    os: Os,
    locality: Locality,
) -> Vec<VersionReq> {
    let mut reqs = Vec::new();
    for package in [SnowLumaLinuxPackage::Full, SnowLumaLinuxPackage::Lite] {
        for id in graph_component_ids(registry) {
            if id == target {
                continue;
            }
            let Ok(consumer) = graph_component(registry, id, package) else {
                continue;
            };
            for req in consumer.requirements(os, locality) {
                if req.component_id() == Some(target) {
                    if let Some(v) = req.version_req() {
                        if !reqs.contains(v) {
                            reqs.push(v.clone());
                        }
                    }
                }
            }
        }
    }
    reqs
}

/// 整张图的文本快照:组件 × 目标 × 变体 → 直接依赖边。测试用黄金文件,
/// 改任何组件的 requirements() 都会让它 diff 出来
pub fn render_dependency_graph(registry: &AppFrameworkRegistry) -> String {
    let targets = [
        (Os::Windows, Locality::Local),
        (Os::Linux, Locality::Local),
        (Os::Linux, Locality::Remote),
    ];
    let mut out = String::new();
    for (os, locality) in targets {
        for id in graph_component_ids(registry) {
            let variants: &[SnowLumaLinuxPackage] = if id == ComponentId::SnowLuma {
                &[SnowLumaLinuxPackage::Full, SnowLumaLinuxPackage::Lite]
            } else {
                &[SnowLumaLinuxPackage::Full]
            };
            for package in variants {
                // 缺占位实例的组件直接不出现在快照里,黄金对比会把它暴露出来
                let Ok(comp) = graph_component(registry, id, *package) else {
                    continue;
                };
                if !comp.supported_targets().contains(&(os, locality)) {
                    continue;
                }
                let variant = match (id, package) {
                    (ComponentId::SnowLuma, SnowLumaLinuxPackage::Full) => "[full]",
                    (ComponentId::SnowLuma, SnowLumaLinuxPackage::Lite) => "[lite]",
                    _ => "",
                };
                out.push_str(&format!(
                    "{os:?}/{locality:?} {}{variant}\n",
                    id.as_str()
                ));
                for req in comp.requirements(os, locality) {
                    out.push_str(&format!("  {}\n", render_requirement(&req)));
                }
            }
        }
    }
    out
}

fn render_requirement(req: &Requirement) -> String {
    match req {
        Requirement::Component { id, version, phase } => match version {
            VersionReq::Any => format!("component {} ({phase:?})", id.as_str()),
            VersionReq::Semver { range } => {
                format!("component {} {range} ({phase:?})", id.as_str())
            }
        },
        Requirement::HostCommand { command, package } => {
            format!("host_command {command} <- {package} (Install)")
        }
        Requirement::HostPackages { group } => {
            format!("host_packages {} (Both)", group.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 黄金快照:依赖图的唯一事实。改组件 requirements() 时同步改这里,
    // 顺手就能看出「这条边到底该不该有」
    const GOLDEN: &str = "\
Windows/Local napcat
  component qq (Both)
Windows/Local snowluma[full]
  component qq (Both)
Windows/Local snowluma[lite]
  component nodejs ^22.13.0 || >=23.4.0 (Both)
  component qq (Both)
Windows/Local nodejs
Windows/Local uv
Windows/Local qq
Windows/Local desktop_self
Windows/Local astrbot
  component uv >=0.4 (Both)
Windows/Local karin
  component nodejs >=18 (Both)
Windows/Local maibot
  component uv >=0.4 (Both)
Windows/Local nonebot2
  component uv >=0.4 (Both)
Linux/Local napcat
  component qq (Both)
  host_command unzip <- unzip (Install)
Linux/Local snowluma[full]
  component qq (Both)
  host_command tar <- tar (Install)
Linux/Local snowluma[lite]
  component nodejs ^22.13.0 || >=23.4.0 (Both)
  component qq (Both)
  host_command tar <- tar (Install)
Linux/Local nodejs
  host_command tar <- tar (Install)
Linux/Local uv
  host_command tar <- tar (Install)
Linux/Local qq
  host_packages qq_dependencies (Both)
Linux/Local novnc
Linux/Local desktop_self
Linux/Local astrbot
  component uv >=0.4 (Both)
Linux/Local karin
  component nodejs >=18 (Both)
Linux/Local maibot
  component uv >=0.4 (Both)
Linux/Local nonebot2
  component uv >=0.4 (Both)
Linux/Remote napcat
  component qq (Both)
  host_command unzip <- unzip (Install)
Linux/Remote snowluma[full]
  component qq (Both)
  component novnc (Both)
  host_command tar <- tar (Install)
Linux/Remote snowluma[lite]
  component nodejs ^22.13.0 || >=23.4.0 (Both)
  component qq (Both)
  component novnc (Both)
  host_command tar <- tar (Install)
Linux/Remote nodejs
  host_command tar <- tar (Install)
Linux/Remote uv
  host_command tar <- tar (Install)
Linux/Remote qq
  host_packages qq_dependencies (Both)
Linux/Remote novnc
Linux/Remote ncd_watch
Linux/Remote astrbot
  component uv >=0.4 (Both)
Linux/Remote karin
  component nodejs >=18 (Both)
Linux/Remote maibot
  component uv >=0.4 (Both)
Linux/Remote nonebot2
  component uv >=0.4 (Both)
";

    #[test]
    fn dependency_graph_matches_golden_snapshot() {
        let rendered = render_dependency_graph(&AppFrameworkRegistry::with_builtin());
        assert!(
            rendered == GOLDEN,
            "dependency graph changed; update GOLDEN if intended.\n--- rendered ---\n{rendered}\n--- golden ---\n{GOLDEN}"
        );
    }

    #[test]
    fn closure_is_topological_and_merges_shared_targets() {
        let registry = AppFrameworkRegistry::with_builtin();
        let root =
            graph_component(&registry, ComponentId::SnowLuma, SnowLumaLinuxPackage::Lite).unwrap();
        let nodes = requirement_closure(
            &registry,
            root.as_ref(),
            Os::Linux,
            Locality::Remote,
            RequirementPhase::Install,
        );
        let labels: Vec<String> = nodes.iter().map(|n| n.target.label()).collect();
        // tar 被 nodejs 和 snowluma 同时要到:先在 nodejs 子树里出现,之后被合并
        assert_eq!(
            labels,
            vec!["tar", "nodejs", "qq_dependencies", "qq", "novnc"]
        );
        let tar = &nodes[0];
        assert_eq!(
            tar.required_by,
            vec![ComponentId::NodeJs, ComponentId::SnowLuma]
        );
        let node = &nodes[1];
        assert_eq!(node.required_by, vec![ComponentId::SnowLuma]);
        assert_eq!(
            node.version_reqs,
            vec![VersionReq::semver(SnowLumaComponent::NODE_VERSION_RANGE)]
        );
    }

    #[test]
    fn run_phase_drops_install_only_edges() {
        let registry = AppFrameworkRegistry::with_builtin();
        let root =
            graph_component(&registry, ComponentId::SnowLuma, SnowLumaLinuxPackage::Lite).unwrap();
        let nodes = requirement_closure(
            &registry,
            root.as_ref(),
            Os::Linux,
            Locality::Remote,
            RequirementPhase::Run,
        );
        let labels: Vec<String> = nodes.iter().map(|n| n.target.label()).collect();
        assert_eq!(labels, vec!["nodejs", "qq_dependencies", "qq", "novnc"]);
    }

    #[test]
    fn node_constraints_come_from_consumers_not_node_itself() {
        // 先按 full 包遍历（Karin 的约束先进），再 lite 包（SnowLuma lite 才要 Node）
        let registry = AppFrameworkRegistry::with_builtin();
        let reqs =
            catalog_version_reqs_for(&registry, ComponentId::NodeJs, Os::Windows, Locality::Local);
        assert_eq!(
            reqs,
            vec![
                VersionReq::semver(">=18"),
                VersionReq::semver(SnowLumaComponent::NODE_VERSION_RANGE),
            ]
        );
        assert!(
            catalog_version_reqs_for(&registry, ComponentId::Qq, Os::Linux, Locality::Remote)
                .is_empty()
        );
    }

    #[test]
    fn graph_ids_follow_host_catalog_then_registry() {
        let registry = AppFrameworkRegistry::with_builtin();
        let ids = graph_component_ids(&registry);
        for m in registry.manifests() {
            let id = ComponentId::parse(&m.component_id).expect("framework component_id");
            assert!(ids.contains(&id), "{} 必须进依赖图，不能靠手写名单", m.component_id);
            assert!(graph_component(&registry, id, SnowLumaLinuxPackage::Full).is_ok());
        }
        let host: Vec<ComponentId> = ids
            .iter()
            .copied()
            .filter(|id| !id.is_app_framework())
            .collect();
        assert_eq!(
            host,
            vec![
                ComponentId::NapCat,
                ComponentId::SnowLuma,
                ComponentId::NodeJs,
                ComponentId::Uv,
                ComponentId::Qq,
                ComponentId::NoVnc,
                ComponentId::NcdWatch,
                ComponentId::DesktopSelf,
            ]
        );
    }

    #[test]
    fn uv_constraints_come_from_nonebot2_only() {
        let registry = AppFrameworkRegistry::with_builtin();
        let reqs = catalog_version_reqs_for(&registry, ComponentId::Uv, Os::Linux, Locality::Remote);
        assert_eq!(reqs, vec![VersionReq::semver(">=0.4")]);
        // Python 系框架不拖 Node，Node 系框架不拖 uv
        let nb2 =
            graph_component(&registry, ComponentId::NoneBot2, SnowLumaLinuxPackage::Full).unwrap();
        let nodes = requirement_closure(
            &registry,
            nb2.as_ref(),
            Os::Linux,
            Locality::Remote,
            RequirementPhase::Install,
        );
        let labels: Vec<String> = nodes.iter().map(|n| n.target.label()).collect();
        assert_eq!(labels, vec!["tar", "uv"]);
    }

    #[test]
    fn unregistered_framework_is_an_error_not_a_panic() {
        let empty = AppFrameworkRegistry::new();
        assert!(graph_component(&empty, ComponentId::Karin, SnowLumaLinuxPackage::Full).is_err());
        assert!(!graph_component_ids(&empty).contains(&ComponentId::Karin));
        // 主机组件不靠注册表,空表也照常展开
        let root =
            graph_component(&empty, ComponentId::SnowLuma, SnowLumaLinuxPackage::Lite).unwrap();
        let nodes = requirement_closure(
            &empty,
            root.as_ref(),
            Os::Linux,
            Locality::Remote,
            RequirementPhase::Run,
        );
        assert_eq!(nodes.len(), 4);
    }
}
