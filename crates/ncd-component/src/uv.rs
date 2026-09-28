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

use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, Host, HostCommand, HostError, HostPath, HostShell, Locality, Os,
};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::requirement::Requirement;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 默认锁定的 uv 版本（与上游 release tag 同形，不带 v）
pub const UV_DEFAULT_VERSION: &str = "0.12.8";

/// UV_DEFAULT_VERSION 各发行包的 sha256，取自 GitHub release 的 asset digest。
/// 下载会走第三方镜像，只有钉在源码里的摘要挡得住被换过的包；升版本时这张表跟着一起换
const UV_DEFAULT_SHA256: &[(&str, &str)] = &[
    ("uv-x86_64-pc-windows-msvc.zip", "e07acf3f8a29fe41f9e04b799c3325cb0e0893836bb222bf102829b45c679ad6"),
    ("uv-aarch64-pc-windows-msvc.zip", "84b821c551802c200a32e25f9d1d960ef15e248f54f6a1bd9e1eb62934669da8"),
    ("uv-i686-pc-windows-msvc.zip", "9b38cad9b06e0a910e606510cdb4ad2c4eb4f320c4f4c4ba90dd13ed1115c5b0"),
    ("uv-x86_64-unknown-linux-gnu.tar.gz", "2e2b37e9811e17675a9e70bed5e1a58fc8c0388be63d751d72cc735188c149ff"),
    ("uv-aarch64-unknown-linux-gnu.tar.gz", "ba8661f4fd207c8e94814191598e619b355ac10d5014e851e21eb800f9ef2b00"),
    ("uv-i686-unknown-linux-gnu.tar.gz", "739cfea6b2958da57106e6ff1b0f95ecb17522ce84fc8e07c8606b2f427a4e39"),
    ("uv-armv7-unknown-linux-gnueabihf.tar.gz", "bc80826f631f8836a974a88b8cf797935bc83f15552828ad5de0195f6246e333"),
];

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

    /// 只有默认版本有内置摘要；换了版本（或下载模板）的调用方自己负责来源
    fn expected_sha256(&self, asset: &str) -> Option<&'static str> {
        if self.version != UV_DEFAULT_VERSION {
            return None;
        }
        UV_DEFAULT_SHA256
            .iter()
            .find(|(name, _)| *name == asset)
            .map(|(_, sha)| *sha)
    }

    fn build_download_url(&self, os: Os, arch: Arch) -> Result<String, ActionError> {
        let asset = Self::asset_name(os, arch)?;
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

        // 远端 Host::arch() 写死 x86_64，ARM 服务器上会传一个跑不起来的 uv 上去
        let arch = if host.locality() == Locality::Remote {
            match crate::ncd_watch::probe_remote_arch(host).await {
                Ok(arch) => arch,
                Err(e) => {
                    ctx.warn(format!("{e}；按 x86_64 装")).await;
                    host.arch()
                }
            }
        } else {
            host.arch()
        };

        // 1. 下载
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "下载 uv 发行包".into(),
        })
        .await;
        let url = self.build_download_url(host.os(), arch)?;
        let asset = Self::asset_name(host.os(), arch)?;
        let expected_sha256 = self.expected_sha256(&asset);
        if expected_sha256.is_none() {
            ctx.warn(format!("uv {} 没有内置校验值，下载后不做 sha256 校验", self.version))
                .await;
        }
        let ext = if host.os() == Os::Windows { "zip" } else { "tar.gz" };
        let file_name = format!("ncd-uv-{}-{}.{ext}", self.version, std::process::id());
        let local_tmp = std::env::temp_dir().join(&file_name);
        let helper = DownloadHelper::new()?;
        let mirrors = ncd_network::build_mirror_urls(&url, None);
        helper
            .download_with_mirrors(&mirrors, &local_tmp, expected_sha256, ctx, 1)
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
        // 攒到 install_dir 隔壁的 .new，装齐了再换过去：解压到一半失败也不会剩半个
        // 目录顶掉能用的旧 uv，换过去那一下只差一次改名
        let new_dir = match self.install_dir.parent() {
            Some(parent) => parent.join(format!(
                "{}.new",
                self.install_dir.file_name().unwrap_or("uv")
            )),
            None => self.install_dir.join(".new"),
        };
        let _ = host.remove_dir_all(&new_dir).await;
        host.create_dir_all(&new_dir).await?;
        if host.os() == Os::Windows {
            // zip 根目录即二进制
            for name in ["uv.exe", "uvx.exe", "uvw.exe"] {
                let src = stage_dir.join(name);
                if host.exists(&src).await? {
                    let bytes = host.read_file(&src).await?;
                    host.write_file(&new_dir.join(name), &bytes).await?;
                }
            }
        } else {
            let triple = Self::target_triple(host.os(), arch)?;
            let root = BashShell.escape(stage_dir.join(format!("uv-{triple}")).as_posix());
            let dest = BashShell.escape(new_dir.as_posix());
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
        let _ = host.remove_dir_all(&self.install_dir).await;
        host.rename(&new_dir, &self.install_dir).await?;
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

    // 漏一格的后果是那台机器静默不校验，装 uv 时按发行包名逐个过一遍
    #[test]
    fn default_version_pins_sha256_for_every_shipped_asset() {
        let comp = UvComponent::new(UV_DEFAULT_VERSION, HostPath::from_posix("/tmp/uv"));
        for os in [Os::Windows, Os::Linux] {
            for arch in [Arch::X86_64, Arch::Aarch64, Arch::X86, Arch::Armv7] {
                let Ok(asset) = UvComponent::asset_name(os, arch) else {
                    continue;
                };
                let sha = comp.expected_sha256(&asset);
                assert!(sha.is_some(), "{asset}");
                assert_eq!(sha.unwrap().len(), 64, "{asset}");
            }
        }
    }

    #[test]
    fn other_versions_have_no_pinned_sha256() {
        let comp = UvComponent::new("0.12.9", HostPath::from_posix("/tmp/uv"));
        assert!(comp.expected_sha256("uv-x86_64-unknown-linux-gnu.tar.gz").is_none());
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
