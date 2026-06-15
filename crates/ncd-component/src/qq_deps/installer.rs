//! QQ 系统依赖安装器。
//!
//! 处理提权、重试、进度上报等逻辑。

use ncd_domain::{FailedPackage, InstallDependenciesResult};
use ncd_host::{Host, HostCommand, HostError};
use ncd_host::remote::SudoAccess;

use crate::context::ActionCtx;
use crate::error::ActionError;

/// 包管理器类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManagerType {
    Apt,
    Dnf,
    Unknown,
}

/// QQ 依赖安装器。
pub struct QqDependencyInstaller;

impl QqDependencyInstaller {
    /// 自动安装缺失依赖。
    ///
    /// 会检查 sudo 权限，需要密码时通过 elevation_required 标志通知上层。
    pub async fn install(
        &self,
        host: &dyn Host,
        missing: Vec<String>,
        ctx: &mut ActionCtx,
    ) -> Result<InstallDependenciesResult, ActionError> {
        if missing.is_empty() {
            return Ok(InstallDependenciesResult {
                success: true,
                installed: vec![],
                failed: vec![],
                elevation_required: false,
            });
        }

        // 检查提权能力（返回 SudoAccess，不再直接报错）
        let sudo_access = self.check_sudo_access(host).await?;

        // 如果需要密码，返回 elevation_required=true，让上层弹窗
        if matches!(sudo_access, SudoAccess::PasswordRequired) {
            return Ok(InstallDependenciesResult {
                success: false,
                installed: vec![],
                failed: vec![],
                elevation_required: true,
            });
        }

        // 关键改进：一次性探测包管理器类型，避免重复探测
        let pkg_mgr = self.detect_package_manager(host).await;

        // 刷新包索引
        if let Err(e) = self.refresh_package_index(host, pkg_mgr).await {
            tracing::warn!("refresh package index failed: {e}");
        }

        let mut installed = Vec::new();
        let mut failed = Vec::new();

        // 批量安装（简化实现：逐个安装）
        for (idx, pkg) in missing.iter().enumerate() {
            ctx.emit(crate::context::ProgressKind::StepProgress {
                step: 0,
                percent: ((idx * 100) / missing.len()) as u8,
                message: format!("安装 {pkg}"),
                speed_bps: None,
                downloaded_bytes: None,
                total_bytes: None,
                download_stage: None,
                docker_layers: None,
            })
            .await;

            match self.install_package_with_retry(host, pkg, pkg_mgr, ctx).await {
                Ok(_) => installed.push(pkg.clone()),
                Err(e) => {
                    failed.push(FailedPackage {
                        name: pkg.clone(),
                        reason: e.to_string(),
                    });
                }
            }
        }

        Ok(InstallDependenciesResult {
            success: failed.is_empty(),
            installed,
            failed,
            elevation_required: false,
        })
    }

    /// 检查 sudo 访问（使用 ncd_host 的 probe_sudo）。
    async fn check_sudo_access(&self, host: &dyn Host) -> Result<SudoAccess, ActionError> {
        Ok(ncd_host::remote::probe_sudo(host).await)
    }

    /// 探测包管理器类型（一次性探测，避免重复）。
    async fn detect_package_manager(&self, host: &dyn Host) -> PackageManagerType {
        let cmd_check_apt = HostCommand::new("command").arg("-v").arg("apt-get");
        if host.run_to_string(cmd_check_apt).await.map_or(false, |o| o.success()) {
            return PackageManagerType::Apt;
        }

        let cmd_check_dnf = HostCommand::new("command").arg("-v").arg("dnf");
        if host.run_to_string(cmd_check_dnf).await.map_or(false, |o| o.success()) {
            return PackageManagerType::Dnf;
        }

        PackageManagerType::Unknown
    }

    /// 刷新包索引。
    async fn refresh_package_index(&self, host: &dyn Host, pkg_mgr: PackageManagerType) -> Result<(), HostError> {
        match pkg_mgr {
            PackageManagerType::Apt => {
                let cmd = HostCommand::new("sudo").arg("apt-get").arg("update").arg("-y").arg("-qq");
                host.run_to_string(cmd).await?;
            }
            PackageManagerType::Dnf => {
                let cmd = HostCommand::new("sudo").arg("dnf").arg("makecache").arg("--refresh");
                host.run_to_string(cmd).await?;
            }
            PackageManagerType::Unknown => {
                return Err(HostError::CommandFailed {
                    program: "package_manager".to_string(),
                    exit_code: None,
                    stderr: "未知的包管理器".to_string(),
                });
            }
        }
        Ok(())
    }

    /// 安装单个包（带网络重试，指数退避 5s → 10s → 20s）。
    async fn install_package_with_retry(
        &self,
        host: &dyn Host,
        package: &str,
        pkg_mgr: PackageManagerType,
        ctx: &mut ActionCtx,
    ) -> Result<(), HostError> {
        const MAX_RETRIES: u32 = 3;
        let mut last_err = None;

        for attempt in 0..MAX_RETRIES {
            match self.install_package(host, package, pkg_mgr).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    let is_network = is_network_error(&e);
                    last_err = Some(e);

                    // 只对网络错误重试，其他错误立即返回
                    if !is_network || attempt == MAX_RETRIES - 1 {
                        break;
                    }

                    let wait_secs = 5u64 * 2u64.pow(attempt);
                    ctx.warn(format!(
                        "{package} 安装失败（网络），{wait_secs}s 后重试 ({}/{MAX_RETRIES})",
                        attempt + 2
                    ))
                    .await;
                    tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| HostError::CommandFailed {
            program: "install".to_string(),
            exit_code: None,
            stderr: "unknown install error".to_string(),
        }))
    }

    /// 安装单个包。
    async fn install_package(&self, host: &dyn Host, package: &str, pkg_mgr: PackageManagerType) -> Result<(), HostError> {
        match pkg_mgr {
            PackageManagerType::Apt => {
                let cmd = HostCommand::new("sudo")
                    .arg("apt-get")
                    .arg("install")
                    .arg("-y")
                    .arg("-qq")
                    .arg(package);
                let output = host.run_to_string(cmd).await?;
                if !output.success() {
                    return Err(HostError::CommandFailed {
                        program: "apt-get".to_string(),
                        exit_code: output.exit_code,
                        stderr: output.stderr,
                    });
                }
            }
            PackageManagerType::Dnf => {
                let cmd = HostCommand::new("sudo")
                    .arg("dnf")
                    .arg("install")
                    .arg("-y")
                    .arg(package);
                let output = host.run_to_string(cmd).await?;
                if !output.success() {
                    return Err(HostError::CommandFailed {
                        program: "dnf".to_string(),
                        exit_code: output.exit_code,
                        stderr: output.stderr,
                    });
                }
            }
            PackageManagerType::Unknown => {
                return Err(HostError::CommandFailed {
                    program: "package_manager".to_string(),
                    exit_code: None,
                    stderr: "未知的包管理器，无法安装依赖".to_string(),
                });
            }
        }
        Ok(())
    }
}

/// 判断错误是否为网络相关（用于决定是否重试）。
fn is_network_error(err: &HostError) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("could not resolve")
        || msg.contains("connection")
        || msg.contains("timed out")
        || msg.contains("timeout")
        || msg.contains("temporary failure")
        || msg.contains("network")
        || msg.contains("unable to fetch")
        || msg.contains("failed to fetch")
}
