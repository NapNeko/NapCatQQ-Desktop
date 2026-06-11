//! Docker 安装进度：复用 [`ncd_component::ProgressKind`]，经 Tauri 推到前端。

use std::sync::Arc;

use ncd_component::{ProgressEvent, ProgressKind, ProgressLogLevel};
use ncd_domain::DockerInstallReport;
use ncd_host::remote::{probe_sudo, SudoAccess};
use ncd_host::{Host, HostCommand, Os};
use tracing::{error, info, warn};

use super::cli::{DockerCli, DockerCliError};
use super::install::{aliyun_install_script, looks_like_bad_sudo_password, DOCKER_DESKTOP_URL};

pub const INSTALL_TOTAL_STEPS: u32 = 6;

pub type InstallProgressEmit = Arc<dyn Fn(ProgressKind) + Send + Sync>;

fn emit_log(emit: &InstallProgressEmit, level: ProgressLogLevel, message: impl Into<String>) {
    emit(ProgressKind::Log {
        level,
        message: message.into(),
    });
}

fn emit_step_begin(emit: &InstallProgressEmit, step: u32, message: impl Into<String>) {
    emit(ProgressKind::StepBegin {
        step,
        message: message.into(),
    });
}

fn emit_step_end(emit: &InstallProgressEmit, step: u32, ok: bool) {
    emit(ProgressKind::StepEnd { step, ok });
}

fn emit_step_progress(emit: &InstallProgressEmit, step: u32, percent: u8, message: impl Into<String>) {
    emit(ProgressKind::StepProgress {
        step,
        percent,
        message: message.into(),
        speed_bps: None,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: None,
    });
}

fn finish_install(emit: &InstallProgressEmit, ok: bool) {
    emit(ProgressKind::Finished { ok });
}

/// 带进度回调的安装入口。`emit` 由 Tauri 层接到 EventBus。
/// `ssh_linux_username`：远端 SSH 登录名，用于 usermod 进 docker 组（比 SUDO_USER 更可靠）。
pub async fn install_docker_with_progress(
    host: &dyn Host,
    sudo_password: Option<&str>,
    ssh_linux_username: Option<&str>,
    emit: InstallProgressEmit,
) -> Result<DockerInstallReport, DockerCliError> {
    emit(ProgressKind::Started {
        total_steps: INSTALL_TOTAL_STEPS,
    });

    info!(
        target: "ncd_deploy::docker",
        os = ?host.os(),
        "开始检查并安装 Docker"
    );
    emit_step_begin(&emit, 1, "检查 Docker 是否已就绪…");

    let cli = DockerCli::new(host);
    let status = cli.probe().await;
    if status.ready_to_deploy() {
        emit_step_end(&emit, 1, true);
        emit_log(
            &emit,
            ProgressLogLevel::Info,
            format!("Docker 已就绪（{}）", status.version),
        );
        finish_install(&emit, true);
        info!(
            target: "ncd_deploy::docker",
            version = %status.version,
            "Docker 已就绪，跳过安装"
        );
        return Ok(
            DockerInstallReport::already_installed(&status.version).with_probed_status(status),
        );
    }
    emit_step_end(&emit, 1, true);

    match host.os() {
        Os::Linux => {
            install_docker_linux_with_progress(host, sudo_password, ssh_linux_username, emit).await
        }
        Os::Windows => {
            emit_log(
                &emit,
                ProgressLogLevel::Warn,
                "Windows 需手动安装 Docker Desktop",
            );
            finish_install(&emit, false);
            Ok(DockerInstallReport::manual_required(
                "Windows 请安装 Docker Desktop 后重试（需要 WSL2 后端）",
                Some(DOCKER_DESKTOP_URL.to_string()),
            ))
        }
        Os::MacOs => {
            emit_log(&emit, ProgressLogLevel::Warn, "macOS 需手动安装 Docker Desktop");
            finish_install(&emit, false);
            Ok(DockerInstallReport::manual_required(
                "macOS 请安装 Docker Desktop 后重试",
                Some(DOCKER_DESKTOP_URL.to_string()),
            ))
        }
    }
}

async fn install_docker_linux_with_progress(
    host: &dyn Host,
    sudo_password: Option<&str>,
    ssh_linux_username: Option<&str>,
    emit: InstallProgressEmit,
) -> Result<DockerInstallReport, DockerCliError> {
    emit_step_begin(&emit, 2, "检查 sudo 提权…");
    let elevation = match probe_sudo(host).await {
        SudoAccess::RootAlready | SudoAccess::Passwordless => {
            emit_step_end(&emit, 2, true);
            true
        }
        SudoAccess::PasswordRequired => match sudo_password {
            Some(pw) => {
                host.set_elevation_password(Some(pw.to_string())).await;
                emit_step_end(&emit, 2, true);
                true
            }
            None => {
                emit_step_end(&emit, 2, false);
                emit_log(
                    &emit,
                    ProgressLogLevel::Warn,
                    "需要 sudo 密码，请在弹框中输入后重试",
                );
                finish_install(&emit, false);
                warn!(
                    target: "ncd_deploy::docker",
                    "安装 Docker 需要 sudo 密码，等待用户在弹框输入"
                );
                return Ok(DockerInstallReport::need_sudo_password(
                    "这台远端是密钥登录且未保存密码，安装 Docker 需要 sudo 权限。请输入 sudo 密码后重试。",
                ));
            }
        },
    };
    if !elevation {
        return Ok(DockerInstallReport::need_sudo_password(
            "这台远端是密钥登录且未保存密码，安装 Docker 需要 sudo 权限。请输入 sudo 密码后重试。",
        ));
    }

    emit_step_begin(&emit, 3, "执行安装脚本（阿里云源，约 3–10 分钟）…");
    emit_step_progress(&emit, 3, 2, "正在连接远端并启动 apt/dnf…");
    info!(
        target: "ncd_deploy::docker",
        "正在远端执行 docker-ce 安装脚本（阿里云源，最长约 10 分钟，请稍候）"
    );

    let run_script = HostCommand::new("sh")
        .arg("-c")
        .arg(aliyun_install_script())
        .elevated()
        .timeout(std::time::Duration::from_secs(600));

    let emit_stream = emit.clone();
    let mut line_no: u32 = 0;
    let mut last_emit_percent: u8 = 2;
    let out = host
        .run_streaming(
            run_script,
            Box::new(move |_source, line| {
                let t = line.trim();
                if t.is_empty() {
                    return;
                }
                line_no += 1;
                let notable = t.starts_with("Get:")
                    || t.starts_with("Ign:")
                    || t.starts_with("Fetched")
                    || t.contains("Setting up")
                    || t.contains("Unpacking")
                    || t.contains("docker-ce")
                    || t.contains("E:")
                    || t.contains("error")
                    || t.contains("Error");

                if notable {
                    let level = if t.contains("E:") || t.to_ascii_lowercase().contains("error") {
                        ProgressLogLevel::Warn
                    } else {
                        ProgressLogLevel::Info
                    };
                    emit_stream(ProgressKind::Log {
                        level,
                        message: truncate_line(t, 240),
                    });
                    let pct = script_line_to_percent(line_no, t);
                    if pct > last_emit_percent {
                        last_emit_percent = pct;
                        emit_stream(ProgressKind::StepProgress {
                            step: 3,
                            percent: pct.min(88),
                            message: truncate_line(t, 120),
                            speed_bps: None,
                            downloaded_bytes: None,
                            total_bytes: None,
                            download_stage: None,
                        });
                    }
                }
            }),
        )
        .await?;

    if !out.success() {
        emit_step_end(&emit, 3, false);
        if looks_like_bad_sudo_password(&out.stderr) {
            emit_log(&emit, ProgressLogLevel::Error, "sudo 密码不正确");
            finish_install(&emit, false);
            warn!(
                target: "ncd_deploy::docker",
                "Docker 安装脚本: sudo 密码不正确"
            );
            return Ok(DockerInstallReport::need_sudo_password(
                "sudo 密码不正确，请重新输入。",
            ));
        }
        let detail = out.stderr.trim();
        error!(
            target: "ncd_deploy::docker",
            err = %detail,
            "Docker 安装脚本执行失败"
        );
        emit_log(
            &emit,
            ProgressLogLevel::Error,
            truncate_line(detail, 200),
        );
        finish_install(&emit, false);
        return Ok(DockerInstallReport::manual_required(
            format!(
                "安装脚本执行失败：{}。可登录远端按阿里云文档手动配置 docker-ce 仓库后重试。",
                out.stderr.trim()
            ),
            None,
        ));
    }
    emit_step_progress(&emit, 3, 90, "安装脚本已完成");
    emit_step_end(&emit, 3, true);

    emit_step_begin(&emit, 4, "启动 Docker 服务…");
    info!(
        target: "ncd_deploy::docker",
        "安装脚本已完成，正在启动 docker 服务并配置用户组"
    );
    let enable = HostCommand::new("systemctl")
        .arg("enable")
        .arg("--now")
        .arg("docker")
        .elevated()
        .timeout(std::time::Duration::from_secs(60));
    let enable_out = host.run_to_string(enable).await?;
    emit_step_end(&emit, 4, enable_out.success());
    if !enable_out.success() {
        emit_log(
            &emit,
            ProgressLogLevel::Warn,
            "systemctl 启动 docker 未成功，将继续探测 daemon 状态",
        );
    } else {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }

    emit_step_begin(&emit, 5, "将当前用户加入 docker 组…");
    let usermod_script = docker_usermod_script(ssh_linux_username);
    let usermod = HostCommand::new("sh")
        .arg("-c")
        .arg(usermod_script)
        .elevated()
        .timeout(std::time::Duration::from_secs(30));
    let usermod_out = host.run_to_string(usermod).await?;
    emit_step_end(&emit, 5, usermod_out.success());
    if !usermod_out.success() {
        emit_log(
            &emit,
            ProgressLogLevel::Warn,
            "usermod 未成功，后续命令将尝试 sudo 访问 Docker",
        );
    }

    emit_step_begin(&emit, 6, "验证 Docker 与 Compose 插件…");
    let status = finalize_linux_docker_after_install(host, &emit).await?;
    info!(
        target: "ncd_deploy::docker",
        installed = status.installed,
        daemon_running = status.daemon_running,
        compose_available = status.compose_available,
        version = %status.version,
        "安装后 Docker 探测结果"
    );
    emit_log(
        &emit,
        ProgressLogLevel::Info,
        format!(
            "探测：installed={} daemon={} compose={} version={}",
            status.installed,
            status.daemon_running,
            status.compose_available,
            status.version
        ),
    );

    if status.ready_to_deploy() {
        emit_step_end(&emit, 6, true);
        emit_step_progress(&emit, 6, 100, "Docker 可部署");
        finish_install(&emit, true);
        info!(target: "ncd_deploy::docker", "Docker 安装完成且可部署");
        return Ok(DockerInstallReport::installed().with_probed_status(status));
    }

    emit_step_end(&emit, 6, false);
    let msg = if status.installed && status.daemon_running {
        "Docker 已安装但缺 compose v2 插件，请在远端执行 sudo apt-get install -y docker-compose-plugin（dnf/yum 同名包）后重试".to_string()
    } else if status.installed {
        "Docker 已安装但 daemon 未运行，请在远端执行 sudo systemctl start docker".to_string()
    } else {
        "安装脚本执行完毕但仍探测不到 Docker，请登录远端手动检查".to_string()
    };
    emit_log(&emit, ProgressLogLevel::Warn, &msg);
    finish_install(&emit, false);
    Ok(
        DockerInstallReport::manual_required(msg, None).with_probed_status(status),
    )
}

fn docker_usermod_script(ssh_linux_username: Option<&str>) -> String {
    if let Some(u) = ssh_linux_username.map(str::trim).filter(|s| !s.is_empty()) {
        let escaped = u.replace('\'', "'\\''");
        return format!("usermod -aG docker '{escaped}'");
    }
    "usermod -aG docker \"${SUDO_USER:-$(logname 2>/dev/null)}\"".to_string()
}

async fn try_install_compose_plugin(host: &dyn Host) -> Result<(), DockerCliError> {
    let script = r#"set -e
if command -v apt-get >/dev/null 2>&1; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq docker-compose-plugin
elif command -v dnf >/dev/null 2>&1; then
  dnf install -y docker-compose-plugin
elif command -v yum >/dev/null 2>&1; then
  yum install -y docker-compose-plugin
else
  exit 1
fi"#;
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg(script)
        .elevated()
        .timeout(std::time::Duration::from_secs(300));
    let out = host.run_to_string(cmd).await?;
    if out.success() {
        Ok(())
    } else {
        Err(DockerCliError::Host(ncd_host::HostError::CommandFailed {
            program: "docker-compose-plugin install".into(),
            exit_code: out.exit_code,
            stderr: out.stderr,
        }))
    }
}

async fn finalize_linux_docker_after_install(
    host: &dyn Host,
    emit: &InstallProgressEmit,
) -> Result<ncd_domain::DockerStatus, DockerCliError> {
    let mut cli = DockerCli::new(host);
    let mut status = cli.probe().await;

    for attempt in 0..4u32 {
        if status.ready_to_deploy() {
            return Ok(status);
        }
        if !status.installed {
            return Ok(status);
        }
        if !status.daemon_running {
            emit_log(
                emit,
                ProgressLogLevel::Info,
                format!("等待 Docker 守护进程就绪（{}/4）…", attempt + 1),
            );
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            cli = DockerCli::new(host);
            status = cli.probe().await;
            continue;
        }
        if !status.compose_available {
            emit_log(
                emit,
                ProgressLogLevel::Info,
                "正在补装 docker-compose-plugin…",
            );
            if try_install_compose_plugin(host).await.is_ok() {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            } else {
                emit_log(
                    emit,
                    ProgressLogLevel::Warn,
                    "补装 compose 插件未成功，将重试探测",
                );
            }
            cli = DockerCli::new(host);
            status = cli.probe().await;
            continue;
        }
        break;
    }
    Ok(status)
}

fn truncate_line(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn script_line_to_percent(line_no: u32, line: &str) -> u8 {
    if line.contains("Setting up") || line.contains("Unpacking docker") {
        return 75;
    }
    if line.contains("docker-ce") || line.contains("docker-compose-plugin") {
        return 60;
    }
    if line.starts_with("Fetched") {
        return 45;
    }
    if line.starts_with("Get:") {
        return 20 + (line_no % 20) as u8;
    }
    10 + (line_no % 70).min(30) as u8
}

/// Tauri 层用：把 ProgressKind 包装成 ProgressEvent（时间戳由 ProgressEvent::new 填充）。
pub fn progress_event(kind: ProgressKind) -> ProgressEvent {
    ProgressEvent::new(kind)
}