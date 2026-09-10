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
pub fn graph_component_ids() -> Vec<ComponentId> {
    let mut ids = HOST_GRAPH_COMPONENT_IDS.to_vec();
    for m in AppFrameworkRegistry::with_builtin().manifests() {
        if let Some(id) = ComponentId::parse(&m.component_id) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

/// 只为调 requirements() 的占位实例;路径都是假的,别拿去 detect / install
pub fn graph_component(id: ComponentId, package: SnowLumaLinuxPackage) -> Arc<dyn Component> {
    if id.is_app_framework() {
        return AppFrameworkRegistry::with_builtin()
            .by_component_id(id.as_str())
            .expect("app framework ComponentId must be registered")
            .component(&graph_placeholder_spec());
    }
    let x = HostPath::from_posix("/x");
    match id {
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
        _ => panic!("graph_component missing host arm for {}", id.as_str()),
    }
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
    root: &dyn Component,
    os: Os,
    locality: Locality,
    phase: RequirementPhase,
) -> Vec<ClosureNode> {
    let mut out: Vec<ClosureNode> = Vec::new();
    let mut visiting: Vec<ComponentId> = vec![root.id()];
    walk(root, os, locality, phase, &mut visiting, &mut out);
    out
}

fn walk(
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
            visiting.push(child_id);
            let child = graph_component(child_id, SnowLumaLinuxPackage::Full);
            walk(child.as_ref(), os, locality, phase, visiting, out);
            visiting.pop();
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
pub fn catalog_version_reqs_for(target: ComponentId, os: Os, locality: Locality) -> Vec<VersionReq> {
    let mut reqs = Vec::new();
    for package in [SnowLumaLinuxPackage::Full, SnowLumaLinuxPackage::Lite] {
        for id in graph_component_ids() {
            if id == target {
                continue;
            }
            for req in graph_component(id, package).requirements(os, locality) {
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
pub fn render_dependency_graph() -> String {
    let targets = [
        (Os::Windows, Locality::Local),
        (Os::Linux, Locality::Local),
        (Os::Linux, Locality::Remote),
    ];
    let mut out = String::new();
    for (os, locality) in targets {
        for id in graph_component_ids() {
            let variants: &[SnowLumaLinuxPackage] = if id == ComponentId::SnowLuma {
                &[SnowLumaLinuxPackage::Full, SnowLumaLinuxPackage::Lite]
            } else {
                &[SnowLumaLinuxPackage::Full]
            };
            for package in variants {
                let comp = graph_component(id, *package);
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
Linux/Remote nonebot2
  component uv >=0.4 (Both)
";

    #[test]
    fn dependency_graph_matches_golden_snapshot() {
        let rendered = render_dependency_graph();
        assert!(
            rendered == GOLDEN,
            "dependency graph changed; update GOLDEN if intended.\n--- rendered ---\n{rendered}\n--- golden ---\n{GOLDEN}"
        );
    }

    #[test]
    fn closure_is_topological_and_merges_shared_targets() {
        let root = graph_component(ComponentId::SnowLuma, SnowLumaLinuxPackage::Lite);
        let nodes = requirement_closure(
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
        let root = graph_component(ComponentId::SnowLuma, SnowLumaLinuxPackage::Lite);
        let nodes = requirement_closure(
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
        let reqs = catalog_version_reqs_for(ComponentId::NodeJs, Os::Windows, Locality::Local);
        assert_eq!(
            reqs,
            vec![
                VersionReq::semver(">=18"),
                VersionReq::semver(SnowLumaComponent::NODE_VERSION_RANGE),
            ]
        );
        assert!(catalog_version_reqs_for(ComponentId::Qq, Os::Linux, Locality::Remote).is_empty());
    }

    #[test]
    fn graph_ids_follow_host_catalog_then_registry() {
        use ncd_appframework::AppFrameworkRegistry;
        let ids = graph_component_ids();
        for m in AppFrameworkRegistry::with_builtin().manifests() {
            let id = ComponentId::parse(&m.component_id).expect("framework component_id");
            assert!(ids.contains(&id), "{} 必须进依赖图，不能靠手写名单", m.component_id);
            let _ = graph_component(id, SnowLumaLinuxPackage::Full);
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
        let reqs = catalog_version_reqs_for(ComponentId::Uv, Os::Linux, Locality::Remote);
        assert_eq!(reqs, vec![VersionReq::semver(">=0.4")]);
        // Python 系框架不拖 Node，Node 系框架不拖 uv
        let nb2 = graph_component(ComponentId::NoneBot2, SnowLumaLinuxPackage::Full);
        let nodes = requirement_closure(nb2.as_ref(), Os::Linux, Locality::Remote, RequirementPhase::Install);
        let labels: Vec<String> = nodes.iter().map(|n| n.target.label()).collect();
        assert_eq!(labels, vec!["tar", "uv"]);
    }
}
