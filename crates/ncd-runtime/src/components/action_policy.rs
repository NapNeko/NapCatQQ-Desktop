//! 组件动作策略（纯逻辑）
//!
//! 从 Layer4 command 下沉：任务资源、catalog 顺序、release tag / sha 辅助、远端布局
//! 探测结果解析。依赖闭包不在这里(见 graph.rs / resolver.rs)。不含 Host I/O 与 task 提交。

use std::path::Path;
use std::sync::Arc;

use ncd_component::{
    Component, ComponentId, ComponentInfo, DependencyTarget, DesktopSelfComponent,
    HostPackageGroup, NapCatComponent, NcdWatchComponent, NoVncComponent, NodeJsComponent,
    QQComponent, SnowLumaComponent, UvComponent,
};
use ncd_deploy::StepKind;
use ncd_domain::DeploymentTaskResource;
use ncd_domain::SnowLumaLinuxPackage;
use ncd_domain::release_snapshot::ReleaseInfo;
pub use ncd_domain::{infer_snowluma_linux_package, is_bundled_snowluma_node};
use ncd_host::{Arch, HostPath, Locality, Os};

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

/// 任务里的组件标识：协议组件一台 host 一份（`napcat`）；应用端组件按实例（`karin@<instance_id>`），
/// 这样同机两个 Karin 实例的安装互不去重、互不串行
pub fn component_target_label(component_id: ComponentId, scope: Option<&str>) -> String {
    match scope {
        Some(scope) if component_id.is_app_framework() => {
            format!("{}@{scope}", component_id.as_str())
        }
        _ => component_id.as_str().to_string(),
    }
}

pub fn component_dedupe_key(
    host_id: &str,
    component_id: ComponentId,
    kind: StepKind,
    scope: Option<&str>,
) -> String {
    format!(
        "component:{}:{}:{}",
        host_id,
        component_target_label(component_id, scope),
        kind.as_str()
    )
}

/// 装 / 更新前要先把依赖闭包补齐;卸载 / 校验 / 单独装依赖不需要
pub fn component_action_needs_runtime_closure(kind: StepKind) -> bool {
    matches!(
        kind,
        StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Update
    )
}

pub fn component_task_resources(
    component_id: ComponentId,
    host_id: &str,
    kind: StepKind,
    host_os: Os,
    host_locality: Locality,
    scope: Option<&str>,
) -> Vec<DeploymentTaskResource> {
    let mut resources = Vec::new();
    if !matches!(kind, StepKind::Verify) {
        resources.push(DeploymentTaskResource::InstallTarget {
            host_id: host_id.to_string(),
            target: component_target_label(component_id, scope),
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
            | ComponentId::Uv
            | ComponentId::Qq
            | ComponentId::NcdWatch
            | ComponentId::Karin
            | ComponentId::NoneBot2
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

/// 组件元数据：Framework → RuntimeDep → SelfApp
pub fn component_catalog() -> Vec<ComponentInfo> {
    vec![
        NapCatComponent::info(),
        SnowLumaComponent::info(),
        NodeJsComponent::info(),
        UvComponent::info(),
        QQComponent::info(),
        NoVncComponent::info(),
        NcdWatchComponent::info(),
        DesktopSelfComponent::info(),
    ]
}

/// 依赖目标给用户看的名字:组件取 catalog 显示名,命令就是命令本身
pub fn dependency_target_display_name(target: &DependencyTarget) -> String {
    match target {
        DependencyTarget::Component { id } => component_catalog()
            .into_iter()
            .find(|info| info.id == *id)
            .map(|info| info.display_name)
            .unwrap_or_else(|| id.as_str().to_string()),
        DependencyTarget::HostCommand { command, .. } => command.clone(),
        DependencyTarget::HostPackages {
            group: HostPackageGroup::QqDependencies,
        } => "QQ 系统依赖".to_string(),
    }
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

/// 官方 Linux 发行物：完整包自带 node，lite 需用户自备 Node 22.13+。
pub fn snowluma_linux_release_asset(
    tag: &str,
    arch: Arch,
    package: SnowLumaLinuxPackage,
) -> String {
    let triple = match arch {
        Arch::Aarch64 => "linux-arm64",
        _ => "linux-x64",
    };
    let lite = match package {
        SnowLumaLinuxPackage::Full => "",
        SnowLumaLinuxPackage::Lite => "-lite",
    };
    format!("SnowLuma-{tag}-{triple}{lite}.tar.gz")
}

/// 官方 Windows 发行物：完整包自带 node.exe，lite 需用户自备或由桌面端安装 Node 22.13+。
pub fn snowluma_windows_release_asset(
    tag: &str,
    package: SnowLumaLinuxPackage,
) -> String {
    let lite = match package {
        SnowLumaLinuxPackage::Full => "",
        SnowLumaLinuxPackage::Lite => "-lite",
    };
    format!("SnowLuma-{tag}-win-x64{lite}.zip")
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
    use ncd_domain::RemoteSelectedPaths;

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
                ComponentId::Uv,
                ComponentId::Qq,
                ComponentId::NoVnc,
                ComponentId::NcdWatch,
                ComponentId::DesktopSelf,
            ]
        );
        // 应用端框架按实例装，不进 catalog
        assert!(list.iter().all(|info| !info.id.is_app_framework()));
    }

    #[test]
    fn runtime_closure_only_for_install_like_actions() {
        for kind in [
            StepKind::EnsureInstalled,
            StepKind::ForceInstall,
            StepKind::Update,
        ] {
            assert!(component_action_needs_runtime_closure(kind), "{kind:?}");
        }
        for kind in [
            StepKind::Verify,
            StepKind::Uninstall,
            StepKind::EnsureDependencies,
        ] {
            assert!(!component_action_needs_runtime_closure(kind), "{kind:?}");
        }
    }

    #[test]
    fn snowluma_linux_release_asset_is_full_bundle_not_lite() {
        assert_eq!(
            snowluma_linux_release_asset("v1.14.13", Arch::X86_64, SnowLumaLinuxPackage::Full),
            "SnowLuma-v1.14.13-linux-x64.tar.gz"
        );
        assert_eq!(
            snowluma_linux_release_asset("v1.14.13", Arch::Aarch64, SnowLumaLinuxPackage::Full),
            "SnowLuma-v1.14.13-linux-arm64.tar.gz"
        );
        assert_eq!(
            snowluma_linux_release_asset("v1.14.13", Arch::X86_64, SnowLumaLinuxPackage::Lite),
            "SnowLuma-v1.14.13-linux-x64-lite.tar.gz"
        );
    }

    #[test]
    fn infer_package_from_bundled_vs_portable_node() {
        let full = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/opt/snowluma".into()),
            node_bin: Some("/opt/snowluma/node".into()),
            needs_sudo: false,
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            infer_snowluma_linux_package(Some(&full)),
            SnowLumaLinuxPackage::Full
        );
        let lite = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/home/u/snowluma-remote/workspace/snowluma".into()),
            node_bin: Some("/home/u/snowluma-remote/workspace/node/bin/node".into()),
            needs_sudo: false,
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            infer_snowluma_linux_package(Some(&lite)),
            SnowLumaLinuxPackage::Lite
        );
        assert_eq!(
            infer_snowluma_linux_package(None),
            SnowLumaLinuxPackage::Full
        );
        let installed_without_node = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/opt/snowluma".into()),
            node_bin: None,
            needs_sudo: false,
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            infer_snowluma_linux_package(Some(&installed_without_node)),
            SnowLumaLinuxPackage::Full
        );
        assert!(is_bundled_snowluma_node(
            "/opt/snowluma/node",
            Some("/opt/snowluma")
        ));
        assert!(!is_bundled_snowluma_node(
            "/home/u/snowluma-remote/workspace/node/bin/node",
            Some("/home/u/snowluma-remote/workspace/snowluma")
        ));
    }

    #[test]
    fn component_package_manager_resources_cover_direct_pkg_commands_only() {
        let resources = component_task_resources(
            ComponentId::NoVnc,
            "remote:a",
            StepKind::Uninstall,
            Os::Linux,
            Locality::Remote,
            None,
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
            None,
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
            None,
        );
        assert!(resources.contains(&DeploymentTaskResource::PackageManager {
            host_id: "remote:a".to_string(),
        }));
    }

    #[test]
    fn app_framework_components_are_scoped_per_instance() {
        assert_eq!(
            component_dedupe_key("local", ComponentId::Karin, StepKind::EnsureInstalled, Some("k1")),
            "component:local:karin@k1:ensure_installed"
        );
        // 协议组件忽略 scope：一台 host 只有一份 NapCat
        assert_eq!(
            component_dedupe_key("local", ComponentId::NapCat, StepKind::EnsureInstalled, Some("k1")),
            component_dedupe_key("local", ComponentId::NapCat, StepKind::EnsureInstalled, None),
        );
        let resources = component_task_resources(
            ComponentId::Karin,
            "local",
            StepKind::EnsureInstalled,
            Os::Windows,
            Locality::Local,
            Some("k1"),
        );
        assert!(resources.contains(&DeploymentTaskResource::InstallTarget {
            host_id: "local".to_string(),
            target: "karin@k1".to_string(),
        }));
        assert!(resources.contains(&DeploymentTaskResource::GlobalDownloadSlot));
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
    fn list_components_returns_eight_items() {
        assert_eq!(component_catalog().len(), 8);
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
