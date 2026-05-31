//! 帮用户装 docker(如果没有)。
//!
//! Linux:把官方 get.docker.com 脚本先落盘再执行。提权交给 Host 的固有能力——
//! 把 sudo 密码注入 host 后,所有 `.elevated()` 命令由 host 层统一决定走 sudo -S
//! (有密码)还是 sudo -n(免密 / root)。密码来源:调用方(docker 弹框)显式传入,
//! 或 ServerManager 连接时从 keyring 注入。都没有且远端确实要密码时返回
//! NeedSudoPassword,让上层弹框向用户索要后带密码重试。
//!
//! 安装脚本固定加 `--mirror Aliyun`:官方 download.docker.com 在国内常被
//! Connection reset,阿里云镜像(mirrors.aliyun.com/docker-ce)国内稳定、海外也能通。
//!
//! 为什么不再用 `curl ... | sudo sh` 管道:管道右端的 sudo 一旦因为要密码而立刻
//! 退出,左端 curl 往断开的管道写就报 "curl: (23) Failure writing output to
//! destination"——一个缺免密 sudo 的问题被放大成两条费解的错误。先 curl 下载到
//! /tmp 再单独执行,curl 与提权解耦,错误归因清晰。
//!
//! Windows:不能静默装 Docker Desktop,只回一个引导结果让前端弹"去下载"。
//! macOS 同理(本工程目前不在 macOS 上跑 docker 部署,留个明确分支)。

use ncd_domain::DockerInstallReport;
use ncd_host::remote::{probe_sudo, SudoAccess};
use ncd_host::{Host, HostCommand, Os};

use super::cli::{DockerCli, DockerCliError};

/// 兼容旧调用方的别名。结构化结果统一用 [`DockerInstallReport`]。
pub type DockerInstallOutcome = DockerInstallReport;

/// Docker Desktop for Windows 下载页。
const DOCKER_DESKTOP_URL: &str = "https://www.docker.com/products/docker-desktop/";

/// get.docker.com 脚本在远端的落盘路径。固定 /tmp 下,装完不清理也无害。
const INSTALL_SCRIPT_PATH: &str = "/tmp/ncd-get-docker.sh";

/// 在 host 上确保 docker 可用,没有就尝试装。
///
/// sudo_password:上层从 keyring 取到的登录密码或用户在弹框输入的 sudo 密码。
/// None 表示这次没有可用密码——只能靠 root / 免密 sudo,不行就返回 NeedSudoPassword。
pub async fn install_docker(
    host: &dyn Host,
    sudo_password: Option<&str>,
) -> Result<DockerInstallReport, DockerCliError> {
    let cli = DockerCli::new(host);

    // 已装且 daemon 在跑就直接返回,幂等。
    let status = cli.probe().await;
    if status.installed && status.daemon_running {
        return Ok(DockerInstallReport::already_installed(&status.version));
    }

    match host.os() {
        Os::Linux => install_docker_linux(host, sudo_password).await,
        Os::Windows => Ok(DockerInstallReport::manual_required(
            "Windows 请安装 Docker Desktop 后重试（需要 WSL2 后端）",
            Some(DOCKER_DESKTOP_URL.to_string()),
        )),
        Os::MacOs => Ok(DockerInstallReport::manual_required(
            "macOS 请安装 Docker Desktop 后重试",
            Some(DOCKER_DESKTOP_URL.to_string()),
        )),
    }
}

/// 提权可行性:能直接提权(root/免密/已有密码注入 host),还是缺密码得让上层去要。
enum Elevation {
    /// root / 免密 sudo / 已把密码注入 host:elevated 命令都能跑。
    Ready,
    /// 需要密码但一个都没有:让上层弹框去要。
    Need,
}

/// Linux 装 docker:先把提权密码注入 host + 定提权方式,再 curl 落盘 + 执行脚本
/// + 起 daemon + 加用户组。所有 elevated 命令的提权细节由 host 层统一处理。
async fn install_docker_linux(
    host: &dyn Host,
    sudo_password: Option<&str>,
) -> Result<DockerInstallReport, DockerCliError> {
    // 探主机 sudo 能力。root / 免密直接可提权;否则必须有密码。
    let elevation = match probe_sudo(host).await {
        SudoAccess::RootAlready | SudoAccess::Passwordless => Elevation::Ready,
        SudoAccess::PasswordRequired => match sudo_password {
            // 有密码:注入 host,之后所有 .elevated() 命令自动走 sudo -S。
            Some(pw) => {
                host.set_elevation_password(Some(pw.to_string())).await;
                Elevation::Ready
            }
            // host 可能在连接时已被 ServerManager 注入了 keyring 密码,这里探不到
            // 也没显式密码,交给上层弹框要(docker command 层会先用 keyring 兜底)。
            None => Elevation::Need,
        },
    };
    if matches!(elevation, Elevation::Need) {
        return Ok(DockerInstallReport::need_sudo_password(
            "这台远端是密钥登录且未保存密码，安装 Docker 需要 sudo 权限。请输入 sudo 密码后重试。",
        ));
    }

    // curl 下载脚本到 /tmp。这步不提权(写 /tmp 不需要 root),与提权解耦,
    // 避免管道断裂式的 curl:(23)。-f 让 HTTP 错误也返回非零退出。
    let download = HostCommand::new("curl")
        .arg("-fsSL")
        .arg("https://get.docker.com")
        .arg("-o")
        .arg(INSTALL_SCRIPT_PATH)
        .timeout(std::time::Duration::from_secs(120));
    let out = host.run_to_string(download).await?;
    if !out.success() {
        return Ok(DockerInstallReport::manual_required(
            format!(
                "下载 Docker 安装脚本失败：{}。请检查远端网络后重试。",
                out.stderr.trim()
            ),
            None,
        ));
    }

    // 执行脚本(提权)。官方脚本自己判断发行版,装 docker-ce + compose 插件。
    // --mirror Aliyun:把 download.docker.com 换成 mirrors.aliyun.com/docker-ce,
    // 解掉国内访问官方源 Connection reset。提权由 host 注入的密码决定 sudo -S/-n。
    let run_script = HostCommand::new("sh")
        .arg(INSTALL_SCRIPT_PATH)
        .arg("--mirror")
        .arg("Aliyun")
        .elevated()
        .timeout(std::time::Duration::from_secs(600));
    let out = host.run_to_string(run_script).await?;
    if !out.success() {
        // 提权失败最常见就是密码错。区分出来让前端可以重新弹框,而不是笼统报"失败"。
        if looks_like_bad_sudo_password(&out.stderr) {
            return Ok(DockerInstallReport::need_sudo_password(
                "sudo 密码不正确，请重新输入。",
            ));
        }
        return Ok(DockerInstallReport::manual_required(
            format!(
                "安装脚本执行失败：{}。可登录远端手动执行 curl -fsSL https://get.docker.com | sudo sh -s -- --mirror Aliyun",
                out.stderr.trim()
            ),
            None,
        ));
    }

    // 起 daemon 并设开机自启。失败不致命(部分容器化环境没 systemd),忽略错误,
    // 末尾用复探判定 daemon 真实状态。
    let enable = HostCommand::new("systemctl")
        .arg("enable")
        .arg("--now")
        .arg("docker")
        .elevated()
        .timeout(std::time::Duration::from_secs(60));
    let _ = host.run_to_string(enable).await;

    // 把当前用户加进 docker 组,免得每条 docker 命令都要 sudo。需要重新登录才
    // 完全生效,这步失败也不致命。用 $USER 取当前登录名。
    let usermod = HostCommand::new("sh")
        .arg("-c")
        .arg("usermod -aG docker \"$USER\"")
        .elevated()
        .timeout(std::time::Duration::from_secs(30));
    let _ = host.run_to_string(usermod).await;

    // 复探一次确认 daemon 真起来了。
    let cli = DockerCli::new(host);
    let status = cli.probe().await;
    if status.installed && status.daemon_running {
        Ok(DockerInstallReport::installed())
    } else if status.installed {
        Ok(DockerInstallReport::manual_required(
            "Docker 已安装但 daemon 未运行，请在远端执行 sudo systemctl start docker",
            None,
        ))
    } else {
        Ok(DockerInstallReport::manual_required(
            "安装脚本执行完毕但仍探测不到 Docker，请登录远端手动检查",
            None,
        ))
    }
}

/// 从 sudo/stderr 粗判是不是密码错误。sudo 各发行版的提示不完全统一,匹配几个
/// 常见关键片段:英文 "incorrect password" / "Sorry, try again",中文 locale 下的
/// "密码不正确",以及 "a password is required"(密码为空或没读到)。
fn looks_like_bad_sudo_password(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("incorrect password")
        || s.contains("sorry, try again")
        || s.contains("a password is required")
        || s.contains("authentication failure")
        || stderr.contains("密码不正确")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::DockerInstallStatus;

    #[test]
    fn bad_password_detection_matches_common_phrasings() {
        assert!(looks_like_bad_sudo_password("sudo: 1 incorrect password attempt"));
        assert!(looks_like_bad_sudo_password("Sorry, try again."));
        assert!(looks_like_bad_sudo_password("sudo: a password is required"));
        assert!(looks_like_bad_sudo_password("密码不正确"));
        assert!(!looks_like_bad_sudo_password("curl: (6) could not resolve host"));
        assert!(!looks_like_bad_sudo_password(""));
    }

    #[test]
    fn report_constructors_set_status() {
        assert_eq!(
            DockerInstallReport::installed().status,
            DockerInstallStatus::Installed
        );
        assert_eq!(
            DockerInstallReport::need_sudo_password("x").status,
            DockerInstallStatus::NeedSudoPassword
        );
        assert_eq!(
            DockerInstallReport::already_installed("27.0").status,
            DockerInstallStatus::AlreadyInstalled
        );
    }
}
