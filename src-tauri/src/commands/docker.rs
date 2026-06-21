//! Docker 页 Tauri 命令薄壳层
//!
//! 暴露给前端的命令:
//! - docker_probe:探测某主机的 docker 状态
//! - docker_install:缺 docker 时帮装(Linux 脚本 / Windows 引导)
//! - docker_list_containers:列已有容器
//! - docker_list_images:列本地镜像
//! - docker_remove_image:删除本地镜像
//! - docker_container_action:对单个容器 start/stop/restart/remove
//! - docker_logs:取容器最近日志
//! - docker_deploy:在目标主机拉取 NapCat/SnowLuma 镜像(不创建容器;Bot 启动时再起)
//! - docker_compose_down:停并清理一个 compose 部署
//!
//! 所有命令走 host_resolve 选主机(local 或 remote:<id>),错误统一转 String
//! 业务编排尽量薄;真正的 docker 操作在 ncd_deploy::docker::DockerCli

use ncd_component::{ProgressEvent, ProgressKind, ProgressLogLevel};
use ncd_deploy::docker::{
    classify_pull_failure, install_docker_with_progress, progress_event, DockerCli, PullProgress,
};
use ncd_domain::{
    ContainerAction, ContainerInfo, DeployedContainer, DockerFlavor, DockerImageReady,
    DockerInstallReport, DockerInstallStatus, DockerPullSpec, DockerStatus, ImageInfo,
};
use ncd_host::{Host, HostCommand, HostPath, StreamSource};
use ncd_runtime::{DomainEvent, EventBus};
use tauri::State;
use tracing::{error, info, warn};

use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

/// 部署进度写入 Desktop 会话日志(设置页)不记录 docker pull 逐行 stdout,避免刷屏
fn session_log_deploy_progress(
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
            ProgressLogLevel::Info => {
                if message.starts_with("拉取镜像:")
                    || message.starts_with("上一个源失败")
                    || message.starts_with("镜像拉取完成")
                {
                    info!(
                        target: "ncd_tauri::docker",
                        host_id,
                        container,
                        msg = %message,
                        "Docker 部署"
                    );
                }
            }
            _ => {}
        },
        _ => {}
    }
}

#[tauri::command]
pub async fn docker_probe(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<DockerStatus, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    Ok(DockerCli::new(host.as_ref()).probe().await)
}

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
    // 检查主机类型(本地不支持 Docker 安装,由前端过滤)
    let server_id = host_id.strip_prefix("remote:");
    let effective_password = sudo_password.clone().or_else(|| {
        server_id.and_then(|id| state.server_manager.sudo_password(id))
    });

    info!(
        target: "ncd_tauri::docker",
        host_id = %host_id,
        task_id = %task_id,
        "开始安装 Docker（远端 Linux 将执行仓库配置与 apt/dnf 安装，约 3–10 分钟）"
    );

    // 获取包管理器锁,防止同一主机的 apt/dnf 并发冲突
    let _pkg_lock = state.package_lock.acquire(&host_id).await;

    let event_bus = state.event_bus.clone();
    let tid = task_id.clone();
    let emit = std::sync::Arc::new(move |kind: ProgressKind| {
        event_bus.publish(DomainEvent::docker_install_progress(
            tid.clone(),
            progress_event(kind),
        ));
    });

    let ssh_user = if let Some(id) = server_id {
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

    // Docker 安装是长操作,使用隔离连接避免污染缓存连接
    let report = if let Some(id) = server_id {
        let effective_password_clone = effective_password.clone();
        let ssh_user_clone = ssh_user.clone();
        let emit_clone = emit.clone();

        state.server_manager.with_isolated_connection(id, move |host| {
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
        }).await?
    } else {
        // 本地主机用普通连接
        let host = resolve_host_with_autoconnect(&host_id, &state).await?;
        install_docker_with_progress(
            host.as_ref(),
            effective_password.as_deref(),
            ssh_user.as_deref(),
            emit,
        )
        .await
        .map_err(|e| format!("Docker 安装失败: {e}"))?
    };

    match report.status {
        DockerInstallStatus::Installed | DockerInstallStatus::AlreadyInstalled => {
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

    // 用户勾了"记住密码"就存,只要这次密码被验证有效判据是 status != NeedSudoPassword:
    // 能走过提权脚本(没返回 NeedSudoPassword)就说明 sudo 密码是对的——密码有效性
    // 与 docker daemon 起没起来是两回事早先用 == Installed 太严:脚本跑通但 daemon
    // 没立刻就绪会返回 ManualRequired,导致有效密码没被存下,下次安装又弹框
    // 只存用户这次亲手输入的(sudo_password),不把 keyring 里已有的回写一遍
    if report.status != DockerInstallStatus::NeedSudoPassword && remember_sudo == Some(true) {
        if let (Some(id), Some(pw)) = (server_id, sudo_password.as_deref()) {
            let _ = state.server_manager.remember_sudo_password(id, pw);
        }
    }

    Ok(report)
}

#[tauri::command]
pub async fn docker_list_containers(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ContainerInfo>, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    // 先 probe 定夺提权:远端没进 docker 组时裸 docker 会 permission denied,
    // ensure_daemon_ready 探一次让后续命令一致地走 sudo
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.list_containers()
        .await
        .map_err(|e| format!("列容器失败: {e}"))
}

#[tauri::command]
pub async fn docker_list_images(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ImageInfo>, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.list_images()
        .await
        .map_err(|e| format!("列镜像失败: {e}"))
}

#[tauri::command]
pub async fn docker_remove_image(
    host_id: String,
    image_ref: String,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    let force = force.unwrap_or(false);
    cli.remove_image(image_ref.trim(), force)
        .await
        .map_err(|e| {
            error!(
                target: "ncd_tauri::docker",
                host_id = %host_id,
                image_ref = %image_ref,
                force,
                err = %e,
                "Docker 删除镜像失败"
            );
            format!("删除镜像失败: {e}")
        })
}

#[tauri::command]
pub async fn docker_container_action(
    host_id: String,
    name: String,
    action: ContainerAction,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    let result = match action {
        ContainerAction::Start => cli.lifecycle("start", &name).await,
        ContainerAction::Stop => cli.lifecycle("stop", &name).await,
        ContainerAction::Restart => cli.lifecycle("restart", &name).await,
        ContainerAction::Remove => cli.remove(&name).await,
    };
    result.map_err(|e| {
        error!(
            target: "ncd_tauri::docker",
            host_id = %host_id,
            container = %name,
            action = action.as_str(),
            err = %e,
            "Docker 容器操作失败"
        );
        format!("{} 失败: {e}", action.as_str())
    })
}

#[tauri::command]
pub async fn docker_logs(
    host_id: String,
    name: String,
    tail: u32,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.logs(&name, tail.min(2000))
        .await
        .map_err(|e| format!("取日志失败: {e}"))
}

#[tauri::command]
pub async fn docker_image_ready_for_flavor(
    host_id: String,
    flavor: DockerFlavor,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    let image = flavor.default_image();
    cli.image_exists(image)
        .await
        .map_err(|e| format!("探测镜像失败: {e}"))
}

#[tauri::command]
pub async fn docker_deploy(
    host_id: String,
    spec: DockerPullSpec,
    task_id: String,
    state: State<'_, AppState>,
) -> Result<DeployedContainer, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let host_ref: &dyn Host = host.as_ref();

    let flavor_label = format!("{:?}", spec.flavor);
    let log_label = spec.flavor.as_str().to_string();

    let event_bus = state.event_bus.clone();
    let tid = task_id.clone();
    let host_id_log = host_id.clone();
    let container_log = log_label.clone();
    let flavor_log = flavor_label.clone();
    let emit = move |kind: ProgressKind| {
        session_log_deploy_progress(&kind, &host_id_log, &container_log, &flavor_log);
        event_bus.publish(DomainEvent::docker_deploy_progress(
            tid.clone(),
            ProgressEvent::new(kind),
        ));
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

        let event_bus_pull = state.event_bus.clone();
        let tid_pull = task_id.clone();

        let candidates = spec.flavor.pull_candidates();
        let official = spec.flavor.default_image();
        let candidate_count = candidates.len();

        let last_activity_ms = Arc::new(AtomicU64::new(now_epoch_ms()));
        let heartbeat_stop = Arc::new(AtomicBool::new(false));
        let hb_activity = Arc::clone(&last_activity_ms);
        let hb_stop = Arc::clone(&heartbeat_stop);
        let hb_bus = event_bus_pull.clone();
        let hb_tid = tid_pull.clone();
        let heartbeat = tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(45);
            loop {
                tokio::time::sleep(interval).await;
                if hb_stop.load(Ordering::Relaxed) {
                    break;
                }
                let idle_ms = now_epoch_ms().saturating_sub(hb_activity.load(Ordering::Relaxed));
                if idle_ms >= 45_000 {
                    hb_bus.publish(DomainEvent::docker_deploy_progress(
                        hb_tid.clone(),
                        ProgressEvent::new(ProgressKind::Log {
                            level: ProgressLogLevel::Info,
                            message: format!(
                                "仍在拉取，已约 {} 秒无新输出（大镜像或网络慢时正常；若长期为 0 层，可能是镜像站连不上）",
                                idle_ms / 1000
                            ),
                        }),
                    ));
                }
            }
        });

        let activity_for_cb = Arc::clone(&last_activity_ms);
        let bus_for_lines = event_bus_pull.clone();
        let tid_for_lines = tid_pull.clone();
        let new_line_cb = move |idx: usize, image: &str| {
            if idx == 0 {
                bus_for_lines.publish(DomainEvent::docker_deploy_progress(
                    tid_for_lines.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Info,
                        message: format!("拉取镜像: {image}"),
                    }),
                ));
            } else {
                bus_for_lines.publish(DomainEvent::docker_deploy_progress(
                    tid_for_lines.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Warn,
                        message: format!("上一个源失败，改用镜像源重试: {image}"),
                    }),
                ));
            }

            let pull_state = Arc::new(Mutex::new(PullProgress::new()));
            let pull_state_cb = Arc::clone(&pull_state);
            let event_bus_line = bus_for_lines.clone();
            let tid_line = tid_for_lines.clone();
            let activity_line = Arc::clone(&activity_for_cb);

            move |src: StreamSource, line: String| {
                if src == StreamSource::Stdout || src == StreamSource::Stderr {
                    activity_line.store(now_epoch_ms(), Ordering::Relaxed);
                    let mut ps = pull_state_cb.lock().unwrap();
                    ps.update(&line);
                    let (_completed, _total, msg, percent) = ps.summary();
                    let layers = ps.layer_snapshots();
                    event_bus_line.publish(DomainEvent::docker_deploy_progress(
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
                    ));
                }
            }
        };

        let event_bus_fail = event_bus_pull;
        let tid_fail = tid_pull;
        let on_mirror_fail = move |idx: usize, image: &str, err: &ncd_deploy::docker::DockerCliError| {
            let (_kind, detail) = classify_pull_failure(err);
            let line = if detail.len() > 220 {
                format!("{}…", &detail[..220])
            } else {
                detail
            };
            event_bus_fail.publish(DomainEvent::docker_deploy_progress(
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
            ));
        };

        let pull_result = cli
            .pull_with_fallback(&candidates, official, new_line_cb, Some(on_mirror_fail))
            .await;

        heartbeat_stop.store(true, Ordering::Relaxed);
        let _ = heartbeat.await;

        match pull_result {
            Ok(pulled_ref) => {
                let secs = pull_wall_start.elapsed().as_secs();
                let done_msg = format!(
                    "镜像拉取完成：{pulled_ref}（耗时 {secs} 秒；已 tag 为 {official}）"
                );
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
                let (_kind, user_msg) = classify_pull_failure(&e);
                let msg = format!(
                    "{}（已尝试 {} 个镜像源）",
                    user_msg,
                    candidate_count
                );
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

#[tauri::command]
pub async fn docker_compose_down(
    host_id: String,
    name: String,
    remove_volumes: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let host_ref: &dyn Host = host.as_ref();
    let project_dir = resolve_project_dir(&host_id, host_ref, &state, &name).await?;
    let cli = DockerCli::new(host_ref);
    // compose down 要用 compose 插件,走 ensure_ready(probe 定夺提权 + 要求 compose)
    cli.ensure_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.compose_down(&project_dir, remove_volumes)
        .await
        .map_err(|e| {
            error!(
                target: "ncd_tauri::docker",
                host_id = %host_id,
                name = %name,
                err = %e,
                "Docker compose down 失败"
            );
            format!("停止部署失败: {e}")
        })
}

/// 解析 compose project 目录(放 docker-compose.yml 的地方)
/// 本机:<data_root>/docker/<name>(POSIX 化路径,LocalWindowsHost 内部转盘符)
/// 远端:<$HOME>/.napcat-docker/<name>;探不到 $HOME 时返回错误(不回退 /root)
async fn resolve_project_dir(
    host_id: &str,
    host: &dyn Host,
    state: &AppState,
    name: &str,
) -> Result<String, String> {
    if host_id == "local" {
        let base = state.data_root.join("docker").join(name);
        // HostPath::from_windows 把 C:\... 规范成 /c/...,LocalWindowsHost 能还原
        return Ok(HostPath::from_windows(&base.to_string_lossy()).as_posix().to_string());
    }
    // 远端:探 $HOME探不到就 fail-fast,不回退 /root——路径落盘红线:宁可报错让
    // 用户处理,也不把生产数据静默落到错误目录(/root 通常无权限,或污染 root 家目录)
    let home = probe_remote_home(host).await.ok_or_else(|| {
        "无法探测远端 $HOME,已拒绝回退到 /root 部署 Docker(避免把数据落到错误目录)。\
         请确认远端 SSH 用户有正常的家目录后重试。"
            .to_string()
    })?;
    Ok(format!("{home}/.napcat-docker/{name}"))
}

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 远端探 $HOME失败返回 None;调用方须 fail-fast,不得回退 /root
async fn probe_remote_home(host: &dyn Host) -> Option<String> {
    let cmd = HostCommand::new("sh").arg("-c").arg("echo $HOME");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let h = out.stdout.trim().to_string();
            if h.is_empty() { None } else { Some(h) }
        }
        _ => None,
    }
}
