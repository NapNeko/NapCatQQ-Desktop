//! Docker 页 Tauri 命令薄壳层。
//!
//! 暴露给前端的命令:
//! - docker_probe:探测某主机的 docker 状态
//! - docker_install:缺 docker 时帮装(Linux 脚本 / Windows 引导)
//! - docker_list_containers:列已有容器
//! - docker_container_action:对单个容器 start/stop/restart/remove
//! - docker_logs:取容器最近日志
//! - docker_deploy:一键部署 NapCat/SnowLuma(生成凭据 + compose up + 回读地址)
//! - docker_compose_down:停并清理一个 compose 部署
//!
//! 所有命令走 host_resolve 选主机(local 或 remote:<id>),错误统一转 String。
//! 业务编排尽量薄;真正的 docker 操作在 ncd_deploy::docker::DockerCli。

use ncd_component::{ProgressEvent, ProgressKind, ProgressLogLevel};
use ncd_deploy::docker::{
    install_docker_with_progress, progress_event, render_compose, DockerCli, PullProgress,
};
use ncd_domain::{
    ContainerAction, ContainerInfo, DeployedContainer, DockerDeploySpec, DockerFlavor,
    DockerInstallReport, DockerInstallStatus, DockerStatus,
};
use ncd_host::{Host, HostCommand, HostPath, StreamSource};
use ncd_runtime::{DomainEvent, EventBus};
use tauri::State;
use tracing::{error, info, warn};

use crate::AppState;
use crate::commands::host_resolve::{host_display_address, resolve_host_with_autoconnect};

/// 部署进度写入 Desktop 会话日志（设置页）。不记录 docker pull 逐行 stdout，避免刷屏。
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
                "开始 Docker 一键部署"
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
                info!(
                    target: "ncd_tauri::docker",
                    host_id,
                    container,
                    flavor,
                    "Docker 一键部署成功"
                );
            } else {
                error!(
                    target: "ncd_tauri::docker",
                    host_id,
                    container,
                    flavor,
                    "Docker 一键部署失败"
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

/// 安装 docker。返回结构化 report 让前端按 status 分流:installed/alreadyInstalled
/// 弹绿条;needSudoPassword 弹密码输入框;manualRequired 弹红条带手动指引。
///
/// sudo_password:前端弹框收集到的 sudo 密码。None 时后端自动从 keyring 找该服务器
/// 的缓存密码(密码登录机器有,或密码登录后转密钥登录时保留下来的)。两边都没有
/// 且远端确实需要密码时,返回 needSudoPassword 让前端弹框。
/// remember_sudo:用户在弹框勾了"记住密码"。仅当本次显式传了 sudo_password 且安装
/// 成功时,才把它写进 keyring(sudo 槽)。
#[tauri::command]
pub async fn docker_install(
    host_id: String,
    task_id: String,
    sudo_password: Option<String>,
    remember_sudo: Option<bool>,
    state: State<'_, AppState>,
) -> Result<DockerInstallReport, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;

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

    // 获取包管理器锁，防止同一主机的 apt/dnf 并发冲突
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

    let report = install_docker_with_progress(
        host.as_ref(),
        effective_password.as_deref(),
        ssh_user.as_deref(),
        emit,
    )
    .await
    .map_err(|e| format!("Docker 安装失败: {e}"))?;

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

    // Docker 安装可能污染 SSH 会话环境（sudo 缓存、shell 状态等），
    // 主动断开连接，让下次使用时重新建立干净的 SSH 会话。
    if let Some(id) = server_id {
        state.server_manager.disconnect(id).await;
    }

    // 用户勾了"记住密码"就存,只要这次密码被验证有效。判据是 status != NeedSudoPassword:
    // 能走过提权脚本(没返回 NeedSudoPassword)就说明 sudo 密码是对的——密码有效性
    // 与 docker daemon 起没起来是两回事。早先用 == Installed 太严:脚本跑通但 daemon
    // 没立刻就绪会返回 ManualRequired,导致有效密码没被存下,下次安装又弹框。
    // 只存用户这次亲手输入的(sudo_password),不把 keyring 里已有的回写一遍。
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
    // ensure_daemon_ready 探一次让后续命令一致地走 sudo。
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.list_containers()
        .await
        .map_err(|e| format!("列容器失败: {e}"))
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
pub async fn docker_deploy(
    host_id: String,
    spec: DockerDeploySpec,
    task_id: String,
    state: State<'_, AppState>,
) -> Result<DeployedContainer, String> {
    spec.validate().map_err(|e| format!("部署参数非法: {e}"))?;
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let host_ref: &dyn Host = host.as_ref();

    let flavor_label = format!("{:?}", spec.flavor);
    let container_name = spec.container_name.clone();

    // 小闭包：捕获 event_bus + task_id，减少重复代码。
    let event_bus = state.event_bus.clone();
    let tid = task_id.clone();
    let host_id_log = host_id.clone();
    let container_log = container_name.clone();
    let flavor_log = flavor_label.clone();
    let emit = move |kind: ProgressKind| {
        session_log_deploy_progress(&kind, &host_id_log, &container_log, &flavor_log);
        event_bus.publish(DomainEvent::docker_deploy_progress(
            tid.clone(),
            ProgressEvent::new(kind),
        ));
    };

    emit(ProgressKind::Started { total_steps: 5 });

    // step 1: 探测 docker
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

    // step 2: 准备目录 + 写 compose 文件
    emit(ProgressKind::StepBegin {
        step: 2,
        message: "准备部署目录...".to_string(),
    });
    let secret = uuid::Uuid::new_v4().simple().to_string();
    let project_dir = match resolve_project_dir(&host_id, host_ref, &state, &spec.container_name).await {
        Ok(d) => d,
        Err(e) => {
            emit(ProgressKind::Log { level: ProgressLogLevel::Error, message: e.clone() });
            emit(ProgressKind::Finished { ok: false });
            return Err(e);
        }
    };
    if let Err(e) = host.create_dir_all(&HostPath::from_posix(&project_dir)).await {
        let msg = format!("创建部署目录失败: {e}");
        emit(ProgressKind::Log { level: ProgressLogLevel::Error, message: msg.clone() });
        emit(ProgressKind::Finished { ok: false });
        return Err(msg);
    }
    let (uid, gid) = probe_uid_gid(host_ref).await;
    let yaml = render_compose(&spec, &secret, uid, gid);
    let compose_path = HostPath::from_posix(format!("{project_dir}/docker-compose.yml"));
    if let Err(e) = host.write_file(&compose_path, yaml.as_bytes()).await {
        let msg = format!("写 compose 文件失败: {e}");
        emit(ProgressKind::Log { level: ProgressLogLevel::Error, message: msg.clone() });
        emit(ProgressKind::Finished { ok: false });
        return Err(msg);
    }
    emit(ProgressKind::StepEnd { step: 2, ok: true });

    // step 3: 拉镜像（流式，逐行更新 layer 计数；多镜像站 fallback）
    emit(ProgressKind::StepBegin {
        step: 3,
        message: "拉取镜像...".to_string(),
    });
    {
        // PullProgress 和 emit 都在主 task 里，用 Mutex 让回调（可能在 tokio spawn
        // 的 reader task 里）安全更新状态。实际上 run_streaming 的 on_line 在同一
        // async task 里被调用（channel 收发在同一 future），Mutex 只是满足 Send 约束。
        use std::sync::{Arc, Mutex};
        let event_bus_pull = state.event_bus.clone();
        let tid_pull = task_id.clone();

        // 镜像候选:国内反代镜像站优先 + 官方直连兜底。compose.yml 写官方名,
        // pull_with_fallback 成功后会 retag 回官方名命中缓存。
        let candidates = spec.flavor.pull_candidates();
        let official = spec.flavor.default_image();

        // 回调工厂:每换一个候选给一份独立 PullProgress(layer 计数不串站),并发
        // 一条日志告知用户当前在试哪个源。idx>0 说明前面的源失败了,降级提示。
        let new_line_cb = move |idx: usize, image: &str| {
            if idx == 0 {
                event_bus_pull.publish(DomainEvent::docker_deploy_progress(
                    tid_pull.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Info,
                        message: format!("拉取镜像: {image}"),
                    }),
                ));
            } else {
                event_bus_pull.publish(DomainEvent::docker_deploy_progress(
                    tid_pull.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Warn,
                        message: format!("上一个源失败，改用镜像源重试: {image}"),
                    }),
                ));
            }

            let pull_state = Arc::new(Mutex::new(PullProgress::new()));
            let pull_state_cb = Arc::clone(&pull_state);
            let event_bus_line = event_bus_pull.clone();
            let tid_line = tid_pull.clone();

            move |src: StreamSource, line: String| {
                // 原始行作为 Log 发出，方便调试。
                event_bus_line.publish(DomainEvent::docker_deploy_progress(
                    tid_line.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Info,
                        message: line.clone(),
                    }),
                ));
                // stdout 才是 docker pull 进度行；stderr 只记日志不更新计数。
                if src == StreamSource::Stdout {
                    let mut ps = pull_state_cb.lock().unwrap();
                    ps.update(&line);
                    let (completed, total, msg) = ps.summary();
                    let percent = if total > 0 {
                        ((completed as u64 * 100) / total as u64) as u8
                    } else {
                        0
                    };
                    event_bus_line.publish(DomainEvent::docker_deploy_progress(
                        tid_line.clone(),
                        ProgressEvent::new(ProgressKind::StepProgress {
                            step: 3,
                            percent,
                            message: msg,
                            speed_bps: None,
                            downloaded_bytes: None,
                            total_bytes: None,
                            download_stage: None,
                        }),
                    ));
                }
            }
        };

        if let Err(e) = cli
            .pull_with_fallback(&candidates, official, new_line_cb)
            .await
        {
            let msg = format!("拉取镜像失败（已尝试 {} 个源）: {e}", candidates.len());
            emit(ProgressKind::Log { level: ProgressLogLevel::Error, message: msg.clone() });
            emit(ProgressKind::Finished { ok: false });
            return Err(msg);
        }
    }
    emit(ProgressKind::StepEnd { step: 3, ok: true });

    // step 4: 起容器
    emit(ProgressKind::StepBegin {
        step: 4,
        message: "启动容器...".to_string(),
    });
    if let Err(e) = cli.compose_up(&project_dir).await {
        let msg = format!("启动容器失败: {e}");
        emit(ProgressKind::Log { level: ProgressLogLevel::Error, message: msg.clone() });
        emit(ProgressKind::Finished { ok: false });
        return Err(msg);
    }
    emit(ProgressKind::StepEnd { step: 4, ok: true });

    // step 5: 回读地址
    emit(ProgressKind::StepBegin {
        step: 5,
        message: "读取部署结果...".to_string(),
    });
    let address = host_display_address(&host_id, &state).await;
    let deployed = build_deployed(&spec, &secret, &address, host_ref).await;
    emit(ProgressKind::StepEnd { step: 5, ok: true });
    emit(ProgressKind::Finished { ok: true });

    Ok(deployed)
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
    // compose down 要用 compose 插件,走 ensure_ready(probe 定夺提权 + 要求 compose)。
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

/// 解析 compose project 目录(放 docker-compose.yml 的地方)。
/// 本机:<data_root>/docker/<name>(POSIX 化路径,LocalWindowsHost 内部转盘符)。
/// 远端:<$HOME>/.napcat-docker/<name>;探不到 $HOME 时返回错误(不回退 /root)。
async fn resolve_project_dir(
    host_id: &str,
    host: &dyn Host,
    state: &AppState,
    name: &str,
) -> Result<String, String> {
    if host_id == "local" {
        let base = state.data_root.join("docker").join(name);
        // HostPath::from_windows 把 C:\... 规范成 /c/...,LocalWindowsHost 能还原。
        return Ok(HostPath::from_windows(&base.to_string_lossy()).as_posix().to_string());
    }
    // 远端:探 $HOME。探不到就 fail-fast,不回退 /root——路径落盘红线:宁可报错让
    // 用户处理,也不把生产数据静默落到错误目录(/root 通常无权限,或污染 root 家目录)。
    let home = probe_remote_home(host).await.ok_or_else(|| {
        "无法探测远端 $HOME,已拒绝回退到 /root 部署 Docker(避免把数据落到错误目录)。\
         请确认远端 SSH 用户有正常的家目录后重试。"
            .to_string()
    })?;
    Ok(format!("{home}/.napcat-docker/{name}"))
}

/// 远端探 $HOME。失败返回 None;调用方须 fail-fast,不得回退 /root。
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

/// 探远端文件属主 uid/gid,用于 compose 卷挂载权限。Linux 上跑 `id -u`/`id -g`
/// 拿登录用户真实值——硬编码 1000 在非默认用户(uid≠1000)的机器上会让挂载目录
/// 属主错配、容器读写权限异常。探测失败退回 1000(最常见默认)。非 Linux(本机
/// Windows Docker Desktop)不在意属主,给 (0,0)。
async fn probe_uid_gid(host: &dyn Host) -> (u32, u32) {
    if !matches!(host.os(), ncd_host::Os::Linux) {
        return (0, 0);
    }
    async fn probe_one(host: &dyn Host, flag: &str) -> Option<u32> {
        let out = host.run_to_string(HostCommand::new("id").arg(flag)).await.ok()?;
        if !out.success() {
            return None;
        }
        out.stdout.trim().parse().ok()
    }
    let uid = probe_one(host, "-u").await.unwrap_or(1000);
    let gid = probe_one(host, "-g").await.unwrap_or(1000);
    (uid, gid)
}

/// 拼部署结果。NapCat 的 WebUI token 就是我们设的 secret;SnowLuma 的 WebUI
/// 密码由容器首启随机生成并打日志,这里尝试 grep 一次拿到(拿不到留 None,
/// 前端提示去看 noVNC / docker logs)。
async fn build_deployed(
    spec: &DockerDeploySpec,
    secret: &str,
    address: &str,
    host: &dyn Host,
) -> DeployedContainer {
    // 找某个容器端口在宿主机上映射到哪个端口,拿来拼 URL。找不到用容器端口兜底。
    let host_port = |container_port: u16| -> u16 {
        spec.ports
            .iter()
            .find(|p| p.container == container_port)
            .map(|p| p.host)
            .unwrap_or(container_port)
    };

    match spec.flavor {
        DockerFlavor::NapCat => {
            let webui = host_port(6099);
            DeployedContainer {
                name: spec.container_name.clone(),
                flavor: DockerFlavor::NapCat,
                webui_url: format!("http://{address}:{webui}/webui"),
                novnc_url: None,
                // NapCat 的 token 就是我们设进 WEBUI_TOKEN 的值。
                webui_secret: Some(secret.to_string()),
            }
        }
        DockerFlavor::SnowLuma => {
            let webui = host_port(5099);
            let novnc = host_port(6081);
            let password = grep_snowluma_password(host, &spec.container_name).await;
            DeployedContainer {
                name: spec.container_name.clone(),
                flavor: DockerFlavor::SnowLuma,
                webui_url: format!("http://{address}:{webui}/"),
                novnc_url: Some(format!("http://{address}:{novnc}/")),
                webui_secret: password,
            }
        }
    }
}

/// 从 SnowLuma 容器日志里 grep 首启随机密码。容器刚起来日志可能还没出密码行,
/// 拿不到返回 None,不阻塞部署成功。
async fn grep_snowluma_password(host: &dyn Host, container: &str) -> Option<String> {
    let cli = DockerCli::new(host);
    let logs = cli.logs(container, 400).await.ok()?;
    for line in logs.lines() {
        // SnowLuma 镜像打的是 "临时密码: xxxx" 或 "initial credentials: user=admin password=xxxx"。
        if let Some(idx) = line.find("临时密码:") {
            let tail = line[idx + "临时密码:".len()..].trim();
            let pw = tail.split_whitespace().next().unwrap_or("").trim();
            if !pw.is_empty() {
                return Some(pw.to_string());
            }
        }
        if let Some(idx) = line.find("password=") {
            let tail = line[idx + "password=".len()..].trim();
            let pw = tail.split_whitespace().next().unwrap_or("").trim();
            if !pw.is_empty() {
                return Some(pw.to_string());
            }
        }
    }
    None
}
