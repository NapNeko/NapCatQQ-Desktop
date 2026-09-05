//! 依赖解析:把 root 组件的 requirements() 闭包逐个探测成 DependencyPlan
//!
//! 「装 X 前要什么」「跑 X 前缺什么」「Y 现在算不算装好」三个问题都从这里回答,
//! 装前闭包(executor)、Bot 启动预检(BotManager)、UI 都只消费 plan,不再各写一份链。
//! 探测串行做:远端走的是同一条 SSH 会话,并发没有收益还容易抖。

use std::sync::Arc;

use ncd_component::qq_deps::{QqDependencyDetector, qq_qqnt_dependencies_v3_2_25};
use ncd_component::{
    Component, ComponentId, DependencyNode, DependencyPlan, DependencyTarget, DetectOutcome,
    HostPackageGroup, RequirementPhase, RequirementStatus, RuntimeReadiness, VersionReq,
};
use ncd_host::Host;

use crate::components::graph::{ClosureNode, requirement_closure};

/// 按 id 实例化依赖组件;通常是 build_component_for_host 的闭包。
/// 失败(比如远端 $HOME 探不到)时该节点记 Unknown,不中断整张图。
pub type ComponentBuilder<'a> = dyn Fn(ComponentId) -> Result<Arc<dyn Component>, String> + Sync + 'a;

pub struct ResolveCtx<'a> {
    pub host: &'a dyn Host,
    pub phase: RequirementPhase,
    pub build: &'a ComponentBuilder<'a>,
}

pub async fn resolve_dependencies(root: &dyn Component, ctx: &ResolveCtx<'_>) -> DependencyPlan {
    let os = ctx.host.os();
    let locality = ctx.host.locality();
    let closure = requirement_closure(root, os, locality, ctx.phase);

    let mut nodes = Vec::with_capacity(closure.len());
    for node in closure {
        let status = probe_node(&node, ctx).await;
        nodes.push(DependencyNode {
            target: node.target,
            required_by: node.required_by,
            version_reqs: node.version_reqs,
            status,
        });
    }

    DependencyPlan {
        root: root.id(),
        host_id: ctx.host.id().to_string(),
        phase: ctx.phase,
        nodes,
    }
}

/// root 自己装没装 + Run 依赖;ctx.phase 会被忽略,固定按 Run 解析
pub async fn resolve_runtime_readiness(
    root: &dyn Component,
    ctx: &ResolveCtx<'_>,
) -> RuntimeReadiness {
    let run_ctx = ResolveCtx {
        host: ctx.host,
        phase: RequirementPhase::Run,
        build: ctx.build,
    };
    let status = probe_built_component(root, &[], run_ctx.host).await;
    let plan = resolve_dependencies(root, &run_ctx).await;
    RuntimeReadiness {
        root: DependencyNode {
            target: DependencyTarget::Component { id: root.id() },
            required_by: Vec::new(),
            version_reqs: Vec::new(),
            status,
        },
        plan,
    }
}

async fn probe_node(node: &ClosureNode, ctx: &ResolveCtx<'_>) -> RequirementStatus {
    match &node.target {
        DependencyTarget::Component { id } => {
            probe_component(*id, &node.version_reqs, ctx).await
        }
        DependencyTarget::HostCommand { command, .. } => {
            if ctx.host.command_exists(command).await {
                RequirementStatus::Satisfied {
                    version: None,
                    source: None,
                }
            } else {
                RequirementStatus::Missing
            }
        }
        DependencyTarget::HostPackages { group } => probe_host_packages(*group, ctx.host).await,
    }
}

async fn probe_component(
    id: ComponentId,
    version_reqs: &[VersionReq],
    ctx: &ResolveCtx<'_>,
) -> RequirementStatus {
    let component = match (ctx.build)(id) {
        Ok(c) => c,
        Err(error) => return RequirementStatus::Unknown { error },
    };
    probe_built_component(component.as_ref(), version_reqs, ctx.host).await
}

async fn probe_built_component(
    component: &dyn Component,
    version_reqs: &[VersionReq],
    host: &dyn Host,
) -> RequirementStatus {
    if component.check_target(host).is_err() {
        return RequirementStatus::Unsupported;
    }
    match component.detect_outcome(host).await {
        Ok(DetectOutcome::Installed(found)) => {
            // 组件自己(如注入了 accept 的 Node)一般已按约束挑过候选;
            // 这里再核一遍,兜住没做内部过滤的组件
            if VersionReq::all_match(version_reqs, &found.version) {
                RequirementStatus::Satisfied {
                    version: Some(found.version),
                    source: Some(found.source),
                }
            } else {
                let ranges = version_reqs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" 且 ");
                RequirementStatus::Unsatisfied {
                    reason: format!("v{} 不满足 {ranges}", found.version),
                    found: Some(found.version),
                }
            }
        }
        Ok(DetectOutcome::Unusable(u)) => RequirementStatus::Unsatisfied {
            found: u.version,
            reason: u.reason,
        },
        Ok(DetectOutcome::NotInstalled) => RequirementStatus::Missing,
        Err(e) => RequirementStatus::Unknown {
            error: e.to_string(),
        },
    }
}

async fn probe_host_packages(group: HostPackageGroup, host: &dyn Host) -> RequirementStatus {
    match group {
        HostPackageGroup::QqDependencies => {
            let detector = QqDependencyDetector::new(qq_qqnt_dependencies_v3_2_25());
            match detector.detect(host, None).await {
                Ok(report) if report.missing.is_empty() => RequirementStatus::Satisfied {
                    version: None,
                    source: None,
                },
                Ok(report) => {
                    let names: Vec<&str> = report.missing.iter().map(|p| p.name.as_str()).collect();
                    RequirementStatus::Unsatisfied {
                        found: None,
                        reason: format!("缺少 {} 个系统库:{}", names.len(), names.join(", ")),
                    }
                }
                Err(e) => RequirementStatus::Unknown {
                    error: e.to_string(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::path::Path;

    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_component::{
        ActionCtx, ActionError, DetectedVersion, LaunchArgs, Requirement, UnusableInstall,
        VerifyReport,
    };
    use ncd_host::shell::BashShell;
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostPath, HostProcess,
        HostShell, Locality, Os, PackageManager,
    };

    struct ScriptedHost {
        /// `command -v X` 的 X → 是否存在
        commands: Vec<&'static str>,
        shell: BashShell,
    }

    #[async_trait]
    impl Host for ScriptedHost {
        fn os(&self) -> Os {
            Os::Linux
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Remote
        }
        fn id(&self) -> &str {
            "remote:test"
        }
        fn shell(&self) -> &dyn HostShell {
            &self.shell
        }
        fn pkg_manager(&self) -> Option<&dyn PackageManager> {
            None
        }
        async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
            Ok(false)
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
            // 默认 command_exists 走 `sh -c "command -v X"`
            let script = cmd.args.get(1).cloned().unwrap_or_default();
            let name = script.trim_start_matches("command -v ").to_string();
            let ok = self.commands.contains(&name.as_str());
            Ok(CommandOutput {
                exit_code: Some(if ok { 0 } else { 1 }),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    /// 可编程组件:声明的边 + 预设探测结果
    struct FakeComponent {
        id: ComponentId,
        reqs: Vec<Requirement>,
        outcome: Result<DetectOutcome, String>,
    }

    #[async_trait]
    impl Component for FakeComponent {
        fn id(&self) -> ComponentId {
            self.id
        }
        fn supported_targets(&self) -> &'static [(Os, Locality)] {
            &[(Os::Linux, Locality::Remote), (Os::Linux, Locality::Local)]
        }
        fn requirements(&self, _: Os, _: Locality) -> Vec<Requirement> {
            self.reqs.clone()
        }
        async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
            Ok(self.detect_outcome(host).await?.into_installed())
        }
        async fn detect_outcome(&self, _: &dyn Host) -> Result<DetectOutcome, ActionError> {
            self.outcome.clone().map_err(ActionError::other)
        }
        async fn install(&self, _: &dyn Host, _: &mut ActionCtx) -> Result<(), ActionError> {
            unreachable!("resolver never installs")
        }
        async fn verify(&self, _: &dyn Host) -> Result<VerifyReport, ActionError> {
            unreachable!()
        }
        fn launch_command(&self, _: &dyn Host, _: &LaunchArgs) -> Result<HostCommand, ActionError> {
            unreachable!()
        }
    }

    fn installed(v: &str) -> Result<DetectOutcome, String> {
        Ok(DetectOutcome::Installed(DetectedVersion {
            version: v.into(),
            source: "/x".into(),
        }))
    }

    fn registry(entries: Vec<FakeComponent>) -> HashMap<ComponentId, Arc<dyn Component>> {
        entries
            .into_iter()
            .map(|c| (c.id, Arc::new(c) as Arc<dyn Component>))
            .collect()
    }

    #[tokio::test]
    async fn lite_snowluma_on_remote_linux_reports_each_dependency_state() {
        // root 用真的 SnowLuma(Lite),依赖用 Fake 顶替真实探测
        let root = ncd_component::SnowLumaComponent::new(
            HostPath::from_posix("/x"),
            "https://example.invalid/x.tar.gz",
        )
        .with_package(ncd_domain::SnowLumaLinuxPackage::Lite);

        let deps = registry(vec![
            FakeComponent {
                id: ComponentId::NodeJs,
                reqs: vec![Requirement::host_command("tar", "tar")],
                outcome: Ok(DetectOutcome::Unusable(UnusableInstall {
                    source: "$PATH/node".into(),
                    version: Some("18.19.1".into()),
                    reason: "v18.19.1 不满足 ^22.13.0 || >=23.4.0".into(),
                })),
            },
            FakeComponent {
                id: ComponentId::Qq,
                reqs: vec![],
                outcome: installed("3.2.25"),
            },
            FakeComponent {
                id: ComponentId::NoVnc,
                reqs: vec![],
                outcome: Ok(DetectOutcome::NotInstalled),
            },
        ]);
        let host = ScriptedHost {
            commands: vec![],
            shell: BashShell,
        };
        let build = |id: ComponentId| {
            deps.get(&id)
                .cloned()
                .ok_or_else(|| format!("no fake for {id:?}"))
        };
        let plan = resolve_dependencies(
            &root,
            &ResolveCtx {
                host: &host,
                phase: RequirementPhase::Install,
                build: &build,
            },
        )
        .await;

        assert_eq!(plan.root, ComponentId::SnowLuma);
        assert_eq!(plan.host_id, "remote:test");
        let by_label: HashMap<String, &DependencyNode> =
            plan.nodes.iter().map(|n| (n.target.label(), n)).collect();
        // 图结构来自 catalog 声明(不是 Fake):tar(Node 子树先出现)、nodejs、
        // qq_dependencies(真 QQ 在 Linux 声明的系统库)、qq、novnc
        assert_eq!(by_label.len(), 5);
        assert_eq!(by_label["tar"].status, RequirementStatus::Missing);
        assert!(matches!(
            by_label["qq_dependencies"].status,
            RequirementStatus::Unknown { .. }
        ));
        assert_eq!(
            by_label["nodejs"].status,
            RequirementStatus::Unsatisfied {
                found: Some("18.19.1".into()),
                reason: "v18.19.1 不满足 ^22.13.0 || >=23.4.0".into(),
            }
        );
        assert_eq!(
            by_label["qq"].status,
            RequirementStatus::Satisfied {
                version: Some("3.2.25".into()),
                source: Some("/x".into()),
            }
        );
        assert_eq!(by_label["novnc"].status, RequirementStatus::Missing);
        assert!(!plan.ready());
        assert_eq!(
            plan.blocking_components(),
            vec![ComponentId::NodeJs, ComponentId::NoVnc]
        );
    }

    #[tokio::test]
    async fn resolver_rechecks_version_even_if_component_said_installed() {
        let root = FakeComponent {
            id: ComponentId::SnowLuma,
            reqs: vec![Requirement::component_version(ComponentId::NodeJs, ">=22")],
            outcome: installed("1"),
        };
        let deps = registry(vec![FakeComponent {
            id: ComponentId::NodeJs,
            reqs: vec![],
            outcome: installed("18.0.0"),
        }]);
        let host = ScriptedHost {
            commands: vec!["tar"],
            shell: BashShell,
        };
        let build = |id: ComponentId| deps.get(&id).cloned().ok_or_else(|| "x".to_string());
        let plan = resolve_dependencies(
            &root,
            &ResolveCtx {
                host: &host,
                phase: RequirementPhase::Run,
                build: &build,
            },
        )
        .await;
        assert_eq!(plan.nodes.len(), 1);
        assert_eq!(
            plan.nodes[0].status,
            RequirementStatus::Unsatisfied {
                found: Some("18.0.0".into()),
                reason: "v18.0.0 不满足 >=22".into(),
            }
        );
    }

    #[tokio::test]
    async fn runtime_readiness_includes_root_and_forces_run_phase() {
        let root = FakeComponent {
            id: ComponentId::NapCat,
            reqs: vec![
                Requirement::component(ComponentId::Qq),
                Requirement::host_command("unzip", "unzip"),
            ],
            outcome: Ok(DetectOutcome::NotInstalled),
        };
        let deps = registry(vec![FakeComponent {
            id: ComponentId::Qq,
            reqs: vec![],
            outcome: installed("3.2.25"),
        }]);
        let host = ScriptedHost {
            commands: vec![],
            shell: BashShell,
        };
        let build = |id: ComponentId| deps.get(&id).cloned().ok_or_else(|| "x".to_string());
        let readiness = resolve_runtime_readiness(
            &root,
            &ResolveCtx {
                host: &host,
                // 传 Install 也按 Run 解析:unzip 这条安装期边不该出现
                phase: RequirementPhase::Install,
                build: &build,
            },
        )
        .await;
        assert_eq!(readiness.plan.phase, RequirementPhase::Run);
        assert_eq!(readiness.root.status, RequirementStatus::Missing);
        assert_eq!(
            readiness.root.target,
            DependencyTarget::Component { id: ComponentId::NapCat }
        );
        let labels: Vec<String> = readiness.plan.nodes.iter().map(|n| n.target.label()).collect();
        assert_eq!(labels, vec!["qq_dependencies", "qq"]);
        assert!(!readiness.ready());
        let blocking: Vec<String> = readiness.blocking().iter().map(|n| n.target.label()).collect();
        assert_eq!(blocking, vec!["napcat", "qq_dependencies"]);
    }

    #[tokio::test]
    async fn builder_failure_and_probe_error_become_unknown_not_panic() {
        let root = FakeComponent {
            id: ComponentId::NapCat,
            reqs: vec![
                Requirement::component(ComponentId::Qq),
                Requirement::component(ComponentId::NoVnc),
                Requirement::host_command("unzip", "unzip"),
            ],
            outcome: installed("1"),
        };
        let deps = registry(vec![FakeComponent {
            id: ComponentId::Qq,
            reqs: vec![],
            outcome: Err("ssh dropped".into()),
        }]);
        let host = ScriptedHost {
            commands: vec!["unzip"],
            shell: BashShell,
        };
        let build = |id: ComponentId| {
            deps.get(&id)
                .cloned()
                .ok_or_else(|| format!("cannot build {}", id.as_str()))
        };
        let plan = resolve_dependencies(
            &root,
            &ResolveCtx {
                host: &host,
                phase: RequirementPhase::Install,
                build: &build,
            },
        )
        .await;
        let statuses: Vec<(String, RequirementStatus)> = plan
            .nodes
            .iter()
            .map(|n| (n.target.label(), n.status.clone()))
            .collect();
        assert_eq!(
            statuses,
            vec![
                (
                    "qq_dependencies".into(),
                    RequirementStatus::Unknown {
                        error: "read os-release: unsupported on this host: test".into()
                    }
                ),
                (
                    "qq".into(),
                    RequirementStatus::Unknown {
                        error: "ssh dropped".into()
                    }
                ),
                (
                    "novnc".into(),
                    RequirementStatus::Unknown {
                        error: "cannot build novnc".into()
                    }
                ),
                (
                    "unzip".into(),
                    RequirementStatus::Satisfied {
                        version: None,
                        source: None
                    }
                ),
            ]
        );
    }
}
