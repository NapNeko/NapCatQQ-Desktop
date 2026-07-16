//! Docker 镜像拉取 deploy

use std::sync::Arc;

use ncd_component::{ProgressEvent, ProgressKind, ProgressLogLevel};
use ncd_deploy::docker::{DockerCli, PullProgress, classify_pull_failure};
use ncd_domain::{
    DeployedContainer, DeploymentTaskKind, DeploymentTaskResource, DockerImageReady, DockerPullSpec,
};
use ncd_host::{Host, StreamSource};
use ncd_runtime::{
    BroadcastEventBus, DeploymentTaskContext, DeploymentTaskRequest, DeploymentTaskRunResult,
};
use tauri::State;
use tokio::sync::oneshot;
use tracing::info;

use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

use super::progress::{now_epoch_ms, publish_docker_deploy_progress, session_log_deploy_progress};

#[tauri::command]
pub async fn docker_deploy(
    host_id: String,
    spec: DockerPullSpec,
    task_id: String,
    state: State<'_, AppState>,
) -> Result<DeployedContainer, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let (tx, rx) = oneshot::channel::<Result<DeployedContainer, String>>();
    let event_bus = state.event_bus.clone();
    let deployment_tasks = state.deployment_tasks.clone();
    let flavor = spec.flavor;
    let requested_task_id = task_id.clone();
    let submitted = deployment_tasks
        .submit(DeploymentTaskRequest {
            task_id: task_id.clone(),
            kind: DeploymentTaskKind::DockerImagePull { flavor },
            host_id: host_id.clone(),
            title: format!("拉取 {} 镜像", flavor.as_str()),
            resources: vec![
                DeploymentTaskResource::DockerDaemon {
                    host_id: host_id.clone(),
                },
                DeploymentTaskResource::DockerImage {
                    host_id: host_id.clone(),
                    flavor,
                },
                DeploymentTaskResource::GlobalDownloadSlot,
            ],
            depends_on: vec![],
            dedupe_key: Some(format!("docker-pull:{host_id}:{}", flavor.as_str())),
            // 可取消：停止会 cancel token 并杀掉 docker pull 子进程
            cancellable: true,
            runner: Box::new(move |task_ctx| {
                Box::pin(async move {
                    let result = docker_deploy_execute(
                        host_id,
                        spec,
                        task_id,
                        host,
                        event_bus,
                        Some(task_ctx),
                    )
                    .await;
                    // finish() 会看 cancel token 把状态标成 Cancelled,这里不必按文案分支
                    let run_result = match &result {
                        Ok(_) => DeploymentTaskRunResult::ok("镜像已就绪"),
                        Err(err) => DeploymentTaskRunResult::failed(err.clone()),
                    };
                    let _ = tx.send(result);
                    run_result
                })
            }),
        })
        .await;

    if submitted != requested_task_id {
        return Err("该主机正在拉取此框架镜像，请在任务队列查看进度".to_string());
    }

    rx.await
        .map_err(|_| "Docker 镜像拉取任务异常结束".to_string())?
}

async fn docker_deploy_execute(
    host_id: String,
    spec: DockerPullSpec,
    task_id: String,
    host: Arc<dyn Host>,
    event_bus: BroadcastEventBus,
    task_ctx: Option<DeploymentTaskContext>,
) -> Result<DeployedContainer, String> {
    let host_ref: &dyn Host = host.as_ref();

    let flavor_label = format!("{:?}", spec.flavor);
    let log_label = spec.flavor.as_str().to_string();

    let event_bus_for_emit = event_bus.clone();
    let tid = task_id.clone();
    let host_id_log = host_id.clone();
    let container_log = log_label.clone();
    let flavor_log = flavor_label.clone();
    let task_ctx_for_emit = task_ctx.clone();
    let emit = move |kind: ProgressKind| {
        session_log_deploy_progress(&kind, &host_id_log, &container_log, &flavor_log);
        publish_docker_deploy_progress(
            &event_bus_for_emit,
            &task_ctx_for_emit,
            tid.clone(),
            ProgressEvent::new(kind),
        );
    };

    emit(ProgressKind::Started { total_steps: 2 });

    emit(ProgressKind::StepBegin {
        step: 1,
        message: "探测 docker 状态...".to_string(),
    });
    let cli = DockerCli::new(host_ref);
    let status = cli.probe().await;
    if !status.ready_to_deploy() {
        let msg = format!(
            "docker 未就绪（installed={} daemon={} compose={}），请先安装/启动 docker",
            status.installed, status.daemon_running, status.compose_available
        );
        emit(ProgressKind::Log {
            level: ProgressLogLevel::Error,
            message: msg.clone(),
        });
        emit(ProgressKind::Finished { ok: false });
        return Err(msg);
    }
    emit(ProgressKind::StepEnd { step: 1, ok: true });

    emit(ProgressKind::StepBegin {
        step: 2,
        message: if spec.flavor == ncd_domain::DockerFlavor::SnowLuma {
            format!(
                "拉取镜像（按层显示进度，压缩包约 {} MB）…",
                ncd_domain::DockerFlavor::SNOWLUMA_COMPRESSED_MB_APPROX
            )
        } else {
            "拉取镜像（按层显示进度）...".to_string()
        },
    });
    {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::Instant;

        let pull_wall_start = Instant::now();

        let event_bus_pull = event_bus.clone();
        let tid_pull = task_id.clone();
        let task_ctx_pull = task_ctx.clone();

        let candidates = spec.resolve_candidates();
        let official = spec.flavor.default_image();
        let candidate_count = candidates.len();
        let cancel = task_ctx.as_ref().map(|c| c.cancel_token());
        if let Some(m) = spec
            .mirror
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            emit(ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: format!("镜像源策略：{m}"),
            });
        } else {
            emit(ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: format!(
                    "自动换源：优先国内镜像站；有进度可拉最长约 {} 分钟，连续 {} 分钟无输出才换源",
                    ncd_deploy::docker::DockerCli::PULL_PER_CANDIDATE_TIMEOUT.as_secs() / 60,
                    ncd_deploy::docker::DockerCli::PULL_STALL_TIMEOUT.as_secs() / 60
                ),
            });
        }

        let last_activity_ms = Arc::new(AtomicU64::new(now_epoch_ms()));
        let heartbeat_stop = Arc::new(AtomicBool::new(false));
        let hb_activity = Arc::clone(&last_activity_ms);
        let hb_stop = Arc::clone(&heartbeat_stop);
        let hb_bus = event_bus_pull.clone();
        let hb_tid = tid_pull.clone();
        let hb_task_ctx = task_ctx_pull.clone();
        let heartbeat = tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(45);
            loop {
                tokio::time::sleep(interval).await;
                if hb_stop.load(Ordering::Relaxed) {
                    break;
                }
                let idle_ms = now_epoch_ms().saturating_sub(hb_activity.load(Ordering::Relaxed));
                if idle_ms >= 45_000 {
                    publish_docker_deploy_progress(
                        &hb_bus,
                        &hb_task_ctx,
                        hb_tid.clone(),
                        ProgressEvent::new(ProgressKind::Log {
                            level: ProgressLogLevel::Info,
                            message: format!(
                                "仍在拉取，已约 {} 秒无新输出（大镜像或网络慢时正常；若长期为 0 层，可能是镜像站连不上）",
                                idle_ms / 1000
                            ),
                        }),
                    );
                }
            }
        });

        let activity_for_cb = Arc::clone(&last_activity_ms);
        let bus_for_lines = event_bus_pull.clone();
        let tid_for_lines = tid_pull.clone();
        let task_ctx_for_lines = task_ctx_pull.clone();
        let new_line_cb = move |idx: usize, image: &str| {
            if idx == 0 {
                publish_docker_deploy_progress(
                    &bus_for_lines,
                    &task_ctx_for_lines,
                    tid_for_lines.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Info,
                        message: format!("拉取镜像: {image}"),
                    }),
                );
            } else {
                publish_docker_deploy_progress(
                    &bus_for_lines,
                    &task_ctx_for_lines,
                    tid_for_lines.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Warn,
                        message: format!("上一个源失败，改用镜像源重试: {image}"),
                    }),
                );
            }

            let pull_state = Arc::new(Mutex::new(PullProgress::new()));
            let pull_state_cb = Arc::clone(&pull_state);
            let event_bus_line = bus_for_lines.clone();
            let tid_line = tid_for_lines.clone();
            let task_ctx_line = task_ctx_for_lines.clone();
            let activity_line = Arc::clone(&activity_for_cb);

            move |src: StreamSource, line: String| {
                if src == StreamSource::Stdout || src == StreamSource::Stderr {
                    activity_line.store(now_epoch_ms(), Ordering::Relaxed);
                    // 进度回调单线程写；若锁被 poison 仍取内层继续更新 UI
                    let mut ps = pull_state_cb
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    ps.update(&line);
                    let (_completed, _total, msg, percent) = ps.summary();
                    let layers = ps.layer_snapshots();
                    publish_docker_deploy_progress(
                        &event_bus_line,
                        &task_ctx_line,
                        tid_line.clone(),
                        ProgressEvent::new(ProgressKind::StepProgress {
                            step: 2,
                            percent,
                            message: msg,
                            speed_bps: None,
                            downloaded_bytes: None,
                            total_bytes: None,
                            download_stage: None,
                            docker_layers: Some(layers),
                        }),
                    );
                }
            }
        };

        let event_bus_fail = event_bus_pull;
        let tid_fail = tid_pull;
        let task_ctx_fail = task_ctx_pull;
        let on_mirror_fail =
            move |idx: usize, image: &str, err: &ncd_deploy::docker::DockerCliError| {
                let (_kind, detail) = classify_pull_failure(err);
                let line = if detail.len() > 220 {
                    format!("{}…", &detail[..220])
                } else {
                    detail
                };
                publish_docker_deploy_progress(
                    &event_bus_fail,
                    &task_ctx_fail,
                    tid_fail.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Warn,
                        message: format!(
                            "源 {}/{} 失败（{}）：{}",
                            idx + 1,
                            candidate_count,
                            image,
                            line
                        ),
                    }),
                );
            };

        let pull_result = cli
            .pull_with_fallback_cancel(
                &candidates,
                official,
                cancel.clone(),
                new_line_cb,
                Some(on_mirror_fail),
            )
            .await;

        heartbeat_stop.store(true, Ordering::Relaxed);
        let _ = heartbeat.await;

        match pull_result {
            Ok(pulled_ref) => {
                let secs = pull_wall_start.elapsed().as_secs();
                let done_msg =
                    format!("镜像拉取完成：{pulled_ref}（耗时 {secs} 秒；已 tag 为 {official}）");
                emit(ProgressKind::Log {
                    level: ProgressLogLevel::Info,
                    message: done_msg.clone(),
                });
                info!(
                    target: "ncd_tauri::docker",
                    host_id = %host_id,
                    flavor = %flavor_label,
                    pulled = %pulled_ref,
                    official = %official,
                    elapsed_secs = secs,
                    "Docker 框架镜像拉取成功"
                );
            }
            Err(e) => {
                if matches!(
                    &e,
                    ncd_deploy::docker::DockerCliError::Host(ncd_host::HostError::Cancelled)
                ) || cancel.as_ref().is_some_and(|c| c.is_cancelled())
                {
                    let msg = "已取消".to_string();
                    emit(ProgressKind::Log {
                        level: ProgressLogLevel::Warn,
                        message: msg.clone(),
                    });
                    emit(ProgressKind::Finished { ok: false });
                    return Err(msg);
                }
                let (_kind, user_msg) = classify_pull_failure(&e);
                let msg = format!("{}（已尝试 {} 个镜像源）", user_msg, candidate_count);
                emit(ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message: msg.clone(),
                });
                emit(ProgressKind::Finished { ok: false });
                return Err(msg);
            }
        }
    }
    emit(ProgressKind::StepEnd { step: 2, ok: true });
    emit(ProgressKind::Finished { ok: true });

    Ok(DockerImageReady {
        flavor: spec.flavor,
        image: spec.flavor.default_image().to_string(),
    })
}
