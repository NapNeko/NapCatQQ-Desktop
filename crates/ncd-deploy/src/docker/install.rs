//! 帮用户装 docker(如果没有)。
//!
//! Linux:不走官方 get.docker.com 一键脚本——get.docker.com / download.docker.com
//! 这两个官方域名在国内被墙(实测 curl 直接 Connection reset),连脚本本身都拉不下来,
//! 脚本里的 --mirror 参数根本没机会生效。改成在远端直接配阿里云 docker-ce 仓库走原生
//! apt/dnf 安装:全程只打 mirrors.aliyun.com(实测 HTTP 200),不碰任何 docker 官方域名。
//!
//! 提权交给 Host 的固有能力——把 sudo 密码注入 host 后,所有 `.elevated()` 命令由
//! host 层统一决定走 sudo -S(有密码)还是 sudo -n(免密 / root)。密码来源:调用方
//! (docker 弹框)显式传入,或 ServerManager 连接时从 keyring 注入。都没有且远端确实
//! 要密码时返回 NeedSudoPassword,让上层弹框向用户索要后带密码重试。
//!
//! 安装脚本是一段自包含 POSIX sh:识别发行版(apt 系 / dnf-yum 系)后写对应的阿里云
//! 仓库再装 docker-ce + compose 插件。不识别的发行版返回 manual_required,不硬撑。
//!
//! Windows:不能静默装 Docker Desktop,只回一个引导结果让前端弹"去下载"。
//! macOS 同理(本工程目前不在 macOS 上跑 docker 部署,留个明确分支)。

use ncd_domain::DockerInstallReport;
use ncd_host::remote::{probe_sudo, SudoAccess};
use ncd_host::{Host, HostCommand, Os};
use tracing::info;

use super::cli::{DockerCli, DockerCliError};

/// 兼容旧调用方的别名。结构化结果统一用 [`DockerInstallReport`]。
pub type DockerInstallOutcome = DockerInstallReport;

/// Docker Desktop for Windows 下载页。
const DOCKER_DESKTOP_URL: &str = "https://www.docker.com/products/docker-desktop/";

/// 阿里云 docker-ce 镜像根。apt 的 gpg/仓库、dnf 的 .repo 都从这里派生。
/// 实测国内 HTTP 200,海外也可达;替代被墙的 download.docker.com。
const ALIYUN_DOCKER_CE: &str = "https://mirrors.aliyun.com/docker-ce";

/// 在 host 上确保 docker 可用,没有就尝试装。
///
/// sudo_password:上层从 keyring 取到的登录密码或用户在弹框输入的 sudo 密码。
/// None 表示这次没有可用密码——只能靠 root / 免密 sudo,不行就返回 NeedSudoPassword。
pub async fn install_docker(
    host: &dyn Host,
    sudo_password: Option<&str>,
) -> Result<DockerInstallReport, DockerCliError> {
    info!(
        target: "ncd_deploy::docker",
        os = ?host.os(),
        "install_docker"
    );
    let cli = DockerCli::new(host);

    // 已装 + daemon 在跑 + compose v2 插件齐全才算幂等就绪直接返回。只看
    // installed && daemon_running 会漏掉缺 compose 的机器:那种机器点「安装」会被
    // 当成"已安装"成功,但部署用 ready_to_deploy() 闸又会被 compose=false 挡住,
    // 用户无法自助补 compose。缺 compose 时继续往下跑安装脚本(脚本会装
    // docker-compose-plugin),把 compose 补齐。
    let status = cli.probe().await;
    if status.ready_to_deploy() {
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

/// Linux 装 docker:先把提权密码注入 host + 定提权方式,再跑阿里云原生安装脚本
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

    // 一段自包含脚本配阿里云仓库 + 装 docker-ce(提权)。识别发行版,全程只打
    // mirrors.aliyun.com。脚本不含外部用户输入,整体交给 host 层 sudo -S sh -c
    // 跑(单引号转义保护)。失败时 stderr 给上层拼错误文案。
    let run_script = HostCommand::new("sh")
        .arg("-c")
        .arg(aliyun_install_script())
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
                "安装脚本执行失败：{}。可登录远端按阿里云文档手动配置 docker-ce 仓库后重试。",
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

    // 把真实登录用户加进 docker 组,之后该用户重连就能免 sudo 跑 docker。
    // 不能用 $USER:这条经 sudo -S sh -c 跑,$USER 在 sudo 上下文里是 root,会把
    // root 加进组而漏掉真正的登录用户。用 $SUDO_USER(sudo 记录的原始调用者,正是
    // SSH 登录用户)优先,免密/root 直连无 SUDO_USER 时 fallback logname。加进组要
    // 重新登录才生效,所以本次会话的探测仍走 sudo 兜底(probe 已处理),不致命。
    let usermod = HostCommand::new("sh")
        .arg("-c")
        .arg("usermod -aG docker \"${SUDO_USER:-$(logname 2>/dev/null)}\"")
        .elevated()
        .timeout(std::time::Duration::from_secs(30));
    let _ = host.run_to_string(usermod).await;

    // 复探一次确认 docker + daemon + compose 都真起来了(ready_to_deploy 三者齐全)。
    let cli = DockerCli::new(host);
    let status = cli.probe().await;
    if status.ready_to_deploy() {
        Ok(DockerInstallReport::installed())
    } else if status.installed && status.daemon_running {
        // docker + daemon 起来了但 compose 插件没装上(极少数源缺包 / 装了旧 v1)。
        // 明确提示补 compose v2,而不是报"已就绪"骗用户去部署再失败。
        Ok(DockerInstallReport::manual_required(
            "Docker 已安装但缺 compose v2 插件，请在远端执行 sudo apt-get install -y docker-compose-plugin（dnf/yum 同名包）后重试",
            None,
        ))
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

/// 生成在远端以 root 跑的阿里云原生安装脚本(POSIX sh)。按包管理器分流:
/// apt 系(Debian/Ubuntu)写 keyring + docker.list 指向阿里云;dnf/yum 系
/// (CentOS/RHEL/Fedora)用 config-manager 加阿里云 .repo。都不是就报错退出。
///
/// 全程只下载 mirrors.aliyun.com,不碰被墙的 download.docker.com。脚本内无
/// 外部用户输入,$() 命令替换(架构 / codename)在远端展开。
fn aliyun_install_script() -> String {
    // apt 用 $ID 区分 ubuntu / debian 子路径(阿里云两套独立目录),codename 取
    // 自 /etc/os-release。dnf/yum 统一用阿里云 centos 的 docker-ce.repo,再把里头的
    // download.docker.com 就地换成阿里云(repo 文件默认指向官方域名,不换照样被墙)。
    format!(
        r#"set -e
ALI="{base}"
if command -v apt-get >/dev/null 2>&1; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq ca-certificates curl gnupg
  install -m 0755 -d /etc/apt/keyrings
  . /etc/os-release
  DISTRO="$ID"
  case "$DISTRO" in ubuntu|debian) : ;; *) DISTRO=ubuntu ;; esac
  curl -fsSL "$ALI/linux/$DISTRO/gpg" | gpg --batch --yes --dearmor -o /etc/apt/keyrings/docker.gpg
  chmod a+r /etc/apt/keyrings/docker.gpg
  CODENAME="$(. /etc/os-release && echo "$VERSION_CODENAME")"
  ARCH="$(dpkg --print-architecture)"
  echo "deb [arch=$ARCH signed-by=/etc/apt/keyrings/docker.gpg] $ALI/linux/$DISTRO $CODENAME stable" > /etc/apt/sources.list.d/docker.list
  apt-get update -qq
  apt-get install -y -qq docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
elif command -v dnf >/dev/null 2>&1; then
  dnf install -y dnf-plugins-core
  dnf config-manager --add-repo "$ALI/linux/centos/docker-ce.repo"
  sed -i "s#download.docker.com#mirrors.aliyun.com/docker-ce#g" /etc/yum.repos.d/docker-ce.repo
  dnf install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
elif command -v yum >/dev/null 2>&1; then
  yum install -y yum-utils
  yum-config-manager --add-repo "$ALI/linux/centos/docker-ce.repo"
  sed -i "s#download.docker.com#mirrors.aliyun.com/docker-ce#g" /etc/yum.repos.d/docker-ce.repo
  yum install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
else
  echo "未识别到 apt-get / dnf / yum 包管理器,无法自动安装 Docker" >&2
  exit 1
fi
"#,
        base = ALIYUN_DOCKER_CE,
    )
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

    #[test]
    fn install_script_uses_aliyun_not_official_docker_domain() {
        let s = aliyun_install_script();
        // 彻底不碰被墙的 get.docker.com(一键脚本域名)。
        assert!(!s.contains("get.docker.com"), "不该再依赖 get.docker.com: {s}");
        // 所有源都打阿里云。
        assert!(s.contains("mirrors.aliyun.com/docker-ce"), "必须走阿里云镜像");
        // download.docker.com 只能出现在 sed 替换式里(把 repo 文件里的官方域名换掉),
        // 不能作为实际下载地址。出现处必须紧跟 mirrors.aliyun.com 替换目标。
        for line in s.lines().filter(|l| l.contains("download.docker.com")) {
            assert!(
                line.trim_start().starts_with("sed"),
                "download.docker.com 只允许出现在 sed 替换行: {line}"
            );
        }
    }

    #[test]
    fn install_script_covers_all_package_managers() {
        let s = aliyun_install_script();
        // apt / dnf / yum 三系分支齐全,不识别时报错退出(exit 1)。
        assert!(s.contains("command -v apt-get"), "缺 apt 分支");
        assert!(s.contains("command -v dnf"), "缺 dnf 分支");
        assert!(s.contains("command -v yum"), "缺 yum 分支");
        assert!(s.contains("exit 1"), "未识别包管理器必须报错退出");
    }

    #[test]
    fn install_script_installs_docker_ce_and_compose_plugin() {
        let s = aliyun_install_script();
        // 装 docker-ce 全家桶 + compose v2 插件(部署 NapCat/SnowLuma 用 compose)。
        assert!(s.contains("docker-ce"), "必须装 docker-ce");
        assert!(s.contains("docker-compose-plugin"), "必须装 compose v2 插件");
        // 非交互,否则 SSH 会话里 apt/dnf 会卡在确认提示。
        assert!(s.contains("DEBIAN_FRONTEND=noninteractive"), "apt 必须非交互");
        assert!(s.contains("set -e"), "出错即停,避免半装状态");
    }
}
