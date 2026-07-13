//! Docker 安装 command

use ncd_component::ProgressKind;
use ncd_deploy::docker::{install_docker_with_progress, progress_event};
use ncd_domain::{
    DeploymentTaskKind, DeploymentTaskResource, DockerInstallReport, DockerInstallStatus,
};
use ncd_runtime::{DeploymentTaskRequest, DeploymentTaskRunResult, DomainEvent, EventBus};
use tauri::State;
use tokio::sync::oneshot;
use tracing::{info, warn};

use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

/// 安装 docker返回结构化 report 让前端按 status 分流:installed/alreadyInstalled
/// 弹绿条;needSudoPassword 弹密码输入框;manualRequired 弹红条带手动指引
///
/// sudo_password:前端弹框收集到的 sudo 密码None 时后端自动从 keyring 找该服务器
/// 的缓存密码(密码登录机器有,或密码登录后转密钥登录时保留下来的)两边都没有
/// 且远端确实需要密码时,返回 needSudoPassword 让前端弹框
/// remember_sudo:用户在弹框勾了"记住密码"仅当本次显式传了 sudo_password 且安装
/// 成功时,才把它写进 keyring(sudo 槽)
#[tauri::command]
pub async fn docker_install(
    host_id: String,
    task_id: String,
    sudo_password: Option<String>,
    remember_sudo: Option<bool>,
    state: State<'_, AppState>,
) -> Result<DockerInstallReport, String> {
    let server_id = host_id.strip_prefix("remote:").map(str::to_string);
    let effective_password = sudo_password.clone().or_else(|| {
        server_id
            .as_deref()
            .and_then(|id| state.server_manager.sudo_password(id))
    });
    let ssh_user = if let Some(id) = server_id.as_deref() {
        state
            .server_manager
            .list_servers()
            .await
            .into_iter()
            .find(|p| p.id == id)
            .map(|p| p.username)
    } else {
        None
    };
    let local_host = if server_id.is_none() {
        Some(resolve_host_with_autoconnect(&host_id, &state).await?)
    } else {
        None
    };

    let (tx, rx) = oneshot::channel::<Result<DockerInstallReport, String>>();
    let event_bus = state.event_bus.clone();
    let server_manager = std::sync::Arc::clone(&state.server_manager);
    let deployment_tasks = state.deployment_tasks.clone();
    let requested_task_id = task_id.clone();
    let resources = vec![
        DeploymentTaskResource::PackageManager {
            host_id: host_id.clone(),
        },
        DeploymentTaskResource::DockerCapability {
            host_id: host_id.clone(),
        },
        DeploymentTaskResource::DockerDaemon {
            host_id: host_id.clone(),
        },
    ];
    let submitted = deployment_tasks
        .submit(DeploymentTaskRequest {
            task_id: task_id.clone(),
            kind: DeploymentTaskKind::DockerInstall,
            host_id: host_id.clone(),
            title: "Docker 安装".to_string(),
            resources,
            depends_on: vec![],
            dedupe_key: Some(format!("docker-install:{host_id}")),
            cancellable: false,
            runner: Box::new(move |task_ctx| {
                Box::pin(async move {
                    info!(
                        target: "ncd_tauri::docker",
                        host_id = %host_id,
                        task_id = %task_id,
                        "开始安装 Docker（远端 Linux 将执行仓库配置与 apt/dnf 安装，约 3–10 分钟）"
                    );

                    let tid = task_id.clone();
                    let task_ctx_for_emit = task_ctx.clone();
                    let event_bus_for_emit = event_bus.clone();
                    let emit = std::sync::Arc::new(move |kind: ProgressKind| {
                        let event = progress_event(kind);
                        let task_ctx = task_ctx_for_emit.clone();
                        let event_for_task = event.clone();
                        tauri::async_runtime::spawn(async move {
                            task_ctx.push_progress(event_for_task).await;
                        });
                        event_bus_for_emit
                            .publish(DomainEvent::docker_install_progress(tid.clone(), event));
                    });

                    let result = if let Some(id) = server_id.as_deref() {
                        let effective_password_clone = effective_password.clone();
                        let ssh_user_clone = ssh_user.clone();
                        let emit_clone = emit.clone();
                        server_manager
                            .with_isolated_connection(id, move |host| {
                                Box::pin(async move {
                                    install_docker_with_progress(
                                        host.as_ref(),
                                        effective_password_clone.as_deref(),
                                        ssh_user_clone.as_deref(),
                                        emit_clone,
                                    )
                                    .await
                                    .map_err(|e| format!("Docker 安装失败: {e}"))
                                })
                            })
                            .await
                    } else if let Some(host) = local_host {
                        install_docker_with_progress(
                            host.as_ref(),
                            effective_password.as_deref(),
                            ssh_user.as_deref(),
                            emit,
                        )
                        .await
                        .map_err(|e| format!("Docker 安装失败: {e}"))
                    } else {
                        Err("无法解析 Docker 安装目标主机".to_string())
                    };

                    if let Ok(report) = &result {
                        match report.status {
                            DockerInstallStatus::Installed
                            | DockerInstallStatus::AlreadyInstalled => {
                                info!(
                                    target: "ncd_tauri::docker",
                                    host_id = %host_id,
                                    status = ?report.status,
                                    "Docker 安装完成"
                                );
                            }
                            DockerInstallStatus::NeedSudoPassword => {
                                warn!(
                                    target: "ncd_tauri::docker",
                                    host_id = %host_id,
                                    msg = %report.message,
                                    "Docker 安装需要 sudo 密码"
                                );
                            }
                            DockerInstallStatus::ManualRequired => {
                                warn!(
                                    target: "ncd_tauri::docker",
                                    host_id = %host_id,
                                    msg = %report.message,
                                    "Docker 安装未达可部署状态"
                                );
                            }
                        }

                        if report.status != DockerInstallStatus::NeedSudoPassword
                            && remember_sudo == Some(true)
                        {
                            if let (Some(id), Some(pw)) =
                                (server_id.as_deref(), sudo_password.as_deref())
                            {
                                let _ = server_manager.remember_sudo_password(id, pw);
                            }
                        }
                    }

                    let run_result = match &result {
                        Ok(report) => {
                            let ok = matches!(
                                report.status,
                                DockerInstallStatus::Installed
                                    | DockerInstallStatus::AlreadyInstalled
                            );
                            if ok {
                                DeploymentTaskRunResult::ok(report.message.clone())
                            } else {
                                DeploymentTaskRunResult::failed(report.message.clone())
                            }
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
        return Err("该主机已有 Docker 安装任务在队列中".to_string());
    }

    rx.await
        .map_err(|_| "Docker 安装任务异常结束".to_string())?
}
