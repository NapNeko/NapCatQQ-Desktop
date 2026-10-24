// 启动预检把只读包查询合并到一次远端请求，不改变安装时的依赖解析。
use std::time::Duration;

use ncd_host::{CommandOutput, Locality, Os};
use serde::Deserialize;

use super::*;

const SCRIPT: &str = include_str!("../../resources/qq-deps-runtime.sh");
const PROBE_BUDGET: Duration = Duration::from_secs(12);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchReply {
    v: u32,
    packages: Vec<BatchPackage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchPackage {
    base: String,
    name: String,
    installed: bool,
}

impl QqDependencyDetector {
    pub async fn detect_runtime(&self, host: &dyn Host) -> Result<QqDependencyReport, ActionError> {
        if host.os() != Os::Linux || host.locality() != Locality::Remote {
            return self.detect(host, None).await;
        }
        let distro = self.detect_distro(host).await?;
        let (satisfied, missing) =
            if matches!(distro.family, DistroFamily::Debian | DistroFamily::Rhel) {
                let output = host
                    .run_to_string(self.runtime_batch_command(&distro)?)
                    .await?;
                self.parse_runtime_batch(&distro, output)?
            } else {
                self.detect_via_package_manager(host, &distro).await?
            };
        let install_command = self.build_install_command(&distro, &missing);
        Ok(QqDependencyReport {
            satisfied,
            missing,
            distro_info: distro,
            install_command,
        })
    }

    fn runtime_batch_command(&self, distro: &DistroInfo) -> Result<HostCommand, ActionError> {
        let family = match distro.family {
            DistroFamily::Debian => "debian",
            DistroFamily::Rhel => "rhel",
            _ => return Err(batch_error()),
        };
        let mut command = HostCommand::new("sh")
            .arg("-c")
            .arg(SCRIPT)
            .arg("ncd-qq-runtime-dependencies")
            .arg(family)
            .timeout(PROBE_BUDGET);
        for dependency in &self.manifest.dependencies {
            let name = match distro.family {
                DistroFamily::Debian => &dependency.debian_package,
                _ => &dependency.rhel_package,
            };
            if !LinuxPackageManager::is_valid_package_name(name) {
                return Err(batch_error());
            }
            command = command
                .arg(name)
                .arg(if dependency.has_t64_variant { "1" } else { "0" });
        }
        Ok(command)
    }

    fn parse_runtime_batch(
        &self,
        distro: &DistroInfo,
        output: CommandOutput,
    ) -> Result<(Vec<PackageStatus>, Vec<PackageStatus>), ActionError> {
        if !output.success() || output.stdout.len() > 64 * 1024 {
            return Err(batch_error());
        }
        let reply: BatchReply = serde_json::from_str(&output.stdout).map_err(|_| batch_error())?;
        if reply.v != 1 || reply.packages.len() != self.manifest.dependencies.len() {
            return Err(batch_error());
        }
        let mut satisfied = Vec::new();
        let mut missing = Vec::new();
        for (dependency, result) in self.manifest.dependencies.iter().zip(reply.packages) {
            let base = match distro.family {
                DistroFamily::Debian => &dependency.debian_package,
                DistroFamily::Rhel => &dependency.rhel_package,
                _ => return Err(batch_error()),
            };
            let allowed_alias = distro.family == DistroFamily::Debian
                && dependency.has_t64_variant
                && result.name == format!("{base}t64");
            if result.base != *base || (result.name != *base && !allowed_alias) {
                return Err(batch_error());
            }
            let status = PackageStatus {
                name: result.name,
                installed_version: None,
                detection_method: DetectionMethod::PackageManager,
            };
            if result.installed {
                satisfied.push(status);
            } else {
                missing.push(status);
            }
        }
        Ok((satisfied, missing))
    }
}

fn batch_error() -> ActionError {
    ActionError::detect_failed("qq_dependencies", "系统依赖批量探测未完成，请重新探测")
}

#[cfg(test)]
#[path = "runtime_batch_tests.rs"]
mod tests;
