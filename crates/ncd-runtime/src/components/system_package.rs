//! 系统包任务的 runner:补主机命令(tar / unzip)与 QQ 系统依赖
//!
//! 从 L4 commands/components/{sys_pkg,qq_deps}.rs 下沉。任务提交在 executor.rs,
//! 这里只负责在拿到 host 之后把事干完并往 task 推进度。

use ncd_component::qq_deps::{QqDependencyDetector, QqDependencyInstaller, qq_qqnt_dependencies_v3_2_25};
use ncd_component::{ActionCtx, DependencyTarget, HostPackageGroup, ProgressEvent, ProgressKind, ProgressLogLevel};
use ncd_domain::InstallDependenciesResult;
use ncd_host::{Host, LinuxPackageManager, Os};

use crate::deploy::tasks::{DeploymentTaskContext, DeploymentTaskRunResult};

pub async fn push_task_progress(task_ctx: &DeploymentTaskContext, kind: ProgressKind) {
    task_ctx.push_progress(ProgressEvent::new(kind)).await;
}

async fn log(task_ctx: &DeploymentTaskContext, level: ProgressLogLevel, message: impl Into<String>) {
    push_task_progress(
        task_ctx,
        ProgressKind::Log {
            level,
            message: message.into(),
        },
    )
    .await;
}

async fn finish(task_ctx: &DeploymentTaskContext, ok: bool) {
    push_task_progress(task_ctx, ProgressKind::StepEnd { step: 1, ok }).await;
    push_task_progress(task_ctx, ProgressKind::Finished { ok }).await;
}

async fn fail(task_ctx: &DeploymentTaskContext, msg: String) -> DeploymentTaskRunResult {
    log(task_ctx, ProgressLogLevel::Error, msg.clone()).await;
    finish(task_ctx, false).await;
    DeploymentTaskRunResult::failed(msg)
}

/// 任务队列里「系统包」一类的标识与文案
pub fn system_package_group(target: &DependencyTarget) -> Option<String> {
    match target {
        DependencyTarget::HostCommand { command, .. } => Some(format!("archive_tool:{command}")),
        DependencyTarget::HostPackages { group } => Some(group.as_str().to_string()),
        DependencyTarget::Component { .. } => None,
    }
}

pub fn system_package_title(target: &DependencyTarget) -> String {
    match target {
        DependencyTarget::HostCommand { command, .. } => format!("准备系统工具 {command}"),
        DependencyTarget::HostPackages {
            group: HostPackageGroup::QqDependencies,
        } => "安装 QQ 系统依赖".to_string(),
        DependencyTarget::Component { id } => id.as_str().to_string(),
    }
}

/// 按目标分派 runner
pub async fn run_system_package_task(
    target: DependencyTarget,
    host: &dyn Host,
    task_ctx: DeploymentTaskContext,
) -> DeploymentTaskRunResult {
    match target {
        DependencyTarget::HostCommand { command, package } => {
            ensure_host_command_task(&command, &package, host, task_ctx).await
        }
        DependencyTarget::HostPackages {
            group: HostPackageGroup::QqDependencies,
        } => install_qq_dependencies_task(host, Vec::new(), None, task_ctx).await,
        DependencyTarget::Component { id } => DeploymentTaskRunResult::failed(format!(
            "{} 不是系统包目标",
            id.as_str()
        )),
    }
}

/// 缺命令就用包管理器装 `package`;已在 PATH 上直接成功
pub async fn ensure_host_command_task(
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
        log(&task_ctx, ProgressLogLevel::Info, format!("{command} 已可用")).await;
        finish(&task_ctx, true).await;
        return DeploymentTaskRunResult::ok(format!("{command} 已就绪"));
    }

    let Some(pm) = LinuxPackageManager::detect(host).await else {
        return fail(
            &task_ctx,
            format!("远端缺少 {command} 且未识别到包管理器，请手动安装 {package} 后重试"),
        )
        .await;
    };

    let access = ncd_host::remote::probe_sudo(host).await;
    let elevation_ok = matches!(
        access,
        ncd_host::remote::SudoAccess::RootAlready | ncd_host::remote::SudoAccess::Passwordless
    ) || host.has_elevation_password().await;
    if !elevation_ok {
        return fail(
            &task_ctx,
            format!(
                "安装 {package} 需要 sudo 密码，请在远端主机配置中保存 sudo 密码，或手动执行 sudo {} 后重试",
                pm.install_hint(&[package])
            ),
        )
        .await;
    }

    log(
        &task_ctx,
        ProgressLogLevel::Info,
        format!("通过 {} 安装 {package}", pm.binary()),
    )
    .await;
    // 装前先刷索引(apt 不刷常见 404),失败不致命
    if let Some(refresh) = pm.refresh_command() {
        let _ = host.run_to_string(refresh.elevated()).await;
    }
    match host
        .run_to_string(pm.install_command(&[package]).elevated())
        .await
    {
        Ok(out) if out.success() && host.command_exists(command).await => {
            finish(&task_ctx, true).await;
            DeploymentTaskRunResult::ok(format!("{command} 已安装"))
        }
        Ok(out) => {
            fail(
                &task_ctx,
                format!(
                    "安装 {package} 后仍无法找到 {command}: exit={:?} stderr={}",
                    out.exit_code,
                    out.stderr.trim()
                ),
            )
            .await
        }
        Err(err) => fail(&task_ctx, format!("安装 {package} 失败: {err}")).await,
    }
}

/// 装 QQ 系统依赖;`packages` 为空时先探测缺什么
pub async fn install_qq_dependencies_task(
    host: &dyn Host,
    packages: Vec<String>,
    sudo_password: Option<String>,
    task_ctx: DeploymentTaskContext,
) -> DeploymentTaskRunResult {
    if host.os() != Os::Linux {
        return DeploymentTaskRunResult::failed(
            "QQ dependencies installation is only supported on Linux",
        );
    }

    push_task_progress(&task_ctx, ProgressKind::Started { total_steps: 1 }).await;
    push_task_progress(
        &task_ctx,
        ProgressKind::StepBegin {
            step: 1,
            message: "安装 QQ 系统依赖".into(),
        },
    )
    .await;

    let packages = if packages.is_empty() {
        log(&task_ctx, ProgressLogLevel::Info, "检测 QQ 系统依赖").await;
        let detector = QqDependencyDetector::new(qq_qqnt_dependencies_v3_2_25());
        match detector.detect(host, None).await {
            Ok(report) => report.missing.into_iter().map(|p| p.name).collect(),
            Err(err) => return fail(&task_ctx, format!("检测 QQ 系统依赖失败: {err}")).await,
        }
    } else {
        packages
    };

    if packages.is_empty() {
        log(&task_ctx, ProgressLogLevel::Info, "QQ 系统依赖已满足").await;
        finish(&task_ctx, true).await;
        return DeploymentTaskRunResult::ok("QQ 系统依赖已满足");
    }

    log(
        &task_ctx,
        ProgressLogLevel::Info,
        format!(
            "缺失 {} 个 QQ 系统依赖: {}",
            packages.len(),
            packages.join(", ")
        ),
    )
    .await;

    match run_qq_installer(host, packages, sudo_password, &task_ctx).await {
        Ok(result) if result.elevation_required => {
            fail(
                &task_ctx,
                "安装 QQ 系统依赖需要 sudo 密码，请在远端主机配置中保存 sudo 密码后重试".to_string(),
            )
            .await
        }
        Ok(result) if result.success => {
            log(
                &task_ctx,
                ProgressLogLevel::Info,
                format!("QQ 系统依赖安装成功: {}", result.installed.join(", ")),
            )
            .await;
            finish(&task_ctx, true).await;
            DeploymentTaskRunResult::ok(format!("已安装 {} 个 QQ 系统依赖", result.installed.len()))
        }
        Ok(result) => fail(&task_ctx, qq_install_failure_message(&result)).await,
        Err(err) => fail(&task_ctx, format!("安装 QQ 系统依赖失败: {err}")).await,
    }
}

/// 用户显式点「安装 QQ 依赖」走这条:包列表 / 密码由调用方给,结果原样返回给 UI
pub async fn run_qq_dependency_install_for_command(
    host: &dyn Host,
    packages: Vec<String>,
    sudo_password: Option<String>,
    task_ctx: DeploymentTaskContext,
) -> Result<InstallDependenciesResult, String> {
    push_task_progress(&task_ctx, ProgressKind::Started { total_steps: 1 }).await;
    push_task_progress(
        &task_ctx,
        ProgressKind::StepBegin {
            step: 1,
            message: "安装 QQ 系统依赖".into(),
        },
    )
    .await;
    log(
        &task_ctx,
        ProgressLogLevel::Info,
        if packages.is_empty() {
            "安装 QQ 系统依赖: 未指定包列表".to_string()
        } else {
            format!("安装 QQ 系统依赖: {}", packages.join(", "))
        },
    )
    .await;

    let result = run_qq_installer(host, packages, sudo_password, &task_ctx)
        .await
        .map_err(|e| e.to_string())?;

    let ok = result.success && !result.elevation_required;
    if ok {
        log(
            &task_ctx,
            ProgressLogLevel::Info,
            format!("QQ 系统依赖安装成功: {}", result.installed.join(", ")),
        )
        .await;
    }
    finish(&task_ctx, ok).await;
    Ok(result)
}

pub fn qq_install_failure_message(result: &InstallDependenciesResult) -> String {
    let failed = result
        .failed
        .iter()
        .map(|f| format!("{}: {}", f.name, f.reason))
        .collect::<Vec<_>>()
        .join(", ");
    if failed.is_empty() {
        "QQ 系统依赖安装失败".to_string()
    } else {
        format!("部分 QQ 系统依赖安装失败: {failed}")
    }
}

/// 把 installer 的 ActionCtx 进度转发到 task
async fn run_qq_installer(
    host: &dyn Host,
    packages: Vec<String>,
    sudo_password: Option<String>,
    task_ctx: &DeploymentTaskContext,
) -> Result<InstallDependenciesResult, ncd_component::ActionError> {
    let (mut ctx, mut rx) = ActionCtx::new();
    let forward_to = task_ctx.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            forward_to.push_progress(event).await;
        }
    });
    QqDependencyInstaller
        .install(host, packages, sudo_password.as_deref(), &mut ctx)
        .await
}
