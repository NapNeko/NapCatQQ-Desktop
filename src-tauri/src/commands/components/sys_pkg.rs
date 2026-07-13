//! 系统包前置任务（archive 工具 / 包管理器探测）

use std::sync::Arc;
use std::time::Duration;

use ncd_component::{ComponentId, ProgressKind, ProgressLogLevel};
use ncd_deploy::StepKind;
use ncd_domain::{DeploymentTaskKind, DeploymentTaskResource};
use ncd_host::{Host, HostCommand};
use ncd_runtime::{
    DeploymentTaskContext, DeploymentTaskRequest, DeploymentTaskRunResult,
    SystemPackagePrerequisite, component_package_prerequisites,
};
use uuid::Uuid;

use super::progress::push_task_progress;
use super::qq_deps::install_qq_dependencies_task;
use crate::AppState;

pub(super) async fn submit_component_package_prerequisites(
    component_id: ComponentId,
    kind: StepKind,
    host_id: &str,
    host: Arc<dyn Host>,
    state: &AppState,
) -> Vec<String> {
    let tasks = component_package_prerequisites(component_id, kind, host.os());
    let mut submitted_ids = Vec::with_capacity(tasks.len());
    for task in tasks {
        if system_package_prerequisite_is_satisfied(&task, host.as_ref()).await {
            tracing::info!(
                host_id,
                package_group = %task.package_group(),
                "skip satisfied system package prerequisite"
            );
            continue;
        }
        let id = submit_system_package_task(task, host_id, Arc::clone(&host), state).await;
        submitted_ids.push(id);
    }
    submitted_ids
}

async fn system_package_prerequisite_is_satisfied(
    task: &SystemPackagePrerequisite,
    host: &dyn Host,
) -> bool {
    match task {
        SystemPackagePrerequisite::ArchiveTool { command, .. } => {
            host.command_exists(command).await
        }
        SystemPackagePrerequisite::QqDependencies => {
            let manifest = ncd_component::qq_deps::qq_qqnt_dependencies_v3_2_25();
            let detector = ncd_component::qq_deps::QqDependencyDetector::new(manifest);
            match detector.detect(host, None).await {
                Ok(report) => report.missing.is_empty(),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "QQ dependency pre-detect failed; submitting system package task"
                    );
                    false
                }
            }
        }
    }
}

async fn submit_system_package_task(
    task: SystemPackagePrerequisite,
    host_id: &str,
    host: Arc<dyn Host>,
    state: &AppState,
) -> String {
    let package_group = task.package_group();
    let task_id = Uuid::new_v4().to_string();
    let server_id = host_id.strip_prefix("remote:").map(str::to_string);
    let server_manager = Arc::clone(&state.server_manager);
    let deployment_tasks = state.deployment_tasks.clone();
    let host_id_owned = host_id.to_string();
    let title = task.title();
    let resources = vec![
        DeploymentTaskResource::PackageManager {
            host_id: host_id_owned.clone(),
        },
        DeploymentTaskResource::InstallTarget {
            host_id: host_id_owned.clone(),
            target: format!("system_package:{package_group}"),
        },
    ];
    deployment_tasks
        .submit(DeploymentTaskRequest {
            task_id: task_id.clone(),
            kind: DeploymentTaskKind::SystemPackage {
                package_group: package_group.clone(),
            },
            host_id: host_id_owned.clone(),
            title,
            resources,
            depends_on: vec![],
            dedupe_key: Some(format!("system-package:{host_id}:{package_group}")),
            cancellable: false,
            runner: Box::new(move |task_ctx| {
                Box::pin(async move {
                    if let Some(id) = server_id {
                        let task_for_run = task.clone();
                        server_manager
                            .with_isolated_connection(&id, move |iso_host| {
                                let task_ctx = task_ctx.clone();
                                Box::pin(async move {
                                    Ok(run_system_package_task(
                                        task_for_run,
                                        iso_host.as_ref(),
                                        task_ctx,
                                    )
                                    .await)
                                })
                            })
                            .await
                            .unwrap_or_else(DeploymentTaskRunResult::failed)
                    } else {
                        run_system_package_task(task, host.as_ref(), task_ctx).await
                    }
                })
            }),
        })
        .await
}

async fn run_system_package_task(
    task: SystemPackagePrerequisite,
    host: &dyn Host,
    task_ctx: DeploymentTaskContext,
) -> DeploymentTaskRunResult {
    match task {
        SystemPackagePrerequisite::ArchiveTool { command, package } => {
            ensure_archive_tool_task(command, package, host, task_ctx).await
        }
        SystemPackagePrerequisite::QqDependencies => {
            install_qq_dependencies_task(host, Vec::new(), None, task_ctx).await
        }
    }
}

async fn ensure_archive_tool_task(
    command: &str,
    package: &str,
    host: &dyn Host,
    task_ctx: DeploymentTaskContext,
) -> DeploymentTaskRunResult {
    push_task_progress(&task_ctx, ProgressKind::Started { total_steps: 1 }).await;
    push_task_progress(
        &task_ctx,
        ProgressKind::StepBegin {
            step: 1,
            message: format!("检查系统工具 {command}"),
        },
    )
    .await;

    if host.command_exists(command).await {
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: format!("{command} 已可用"),
            },
        )
        .await;
        push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: true }).await;
        push_task_progress(&task_ctx, ProgressKind::Finished { ok: true }).await;
        return DeploymentTaskRunResult::ok(format!("{command} 已就绪"));
    }

    let Some(pm) = SystemPackageManager::detect(host).await else {
        let msg = format!("远端缺少 {command} 且未识别到包管理器，请手动安装 {package} 后重试");
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Error,
                message: msg.clone(),
            },
        )
        .await;
        push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
        push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
        return DeploymentTaskRunResult::failed(msg);
    };

    let access = ncd_host::remote::probe_sudo(host).await;
    let has_password = host.has_elevation_password().await;
    let elevation_ok = matches!(
        access,
        ncd_host::remote::SudoAccess::RootAlready | ncd_host::remote::SudoAccess::Passwordless
    ) || has_password;
    if !elevation_ok {
        let msg = format!(
            "安装 {package} 需要 sudo 密码，请在远端主机配置中保存 sudo 密码，或手动执行 sudo {} 后重试",
            pm.install_hint(package)
        );
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Error,
                message: msg.clone(),
            },
        )
        .await;
        push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
        push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
        return DeploymentTaskRunResult::failed(msg);
    }

    let install_line = pm.install_command(package);
    push_task_progress(
        &task_ctx,
        ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: format!("通过 {} 安装 {package}", pm.binary()),
        },
    )
    .await;
    let out = host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(&install_line)
                .elevated()
                .timeout(Duration::from_secs(180)),
        )
        .await;
    match out {
        Ok(out) if out.success() && host.command_exists(command).await => {
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: true }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: true }).await;
            DeploymentTaskRunResult::ok(format!("{command} 已安装"))
        }
        Ok(out) => {
            let msg = format!(
                "安装 {package} 后仍无法找到 {command}: exit={:?} stderr={}",
                out.exit_code,
                out.stderr.trim()
            );
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
            DeploymentTaskRunResult::failed(msg)
        }
        Err(err) => {
            let msg = format!("安装 {package} 失败: {err}");
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
            DeploymentTaskRunResult::failed(msg)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemPackageManager {
    Apt,
    Dnf,
    Yum,
    Apk,
    Pacman,
}

impl SystemPackageManager {
    const ALL: &'static [Self] = &[Self::Apt, Self::Dnf, Self::Yum, Self::Apk, Self::Pacman];

    async fn detect(host: &dyn Host) -> Option<Self> {
        for pm in Self::ALL {
            if host.command_exists(pm.binary()).await {
                return Some(*pm);
            }
        }
        None
    }

    fn binary(self) -> &'static str {
        match self {
            Self::Apt => "apt-get",
            Self::Dnf => "dnf",
            Self::Yum => "yum",
            Self::Apk => "apk",
            Self::Pacman => "pacman",
        }
    }

    fn install_command(self, package: &str) -> String {
        match self {
            Self::Apt => format!("apt-get update && apt-get install -y {package}"),
            Self::Dnf => format!("dnf install -y {package}"),
            Self::Yum => format!("yum install -y {package}"),
            Self::Apk => format!("apk add --no-cache {package}"),
            Self::Pacman => format!("pacman -Sy --noconfirm {package}"),
        }
    }

    fn install_hint(self, package: &str) -> String {
        match self {
            Self::Apt => format!("apt-get install -y {package}"),
            Self::Dnf => format!("dnf install -y {package}"),
            Self::Yum => format!("yum install -y {package}"),
            Self::Apk => format!("apk add {package}"),
            Self::Pacman => format!("pacman -S {package}"),
        }
    }
}
