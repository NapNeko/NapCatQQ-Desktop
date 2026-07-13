//! 组件动作策略（纯逻辑）
//!
//! 从 Layer4 command 下沉：前置依赖闭包、任务资源、包管理前置、catalog 顺序、
//! release tag / sha 辅助、远端布局探测结果解析。不含 Host I/O 与 task 提交。

use std::path::Path;
use std::sync::Arc;

use ncd_component::{
    Component, ComponentId, ComponentInfo, DesktopSelfComponent, NapCatComponent,
    NcdWatchComponent, NoVncComponent, NodeJsComponent, QQComponent, SnowLumaComponent,
};
use ncd_deploy::StepKind;
use ncd_domain::DeploymentTaskResource;
use ncd_domain::release_snapshot::ReleaseInfo;
use ncd_host::{HostPath, Locality, Os};

/// 单步组件任务规格（用于前置闭包与 dedupe）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentTaskSpec {
    pub component_id: ComponentId,
    pub kind: StepKind,
}

/// 远端 NapCat / QQ 安装布局
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteLayout {
    /// 系统安装: /opt/QQ/...
    System,
    /// 用户安装: $HOME/Napcat/...
    Rootless,
}

/// 一台远端主机的布局探测结果
#[derive(Debug, Clone)]
pub struct RemoteHostProbe {
    pub home: Option<String>,
    pub layout: RemoteLayout,
}

impl RemoteHostProbe {
    pub fn local_default() -> Self {
        Self {
            home: None,
            layout: RemoteLayout::Rootless,
        }
    }
}

/// Linux 上组件安装前可能需要的系统包前置
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemPackagePrerequisite {
    ArchiveTool {
        command: &'static str,
        package: &'static str,
    },
    QqDependencies,
}

impl SystemPackagePrerequisite {
    pub fn package_group(&self) -> String {
        match self {
            Self::ArchiveTool { command, .. } => format!("archive_tool:{command}"),
            Self::QqDependencies => "qq_dependencies".to_string(),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::ArchiveTool { command, .. } => format!("准备系统工具 {command}"),
            Self::QqDependencies => "安装 QQ 系统依赖".to_string(),
        }
    }
}

pub fn component_dedupe_key(host_id: &str, component_id: ComponentId, kind: StepKind) -> String {
    format!(
        "component:{}:{}:{}",
        host_id,
        component_id.as_str(),
        kind.as_str()
    )
}

pub fn component_action_needs_runtime_closure(kind: StepKind) -> bool {
    matches!(
        kind,
        StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Update
    )
}

pub fn component_runtime_prerequisites(
    component_id: ComponentId,
    kind: StepKind,
    host_os: Os,
    host_locality: Locality,
) -> Vec<ComponentTaskSpec> {
    if !component_action_needs_runtime_closure(kind) {
        return Vec::new();
    }

    let ensure = |component_id| ComponentTaskSpec {
        component_id,
        kind: StepKind::EnsureInstalled,
    };

    match component_id {
        ComponentId::NapCat => match host_os {
            Os::Windows | Os::Linux => vec![ensure(ComponentId::Qq)],
            _ => Vec::new(),
        },
        ComponentId::SnowLuma => match host_os {
            Os::Windows => vec![ensure(ComponentId::Qq)],
            Os::Linux => {
                let mut deps = vec![ensure(ComponentId::NodeJs), ensure(ComponentId::Qq)];
                if host_locality == Locality::Remote {
                    deps.push(ensure(ComponentId::NoVnc));
                }
                deps
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub fn collect_component_runtime_prerequisites(
    target: ComponentTaskSpec,
    host_os: Os,
    host_locality: Locality,
) -> Vec<ComponentTaskSpec> {
    let mut seen = Vec::new();
    let mut ordered = Vec::new();
    collect_component_runtime_prerequisites_inner(
        target,
        host_os,
        host_locality,
        &mut seen,
        &mut ordered,
    );
    ordered
}

fn collect_component_runtime_prerequisites_inner(
    target: ComponentTaskSpec,
    host_os: Os,
    host_locality: Locality,
    seen: &mut Vec<ComponentTaskSpec>,
    ordered: &mut Vec<ComponentTaskSpec>,
) {
    for dep in
        component_runtime_prerequisites(target.component_id, target.kind, host_os, host_locality)
    {
        if seen.contains(&dep) {
            continue;
        }
        seen.push(dep);
        collect_component_runtime_prerequisites_inner(dep, host_os, host_locality, seen, ordered);
        ordered.push(dep);
    }
}

pub fn direct_runtime_dependency_ids(
    target: ComponentTaskSpec,
    host_os: Os,
    host_locality: Locality,
    submitted: &[(ComponentTaskSpec, String)],
) -> Vec<String> {
    component_runtime_prerequisites(target.component_id, target.kind, host_os, host_locality)
        .into_iter()
        .filter_map(|dep| {
            submitted
                .iter()
                .find(|(spec, _)| *spec == dep)
                .map(|(_, task_id)| task_id.clone())
        })
        .collect()
}

pub fn component_task_resources(
    component_id: ComponentId,
    host_id: &str,
    kind: StepKind,
    host_os: Os,
    host_locality: Locality,
) -> Vec<DeploymentTaskResource> {
    let mut resources = Vec::new();
    if !matches!(kind, StepKind::Verify) {
        resources.push(DeploymentTaskResource::InstallTarget {
            host_id: host_id.to_string(),
            target: component_id.as_str().to_string(),
        });
    }
    if component_needs_download_slot(component_id, kind) {
        resources.push(DeploymentTaskResource::GlobalDownloadSlot);
    }
    if component_needs_package_manager(component_id, kind, host_os, host_locality) {
        resources.push(DeploymentTaskResource::PackageManager {
            host_id: host_id.to_string(),
        });
    }
    resources
}

pub fn component_needs_download_slot(component_id: ComponentId, kind: StepKind) -> bool {
    matches!(
        kind,
        StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Update
    ) && matches!(
        component_id,
        ComponentId::NapCat
            | ComponentId::SnowLuma
            | ComponentId::NodeJs
            | ComponentId::Qq
            | ComponentId::NcdWatch
    )
}

pub fn component_needs_package_manager(
    component_id: ComponentId,
    kind: StepKind,
    host_os: Os,
    _host_locality: Locality,
) -> bool {
    if host_os != Os::Linux {
        return false;
    }
    match component_id {
        ComponentId::NoVnc => matches!(
            kind,
            StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Uninstall
        ),
        ComponentId::Qq => kind == StepKind::EnsureDependencies,
        _ => false,
    }
}

pub fn component_action_cancellable(
    component_id: ComponentId,
    kind: StepKind,
    host_os: Os,
    host_locality: Locality,
) -> bool {
    !component_needs_package_manager(component_id, kind, host_os, host_locality)
}

pub fn component_package_prerequisites(
    component_id: ComponentId,
    kind: StepKind,
    host_os: Os,
) -> Vec<SystemPackagePrerequisite> {
    if host_os != Os::Linux
        || !matches!(
            kind,
            StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Update
        )
    {
        return Vec::new();
    }

    match component_id {
        ComponentId::NapCat => vec![SystemPackagePrerequisite::ArchiveTool {
            command: "unzip",
            package: "unzip",
        }],
        ComponentId::NodeJs | ComponentId::SnowLuma => {
            vec![SystemPackagePrerequisite::ArchiveTool {
                command: "tar",
                package: "tar",
            }]
        }
        ComponentId::Qq => vec![SystemPackagePrerequisite::QqDependencies],
        _ => Vec::new(),
    }
}

/// 组件元数据：Framework → RuntimeDep → SelfApp
pub fn component_catalog() -> Vec<ComponentInfo> {
    vec![
        NapCatComponent::info(),
        SnowLumaComponent::info(),
        NodeJsComponent::info(),
        QQComponent::info(),
        NoVncComponent::info(),
        NcdWatchComponent::info(),
        DesktopSelfComponent::info(),
    ]
}

/// 解析 `probe_remote_host` 的 shell 输出（两行：HOME、system 标记）
pub fn parse_remote_host_probe_stdout(stdout: &str) -> RemoteHostProbe {
    let mut lines = stdout.lines();
    let home = lines
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let system_exists = lines.next().map(str::trim) == Some("1");
    let layout = if system_exists {
        RemoteLayout::System
    } else {
        RemoteLayout::Rootless
    };
    RemoteHostProbe { home, layout }
}

pub fn require_remote_home(remote_home: Option<&str>) -> Result<&str, String> {
    remote_home.ok_or_else(|| {
        "无法探测远端 $HOME,已拒绝回退到 /root 安装(避免组件落到错误目录)。\
         请确认远端 SSH 用户有正常的家目录后重试。"
            .to_string()
    })
}

pub fn normalize_github_release_tag(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.starts_with('v') || t.starts_with('V') {
        t.to_string()
    } else {
        format!("v{t}")
    }
}

pub fn snowluma_github_release_tag(
    latest: Option<&ReleaseInfo>,
    local_version: Option<&str>,
) -> String {
    if let Some(info) = latest {
        if !info.tag.is_empty() {
            return normalize_github_release_tag(&info.tag);
        }
        if !info.version.is_empty() {
            return normalize_github_release_tag(&info.version);
        }
    }
    local_version
        .map(normalize_github_release_tag)
        .unwrap_or_default()
}

pub fn asset_sha256(info: &ReleaseInfo, name: &str) -> Option<String> {
    info.assets
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.sha256.clone())
        .filter(|s| !s.is_empty())
}

pub fn data_root_to_host_path(data_root: &Path, os: Os) -> HostPath {
    let s = data_root.to_string_lossy();
    match os {
        Os::Windows => HostPath::from_windows(&s),
        _ => HostPath::from_posix(s.into_owned()),
    }
}

/// catalog 与 trait supported_targets 一致性检查用（测试）
pub fn catalog_component_pairs_for_target_check() -> Vec<(ComponentInfo, Arc<dyn Component>)> {
    vec![
        (
            NapCatComponent::info(),
            Arc::new(NapCatComponent::new(HostPath::from_posix("/x"))),
        ),
        (
            SnowLumaComponent::info(),
            Arc::new(SnowLumaComponent::new(
                HostPath::from_posix("/x"),
                "https://example.com/x.tar.gz",
            )),
        ),
        (
            NodeJsComponent::info(),
            Arc::new(NodeJsComponent::new("22.12.0", HostPath::from_posix("/x"))),
        ),
        (
            QQComponent::info(),
            Arc::new(QQComponent::default_v3_2_25(HostPath::from_posix("/x"))),
        ),
        (NoVncComponent::info(), Arc::new(NoVncComponent::new())),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component_spec(component_id: ComponentId, kind: StepKind) -> ComponentTaskSpec {
        ComponentTaskSpec { component_id, kind }
    }

    #[test]
    fn normalize_github_release_tag_adds_v_prefix() {
        assert_eq!(normalize_github_release_tag("1.9.5"), "v1.9.5");
        assert_eq!(normalize_github_release_tag("v1.9.5"), "v1.9.5");
        assert_eq!(normalize_github_release_tag(""), "");
    }

    #[test]
    fn catalog_returns_items_in_expected_order() {
        let list = component_catalog();
        let ids: Vec<ComponentId> = list.iter().map(|info| info.id).collect();
        assert_eq!(
            ids,
            vec![
                ComponentId::NapCat,
                ComponentId::SnowLuma,
                ComponentId::NodeJs,
                ComponentId::Qq,
                ComponentId::NoVnc,
                ComponentId::NcdWatch,
                ComponentId::DesktopSelf,
            ]
        );
    }

    #[test]
    fn component_runtime_prerequisites_match_native_runtime_chains() {
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::NapCat,
                StepKind::EnsureInstalled,
                Os::Windows,
                Locality::Local,
            ),
            vec![component_spec(ComponentId::Qq, StepKind::EnsureInstalled)]
        );
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::SnowLuma,
                StepKind::EnsureInstalled,
                Os::Windows,
                Locality::Local,
            ),
            vec![component_spec(ComponentId::Qq, StepKind::EnsureInstalled)]
        );
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::NapCat,
                StepKind::EnsureInstalled,
                Os::Linux,
                Locality::Remote,
            ),
            vec![component_spec(ComponentId::Qq, StepKind::EnsureInstalled)]
        );
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::SnowLuma,
                StepKind::EnsureInstalled,
                Os::Linux,
                Locality::Remote,
            ),
            vec![
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
            ]
        );
    }

    #[test]
    fn component_runtime_prerequisites_only_apply_to_install_like_actions() {
        for kind in [
            StepKind::Verify,
            StepKind::Uninstall,
            StepKind::EnsureDependencies,
        ] {
            assert!(
                component_runtime_prerequisites(
                    ComponentId::SnowLuma,
                    kind,
                    Os::Linux,
                    Locality::Remote,
                )
                .is_empty(),
                "{kind:?} must not auto-submit runtime prerequisites"
            );
        }
    }

    #[test]
    fn force_install_keeps_runtime_prerequisites_as_ensure_installed() {
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::SnowLuma,
                StepKind::ForceInstall,
                Os::Linux,
                Locality::Remote,
            ),
            vec![
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
            ]
        );
    }

    #[test]
    fn update_keeps_runtime_prerequisites_as_ensure_installed() {
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::SnowLuma,
                StepKind::Update,
                Os::Linux,
                Locality::Remote,
            ),
            vec![
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
            ]
        );
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::NapCat,
                StepKind::Update,
                Os::Windows,
                Locality::Local,
            ),
            vec![component_spec(ComponentId::Qq, StepKind::EnsureInstalled)]
        );
    }

    #[test]
    fn collected_snowluma_remote_prerequisites_are_deduped_in_dependency_order() {
        let chain = collect_component_runtime_prerequisites(
            component_spec(ComponentId::SnowLuma, StepKind::EnsureInstalled),
            Os::Linux,
            Locality::Remote,
        );

        assert_eq!(
            chain,
            vec![
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
            ]
        );
    }

    #[test]
    fn direct_runtime_dependency_ids_return_only_direct_component_tasks() {
        let submitted = vec![
            (
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                "node-task".to_string(),
            ),
            (
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                "qq-task".to_string(),
            ),
            (
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
                "novnc-task".to_string(),
            ),
        ];

        let ids = direct_runtime_dependency_ids(
            component_spec(ComponentId::SnowLuma, StepKind::EnsureInstalled),
            Os::Linux,
            Locality::Remote,
            &submitted,
        );

        assert_eq!(ids, vec!["node-task", "qq-task", "novnc-task"]);
    }

    #[test]
    fn linux_archive_component_actions_create_visible_package_prerequisites() {
        assert_eq!(
            component_package_prerequisites(
                ComponentId::NapCat,
                StepKind::EnsureInstalled,
                Os::Linux
            ),
            vec![SystemPackagePrerequisite::ArchiveTool {
                command: "unzip",
                package: "unzip",
            }]
        );
        assert_eq!(
            component_package_prerequisites(ComponentId::NodeJs, StepKind::Update, Os::Linux),
            vec![SystemPackagePrerequisite::ArchiveTool {
                command: "tar",
                package: "tar",
            }]
        );
        assert!(
            component_package_prerequisites(
                ComponentId::NapCat,
                StepKind::EnsureInstalled,
                Os::Windows
            )
            .is_empty()
        );
        assert!(
            component_package_prerequisites(ComponentId::NapCat, StepKind::Verify, Os::Linux)
                .is_empty()
        );
    }

    #[test]
    fn linux_qq_install_creates_dependency_prerequisite() {
        assert_eq!(
            component_package_prerequisites(ComponentId::Qq, StepKind::ForceInstall, Os::Linux),
            vec![SystemPackagePrerequisite::QqDependencies]
        );
    }

    #[test]
    fn component_package_manager_resources_cover_direct_pkg_commands_only() {
        let resources = component_task_resources(
            ComponentId::NoVnc,
            "remote:a",
            StepKind::Uninstall,
            Os::Linux,
            Locality::Remote,
        );
        assert!(resources.contains(&DeploymentTaskResource::PackageManager {
            host_id: "remote:a".to_string(),
        }));

        let resources = component_task_resources(
            ComponentId::NapCat,
            "remote:a",
            StepKind::EnsureInstalled,
            Os::Linux,
            Locality::Remote,
        );
        assert!(
            !resources.contains(&DeploymentTaskResource::PackageManager {
                host_id: "remote:a".to_string(),
            })
        );

        let resources = component_task_resources(
            ComponentId::Qq,
            "remote:a",
            StepKind::EnsureDependencies,
            Os::Linux,
            Locality::Remote,
        );
        assert!(resources.contains(&DeploymentTaskResource::PackageManager {
            host_id: "remote:a".to_string(),
        }));
    }

    #[test]
    fn component_cancellable_matches_safe_runtime_stop_support() {
        assert!(!component_action_cancellable(
            ComponentId::NoVnc,
            StepKind::EnsureInstalled,
            Os::Linux,
            Locality::Remote,
        ));
        assert!(!component_action_cancellable(
            ComponentId::Qq,
            StepKind::EnsureDependencies,
            Os::Linux,
            Locality::Remote,
        ));
        assert!(component_action_cancellable(
            ComponentId::NapCat,
            StepKind::EnsureInstalled,
            Os::Linux,
            Locality::Remote,
        ));
        assert!(component_action_cancellable(
            ComponentId::Qq,
            StepKind::EnsureInstalled,
            Os::Linux,
            Locality::Remote,
        ));
    }

    #[test]
    fn catalog_supported_targets_match_components() {
        for (info, component) in catalog_component_pairs_for_target_check() {
            let from_trait: Vec<(Os, Locality)> = component.supported_targets().to_vec();
            let from_info: Vec<(Os, Locality)> = info
                .supported_targets
                .iter()
                .map(|st| (st.os, st.locality))
                .collect();
            assert_eq!(
                from_info, from_trait,
                "ComponentInfo::supported_targets diverged from Component::supported_targets for {:?}",
                info.id
            );
        }
    }

    #[test]
    fn list_components_returns_seven_items() {
        assert_eq!(component_catalog().len(), 7);
    }

    #[test]
    fn parse_remote_host_probe_stdout_system_and_rootless() {
        let p = parse_remote_host_probe_stdout("/home/alice\n1\n");
        assert_eq!(p.home.as_deref(), Some("/home/alice"));
        assert_eq!(p.layout, RemoteLayout::System);

        let p = parse_remote_host_probe_stdout("/home/bob\n0\n");
        assert_eq!(p.layout, RemoteLayout::Rootless);
    }
}
