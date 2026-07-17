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
//! 仅支持 Linux:Windows/macOS 不做 docker 部署

use ncd_domain::DockerInstallReport;
use ncd_host::Host;

use super::cli::DockerCliError;
use super::install_progress::InstallProgressEmit;

/// 兼容旧调用方的别名,结构化结果统一用 [DockerInstallReport]
pub type DockerInstallOutcome = DockerInstallReport;

/// 装 Docker 时分阶段执行的脚本(按顺序),每段独立超时,失败即停
pub(crate) const DOCKER_INSTALL_PHASES: &[(&str, &str)] = &[
    ("apt_prep", include_str!("../../scripts/docker/apt_prep.sh")),
    ("apt_repo", include_str!("../../scripts/docker/apt_repo.sh")),
    ("apt_install", include_str!("../../scripts/docker/apt_install.sh")),
    ("dnf_install", include_str!("../../scripts/docker/dnf_install.sh")),
    ("yum_install", include_str!("../../scripts/docker/yum_install.sh")),
    ("pkgmgr_check", include_str!("../../scripts/docker/pkgmgr_check.sh")),
];

/// 安装成功后写入 registry 加速(非交互),仅当尚无 daemon.json 或备份后覆盖
pub(crate) fn write_registry_mirrors_script() -> &'static str {
    include_str!("../../scripts/docker/write_registry_mirrors.sh")
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
    DOCKER_INSTALL_PHASES
        .iter()
        .map(|(_, s)| *s)
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
        for (name, script) in DOCKER_INSTALL_PHASES.iter() {
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
        assert!(DOCKER_INSTALL_PHASES.len() >= 5);
    }

    #[test]
    fn install_phase_scripts_have_no_cr() {
        // Windows checkout 若把脚本转成 CRLF,dash 会在 set -e\r 上报 Illegal option
        for (name, script) in DOCKER_INSTALL_PHASES.iter() {
            assert!(
                !script.contains('\r'),
                "阶段 {name} 含 CR,远端 /bin/sh(dash) 会失败"
            );
        }
        let mirrors = write_registry_mirrors_script();
        assert!(!mirrors.contains('\r'), "registry mirrors 脚本含 CR");
    }
}
