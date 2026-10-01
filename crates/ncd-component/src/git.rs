//! GitComponent：git 命令行
//!
//! 云崽（TRSS-Yunzai）装本体、装和更新插件都是 git clone / pull，群里的 `#更新` `#安装插件`
//! 也是它自己调 git，所以 git 是这个框架的运行时依赖，不只是安装期工具。
//!
//! 支持矩阵：(Windows, Local) / (Linux, Local) / (Linux, Remote)
//!
//! - Windows：先认 PATH 上的 git（装过 Git for Windows 的机器很多）；没有就下 MinGit
//!   （git-for-windows 发行的便携包，zip 根目录下 `cmd/git.exe`）放到组件目录
//! - Linux：git 是系统包，只认 PATH；缺了走包管理器装（要 root / 免密 sudo / 注入过提权密码）
//!
//! 探测输出形如 `git version 2.56.0.windows.1` / `git version 2.43.0`，取第三个 token 的前三段。

use async_trait::async_trait;

use ncd_host::linux_pkg::{EnsureCommandError, install_missing_command, plan_missing_command};
use ncd_host::{Arch, ArchiveKind, Host, HostCommand, HostError, HostPath, Locality, Os};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::requirement::Requirement;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

/// 默认锁定的 MinGit 版本（git-for-windows 的 tag 是 `v<ver>.windows.1`）
pub const MINGIT_DEFAULT_VERSION: &str = "2.56.0";
const MINGIT_TAG_SUFFIX: &str = ".windows.1";

/// MINGIT_DEFAULT_VERSION 各发行包的 sha256，取自 GitHub release 的 asset digest。
/// 下载会走镜像，只有钉在源码里的摘要挡得住被换过的包；升版本时这张表跟着一起换
const MINGIT_DEFAULT_SHA256: &[(&str, &str)] = &[
    (
        "MinGit-2.56.0-64-bit.zip",
        "064b440ff870ed5198527e8f3a92cdf5bd2fd0fedf5e718af95e3fdaddeff718",
    ),
    (
        "MinGit-2.56.0-arm64.zip",
        "cb3b0f2d486ea52673227151a5baf5bc13861ff80e74e94e46d614d1bfcd5c06",
    ),
    (
        "MinGit-2.56.0-32-bit.zip",
        "9f8266486c8818b91cbb6b719e35972a406f7560e86b82fbda2f3dcb7c069f12",
    ),
];

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

#[derive(Debug, Clone)]
pub struct GitComponent {
    /// MinGit 版本（只有 Windows 用得上）
    pub version: String,
    /// Windows 组件目录（`data_root/components/Git`）；Linux 上不用
    pub install_dir: HostPath,
}

impl GitComponent {
    pub fn new(version: impl Into<String>, install_dir: HostPath) -> Self {
        Self {
            version: version.into(),
            install_dir,
        }
    }

    /// 桌面端自己装的 git 在哪；Linux 上 git 是系统包，没有这一处
    pub fn managed_binary_path_for_os(install_dir: &HostPath, os: Os) -> Option<HostPath> {
        match os {
            Os::Windows => Some(install_dir.join("cmd").join("git.exe")),
            _ => None,
        }
    }

    pub fn asset_name(version: &str, arch: Arch) -> Result<String, ActionError> {
        let flavor = match arch {
            Arch::X86_64 => "64-bit",
            Arch::Aarch64 => "arm64",
            Arch::X86 => "32-bit",
            Arch::Armv7 => return Err(ActionError::other("MinGit 不提供 ARMv7 发行包")),
        };
        Ok(format!("MinGit-{version}-{flavor}.zip"))
    }

    fn download_url(&self, arch: Arch) -> Result<String, ActionError> {
        let asset = Self::asset_name(&self.version, arch)?;
        Ok(format!(
            "https://github.com/git-for-windows/git/releases/download/v{}{MINGIT_TAG_SUFFIX}/{asset}",
            self.version
        ))
    }

    fn expected_sha256(&self, asset: &str) -> Option<&'static str> {
        if self.version != MINGIT_DEFAULT_VERSION {
            return None;
        }
        MINGIT_DEFAULT_SHA256
            .iter()
            .find(|(name, _)| *name == asset)
            .map(|(_, sha)| *sha)
    }

    /// `git version 2.56.0.windows.1` → `2.56.0`
    pub fn parse_version_output(stdout: &str) -> Option<String> {
        let mut it = stdout.split_whitespace();
        if it.next()? != "git" || it.next()? != "version" {
            return None;
        }
        let raw = it.next()?;
        let parts: Vec<&str> = raw
            .split('.')
            .take_while(|p| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty())
            .take(3)
            .collect();
        (!parts.is_empty()).then(|| parts.join("."))
    }

    async fn probe(
        &self,
        host: &dyn Host,
        program: &str,
        source: &str,
    ) -> Result<Option<DetectedVersion>, ActionError> {
        let cmd = HostCommand::new(program).arg("--version");
        match host.run_to_string(cmd).await {
            Ok(out) if out.success() => Ok(Self::parse_version_output(&out.stdout).map(
                |version| DetectedVersion {
                    version,
                    source: source.to_string(),
                },
            )),
            // PATH 里没有 git 是常态，不算探测出错
            Ok(_) | Err(HostError::CommandFailed { .. }) | Err(HostError::Io(_)) => Ok(None),
            Err(e) => Err(ActionError::Host(e)),
        }
    }

    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::Git,
            display_name: "Git".to_string(),
            description: "版本管理工具，云崽装本体和插件要用".to_string(),
            repo_url: Some("https://git-scm.com".to_string()),
            supported_targets: SUPPORTED
                .iter()
                .map(|(os, loc)| crate::types::SupportedTarget::new(*os, *loc))
                .collect(),
            category: crate::types::ComponentCategory::RuntimeDep,
            uninstall: crate::types::UninstallSupport::Supported,
        }
    }

    async fn install_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "下载 MinGit".into(),
        })
        .await;
        let arch = host.arch();
        let url = self.download_url(arch)?;
        let asset = Self::asset_name(&self.version, arch)?;
        let expected = self.expected_sha256(&asset);
        if expected.is_none() {
            ctx.warn(format!(
                "MinGit {} 没有内置校验值，下载后不做 sha256 校验",
                self.version
            ))
            .await;
        }
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-mingit-{}-{}.zip",
            self.version,
            std::process::id()
        ));
        let helper = DownloadHelper::new()?;
        let mirrors = ncd_network::build_mirror_urls(&url, None);
        helper
            .download_with_mirrors(&mirrors, &local_tmp, expected, ctx, 1)
            .await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "解压".into(),
        })
        .await;
        // 解到隔壁的 .new，装齐了再换过去：解到一半失败不会剩半个目录顶掉能用的旧版
        let new_dir = sibling_dir(&self.install_dir, "new");
        let _ = host.remove_dir_all(&new_dir).await;
        host.create_dir_all(&new_dir).await?;
        let archive = HostPath::from_windows(local_tmp.to_string_lossy().as_ref());
        let extracted = host
            .extract_archive(&archive, &new_dir, ArchiveKind::Zip)
            .await;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        extracted?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: format!("安装到 {}", self.install_dir.as_posix()),
        })
        .await;
        let _ = host.remove_dir_all(&self.install_dir).await;
        host.rename(&new_dir, &self.install_dir).await?;
        let bin = self.install_dir.join("cmd").join("git.exe");
        if self
            .probe(host, bin.as_posix(), bin.as_posix())
            .await?
            .is_none()
        {
            return Err(ActionError::install_step(
                "verify",
                format!("安装后 {} 跑不起来", bin.as_posix()),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn install_linux(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "用包管理器安装 git".into(),
        })
        .await;
        let pm = match plan_missing_command(host, "git").await {
            Ok(None) => {
                ctx.info("git 已可用").await;
                ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
                ctx.emit(ProgressKind::Finished { ok: true }).await;
                return Ok(());
            }
            Ok(Some(pm)) => pm,
            Err(e) => return Err(ensure_error("git", e)),
        };
        ctx.info(format!("通过 {} 安装 git", pm.binary())).await;
        install_missing_command(host, pm, "git", "git")
            .await
            .map_err(|e| ensure_error("git", e))?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }
}

/// 包管理器补装失败时给用户的话：说清楚卡在哪、手动怎么装
pub(crate) fn ensure_error(what: &str, err: EnsureCommandError) -> ActionError {
    let msg = match err {
        EnsureCommandError::NoPackageManager => {
            format!("缺少 {what}，而且没认出这台机器的包管理器，请手动安装后重试")
        }
        EnsureCommandError::NeedsElevation(pm) => format!(
            "缺少 {what}，自动安装要 root 或免密 sudo。请手动执行 sudo {} 后重试，或在远端主机设置里填写 sudo 密码",
            pm.install_hint(&[what])
        ),
        EnsureCommandError::StillMissing { pm, detail } => format!(
            "用 {} 安装 {what} 后仍找不到命令（{detail}），请手动安装后重试",
            pm.binary()
        ),
    };
    ActionError::install_step("ensure_command", msg)
}

/// `<parent>/<name>.<suffix>`，和目标同盘，好直接改名换过去
pub(crate) fn sibling_dir(dir: &HostPath, suffix: &str) -> HostPath {
    let name = dir.file_name().unwrap_or("component").to_string();
    match dir.parent() {
        Some(parent) => parent.join(format!("{name}.{suffix}")),
        None => dir.join(format!(".{suffix}")),
    }
}

#[async_trait]
impl Component for GitComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Git
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, _os: Os, _locality: Locality) -> Vec<Requirement> {
        Vec::new()
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        if let Some(managed) = Self::managed_binary_path_for_os(&self.install_dir, host.os()) {
            if host.exists(&managed).await? {
                if let Some(v) = self
                    .probe(host, managed.as_posix(), managed.as_posix())
                    .await?
                {
                    return Ok(Some(v));
                }
            }
        }
        self.probe(host, "git", "$PATH/git").await
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        match host.os() {
            Os::Windows => self.install_windows(host, ctx).await,
            _ => self.install_linux(host, ctx).await,
        }
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: format!("删除 {}", self.install_dir.as_posix()),
        })
        .await;
        if host.os() == Os::Windows && host.exists(&self.install_dir).await? {
            host.remove_dir_all(&self.install_dir).await?;
        } else {
            ctx.info("git 是系统自带或另装的，桌面端不卸载").await;
        }
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.install(host, ctx).await
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let found = self.detect(host).await?;
        Ok(VerifyReport::ok().with_check(
            "git runs",
            found.is_some(),
            found.map(|v| format!("{} ({})", v.version, v.source)),
        ))
    }

    fn launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let program = Self::managed_binary_path_for_os(&self.install_dir, host.os())
            .map(|p| p.as_posix().to_string())
            .unwrap_or_else(|| "git".to_string());
        Ok(args.apply_to(HostCommand::new(program)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_output_keeps_three_numeric_parts() {
        assert_eq!(
            GitComponent::parse_version_output("git version 2.56.0.windows.1\n"),
            Some("2.56.0".into())
        );
        assert_eq!(
            GitComponent::parse_version_output("git version 2.43.0"),
            Some("2.43.0".into())
        );
        assert_eq!(
            GitComponent::parse_version_output("git version 2.39.5 (Apple Git-154)"),
            Some("2.39.5".into())
        );
        assert_eq!(
            GitComponent::parse_version_output("svn, version 1.14"),
            None
        );
        assert_eq!(GitComponent::parse_version_output(""), None);
    }

    #[test]
    fn asset_names_follow_git_for_windows_release() {
        assert_eq!(
            GitComponent::asset_name("2.56.0", Arch::X86_64).unwrap(),
            "MinGit-2.56.0-64-bit.zip"
        );
        assert_eq!(
            GitComponent::asset_name("2.56.0", Arch::Aarch64).unwrap(),
            "MinGit-2.56.0-arm64.zip"
        );
        assert!(GitComponent::asset_name("2.56.0", Arch::Armv7).is_err());
        let comp = GitComponent::new(MINGIT_DEFAULT_VERSION, HostPath::from_posix("/x"));
        assert_eq!(
            comp.download_url(Arch::X86_64).unwrap(),
            "https://github.com/git-for-windows/git/releases/download/v2.56.0.windows.1/MinGit-2.56.0-64-bit.zip"
        );
    }

    // 漏一格的后果是那台机器静默不校验
    #[test]
    fn default_version_pins_sha256_for_every_windows_asset() {
        let comp = GitComponent::new(MINGIT_DEFAULT_VERSION, HostPath::from_posix("/x"));
        for arch in [Arch::X86_64, Arch::Aarch64, Arch::X86] {
            let asset = GitComponent::asset_name(MINGIT_DEFAULT_VERSION, arch).unwrap();
            let sha = comp.expected_sha256(&asset).expect(&asset);
            assert_eq!(sha.len(), 64, "{asset}");
        }
        let other = GitComponent::new("2.57.0", HostPath::from_posix("/x"));
        assert!(other.expected_sha256("MinGit-2.57.0-64-bit.zip").is_none());
    }

    #[test]
    fn managed_binary_only_on_windows() {
        let dir = HostPath::from_windows(r"C:\ProgramData\NapCatQQ Desktop\components\Git");
        assert_eq!(
            GitComponent::managed_binary_path_for_os(&dir, Os::Windows)
                .unwrap()
                .as_posix(),
            "/c/ProgramData/NapCatQQ Desktop/components/Git/cmd/git.exe"
        );
        assert!(GitComponent::managed_binary_path_for_os(&dir, Os::Linux).is_none());
    }

    #[test]
    fn sibling_dir_sits_next_to_target() {
        let dir = HostPath::from_posix("/data/components/Git");
        assert_eq!(
            sibling_dir(&dir, "new").as_posix(),
            "/data/components/Git.new"
        );
    }

    #[test]
    fn info_is_runtime_dep() {
        let info = GitComponent::info();
        assert_eq!(info.id, ComponentId::Git);
        assert_eq!(info.category, crate::types::ComponentCategory::RuntimeDep);
        assert!(!info.supported_targets.iter().any(|t| t.os == Os::MacOs));
    }
}
