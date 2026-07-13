//! Docker 部署进度：会话日志 + 事件总线

use ncd_component::{ProgressEvent, ProgressKind, ProgressLogLevel};
use ncd_runtime::{BroadcastEventBus, DeploymentTaskContext, DomainEvent, EventBus};
use tracing::{error, info, warn};

/// 部署进度写入 Desktop 会话日志(设置页)不记录 docker pull 逐行 stdout,避免刷屏
pub(crate) fn session_log_deploy_progress(
    kind: &ProgressKind,
    host_id: &str,
    container: &str,
    flavor: &str,
) {
    match kind {
        ProgressKind::Started { total_steps } => {
            info!(
                target: "ncd_tauri::docker",
                host_id,
                container,
                flavor,
                total_steps,
                "开始拉取 Docker 框架镜像"
            );
        }
        ProgressKind::StepBegin { step, message } => {
            info!(
                target: "ncd_tauri::docker",
                host_id,
                container,
                step,
                message = %message,
                "Docker 部署步骤"
            );
        }
        ProgressKind::StepEnd { step, ok } => {
            if *ok {
                info!(
                    target: "ncd_tauri::docker",
                    host_id,
                    container,
                    step,
                    "Docker 部署步骤完成"
                );
            } else {
                warn!(
                    target: "ncd_tauri::docker",
                    host_id,
                    container,
                    step,
                    "Docker 部署步骤失败"
                );
            }
        }
        ProgressKind::Finished { ok } => {
            if *ok {
                // 成功详情见同次拉取末尾的「镜像拉取完成」日志(含实际源与耗时)
            } else {
                error!(
                    target: "ncd_tauri::docker",
                    host_id,
                    container,
                    flavor,
                    "Docker 框架镜像拉取失败"
                );
            }
        }
        ProgressKind::Log { level, message } => match level {
            ProgressLogLevel::Error => {
                error!(
                    target: "ncd_tauri::docker",
                    host_id,
                    container,
                    msg = %message,
                    "Docker 部署"
                );
            }
            ProgressLogLevel::Warn => {
                warn!(
                    target: "ncd_tauri::docker",
                    host_id,
                    container,
                    msg = %message,
                    "Docker 部署"
                );
            }
            ProgressLogLevel::Info
                if message.starts_with("拉取镜像:")
                    || message.starts_with("上一个源失败")
                    || message.starts_with("镜像拉取完成") =>
            {
                info!(
                    target: "ncd_tauri::docker",
                    host_id,
                    container,
                    msg = %message,
                    "Docker 部署"
                );
            }
            ProgressLogLevel::Info => {}
            _ => {}
        },
        _ => {}
    }
}

pub(crate) fn publish_docker_deploy_progress(
    event_bus: &BroadcastEventBus,
    task_ctx: &Option<DeploymentTaskContext>,
    task_id: String,
    event: ProgressEvent,
) {
    if let Some(ctx) = task_ctx.clone() {
        let event_for_task = event.clone();
        tauri::async_runtime::spawn(async move {
            ctx.push_progress(event_for_task).await;
        });
    }
    event_bus.publish(DomainEvent::docker_deploy_progress(task_id, event));
}

pub(crate) fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
