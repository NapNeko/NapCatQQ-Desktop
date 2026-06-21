//! 帮用户装 docker(如果没有)
//!
//! Linux:不走官方 get.docker.com 一键脚本——get.docker.com / download.docker.com
//! 这两个官方域名在国内被墙(实测 curl 直接 Connection reset),连脚本本身都拉不下来,
//! 脚本里的 --mirror 参数根本没机会生效,改成在远端直接配阿里云 docker-ce 仓库走原生
//! apt/dnf 安装:全程只打 mirrors.aliyun.com(实测 HTTP 200),不碰任何 docker 官方域名
//!
//! 安装按阶段拆成多段 shell,每段单独跑并上报 apt/dnf 行级进度(见 install_progress)
//! apt 不用 -qq,保证 Get:/Fetched/Setting up 能进 parse_pkg_mgr_line
//!
//! Windows:不能静默装 Docker Desktop,只回一个引导结果让前端弹"去下载"

use ncd_domain::DockerInstallReport;
use ncd_host::Host;

use super::cli::DockerCliError;
use super::install_progress::InstallProgressEmit;

/// 兼容旧调用方的别名,结构化结果统一用 [DockerInstallReport]
pub type DockerInstallOutcome = DockerInstallReport;

/// Docker Desktop for Windows 下载页
pub(crate) const DOCKER_DESKTOP_URL: &str = "https://www.docker.com/products/docker-desktop/";

/// 阿里云 docker-ce 镜像根,apt 的 gpg/仓库,dnf 的 .repo 都从这里派生
const ALIYUN_DOCKER_CE: &str = "https://mirrors.aliyun.com/docker-ce";

/// 装 Docker 时分阶段执行的脚本(按顺序),每段独立超时,失败即停
pub(crate) fn docker_install_phases() -> Vec<(&'static str, String)> {
    let base = ALIYUN_DOCKER_CE;
    let mut phases: Vec<(&'static str, String)> = Vec::new();

    phases.push((
        "apt_prep",
        r#"set -e
export DEBIAN_FRONTEND=noninteractive
if ! command -v apt-get >/dev/null 2>&1; then exit 0; fi
apt-get update
apt-get install -y ca-certificates curl gnupg
"#
        .to_string(),
    ));

    phases.push((
        "apt_repo",
        format!(
            r#"set -e
if ! command -v apt-get >/dev/null 2>&1; then exit 0; fi
ALI="{base}"
install -m 0755 -d /etc/apt/keyrings
. /etc/os-release
DISTRO="$ID"
case "$DISTRO" in ubuntu|debian) : ;; *) DISTRO=ubuntu ;; esac
curl -fsSL "$ALI/linux/$DISTRO/gpg" | gpg --batch --yes --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg
CODENAME="$(. /etc/os-release && echo "$VERSION_CODENAME")"
ARCH="$(dpkg --print-architecture)"
echo "deb [arch=$ARCH signed-by=/etc/apt/keyrings/docker.gpg] $ALI/linux/$DISTRO $CODENAME stable" > /etc/apt/sources.list.d/docker.list
"#
        ),
    ));

    phases.push((
        "apt_install",
        r#"set -e
if ! command -v apt-get >/dev/null 2>&1; then exit 0; fi
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
"#
        .to_string(),
    ));

    phases.push((
        "dnf_install",
        format!(
            r#"set -e
if command -v apt-get >/dev/null 2>&1; then exit 0; fi
if ! command -v dnf >/dev/null 2>&1; then exit 0; fi
ALI="{base}"
dnf install -y dnf-plugins-core
dnf config-manager --add-repo "$ALI/linux/centos/docker-ce.repo"
sed -i "s#download.docker.com#mirrors.aliyun.com/docker-ce#g" /etc/yum.repos.d/docker-ce.repo
dnf install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
"#
        ),
    ));

    phases.push((
        "yum_install",
        format!(
            r#"set -e
if command -v apt-get >/dev/null 2>&1; then exit 0; fi
if command -v dnf >/dev/null 2>&1; then exit 0; fi
if ! command -v yum >/dev/null 2>&1; then exit 0; fi
ALI="{base}"
yum install -y yum-utils
yum-config-manager --add-repo "$ALI/linux/centos/docker-ce.repo"
sed -i "s#download.docker.com#mirrors.aliyun.com/docker-ce#g" /etc/yum.repos.d/docker-ce.repo
yum install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
"#
        ),
    ));

    phases.push((
        "pkgmgr_check",
        r#"set -e
if command -v apt-get >/dev/null 2>&1; then exit 0; fi
if command -v dnf >/dev/null 2>&1; then exit 0; fi
if command -v yum >/dev/null 2>&1; then exit 0; fi
echo "未识别到 apt-get / dnf / yum 包管理器,无法自动安装 Docker" >&2
exit 1
"#
        .to_string(),
    ));

    phases
}

/// 安装成功后写入 registry 加速(非交互),仅当尚无 daemon.json 或备份后覆盖
pub(crate) fn write_registry_mirrors_script() -> String {
    r#"set -e
mkdir -p /etc/docker
if [ -f /etc/docker/daemon.json ] && [ ! -f /etc/docker/daemon.json.ncd_bak ]; then
  cp /etc/docker/daemon.json /etc/docker/daemon.json.ncd_bak
fi
cat > /etc/docker/daemon.json <<'EOF'
{
  "registry-mirrors": [
    "https://docker.1ms.run",
    "https://docker.m.daocloud.io"
  ]
}
EOF
if command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload 2>/dev/null || true
  systemctl restart docker 2>/dev/null || true
fi
"#
    .to_string()
}

/// 在 host 上确保 docker 可用,没有就尝试装(无进度回调,供测试/旧调用方)
pub async fn install_docker(
    host: &dyn Host,
    sudo_password: Option<&str>,
) -> Result<DockerInstallReport, DockerCliError> {
    use std::sync::Arc;

    let noop: InstallProgressEmit = Arc::new(|_| {});
    super::install_progress::install_docker_with_progress(host, sudo_password, None, noop).await
}

/// 兼容单测:合并脚本文本(逻辑与分阶段一致)
#[cfg(test)]
pub(crate) fn aliyun_install_script() -> String {
    docker_install_phases()
        .into_iter()
        .map(|(_, s)| s)
        .collect::<Vec<_>>()
        .join("\n")
}

/// 从 sudo/stderr 粗判是不是密码错误
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
        assert!(looks_like_bad_sudo_password(
            "sudo: 1 incorrect password attempt"
        ));
        assert!(looks_like_bad_sudo_password("Sorry, try again."));
        assert!(looks_like_bad_sudo_password("sudo: a password is required"));
        assert!(looks_like_bad_sudo_password("密码不正确"));
        assert!(!looks_like_bad_sudo_password(
            "curl: (6) could not resolve host"
        ));
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
        assert!(
            !s.contains("get.docker.com"),
            "不该再依赖 get.docker.com: {s}"
        );
        assert!(
            s.contains("mirrors.aliyun.com/docker-ce"),
            "必须走阿里云镜像"
        );
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
        assert!(s.contains("command -v apt-get"), "缺 apt 分支");
        assert!(s.contains("command -v dnf"), "缺 dnf 分支");
        assert!(s.contains("command -v yum"), "缺 yum 分支");
        assert!(s.contains("exit 1"), "未识别包管理器必须报错退出");
    }

    #[test]
    fn install_script_installs_docker_ce_and_compose_plugin() {
        let s = aliyun_install_script();
        assert!(s.contains("docker-ce"), "必须装 docker-ce");
        assert!(
            s.contains("docker-compose-plugin"),
            "必须装 compose v2 插件"
        );
        assert!(
            s.contains("DEBIAN_FRONTEND=noninteractive"),
            "apt 必须非交互"
        );
        assert!(s.contains("set -e"), "出错即停,避免半装状态");
    }

    #[test]
    fn apt_phases_do_not_use_qq_silent() {
        for (name, script) in docker_install_phases() {
            if name.starts_with("apt_") {
                assert!(
                    !script.contains("-qq"),
                    "apt 阶段 {name} 不应使用 -qq,否则无安装进度输出"
                );
            }
        }
    }

    #[test]
    fn phases_are_non_empty() {
        assert!(docker_install_phases().len() >= 5);
    }
}
