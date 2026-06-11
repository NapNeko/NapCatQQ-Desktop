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
use ncd_host::Host;

use super::cli::DockerCliError;
use super::install_progress::InstallProgressEmit;

/// 兼容旧调用方的别名。结构化结果统一用 [`DockerInstallReport`]。
pub type DockerInstallOutcome = DockerInstallReport;

/// Docker Desktop for Windows 下载页。
pub(crate) const DOCKER_DESKTOP_URL: &str = "https://www.docker.com/products/docker-desktop/";

/// 阿里云 docker-ce 镜像根。apt 的 gpg/仓库、dnf 的 .repo 都从这里派生。
/// 实测国内 HTTP 200,海外也可达;替代被墙的 download.docker.com。
const ALIYUN_DOCKER_CE: &str = "https://mirrors.aliyun.com/docker-ce";

/// 在 host 上确保 docker 可用,没有就尝试装（无进度回调，供测试/旧调用方）。
pub async fn install_docker(
    host: &dyn Host,
    sudo_password: Option<&str>,
) -> Result<DockerInstallReport, DockerCliError> {
    use std::sync::Arc;

    let noop: InstallProgressEmit = Arc::new(|_| {});
    super::install_progress::install_docker_with_progress(host, sudo_password, None, noop).await
}

/// 生成在远端以 root 跑的阿里云原生安装脚本(POSIX sh)。按包管理器分流:
/// apt 系(Debian/Ubuntu)写 keyring + docker.list 指向阿里云;dnf/yum 系
/// (CentOS/RHEL/Fedora)用 config-manager 加阿里云 .repo。都不是就报错退出。
///
/// 全程只下载 mirrors.aliyun.com,不碰被墙的 download.docker.com。脚本内无
/// 外部用户输入,$() 命令替换(架构 / codename)在远端展开。
pub(crate) fn aliyun_install_script() -> String {
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
pub(crate) fn looks_like_bad_sudo_password(stderr: &str) -> bool {
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
