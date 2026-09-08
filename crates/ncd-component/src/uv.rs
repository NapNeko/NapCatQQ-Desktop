//! UvComponent：Python 工具链 uv（单二进制，能按需拉托管 Python）
//!
//! 作为 Python 系应用端（NoneBot2）的运行时依赖，对位 Node 系的 NodeJsComponent。
//! 只装 uv 本体；解释器与虚拟环境由各应用实例目录内的 `uv sync` 按 `pyproject.toml` 自理。
//!
//! 支持矩阵：(Windows, Local) / (Linux, Local) / (Linux, Remote)
//!
//! 上游发行包（astral-sh/uv GitHub Releases，已核对）：
//! - Windows：`uv-<arch>-pc-windows-msvc.zip`，zip 根目录直接是 `uv.exe` / `uvx.exe` / `uvw.exe`
//! - Linux：`uv-<arch>-unknown-linux-gnu.tar.gz`，顶层一个 `uv-<triple>/` 目录，内含 `uv` / `uvx`
//!
//! 探测：组件目录 `<install_dir>/uv[.exe]` → PATH 上的 `uv`；`uv --version` 输出形如
//! `uv 0.12.8 (abc1234 2026-08-01)`，取第二个 token。

use async_trait::async_trait;

use ncd_host::{Arch, ArchiveKind, Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::requirement::Requirement;
use crate::shell_quote;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 默认锁定的 uv 版本（与上游 release tag 同形，不带 v）
pub const UV_DEFAULT_VERSION: &str = "0.12.8";

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

#[derive(Debug, Clone)]
pub struct UvComponent {
    /// 期望版本（如 "0.12.8"）
    pub version: String,
    /// 安装目录（本机 `data_root/components/Uv`，远端 `$HOME/ncd/tools/uv`）
    pub install_dir: HostPath,
    /// 下载模板；`{version}` / `{asset}` 占位。默认 GitHub Releases
    pub download_url_template: Option<String>,
    /// 远端临时目录（放归档 / 解压 stage）
    pub tmp_dir: HostPath,
}

impl UvComponent {
    pub fn new(version: impl Into<String>, install_dir: HostPath) -> Self {
        Self {
            version: version.into(),
            install_dir,
            download_url_template: None,
            tmp_dir: HostPath::from_posix("/tmp"),
        }
    }

    pub fn with_url_template(mut self, template: impl Into<String>) -> Self {
        self.download_url_template = Some(template.into());
        self
    }

    pub fn with_tmp_dir(mut self, tmp: HostPath) -> Self {
        self.tmp_dir = tmp;
        self
    }

    /// 远端默认安装目录：`$HOME/ncd/tools/uv`（与应用实例的 `$HOME/ncd/apps` 同根）
    pub fn default_remote_install_dir(home: &str) -> HostPath {
        HostPath::from_posix(ncd_domain::join_under(home, "ncd/tools/uv"))
    }

    pub fn uv_binary_path_for_os(install_dir: &HostPath, os: Os) -> HostPath {
        match os {
            Os::Windows => install_dir.join("uv.exe"),
            _ => install_dir.join("uv"),
        }
    }

    pub fn uv_binary_path(&self, host: &dyn Host) -> HostPath {
        Self::uv_binary_path_for_os(&self.install_dir, host.os())
    }

    /// 上游 target triple（决定资产名与 tar.gz 顶层目录名）
    pub fn target_triple(os: Os, arch: Arch) -> Result<String, ActionError> {
        let arch_s = match arch {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
            Arch::X86 => "i686",
            Arch::Armv7 => "armv7",
        };
        let triple = match (os, arch) {
            (Os::Windows, Arch::Armv7) => {
                return Err(ActionError::other("uv 不提供 Windows ARMv7 发行包"));
            }
            (Os::Windows, _) => format!("{arch_s}-pc-windows-msvc"),
            (Os::Linux, Arch::Armv7) => "armv7-unknown-linux-gnueabihf".to_string(),
            (Os::Linux, _) => format!("{arch_s}-unknown-linux-gnu"),
            (Os::MacOs, _) => format!("{arch_s}-apple-darwin"),
        };
        Ok(triple)
    }

    pub fn asset_name(os: Os, arch: Arch) -> Result<String, ActionError> {
        let triple = Self::target_triple(os, arch)?;
        let ext = if os == Os::Windows { "zip" } else { "tar.gz" };
        Ok(format!("uv-{triple}.{ext}"))
    }

    fn build_download_url(&self, host: &dyn Host) -> Result<String, ActionError> {
        let asset = Self::asset_name(host.os(), host.arch())?;
        let template = self.download_url_template.clone().unwrap_or_else(|| {
            "https://github.com/astral-sh/uv/releases/download/{version}/{asset}".to_string()
        });
        Ok(template
            .replace("{version}", &self.version)
            .replace("{asset}", &asset))
    }

    /// `uv 0.12.8 (hash date)` → `0.12.8`
    pub fn parse_version_output(stdout: &str) -> Option<String> {
        let mut it = stdout.split_whitespace();
        let head = it.next()?;
        if head != "uv" {
            return None;
        }
        it.next()
            .map(|s| s.trim_start_matches('v').to_string())
            .filter(|s| !s.is_empty())
    }

    async fn probe(
        &self,
        host: &dyn Host,
        program: &str,
        source: &str,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        let cmd = HostCommand::new(program).arg("--version");
        match host.run_to_string(cmd).await {
            Ok(out) if out.success() => Ok(Self::parse_version_output(&out.stdout).map(|version| {
                DetectedVersion {
                    version,
                    source: source.to_string(),
                }
            })),
            // PATH 里没有 uv 是常态，不算探测出错
            Ok(_) | Err(HostError::CommandFailed { .. }) | Err(HostError::Io(_)) => Ok(None),
            Err(e) => Err(ActionError::Host(e)),
        }
    }

    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::Uv,
            display_name: "uv".to_string(),
            description: "Python 包管理与解释器工具链".to_string(),
            repo_url: Some("https://github.com/astral-sh/uv".to_string()),
            supported_targets: SUPPORTED
                .iter()
                .map(|(os, loc)| crate::types::SupportedTarget::new(*os, *loc))
                .collect(),
            category: crate::types::ComponentCategory::RuntimeDep,
        }
    }
}

#[async_trait]
impl Component for UvComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Uv
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, os: Os, _locality: Locality) -> Vec<Requirement> {
        if os == Os::Linux {
            vec![Requirement::host_command("tar", "tar")]
        } else {
            Vec::new()
        }
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let managed = self.uv_binary_path(host);
        if host.exists(&managed).await? {
            if let Some(v) = self.probe(host, managed.as_posix(), managed.as_posix()).await? {
                return Ok(Some(v));
            }
        }
        self.probe(host, "uv", "$PATH/uv").await
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;

        // 1. 下载
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "下载 uv 发行包".into(),
        })
        .await;
        let url = self.build_download_url(host)?;
        let ext = if host.os() == Os::Windows { "zip" } else { "tar.gz" };
        let file_name = format!("ncd-uv-{}-{}.{ext}", self.version, std::process::id());
        let local_tmp = std::env::temp_dir().join(&file_name);
        let helper = DownloadHelper::new()?;
        let mirrors = ncd_network::build_mirror_urls(&url, None);
        helper
            .download_with_mirrors(&mirrors, &local_tmp, None, ctx, 1)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // 2. 上传到目标主机
        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "传输到目标主机".into(),
        })
        .await;
        let remote_archive = self.tmp_dir.join(&file_name);
        host.create_dir_all(&self.tmp_dir).await?;
        host.upload(&local_tmp, &remote_archive).await?;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        // 3. 解压到 stage
        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "解压".into(),
        })
        .await;
        let stage_dir = self
            .tmp_dir
            .join(format!("ncd-uv-stage-{}", std::process::id()));
        let _ = host.remove_dir_all(&stage_dir).await;
        host.create_dir_all(&stage_dir).await?;
        let kind = if host.os() == Os::Windows {
            ArchiveKind::Zip
        } else {
            ArchiveKind::TarGz
        };
        host.extract_archive(&remote_archive, &stage_dir, kind)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        // 4. 落到 install_dir
        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: format!("安装到 {}", self.install_dir.as_posix()),
        })
        .await;
        let _ = host.remove_dir_all(&self.install_dir).await;
        host.create_dir_all(&self.install_dir).await?;
        if host.os() == Os::Windows {
            // zip 根目录即二进制
            for name in ["uv.exe", "uvx.exe", "uvw.exe"] {
                let src = stage_dir.join(name);
                if host.exists(&src).await? {
                    let bytes = host.read_file(&src).await?;
                    host.write_file(&self.install_dir.join(name), &bytes).await?;
                }
            }
        } else {
            let triple = Self::target_triple(host.os(), host.arch())?;
            let root = shell_quote(stage_dir.join(format!("uv-{triple}")).as_posix());
            let dest = shell_quote(self.install_dir.as_posix());
            let mv = HostCommand::new("sh").arg("-c").arg(format!(
                "mv {root}/uv {root}/uvx {dest}/ && chmod +x {dest}/uv {dest}/uvx"
            ));
            let out = host.run_to_string(mv).await?;
            if !out.success() {
                return Err(ActionError::install_step(
                    "mv_install",
                    format!("exit={:?}: {}", out.exit_code, out.stderr.trim()),
                ));
            }
        }
        let _ = host.remove_dir_all(&stage_dir).await;
        let _ = host.remove_file(&remote_archive).await;

        let bin = self.uv_binary_path(host);
        if !host.exists(&bin).await? {
            return Err(ActionError::install_step(
                "verify",
                format!("安装后未找到 {}", bin.as_posix()),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("删除 {}", self.install_dir.as_posix()),
        })
        .await;
        if host.exists(&self.install_dir).await? {
            host.remove_dir_all(&self.install_dir).await?;
        } else {
            ctx.info("uv 独立组件目录不存在（当前为外部 / PATH 安装），无需删除。")
                .await;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.install(host, ctx).await
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let bin = self.uv_binary_path(host);
        let exists = host.exists(&bin).await?;
        let mut report =
            VerifyReport::ok().with_check("uv binary exists", exists, Some(bin.as_posix().to_string()));
        if exists {
            match self.probe(host, bin.as_posix(), bin.as_posix()).await? {
                Some(v) => {
                    report = report.with_check(
                        "version matches",
                        v.version == self.version,
                        Some(format!("expected={} actual={}", self.version, v.version)),
                    );
                }
                None => {
                    report = report.with_check("version executable", false, None);
                }
            }
        }
        Ok(report)
    }

    fn launch_command(&self, host: &dyn Host, args: &LaunchArgs) -> Result<HostCommand, ActionError> {
        Ok(args.apply_to(HostCommand::new(self.uv_binary_path(host).as_posix())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_match_upstream_release_layout() {
        assert_eq!(
            UvComponent::asset_name(Os::Windows, Arch::X86_64).unwrap(),
            "uv-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(
            UvComponent::asset_name(Os::Linux, Arch::X86_64).unwrap(),
            "uv-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            UvComponent::asset_name(Os::Linux, Arch::Aarch64).unwrap(),
            "uv-aarch64-unknown-linux-gnu.tar.gz"
        );
        assert!(UvComponent::asset_name(Os::Windows, Arch::Armv7).is_err());
    }

    #[test]
    fn version_output_parses_second_token() {
        assert_eq!(
            UvComponent::parse_version_output("uv 0.12.8 (0c1e2f3 2026-08-01)\n"),
            Some("0.12.8".to_string())
        );
        assert_eq!(UvComponent::parse_version_output("Python 3.12.1"), None);
        assert_eq!(UvComponent::parse_version_output(""), None);
    }

    #[test]
    fn binary_and_remote_dir_layout() {
        let dir = HostPath::from_posix("/home/u/ncd/tools/uv");
        assert_eq!(
            UvComponent::uv_binary_path_for_os(&dir, Os::Linux).as_posix(),
            "/home/u/ncd/tools/uv/uv"
        );
        assert_eq!(
            UvComponent::uv_binary_path_for_os(&dir, Os::Windows).as_posix(),
            "/home/u/ncd/tools/uv/uv.exe"
        );
        assert_eq!(
            UvComponent::default_remote_install_dir("/home/u").as_posix(),
            "/home/u/ncd/tools/uv"
        );
    }

    #[test]
    fn info_is_runtime_dep_without_macos() {
        let info = UvComponent::info();
        assert_eq!(info.id, ComponentId::Uv);
        assert_eq!(info.category, crate::types::ComponentCategory::RuntimeDep);
        assert!(!info.supported_targets.iter().any(|t| t.os == Os::MacOs));
    }
}
