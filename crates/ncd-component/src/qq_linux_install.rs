//! Linux QQ 安装权限与系统包安装、用户目录解包。

use super::*;
use ncd_host::CommandOutput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinuxInstallAccess {
    Rootless,
    SystemRoot,
    SystemSudo,
}

impl QQComponent {
    pub(super) async fn prepare_linux_install(
        &self,
        host: &dyn Host,
    ) -> Result<LinuxInstallAccess, ActionError> {
        if self.install_base_dir.as_posix() != "/" {
            return Ok(LinuxInstallAccess::Rootless);
        }

        let identity = host.run_to_string(HostCommand::new("id").arg("-u")).await?;
        if !identity.success() {
            return Err(ActionError::install_step(
                "qq_permissions",
                command_failure_detail(&identity),
            ));
        }
        if identity.stdout.trim() == "0" {
            return Ok(LinuxInstallAccess::SystemRoot);
        }

        // 下载前实际验证 Host 持有的密码；仅检查 has_elevation_password 会漏掉过期密码。
        let output = host
            .run_to_string(
                HostCommand::new("true")
                    .elevated()
                    .timeout(Duration::from_secs(30)),
            )
            .await
            .map_err(|error| map_elevation_error(error, LinuxInstallAccess::SystemSudo))?;
        if !output.success() {
            return Err(elevation_required(command_failure_detail(&output)));
        }
        Ok(LinuxInstallAccess::SystemSudo)
    }

    pub(super) async fn linux_package_format(
        &self,
        host: &dyn Host,
        access: LinuxInstallAccess,
    ) -> Result<PackageFormat, ActionError> {
        if access == LinuxInstallAccess::Rootless {
            return self.detect_package_format(host).await;
        }
        for (binary, format) in [("dpkg", PackageFormat::Deb), ("rpm", PackageFormat::Rpm)] {
            if host.command_exists(binary).await {
                return Ok(format);
            }
        }
        Err(ActionError::install_step(
            "detect_pkg_format",
            "系统 QQ 安装需要 dpkg 或 rpm，当前主机均不可用",
        ))
    }

    fn linux_package_command(
        &self,
        format: PackageFormat,
        package: &HostPath,
        access: LinuxInstallAccess,
    ) -> HostCommand {
        let package = package.as_posix();
        let base = self.install_base_dir.as_posix();
        let command = match (access, format) {
            (LinuxInstallAccess::Rootless, PackageFormat::Deb) => HostCommand::new("dpkg-deb")
                .arg("-x")
                .arg(package)
                .arg(base),
            (LinuxInstallAccess::Rootless, PackageFormat::Rpm) => {
                HostCommand::new("sh").arg("-c").arg(format!(
                    "rpm2cpio {} | (cd {} && cpio -idm)",
                    BashShell.escape(package),
                    BashShell.escape(base)
                ))
            }
            (_, PackageFormat::Deb) => {
                // 系统布局交给 dpkg 更新包数据库，并复用现有锁等待，不能直接解包到 /。
                let script = ncd_host::apt_lock::wrap_sh_script_with_dpkg_wait(&format!(
                    "DEBIAN_FRONTEND=noninteractive dpkg --install {}",
                    BashShell.escape(package)
                ));
                HostCommand::new("sh").arg("-c").arg(script)
            }
            (_, PackageFormat::Rpm) => HostCommand::new("rpm")
                .arg("--upgrade")
                .arg("--replacepkgs")
                .arg(package),
        }
        .timeout(Duration::from_secs(1800));

        if access == LinuxInstallAccess::SystemSudo {
            command.elevated()
        } else {
            command
        }
    }

    pub(super) async fn install_linux_package(
        &self,
        host: &dyn Host,
        format: PackageFormat,
        package: &HostPath,
        access: LinuxInstallAccess,
    ) -> Result<(), ActionError> {
        let command = self.linux_package_command(format, package, access);
        let output = host
            .run_to_string(command)
            .await
            .map_err(|error| map_elevation_error(error, access))?;
        if output.success() {
            return Ok(());
        }
        let detail = command_failure_detail(&output);
        if access == LinuxInstallAccess::SystemSudo
            && is_sudo_authentication_failure(&output.stderr)
        {
            return Err(elevation_required(detail));
        }
        Err(ActionError::install_step(
            if access == LinuxInstallAccess::Rootless {
                "extract_qq"
            } else {
                "install_qq"
            },
            detail,
        ))
    }
}

fn command_failure_detail(output: &CommandOutput) -> String {
    format!(
        "exit={:?} stderr={} stdout={}",
        output.exit_code,
        output.stderr.trim(),
        output.stdout.trim()
    )
}

fn elevation_required(detail: impl AsRef<str>) -> ActionError {
    ActionError::install_step(
        "qq_permissions",
        format!(
            "elevation_required: 安装或更新系统 QQ 需要 sudo 权限，请输入有效的 sudo 密码后重试。{}",
            detail.as_ref()
        ),
    )
}

fn is_sudo_authentication_failure(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    let has_sudo_error = stderr
        .lines()
        .any(|line| line.trim_start().starts_with("sudo:"));
    has_sudo_error
        && [
            "incorrect password",
            "sorry, try again",
            "a password is required",
            "no password was provided",
            "authentication failure",
            "a terminal is required",
            "no tty present",
            "must have a tty",
            "not in the sudoers",
            "not allowed to execute",
            "may not run sudo",
            "密码不正确",
            "需要密码",
            "不在 sudoers",
        ]
        .into_iter()
        .any(|message| stderr.contains(message))
}

fn map_elevation_error(error: HostError, access: LinuxInstallAccess) -> ActionError {
    if access == LinuxInstallAccess::SystemSudo
        && matches!(error, HostError::ElevationFailed { .. })
    {
        elevation_required(error.to_string())
    } else {
        ActionError::Host(error)
    }
}

#[cfg(test)]
#[path = "qq_linux_tests.rs"]
mod tests;
