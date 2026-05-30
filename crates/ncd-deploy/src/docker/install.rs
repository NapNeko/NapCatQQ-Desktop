//! 帮用户装 docker(如果没有)。
//!
//! Linux:跑官方 `curl -fsSL https://get.docker.com | sh`,再 enable daemon +
//! 把当前用户加进 docker 组。需要 sudo;远端没配 NOPASSWD sudo 时会失败,把
//! stderr 透出去让前端提示用户手动装。
//!
//! Windows:不能静默装 Docker Desktop,只回一个引导结果让前端弹"去下载"。
//! macOS 同理(本工程目前不在 macOS 上跑 docker 部署,留个明确分支)。

use ncd_host::{Host, HostCommand, Os};

use super::cli::{DockerCli, DockerCliError};

/// 安装尝试的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerInstallOutcome {
    /// 已经装好了,什么都没做。
    AlreadyInstalled { version: String },
    /// 这次成功装上了。
    Installed,
    /// 装不了,需要用户手动处理。reason 给前端展示,download_url 可选给个下载入口。
    ManualRequired {
        reason: String,
        download_url: Option<String>,
    },
}

/// Docker Desktop for Windows 下载页。
const DOCKER_DESKTOP_URL: &str = "https://www.docker.com/products/docker-desktop/";

/// 在 host 上确保 docker 可用,没有就尝试装。
pub async fn install_docker(host: &dyn Host) -> Result<DockerInstallOutcome, DockerCliError> {
    let cli = DockerCli::new(host);

    // 已装且 daemon 在跑就直接返回,幂等。
    let status = cli.probe().await;
    if status.installed && status.daemon_running {
        return Ok(DockerInstallOutcome::AlreadyInstalled {
            version: status.version,
        });
    }

    match host.os() {
        Os::Linux => install_docker_linux(host).await,
        Os::Windows => Ok(DockerInstallOutcome::ManualRequired {
            reason: "Windows 请安装 Docker Desktop 后重试（需要 WSL2 后端）".to_string(),
            download_url: Some(DOCKER_DESKTOP_URL.to_string()),
        }),
        Os::MacOs => Ok(DockerInstallOutcome::ManualRequired {
            reason: "macOS 请安装 Docker Desktop 后重试".to_string(),
            download_url: Some(DOCKER_DESKTOP_URL.to_string()),
        }),
    }
}

/// Linux 装 docker:官方一键脚本 + 起 daemon + 加用户组。
async fn install_docker_linux(host: &dyn Host) -> Result<DockerInstallOutcome, DockerCliError> {
    // 官方脚本自己判断发行版,装 docker-ce + compose 插件。用 sh -c 串起来。
    // 需要 root;远端非 root 用户走 sudo -n,没配 NOPASSWD 会失败并把提示透出去。
    let install_script = "curl -fsSL https://get.docker.com | sudo -n sh";
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg(install_script)
        .timeout(std::time::Duration::from_secs(600));
    let out = host.run_to_string(cmd).await?;
    if !out.success() {
        return Ok(DockerInstallOutcome::ManualRequired {
            reason: format!(
                "自动安装失败（可能缺少免密 sudo）：{}。可手动执行 curl -fsSL https://get.docker.com | sh",
                out.stderr.trim()
            ),
            download_url: None,
        });
    }

    // 起 daemon 并设开机自启。失败不致命(部分容器化环境没 systemd),只记进 reason。
    let enable = HostCommand::new("sh")
        .arg("-c")
        .arg("sudo -n systemctl enable --now docker")
        .timeout(std::time::Duration::from_secs(60));
    let _ = host.run_to_string(enable).await;

    // 把当前用户加进 docker 组,免得每条 docker 命令都要 sudo。需要重新登录生效,
    // 但我们后续命令仍可能走 sudo 兜底,所以这步失败也不致命。
    let usermod = HostCommand::new("sh")
        .arg("-c")
        .arg("sudo -n usermod -aG docker \"$USER\"")
        .timeout(std::time::Duration::from_secs(30));
    let _ = host.run_to_string(usermod).await;

    // 复探一次确认 daemon 真起来了。
    let cli = DockerCli::new(host);
    let status = cli.probe().await;
    if status.installed && status.daemon_running {
        Ok(DockerInstallOutcome::Installed)
    } else if status.installed {
        Ok(DockerInstallOutcome::ManualRequired {
            reason: "docker 已安装但 daemon 未运行，请执行 sudo systemctl start docker"
                .to_string(),
            download_url: None,
        })
    } else {
        Ok(DockerInstallOutcome::ManualRequired {
            reason: "安装脚本执行完毕但仍探测不到 docker，请手动检查".to_string(),
            download_url: None,
        })
    }
}
