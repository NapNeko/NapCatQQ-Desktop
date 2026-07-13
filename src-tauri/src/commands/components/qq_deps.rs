//! QQ 系统依赖 detect / install 命令与任务 runner

use std::sync::Arc;

use ncd_component::{ProgressKind, ProgressLogLevel};
use ncd_domain::{
    DeploymentTaskKind, DeploymentTaskResource, InstallDependenciesResult, QqDependencyReport,
};
use ncd_host::{Host, Os};
use ncd_runtime::{DeploymentTaskContext, DeploymentTaskRequest, DeploymentTaskRunResult};
use tauri::State;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::progress::push_task_progress;
use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

pub(super) async fn install_qq_dependencies_task(
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
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: "检测 QQ 系统依赖".into(),
            },
        )
        .await;
        let manifest = ncd_component::qq_deps::qq_qqnt_dependencies_v3_2_25();
        let detector = ncd_component::qq_deps::QqDependencyDetector::new(manifest);
        match detector.detect(host, None).await {
            Ok(report) => report.missing.into_iter().map(|p| p.name).collect(),
            Err(err) => {
                let msg = format!("检测 QQ 系统依赖失败: {err}");
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
        }
    } else {
        packages
    };

    if packages.is_empty() {
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: "QQ 系统依赖已满足".into(),
            },
        )
        .await;
        push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: true }).await;
        push_task_progress(&task_ctx, ProgressKind::Finished { ok: true }).await;
        return DeploymentTaskRunResult::ok("QQ 系统依赖已满足");
    }

    push_task_progress(
        &task_ctx,
        ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: format!(
                "缺失 {} 个 QQ 系统依赖: {}",
                packages.len(),
                packages.join(", ")
            ),
        },
    )
    .await;

    let (mut ctx, mut rx) = ncd_component::ActionCtx::new();
    let task_ctx_for_progress = task_ctx.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(progress_event) = rx.recv().await {
            task_ctx_for_progress.push_progress(progress_event).await;
        }
    });

    let installer = ncd_component::qq_deps::QqDependencyInstaller;
    let result = installer
        .install(host, packages, sudo_password.as_deref(), &mut ctx)
        .await;

    match result {
        Ok(result) if result.elevation_required => {
            let msg = "安装 QQ 系统依赖需要 sudo 密码，请在远端主机配置中保存 sudo 密码后重试"
                .to_string();
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
        Ok(result) if result.success => {
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Info,
                    message: format!("QQ 系统依赖安装成功: {}", result.installed.join(", ")),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: true }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: true }).await;
            DeploymentTaskRunResult::ok(format!("已安装 {} 个 QQ 系统依赖", result.installed.len()))
        }
        Ok(result) => {
            let failed_list = result
                .failed
                .iter()
                .map(|f| format!("{}: {}", f.name, f.reason))
                .collect::<Vec<_>>()
                .join(", ");
            let msg = if failed_list.is_empty() {
                "QQ 系统依赖安装失败".to_string()
            } else {
                format!("部分 QQ 系统依赖安装失败: {failed_list}")
            };
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
            let msg = format!("安装 QQ 系统依赖失败: {err}");
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

async fn run_qq_dependency_install_for_command(
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
    push_task_progress(
        &task_ctx,
        ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: if packages.is_empty() {
                "安装 QQ 系统依赖: 未指定包列表".into()
            } else {
                format!("安装 QQ 系统依赖: {}", packages.join(", "))
            },
        },
    )
    .await;

    let (mut ctx, mut rx) = ncd_component::ActionCtx::new();
    let task_ctx_for_progress = task_ctx.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(progress_event) = rx.recv().await {
            task_ctx_for_progress.push_progress(progress_event).await;
        }
    });

    let installer = ncd_component::qq_deps::QqDependencyInstaller;
    let result = installer
        .install(host, packages, sudo_password.as_deref(), &mut ctx)
        .await
        .map_err(|e| e.to_string())?;

    let ok = result.success && !result.elevation_required;
    if ok {
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: format!("QQ 系统依赖安装成功: {}", result.installed.join(", ")),
            },
        )
        .await;
    }
    push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok }).await;
    push_task_progress(&task_ctx, ProgressKind::Finished { ok }).await;
    Ok(result)
}

/// 检测 QQ 系统依赖(仅 Linux 远端)
#[tauri::command]
pub async fn detect_qq_dependencies(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<QqDependencyReport, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;

    if host.os() != ncd_host::Os::Linux {
        return Err("QQ dependencies check is only supported on Linux".to_string());
    }

    let manifest = ncd_component::qq_deps::qq_qqnt_dependencies_v3_2_25();
    let detector = ncd_component::qq_deps::QqDependencyDetector::new(manifest);

    let report = detector
        .detect(host.as_ref(), None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(report)
}

/// 安装 QQ 系统依赖(仅 Linux 远端)
#[tauri::command]
pub async fn install_qq_dependencies(
    host_id: String,
    packages: Vec<String>,
    sudo_password: Option<String>,
    state: State<'_, AppState>,
) -> Result<InstallDependenciesResult, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;

    if host.os() != ncd_host::Os::Linux {
        return Err("QQ dependencies installation is only supported on Linux".to_string());
    }

    let effective_password = sudo_password.clone().or_else(|| {
        host_id
            .strip_prefix("remote:")
            .and_then(|id| state.server_manager.sudo_password(id))
    });

    tracing::info!(
        "[install_qq_dependencies] host={}, sudo_password={}, effective_password={}",
        host_id,
        sudo_password
            .as_ref()
            .map(|_| "<provided>")
            .unwrap_or("<none>"),
        effective_password
            .as_ref()
            .map(|_| "<resolved>")
            .unwrap_or("<none>")
    );

    let (tx, rx) = oneshot::channel::<Result<InstallDependenciesResult, String>>();
    let task_id = Uuid::new_v4().to_string();
    let requested_task_id = task_id.clone();
    let server_id = host_id.strip_prefix("remote:").map(str::to_string);
    let server_manager = Arc::clone(&state.server_manager);
    let local_host = if server_id.is_none() {
        Some(host)
    } else {
        None
    };
    let submitted = state
        .deployment_tasks
        .submit(DeploymentTaskRequest {
            task_id: task_id.clone(),
            kind: DeploymentTaskKind::SystemPackage {
                package_group: "qq_dependencies".to_string(),
            },
            host_id: host_id.clone(),
            title: "安装 QQ 系统依赖".to_string(),
            resources: vec![
                DeploymentTaskResource::PackageManager {
                    host_id: host_id.clone(),
                },
                DeploymentTaskResource::InstallTarget {
                    host_id: host_id.clone(),
                    target: "system_package:qq_dependencies".to_string(),
                },
            ],
            depends_on: vec![],
            dedupe_key: Some(format!("system-package:{host_id}:qq_dependencies")),
            cancellable: false,
            runner: Box::new(move |task_ctx| {
                Box::pin(async move {
                    let result = if let Some(id) = server_id {
                        let packages = packages.clone();
                        let effective_password = effective_password.clone();
                        server_manager
                            .with_isolated_connection(&id, move |iso_host| {
                                let task_ctx = task_ctx.clone();
                                Box::pin(async move {
                                    run_qq_dependency_install_for_command(
                                        iso_host.as_ref(),
                                        packages,
                                        effective_password,
                                        task_ctx,
                                    )
                                    .await
                                })
                            })
                            .await
                    } else if let Some(host) = local_host {
                        run_qq_dependency_install_for_command(
                            host.as_ref(),
                            packages,
                            effective_password,
                            task_ctx,
                        )
                        .await
                    } else {
                        Err("无法解析 QQ 依赖安装目标主机".to_string())
                    };

                    let run_result = match &result {
                        Ok(result) if result.success => {
                            DeploymentTaskRunResult::ok("QQ 系统依赖已就绪")
                        }
                        Ok(result) if result.elevation_required => {
                            DeploymentTaskRunResult::failed("安装 QQ 系统依赖需要 sudo 密码")
                        }
                        Ok(result) => {
                            let failed = result
                                .failed
                                .iter()
                                .map(|f| format!("{}: {}", f.name, f.reason))
                                .collect::<Vec<_>>()
                                .join(", ");
                            DeploymentTaskRunResult::failed(if failed.is_empty() {
                                "QQ 系统依赖安装失败".to_string()
                            } else {
                                format!("部分 QQ 系统依赖安装失败: {failed}")
                            })
                        }
                        Err(err) => DeploymentTaskRunResult::failed(err.clone()),
                    };
                    let _ = tx.send(result);
                    run_result
                })
            }),
        })
        .await;

    if submitted != requested_task_id {
        return Err("该主机已有 QQ 系统依赖安装任务在队列中".to_string());
    }

    rx.await
        .map_err(|_| "QQ 系统依赖安装任务异常结束".to_string())?
}

/// 记住远端服务器的 sudo 密码(用于提权操作)
#[tauri::command]
pub async fn remember_sudo_password(
    server_id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .server_manager
        .remember_sudo_password(&server_id, &password)?;
    Ok(())
}
