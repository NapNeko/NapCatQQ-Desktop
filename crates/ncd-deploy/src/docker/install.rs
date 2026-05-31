//! 帮用户装 docker(如果没有)。
//!
//! Linux:把官方 get.docker.com 脚本先落盘再执行,提权策略自适应——能 root /
//! 免密 sudo 就不要密码,否则用调用方传进来的 sudo 密码走 sudo -S。都给不出
//! 密码时返回 NeedSudoPassword,让上层弹框向用户索要后带密码重试。
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

/// 解析后的提权方式:要么直接知道不用密码,要么带上这次要喂的密码,要么放弃。
enum Elevation<'a> {
    /// root 或免密 sudo:提权命令不带密码(sudo -n / 无 sudo)。
    NoPassword,
    /// 需要且拿到了密码:提权命令走 sudo -S,密码喂 stdin。
    WithPassword(&'a str),
    /// 需要密码但一个都没有:让上层去要。
    Need,
}

/// Linux 装 docker:先定提权方式,再 curl 落盘 + 执行脚本 + 起 daemon + 加用户组。
async fn install_docker_linux(
    host: &dyn Host,
    sudo_password: Option<&str>,
) -> Result<DockerInstallReport, DockerCliError> {
    // 第一档:探主机本身的 sudo 能力。root / 免密直接无密码提权。
    let elevation = match probe_sudo(host).await {
        SudoAccess::RootAlready | SudoAccess::Passwordless => Elevation::NoPassword,
        // 第二、三档:需要密码。有就用,没有就让上层弹框去要。
        SudoAccess::PasswordRequired => match sudo_password {
            Some(pw) => Elevation::WithPassword(pw),
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
    let run_script = elevated(
        HostCommand::new("sh").arg(INSTALL_SCRIPT_PATH),
        &elevation,
    )
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
                "安装脚本执行失败：{}。可登录远端手动执行 curl -fsSL https://get.docker.com | sudo sh",
                out.stderr.trim()
            ),
            None,
        ));
    }

    // 起 daemon 并设开机自启。失败不致命(部分容器化环境没 systemd),忽略错误,
    // 末尾用复探判定 daemon 真实状态。
    let enable = elevated(
        HostCommand::new("systemctl").arg("enable").arg("--now").arg("docker"),
        &elevation,
    )
    .timeout(std::time::Duration::from_secs(60));
    let _ = host.run_to_string(enable).await;

    // 把当前用户加进 docker 组,免得每条 docker 命令都要 sudo。需要重新登录才
    // 完全生效,这步失败也不致命。用 $USER 取当前登录名。
    let usermod = elevated(
        HostCommand::new("sh")
            .arg("-c")
            .arg("usermod -aG docker \"$USER\""),
        &elevation,
    )
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

/// 按提权方式给命令打标:无密码档只标 .elevated()(host 层走 sudo -n);有密码档
/// 标 .elevated() 并把密码塞进 stdin(host 层走 sudo -S 从 stdin 读)。
fn elevated(cmd: HostCommand, elevation: &Elevation<'_>) -> HostCommand {
    match elevation {
        Elevation::NoPassword => cmd.elevated(),
        Elevation::WithPassword(pw) => cmd.elevated().stdin(pw.as_bytes().to_vec()),
        // Need 在调用前已被拦截返回,不会走到这里。
        Elevation::Need => cmd,
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
    fn elevated_no_password_only_marks_flag() {
        let cmd = elevated(HostCommand::new("sh").arg("x"), &Elevation::NoPassword);
        assert!(cmd.elevated);
        assert!(cmd.stdin.is_none());
    }

    #[test]
    fn elevated_with_password_feeds_stdin() {
        let cmd = elevated(HostCommand::new("sh").arg("x"), &Elevation::WithPassword("hunter2"));
        assert!(cmd.elevated);
        assert_eq!(cmd.stdin.as_deref(), Some(b"hunter2".as_slice()));
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
