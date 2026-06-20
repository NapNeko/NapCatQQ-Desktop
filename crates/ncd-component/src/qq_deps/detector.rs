//! QQ 系统依赖检测器。
//!
//! 混合策略：QQ 已装时用 ldd 检测动态库，未装时用包管理器预检。

use ncd_domain::{
    DetectionMethod, DistroFamily, DistroInfo, PackageStatus, QqDependencyReport,
};
use ncd_host::{Host, HostCommand, HostPath};

use crate::error::ActionError;
use crate::qq_deps::QQDependencyManifest;

/// QQ 依赖检测器。
pub struct QqDependencyDetector {
    manifest: QQDependencyManifest,
}

impl QqDependencyDetector {
    pub fn new(manifest: QQDependencyManifest) -> Self {
        Self { manifest }
    }

    /// 检测 QQ 依赖状态。
    ///
    /// 策略：
    /// - 若 `qq_binary` 为 Some：用 ldd 检测动态库加载（最准确）
    /// - 若 `qq_binary` 为 None：用包管理器查询预定义清单
    pub async fn detect(
        &self,
        host: &dyn Host,
        qq_binary: Option<&HostPath>,
    ) -> Result<QqDependencyReport, ActionError> {
        let distro = self.detect_distro(host).await?;

        let (satisfied, missing) = if let Some(qq_path) = qq_binary {
            self.detect_via_ldd(host, qq_path, &distro).await?
        } else {
            self.detect_via_package_manager(host, &distro).await?
        };

        let install_cmd = self.build_install_command(&distro, &missing);

        Ok(QqDependencyReport {
            satisfied,
            missing,
            distro_info: distro,
            install_command: install_cmd,
        })
    }

    /// 探测发行版信息（读 /etc/os-release）。
    async fn detect_distro(&self, host: &dyn Host) -> Result<DistroInfo, ActionError> {
        let os_release = HostPath::from_posix("/etc/os-release");
        let content = host
            .read_file(&os_release)
            .await
            .map_err(|e| ActionError::other(format!("read os-release: {e}")))?;

        let text = String::from_utf8_lossy(&content);
        let mut name = String::new();
        let mut version = String::new();

        for line in text.lines() {
            if let Some(val) = line.strip_prefix("ID=") {
                name = val.trim_matches('"').to_string();
            } else if let Some(val) = line.strip_prefix("VERSION_ID=") {
                version = val.trim_matches('"').to_string();
            }
        }

        let family = match name.as_str() {
            "ubuntu" | "debian" | "linuxmint" => DistroFamily::Debian,
            "rhel" | "centos" | "fedora" | "rocky" | "almalinux" => DistroFamily::Rhel,
            "arch" | "manjaro" => DistroFamily::Arch,
            _ => DistroFamily::Unknown,
        };

        Ok(DistroInfo {
            family,
            name,
            version,
        })
    }

    /// 方案 A：ldd 检测（已装 QQ 时最准确）。
    async fn detect_via_ldd(
        &self,
        host: &dyn Host,
        qq_binary: &HostPath,
        _distro: &DistroInfo,
    ) -> Result<(Vec<PackageStatus>, Vec<PackageStatus>), ActionError> {
        let cmd = HostCommand::new("ldd").arg(qq_binary.as_posix());
        let output = host.run_to_string(cmd).await.map_err(|e| {
            ActionError::other(format!("ldd command failed: {e}"))
        })?;

        if !output.success() {
            return Err(ActionError::other(format!(
                "ldd failed: {}",
                output.stderr
            )));
        }

        // 解析 ldd 输出，找 "not found" 的库
        let missing_libs: Vec<String> = output
            .stdout
            .lines()
            .filter(|line| line.contains("not found"))
            .filter_map(|line| line.split_whitespace().next().map(|s| s.to_string()))
            .collect();

        let mut missing = Vec::new();
        for lib in &missing_libs {
            missing.push(PackageStatus {
                name: lib.clone(),
                installed_version: None,
                detection_method: DetectionMethod::Ldd,
            });
        }

        // ldd 只报缺失库，其余视为已满足（不逐一枚举已装项）
        let satisfied = vec![];

        Ok((satisfied, missing))
    }

    /// 方案 B：包管理器批量查询（未装 QQ 时预检）。
    async fn detect_via_package_manager(
        &self,
        host: &dyn Host,
        distro: &DistroInfo,
    ) -> Result<(Vec<PackageStatus>, Vec<PackageStatus>), ActionError> {
        let packages = self.resolve_package_names(host, distro).await?;
        let mut satisfied = Vec::new();
        let mut missing = Vec::new();

        // 根据发行版选择查询命令
        for pkg in packages {
            let installed = match distro.family {
                DistroFamily::Debian => self.check_deb_package(host, &pkg).await,
                DistroFamily::Rhel => self.check_rpm_package(host, &pkg).await,
                _ => false,
            };

            if installed {
                satisfied.push(PackageStatus {
                    name: pkg,
                    installed_version: None,
                    detection_method: DetectionMethod::PackageManager,
                });
            } else {
                missing.push(PackageStatus {
                    name: pkg,
                    installed_version: None,
                    detection_method: DetectionMethod::PackageManager,
                });
            }
        }

        Ok((satisfied, missing))
    }

    /// 检查 Debian/Ubuntu 包是否已安装。
    async fn check_deb_package(&self, host: &dyn Host, package: &str) -> bool {
        let cmd = HostCommand::new("dpkg-query")
            .arg("-W")
            .arg("-f=${Status}")
            .arg(package);

        if let Ok(output) = host.run_to_string(cmd).await {
            output.success() && output.stdout.contains("install ok installed")
        } else {
            false
        }
    }

    /// 检查 RHEL/CentOS 包是否已安装。
    async fn check_rpm_package(&self, host: &dyn Host, package: &str) -> bool {
        let cmd = HostCommand::new("rpm").arg("-q").arg(package);

        if let Ok(output) = host.run_to_string(cmd).await {
            output.success()
        } else {
            false
        }
    }

    /// 根据发行版解析实际要查询 / 安装的包名清单。
    ///
    /// Debian 系：对标记 has_t64_variant 的包逐个 `apt-cache show <pkg>t64` 探测，
    /// 存在 t64 变体就用 t64 名，否则回退原名。这跟官方 install.sh 一致，比按
    /// 版本号一刀切加后缀稳健——某个小众包若在当前发行版没 t64 变体，强制加后缀
    /// 会让 dpkg/apt 报 "unable to locate package"。
    async fn resolve_package_names(
        &self,
        host: &dyn Host,
        distro: &DistroInfo,
    ) -> Result<Vec<String>, ActionError> {
        match distro.family {
            DistroFamily::Debian => {
                let mut names = Vec::with_capacity(self.manifest.dependencies.len());
                for dep in &self.manifest.dependencies {
                    if dep.has_t64_variant
                        && self.t64_variant_available(host, &dep.debian_package).await
                    {
                        names.push(format!("{}t64", dep.debian_package));
                    } else {
                        names.push(dep.debian_package.clone());
                    }
                }
                Ok(names)
            }
            DistroFamily::Rhel => Ok(self
                .manifest
                .dependencies
                .iter()
                .map(|dep| dep.rhel_package.clone())
                .collect()),
            _ => Ok(Vec::new()),
        }
    }

    /// `apt-cache show <pkg>t64` 退出码 0 表示该 t64 变体在仓库里存在。
    async fn t64_variant_available(&self, host: &dyn Host, debian_package: &str) -> bool {
        let t64_name = format!("{debian_package}t64");
        let cmd = HostCommand::new("apt-cache").arg("show").arg(&t64_name);
        matches!(
            host.run_to_string(cmd).await,
            Ok(out) if out.success()
        )
    }

    /// 构建安装命令（用于用户复制粘贴）。
    fn build_install_command(&self, distro: &DistroInfo, missing: &[PackageStatus]) -> Option<String> {
        if missing.is_empty() {
            return None;
        }

        let pkg_names: Vec<String> = missing.iter().map(|p| p.name.clone()).collect();

        match distro.family {
            DistroFamily::Debian => Some(format!(
                "sudo apt-get install -y {}",
                pkg_names.join(" ")
            )),
            DistroFamily::Rhel => Some(format!("sudo dnf install -y {}", pkg_names.join(" "))),
            _ => None,
        }
    }
}
