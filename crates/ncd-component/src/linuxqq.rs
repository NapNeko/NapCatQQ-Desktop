//! `LinuxQQComponent`:LinuxQQ runtime 组件(rootless 安装)。
//!
//! 对齐 NapCat-Installer-main 官方一键脚本的 rootless 安装路径。
//!
//! 安装路径(对齐官方脚本 install.sh):
//! - `$INSTALL_BASE_DIR/opt/QQ/`:QQ 解压根
//! - `$INSTALL_BASE_DIR/opt/QQ/qq`:QQ 可执行
//! - `$INSTALL_BASE_DIR/opt/QQ/resources/app/package.json`:版本探测点
//!
//! 版本号说明:腾讯 LinuxQQ 没有"latest"端点,版本号 + hash segment 都是硬编码,
//! 改版时手动同步。当前(2026-05)锁定 `3.2.25-45758`(hash `7516007c`),与官方
//! 脚本和 legacy `_LINUXQQ_VERSION` 一致。
//!
//! 安装流程(rootless):
//! 1. 探测 dpkg-deb 或 rpm2cpio 哪个可用(用 `which` 或 `command -v`)
//! 2. 下载对应包 → 上传到远端 `<tmp>/linuxqq.<deb|rpm>`
//! 3. dpkg-deb -x 或 rpm2cpio | cpio -idm 解压到 `<install_base_dir>`
//! 4. 删除安装包,清理临时文件

use async_trait::async_trait;

use ncd_host::{Arch, Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 包格式(rootless 模式只需要 dpkg / rpm 两种)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageFormat {
    Deb,
    Rpm,
}

/// LinuxQQ component 配置。
#[derive(Debug, Clone)]
pub struct LinuxQQComponent {
    /// 版本号(如 "3.2.25-45758")
    pub version: String,
    /// 腾讯 CDN URL 中的 hash 段(如 "7516007c")
    pub url_hash_segment: String,
    /// 安装根目录(对齐官方 `$HOME/Napcat`)
    pub install_base_dir: HostPath,
    /// 期望 SHA256(可选,腾讯不提供官方 SHA256,通常为 None)
    pub expected_sha256: Option<String>,
    /// 临时目录
    pub tmp_dir: HostPath,
}

impl LinuxQQComponent {
    /// 创建一个 LinuxQQ component 描述(自定义所有字段)。
    pub fn new(
        version: impl Into<String>,
        url_hash_segment: impl Into<String>,
        install_base_dir: HostPath,
    ) -> Self {
        Self {
            version: version.into(),
            url_hash_segment: url_hash_segment.into(),
            install_base_dir,
            expected_sha256: None,
            tmp_dir: HostPath::from_posix("/tmp"),
        }
    }

    /// 默认配置:锁定已知版本 v3.2.25-45758,安装到 `$HOME/Napcat`。
    /// 腾讯改版后通过更新本默认值即可,业务代码不改。
    pub fn default_v3_2_25(install_base_dir: HostPath) -> Self {
        Self::new("3.2.25-45758", "7516007c", install_base_dir)
    }

    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.expected_sha256 = Some(sha256.into());
        self
    }

    pub fn with_tmp_dir(mut self, tmp: HostPath) -> Self {
        self.tmp_dir = tmp;
        self
    }

    fn package_filename(&self, pkg: PackageFormat, arch: Arch) -> Result<String, ActionError> {
        let arch_str = match (pkg, arch) {
            (PackageFormat::Deb, Arch::X86_64) => "amd64",
            (PackageFormat::Deb, Arch::Aarch64) => "arm64",
            (PackageFormat::Rpm, Arch::X86_64) => "x86_64",
            (PackageFormat::Rpm, Arch::Aarch64) => "aarch64",
            _ => {
                return Err(ActionError::UnsupportedTarget {
                    component: "linuxqq".into(),
                    os: Os::Linux,
                    locality: Locality::Remote,
                });
            }
        };
        let ext = match pkg {
            PackageFormat::Deb => "deb",
            PackageFormat::Rpm => "rpm",
        };
        Ok(format!(
            "linuxqq_{}_{arch_str}.{ext}",
            self.version
        ))
    }

    fn build_download_url(&self, pkg: PackageFormat, arch: Arch) -> Result<String, ActionError> {
        let filename = self.package_filename(pkg, arch)?;
        Ok(format!(
            "https://dldir1.qq.com/qqfile/qq/QQNT/{}/{filename}",
            self.url_hash_segment
        ))
    }

    /// 探测远端有 dpkg-deb 还是 rpm2cpio。dpkg 优先(deb 更普遍)。
    async fn detect_package_format(&self, host: &dyn Host) -> Result<PackageFormat, ActionError> {
        // 用 sh -c "command -v X" 探测,退出码 0 = 存在
        for (binary, fmt) in &[
            ("dpkg-deb", PackageFormat::Deb),
            ("rpm2cpio", PackageFormat::Rpm),
        ] {
            let cmd = HostCommand::new("sh")
                .arg("-c")
                .arg(format!("command -v {binary}"));
            if let Ok(out) = host.run_to_string(cmd).await {
                if out.success() && !out.stdout.trim().is_empty() {
                    return Ok(*fmt);
                }
            }
        }
        Err(ActionError::install_step(
            "detect_pkg_format",
            "neither dpkg-deb nor rpm2cpio found on host",
        ))
    }

    fn qq_base_path(&self) -> HostPath {
        self.install_base_dir.join("opt/QQ")
    }

    fn qq_executable(&self) -> HostPath {
        self.qq_base_path().join("qq")
    }

    fn qq_package_json(&self) -> HostPath {
        self.qq_base_path().join("resources/app/package.json")
    }
}

#[async_trait]
impl Component for LinuxQQComponent {
    fn id(&self) -> ComponentId {
        ComponentId::LinuxQq
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        // 仅 Linux,本地 / 远端都支持
        &[
            (Os::Linux, Locality::Local),
            (Os::Linux, Locality::Remote),
        ]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let pkg_json = self.qq_package_json();
        if !host.exists(&pkg_json).await? {
            return Ok(None);
        }

        let bytes = match host.read_file(&pkg_json).await {
            Ok(b) => b,
            Err(HostError::PathNotFound { .. }) => return Ok(None),
            Err(e) => return Err(ActionError::Host(e)),
        };

        let json: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
            ActionError::detect_failed("linuxqq", format!("parse package.json: {e}"))
        })?;

        let ver = json
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ActionError::detect_failed("linuxqq", "missing version field in package.json")
            })?;

        Ok(Some(DetectedVersion {
            version: ver.to_string(),
            source: format!("{pkg_json}"),
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;

        // ===== Step 1:探测包格式 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "detect package format".into(),
        })
        .await;
        let pkg_format = self.detect_package_format(host).await?;
        ctx.info(format!("package format: {pkg_format:?}")).await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // ===== Step 2:下载 LinuxQQ 包到本地 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "download LinuxQQ package".into(),
        })
        .await;
        let url = self.build_download_url(pkg_format, host.arch())?;
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-linuxqq-{}-{}.{}",
            self.version,
            std::process::id(),
            match pkg_format {
                PackageFormat::Deb => "deb",
                PackageFormat::Rpm => "rpm",
            }
        ));

        let helper = DownloadHelper::new()?;
        helper
            .download_to_file(&url, &local_tmp, self.expected_sha256.as_deref(), ctx, 2)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // ===== Step 3:上传到 host =====
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "upload package to host".into(),
        })
        .await;
        host.create_dir_all(&self.tmp_dir).await?;
        let remote_pkg = self.tmp_dir.join(format!(
            "ncd-linuxqq-{}.{}",
            std::process::id(),
            match pkg_format {
                PackageFormat::Deb => "deb",
                PackageFormat::Rpm => "rpm",
            }
        ));
        host.upload(&local_tmp, &remote_pkg).await?;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // ===== Step 4:rootless 解压 =====
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "extract LinuxQQ".into(),
        })
        .await;
        host.create_dir_all(&self.install_base_dir).await?;
        let install_base = self.install_base_dir.as_posix();
        let pkg_path = remote_pkg.as_posix();

        let extract_cmd = match pkg_format {
            PackageFormat::Deb => HostCommand::new("dpkg-deb")
                .arg("-x")
                .arg(pkg_path)
                .arg(install_base),
            PackageFormat::Rpm => {
                // rpm2cpio <pkg> | (cd <install_base> && cpio -idm)
                HostCommand::new("sh").arg("-c").arg(format!(
                    "rpm2cpio {} | (cd {} && cpio -idm)",
                    pkg_path, install_base
                ))
            }
        };
        let out = host.run_to_string(extract_cmd).await?;
        if !out.success() {
            return Err(ActionError::install_step(
                "extract_linuxqq",
                format!(
                    "exit={:?} stderr={}",
                    out.exit_code,
                    out.stderr.trim()
                ),
            ));
        }
        // 清理远端安装包
        let _ = host.remove_file(&remote_pkg).await;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let qq_bin = self.qq_executable();
        let pkg_json = self.qq_package_json();
        let bin_exists = host.exists(&qq_bin).await?;
        let json_exists = host.exists(&pkg_json).await?;

        let mut report = VerifyReport::ok()
            .with_check("qq executable exists", bin_exists, Some(format!("{qq_bin}")))
            .with_check(
                "package.json exists",
                json_exists,
                Some(format!("{pkg_json}")),
            );

        if json_exists {
            match self.detect(host).await {
                Ok(Some(v)) => {
                    let matches = v.version == self.version;
                    report = report.with_check(
                        "version matches",
                        matches,
                        Some(format!("expected={} actual={}", self.version, v.version)),
                    );
                }
                Ok(None) => {
                    report = report.with_check(
                        "version detect",
                        false,
                        Some("detect returned None despite package.json existing".into()),
                    );
                }
                Err(e) => {
                    report = report.with_check(
                        "version detect",
                        false,
                        Some(format!("{e}")),
                    );
                }
            }
        }
        Ok(report)
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        // QQ 启动命令:`<install_base>/opt/QQ/qq <extra_args>`
        // 实际生产场景往往还会加 `--no-sandbox` 等(由 backend 自己拼,不在 Component 这层)
        let mut cmd = HostCommand::new(self.qq_executable().as_posix());
        for a in &args.extra_args {
            cmd = cmd.arg(a);
        }
        for (k, v) in &args.extra_env {
            cmd = cmd.env(k, v);
        }
        if let Some(wd) = &args.working_dir {
            cmd = cmd.working_dir(wd.clone());
        }
        Ok(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp() -> LinuxQQComponent {
        LinuxQQComponent::default_v3_2_25(HostPath::from_posix("/home/test/Napcat"))
    }

    #[test]
    fn default_locks_known_version() {
        let c = comp();
        assert_eq!(c.version, "3.2.25-45758");
        assert_eq!(c.url_hash_segment, "7516007c");
    }

    #[test]
    fn package_filename_amd64_deb() {
        let c = comp();
        let name = c
            .package_filename(PackageFormat::Deb, Arch::X86_64)
            .unwrap();
        assert_eq!(name, "linuxqq_3.2.25-45758_amd64.deb");
    }

    #[test]
    fn package_filename_arm64_deb() {
        let c = comp();
        let name = c
            .package_filename(PackageFormat::Deb, Arch::Aarch64)
            .unwrap();
        assert_eq!(name, "linuxqq_3.2.25-45758_arm64.deb");
    }

    #[test]
    fn package_filename_x86_64_rpm() {
        let c = comp();
        let name = c
            .package_filename(PackageFormat::Rpm, Arch::X86_64)
            .unwrap();
        assert_eq!(name, "linuxqq_3.2.25-45758_x86_64.rpm");
    }

    #[test]
    fn package_filename_aarch64_rpm() {
        let c = comp();
        let name = c
            .package_filename(PackageFormat::Rpm, Arch::Aarch64)
            .unwrap();
        assert_eq!(name, "linuxqq_3.2.25-45758_aarch64.rpm");
    }

    #[test]
    fn package_filename_unsupported_arch_returns_error() {
        let c = comp();
        let err = c.package_filename(PackageFormat::Deb, Arch::X86).unwrap_err();
        assert!(matches!(err, ActionError::UnsupportedTarget { .. }));
    }

    #[test]
    fn download_url_format_matches_official() {
        let c = comp();
        let url = c.build_download_url(PackageFormat::Deb, Arch::X86_64).unwrap();
        // 与官方 install.sh L640 完全一致
        assert_eq!(
            url,
            "https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_amd64.deb"
        );
    }

    #[test]
    fn download_url_arm64_deb() {
        let c = comp();
        let url = c.build_download_url(PackageFormat::Deb, Arch::Aarch64).unwrap();
        assert_eq!(
            url,
            "https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_arm64.deb"
        );
    }

    #[test]
    fn paths_align_with_official_install_layout() {
        let c = comp();
        // 官方 install.sh L15 / L19 / L21
        assert_eq!(c.qq_base_path().as_posix(), "/home/test/Napcat/opt/QQ");
        assert_eq!(c.qq_executable().as_posix(), "/home/test/Napcat/opt/QQ/qq");
        assert_eq!(
            c.qq_package_json().as_posix(),
            "/home/test/Napcat/opt/QQ/resources/app/package.json"
        );
    }

    #[test]
    fn supported_targets_only_linux() {
        let c = comp();
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Local)));
        assert!(c.supported_targets().contains(&(Os::Linux, Locality::Remote)));
        assert!(!c.supported_targets().contains(&(Os::Windows, Locality::Local)));
    }

    #[test]
    fn id_returns_linuxqq() {
        assert_eq!(comp().id(), ComponentId::LinuxQq);
    }

    #[test]
    fn install_base_dir_can_be_custom() {
        let c = LinuxQQComponent::default_v3_2_25(HostPath::from_posix("/opt/napcat"));
        assert_eq!(c.qq_executable().as_posix(), "/opt/napcat/opt/QQ/qq");
    }
}
