//! Docker 安装进度:复用 [ncd_component::ProgressKind],经 Tauri 推到前端
//!
//! Step 3 按 [super::install::DOCKER_INSTALL_PHASES] 分段执行,每段走
//! [super::pkg_install_emit::run_pkg_with_emit](与组件装包同源 apt/dnf 解析 + 静默心跳)

use std::sync::Arc;

use ncd_component::{ProgressEvent, ProgressKind, ProgressLogLevel};
use ncd_domain::DockerInstallReport;
use ncd_host::remote::{SudoAccess, probe_sudo};
use ncd_host::{Host, HostCommand, Os, host_command_wrap_dpkg_wait_for_apt, truncate_pkg_line};
use tracing::{error, info, warn};

use super::cli::{DockerCli, DockerCliError};
use super::install::{
    DOCKER_INSTALL_PHASES, looks_like_bad_sudo_password, write_registry_mirrors_script,
};
use super::pkg_install_emit::run_pkg_with_emit;

pub const INSTALL_TOTAL_STEPS: u32 = 7;

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

fn emit_step_progress(
    emit: &InstallProgressEmit,
    step: u32,
    percent: u8,
    message: impl Into<String>,
) {
    emit(ProgressKind::StepProgress {
        step,
        percent,
        message: message.into(),
        speed_bps: None,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: None,
        docker_layers: None,
    });
}

fn finish_install(emit: &InstallProgressEmit, ok: bool) {
    emit(ProgressKind::Finished { ok });
}

/// 带进度回调的安装入口,emit 由 Tauri 层接到 EventBus
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
            DockerInstallReport::already_installed(&status.version).with_probed_status(status)
        );
    }
    emit_step_end(&emit, 1, true);

    match host.os() {
        Os::Linux => {
            install_docker_linux_with_progress(host, sudo_password, ssh_linux_username, emit).await
        }
        // docker 只在 Linux 部署:Windows/macOS 不引导装 Docker Desktop,直接返回不支持
        _ => {
            emit_log(
                &emit,
                ProgressLogLevel::Warn,
                "Windows/macOS 不支持 docker 部署,请在 Linux 主机操作",
            );
            finish_install(&emit, false);
            Ok(DockerInstallReport::manual_required(
                "Windows/macOS 不支持 docker 部署,请在 Linux 主机操作",
                None,
            ))
        }
    }
}

struct PhaseSpec {
    idle: &'static str,
    floor: u8,
    cap: u8,
    timeout_secs: u64,
}

fn phase_spec(name: &str) -> PhaseSpec {
    match name {
        "apt_prep" => PhaseSpec {
            idle: "更新软件源并安装 curl、gnupg…",
            floor: 5,
            cap: 24,
            timeout_secs: 300,
        },
        "apt_repo" => PhaseSpec {
            idle: "配置 Docker CE 阿里云源…",
            floor: 24,
            cap: 38,
            timeout_secs: 120,
        },
        "apt_install" => PhaseSpec {
            idle: "安装 docker-ce 与 compose 插件（可能较久）…",
            floor: 38,
            cap: 86,
            timeout_secs: 480,
        },
        "dnf_install" => PhaseSpec {
            idle: "dnf 安装 Docker CE…",
            floor: 8,
            cap: 88,
            timeout_secs: 600,
        },
        "yum_install" => PhaseSpec {
            idle: "yum 安装 Docker CE…",
            floor: 8,
            cap: 88,
            timeout_secs: 600,
        },
        "pkgmgr_check" => PhaseSpec {
            idle: "检查包管理器…",
            floor: 2,
            cap: 5,
            timeout_secs: 30,
        },
        _ => PhaseSpec {
            idle: "执行安装…",
            floor: 5,
            cap: 80,
            timeout_secs: 300,
        },
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

    emit_step_begin(&emit, 3, "安装 Docker（阿里云源，分步显示 apt/dnf 进度）…");
    info!(
        target: "ncd_deploy::docker",
        "分阶段安装 docker-ce（阿里云源）"
    );

    for (phase_name, script) in DOCKER_INSTALL_PHASES.iter() {
        let spec = phase_spec(phase_name);
        emit_log(&emit, ProgressLogLevel::Info, format!("→ {}", spec.idle));

        let cmd = HostCommand::new("sh")
            .arg("-c")
            .arg(*script)
            .elevated()
            .timeout(std::time::Duration::from_secs(spec.timeout_secs));

        let out = run_pkg_with_emit(host, &emit, cmd, 3, spec.floor, spec.cap, spec.idle).await?;

        if !out.success() {
            emit_step_end(&emit, 3, false);
            let detail = if !out.stderr.trim().is_empty() {
                out.stderr.trim().to_string()
            } else {
                out.stdout.trim().to_string()
            };
            if looks_like_bad_sudo_password(&detail) {
                emit_log(&emit, ProgressLogLevel::Error, "sudo 密码不正确");
                finish_install(&emit, false);
                return Ok(DockerInstallReport::need_sudo_password(
                    "sudo 密码不正确，请重新输入。",
                ));
            }
            error!(
                target: "ncd_deploy::docker",
                phase = phase_name,
                err = %detail,
                "Docker 安装阶段失败"
            );
            emit_log(
                &emit,
                ProgressLogLevel::Error,
                truncate_pkg_line(&detail, 200),
            );
            finish_install(&emit, false);
            return Ok(DockerInstallReport::manual_required(
                format!(
                    "安装阶段「{}」失败：{}。可登录远端按阿里云文档手动配置 docker-ce 后重试。",
                    spec.idle,
                    truncate_pkg_line(&detail, 120)
                ),
                None,
            ));
        }
    }

    emit_step_progress(&emit, 3, 90, "包管理器安装阶段已完成");
    emit_step_end(&emit, 3, true);

    emit_step_begin(&emit, 4, "启动 Docker 服务…");
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
            status.installed, status.daemon_running, status.compose_available, status.version
        ),
    );

    if !status.ready_to_deploy() {
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
        return Ok(DockerInstallReport::manual_required(msg, None).with_probed_status(status));
    }

    emit_step_end(&emit, 6, true);

    emit_step_begin(&emit, 7, "配置 Docker 镜像加速…");
    let mirror_cmd = HostCommand::new("sh")
        .arg("-c")
        .arg(write_registry_mirrors_script())
        .elevated()
        .timeout(std::time::Duration::from_secs(60));
    match host.run_to_string(mirror_cmd).await {
        Ok(m) if m.success() => {
            emit_log(&emit, ProgressLogLevel::Info, "已写入 registry-mirrors");
            emit_step_end(&emit, 7, true);
        }
        Ok(m) => {
            emit_log(
                &emit,
                ProgressLogLevel::Warn,
                format!(
                    "镜像加速未写入（可稍后在远端手动配置）：{}",
                    truncate_pkg_line(m.stderr.trim(), 80)
                ),
            );
            emit_step_end(&emit, 7, true);
        }
        Err(e) => {
            emit_log(&emit, ProgressLogLevel::Warn, format!("镜像加速跳过：{e}"));
            emit_step_end(&emit, 7, true);
        }
    }

    emit_step_progress(&emit, 7, 100, "Docker 可部署");
    finish_install(&emit, true);
    info!(target: "ncd_deploy::docker", "Docker 安装完成且可部署");
    Ok(DockerInstallReport::installed().with_probed_status(status))
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
  apt-get update
  apt-get install -y docker-compose-plugin
elif command -v dnf >/dev/null 2>&1; then
  dnf install -y docker-compose-plugin
elif command -v yum >/dev/null 2>&1; then
  yum install -y docker-compose-plugin
else
  exit 1
fi"#;
    let cmd = host_command_wrap_dpkg_wait_for_apt(
        HostCommand::new("sh")
            .arg("-c")
            .arg(script)
            .elevated()
            .timeout(std::time::Duration::from_secs(300)),
    );
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

    for attempt in 0..6u32 {
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
                format!("等待 Docker 守护进程就绪（{}/6）…", attempt + 1),
            );
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
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

/// Tauri 层用:把 ProgressKind 包装成 ProgressEvent(时间戳由 ProgressEvent::new 填充)
pub fn progress_event(kind: ProgressKind) -> ProgressEvent {
    ProgressEvent::new(kind)
}
