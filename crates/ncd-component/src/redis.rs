//! RedisComponent：redis-server 二进制
//!
//! 云崽（TRSS-Yunzai）启动时连 `config/config/redis.yaml` 里的地址，连不上且地址是 127.0.0.1
//! 就自己 `spawn(path, ["--port", port])` 拉一个，进程跟着云崽走（退出前 SAVE 再杀）。
//! 所以桌面端只装二进制、把 `path` 指过去，不另管 redis 进程；每个实例用自己的口。
//!
//! 支持矩阵：(Windows, Local) / (Linux, Local) / (Linux, Remote)
//!
//! - Windows：redis-windows 项目的 msys2 构建（zip 顶层一个 `Redis-<ver>-Windows-x64-msys2/` 目录，
//!   redis-server.exe 和几个 msys dll 并排），放到组件目录
//! - Linux：PATH 上已有 `redis-server` / `valkey-server` 就用；没有就下 Valkey 官方的 Ubuntu 22.04
//!   构建（要 glibc ≥ 2.34、libssl.so.3，放 `$HOME/ncd/tools/valkey`），跑不起来再走包管理器装
//!
//! 版本输出形如 `Redis server v=8.10.2 sha=…` / `Valkey server v=9.0.6 …`，取 `v=` 后面那段。

use async_trait::async_trait;

use ncd_host::linux_pkg::{install_missing_command, plan_missing_command};
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, Host, HostCommand, HostError, HostPath, HostShell, Locality, Os,
};

use crate::context::{ActionCtx, ProgressKind};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::git::{ensure_error, sibling_dir};
use crate::requirement::Requirement;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

pub const REDIS_WINDOWS_VERSION: &str = "8.10.2";
pub const VALKEY_LINUX_VERSION: &str = "9.0.6";

/// 发行包 sha256：Windows 取自 GitHub release 的 asset digest，Valkey 取自官方 `.sha256`
/// （arm64 那份官方没挂校验文件，是下载后自己算的）。升版本时这张表跟着一起换
const PINNED_SHA256: &[(&str, &str)] = &[
    (
        "Redis-8.10.2-Windows-x64-msys2.zip",
        "7c8cebd50347eaa1d9e784da842ed47a4f33394637835531d6614777b950ee85",
    ),
    (
        "valkey-9.0.6-jammy-x86_64.tar.gz",
        "b79800f433bc4f26177b437f63fc9e0b4a02b73049e927407fb1269d0d59d41b",
    ),
    (
        "valkey-9.0.6-jammy-arm64.tar.gz",
        "c8bcafe7351a40f4932d9c0522d9de26faba59905d86a731699b54c3bb4eb0aa",
    ),
];

/// Valkey 的 jammy 构建链接 glibc 2.34 的符号
const VALKEY_MIN_GLIBC: (u32, u32) = (2, 34);

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

#[derive(Debug, Clone)]
pub struct RedisComponent {
    /// 本机 `data_root/components/Redis`，远端 `$HOME/ncd/tools/valkey`
    pub install_dir: HostPath,
    /// 远端临时目录（放归档 / 解压 stage）
    pub tmp_dir: HostPath,
}

impl RedisComponent {
    pub fn new(install_dir: HostPath) -> Self {
        Self {
            install_dir,
            tmp_dir: HostPath::from_posix("/tmp"),
        }
    }

    pub fn with_tmp_dir(mut self, tmp: HostPath) -> Self {
        self.tmp_dir = tmp;
        self
    }

    pub fn default_remote_install_dir(home: &str) -> HostPath {
        HostPath::from_posix(ncd_domain::join_under(home, "ncd/tools/valkey"))
    }

    /// 桌面端自己装的 redis-server 在哪（装没装另说）
    pub fn managed_binary_path_for_os(install_dir: &HostPath, os: Os) -> HostPath {
        match os {
            Os::Windows => install_dir.join("redis-server.exe"),
            _ => install_dir.join("bin").join("valkey-server"),
        }
    }

    pub fn windows_asset() -> String {
        format!("Redis-{REDIS_WINDOWS_VERSION}-Windows-x64-msys2.zip")
    }

    pub fn valkey_asset(arch: Arch) -> Result<String, ActionError> {
        let a = match arch {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "arm64",
            other => {
                return Err(ActionError::other(format!(
                    "Valkey 只发 x86_64 / arm64 的 Linux 构建，这台是 {other:?}"
                )));
            }
        };
        Ok(format!("valkey-{VALKEY_LINUX_VERSION}-jammy-{a}.tar.gz"))
    }

    fn pinned_sha256(asset: &str) -> Option<&'static str> {
        PINNED_SHA256
            .iter()
            .find(|(name, _)| *name == asset)
            .map(|(_, sha)| *sha)
    }

    /// `Redis server v=8.10.2 sha=…` → `8.10.2`
    pub fn parse_version_output(stdout: &str) -> Option<String> {
        let line = stdout.lines().next()?;
        let lower = line.to_ascii_lowercase();
        if !lower.contains("redis") && !lower.contains("valkey") {
            return None;
        }
        line.split_whitespace()
            .find_map(|t| t.strip_prefix("v="))
            .map(str::to_string)
            .filter(|v| !v.is_empty())
    }

    /// `ldd (Ubuntu GLIBC 2.35-0ubuntu3.8) 2.35` / `ldd (GNU libc) 2.34` → (2, 35)
    pub fn parse_glibc_version(ldd_first_line: &str) -> Option<(u32, u32)> {
        let last = ldd_first_line.split_whitespace().last()?;
        let mut it = last.split('.');
        let major = it.next()?.parse().ok()?;
        let minor = it
            .next()?
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()?;
        Some((major, minor))
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
            Ok(_) | Err(HostError::CommandFailed { .. }) | Err(HostError::Io(_)) => Ok(None),
            Err(e) => Err(ActionError::Host(e)),
        }
    }

    /// 能给云崽 `redis.yaml` 填的二进制：托管的优先，其次 PATH 上的（写绝对路径，
    /// 云崽的进程环境里 PATH 不一定和 SSH 探测时一样）。都没有是 None
    pub async fn resolve_binary(&self, host: &dyn Host) -> Result<Option<HostPath>, ActionError> {
        let managed = Self::managed_binary_path_for_os(&self.install_dir, host.os());
        if host.exists(&managed).await?
            && self
                .probe(host, managed.as_posix(), managed.as_posix())
                .await?
                .is_some()
        {
            return Ok(Some(managed));
        }
        for name in ["redis-server", "valkey-server"] {
            if let Some(path) = host.which(name).await? {
                return Ok(Some(match host.os() {
                    Os::Windows => HostPath::from_windows(&path),
                    _ => HostPath::from_posix(path),
                }));
            }
        }
        Ok(None)
    }

    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::Redis,
            display_name: "Redis".to_string(),
            description: "键值数据库，云崽存账号绑定和冷却用".to_string(),
            repo_url: Some("https://github.com/redis-windows/redis-windows".to_string()),
            supported_targets: SUPPORTED
                .iter()
                .map(|(os, loc)| crate::types::SupportedTarget::new(*os, *loc))
                .collect(),
            category: crate::types::ComponentCategory::RuntimeDep,
        }
    }

    async fn download(
        &self,
        ctx: &mut ActionCtx,
        url: &str,
        asset: &str,
        step: u32,
    ) -> Result<std::path::PathBuf, ActionError> {
        let local_tmp = std::env::temp_dir().join(format!("ncd-{}-{asset}", std::process::id()));
        let helper = DownloadHelper::new()?;
        let mirrors = if url.starts_with("https://github.com/") {
            ncd_network::build_mirror_urls(url, None)
        } else {
            vec![url.to_string()]
        };
        helper
            .download_with_mirrors(&mirrors, &local_tmp, Self::pinned_sha256(asset), ctx, step)
            .await?;
        Ok(local_tmp)
    }

    async fn install_windows(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "下载 Redis".into(),
        })
        .await;
        let asset = Self::windows_asset();
        let url = format!(
            "https://github.com/redis-windows/redis-windows/releases/download/{REDIS_WINDOWS_VERSION}/{asset}"
        );
        let local_tmp = self.download(ctx, &url, &asset, 1).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "解压".into(),
        })
        .await;
        let stage = sibling_dir(&self.install_dir, "stage");
        let _ = host.remove_dir_all(&stage).await;
        host.create_dir_all(&stage).await?;
        let archive = HostPath::from_windows(local_tmp.to_string_lossy().as_ref());
        let extracted = host
            .extract_archive(&archive, &stage, ArchiveKind::Zip)
            .await;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        extracted?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: format!("安装到 {}", self.install_dir.as_posix()),
        })
        .await;
        let top = stage.join(asset.trim_end_matches(".zip"));
        let _ = host.remove_dir_all(&self.install_dir).await;
        host.rename(&top, &self.install_dir).await?;
        let _ = host.remove_dir_all(&stage).await;
        self.verify_installed(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify_installed(&self, host: &dyn Host) -> Result<(), ActionError> {
        let bin = Self::managed_binary_path_for_os(&self.install_dir, host.os());
        match self.probe(host, bin.as_posix(), bin.as_posix()).await? {
            Some(_) => Ok(()),
            None => Err(ActionError::install_step(
                "verify",
                format!("安装后 {} 跑不起来", bin.as_posix()),
            )),
        }
    }

    /// Valkey 官方构建能不能在这台机上跑：glibc 够新、有 libssl.so.3
    async fn valkey_build_fits(&self, host: &dyn Host) -> Result<(), String> {
        let out = host
            .run_to_string(HostCommand::new("sh").arg("-c").arg(
                "ldd --version 2>&1 | head -n1; \
                 (ldconfig -p 2>/dev/null | grep -q 'libssl.so.3 ' \
                   || ls /lib/*/libssl.so.3 /usr/lib/*/libssl.so.3 /usr/lib64/libssl.so.3 >/dev/null 2>&1) \
                   && echo SSL3=1 || echo SSL3=0",
            ))
            .await
            .map_err(|e| e.to_string())?;
        let first = out.stdout.lines().next().unwrap_or_default();
        match Self::parse_glibc_version(first) {
            Some(v) if v >= VALKEY_MIN_GLIBC => {}
            Some((a, b)) => return Err(format!("glibc {a}.{b} 太旧（要 2.34 以上）")),
            None => return Err("看不出 glibc 版本（可能是 musl 系统）".into()),
        }
        if !out.stdout.contains("SSL3=1") {
            return Err("缺 libssl.so.3（OpenSSL 3）".into());
        }
        Ok(())
    }

    async fn install_valkey(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        let arch = if host.locality() == Locality::Remote {
            crate::ncd_watch::probe_remote_arch(host).await?
        } else {
            host.arch()
        };
        let asset = Self::valkey_asset(arch)?;
        let url = format!("https://download.valkey.io/releases/{asset}");
        let local_tmp = self.download(ctx, &url, &asset, 2).await?;

        let remote_archive = self
            .tmp_dir
            .join(format!("ncd-{}-{asset}", std::process::id()));
        host.create_dir_all(&self.tmp_dir).await?;
        let uploaded = host.upload(&local_tmp, &remote_archive).await;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        uploaded?;

        let stage = self
            .tmp_dir
            .join(format!("ncd-valkey-stage-{}", std::process::id()));
        let _ = host.remove_dir_all(&stage).await;
        host.create_dir_all(&stage).await?;
        host.extract_archive(&remote_archive, &stage, ArchiveKind::TarGz)
            .await?;
        let top = stage.join(asset.trim_end_matches(".tar.gz"));
        let new_dir = sibling_dir(&self.install_dir, "new");
        let _ = host.remove_dir_all(&new_dir).await;
        if let Some(parent) = new_dir.parent() {
            host.create_dir_all(&parent).await?;
        }
        // /tmp 和 $HOME 常不在同一个文件系统，改名会跨盘失败，用 mv 兜住
        let mv = HostCommand::new("sh").arg("-c").arg(format!(
            "mv {} {} && chmod +x {}/bin/*",
            BashShell.escape(top.as_posix()),
            BashShell.escape(new_dir.as_posix()),
            BashShell.escape(new_dir.as_posix())
        ));
        let out = host.run_to_string(mv).await?;
        if !out.success() {
            return Err(ActionError::install_step(
                "mv_install",
                format!("exit={:?}: {}", out.exit_code, out.stderr.trim()),
            ));
        }
        let _ = host.remove_dir_all(&self.install_dir).await;
        host.rename(&new_dir, &self.install_dir).await?;
        let _ = host.remove_dir_all(&stage).await;
        let _ = host.remove_file(&remote_archive).await;
        self.verify_installed(host).await
    }

    async fn install_linux(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "检查系统".into(),
        })
        .await;
        let fits = self.valkey_build_fits(host).await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: format!("下载 Valkey {VALKEY_LINUX_VERSION}"),
        })
        .await;
        let valkey = match &fits {
            Ok(()) => self.install_valkey(host, ctx).await,
            Err(why) => Err(ActionError::other(why.clone())),
        };
        match valkey {
            Ok(()) => {
                ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;
                ctx.emit(ProgressKind::StepBegin {
                    step: 3,
                    message: "完成".into(),
                })
                .await;
                ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
                ctx.emit(ProgressKind::Finished { ok: true }).await;
                return Ok(());
            }
            Err(e) => {
                ctx.warn(format!(
                    "Valkey 预编译包用不了：{e}；改用系统包管理器装 redis"
                ))
                .await;
                ctx.emit(ProgressKind::StepEnd { step: 2, ok: false }).await;
            }
        }

        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "用包管理器安装 redis".into(),
        })
        .await;
        let pm = match plan_missing_command(host, "redis-server").await {
            Ok(None) => {
                ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
                ctx.emit(ProgressKind::Finished { ok: true }).await;
                return Ok(());
            }
            Ok(Some(pm)) => pm,
            Err(e) => return Err(ensure_error("redis-server", e)),
        };
        let package = match pm {
            ncd_host::LinuxPackageManager::Apt => "redis-server",
            _ => "redis",
        };
        ctx.info(format!("通过 {} 安装 {package}", pm.binary()))
            .await;
        install_missing_command(host, pm, "redis-server", package)
            .await
            .map_err(|e| ensure_error("redis-server", e))?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }
}

#[async_trait]
impl Component for RedisComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Redis
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
        let managed = Self::managed_binary_path_for_os(&self.install_dir, host.os());
        if host.exists(&managed).await? {
            if let Some(v) = self
                .probe(host, managed.as_posix(), managed.as_posix())
                .await?
            {
                return Ok(Some(v));
            }
        }
        for name in ["redis-server", "valkey-server"] {
            if let Some(v) = self.probe(host, name, &format!("$PATH/{name}")).await? {
                return Ok(Some(v));
            }
        }
        Ok(None)
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
        if host.exists(&self.install_dir).await? {
            host.remove_dir_all(&self.install_dir).await?;
        } else {
            ctx.info("Redis 是系统包或另装的，桌面端不卸载").await;
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
            "redis-server runs",
            found.is_some(),
            found.map(|v| format!("{} ({})", v.version, v.source)),
        ))
    }

    fn launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let bin = Self::managed_binary_path_for_os(&self.install_dir, host.os());
        Ok(args.apply_to(HostCommand::new(bin.as_posix())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_output_reads_v_field_for_both_flavors() {
        assert_eq!(
            RedisComponent::parse_version_output(
                "Redis server v=8.10.2 sha=00000000:0 malloc=libc bits=64 build=7e74\n"
            ),
            Some("8.10.2".into())
        );
        assert_eq!(
            RedisComponent::parse_version_output("Valkey server v=9.0.6 sha=0 malloc=jemalloc"),
            Some("9.0.6".into())
        );
        assert_eq!(RedisComponent::parse_version_output("v=1.0"), None);
        assert_eq!(RedisComponent::parse_version_output(""), None);
    }

    #[test]
    fn glibc_line_parses_distro_and_plain_forms() {
        assert_eq!(
            RedisComponent::parse_glibc_version("ldd (Ubuntu GLIBC 2.35-0ubuntu3.8) 2.35"),
            Some((2, 35))
        );
        assert_eq!(
            RedisComponent::parse_glibc_version("ldd (Debian GLIBC 2.36-9+deb12u7) 2.36"),
            Some((2, 36))
        );
        assert_eq!(
            RedisComponent::parse_glibc_version("ldd (GNU libc) 2.17"),
            Some((2, 17))
        );
        assert_eq!(
            RedisComponent::parse_glibc_version("musl libc (x86_64)"),
            None
        );
        assert!(Some((2, 17)) < Some(VALKEY_MIN_GLIBC));
    }

    #[test]
    fn assets_and_pins_cover_every_shipped_build() {
        assert_eq!(
            RedisComponent::windows_asset(),
            "Redis-8.10.2-Windows-x64-msys2.zip"
        );
        assert_eq!(
            RedisComponent::valkey_asset(Arch::X86_64).unwrap(),
            "valkey-9.0.6-jammy-x86_64.tar.gz"
        );
        assert_eq!(
            RedisComponent::valkey_asset(Arch::Aarch64).unwrap(),
            "valkey-9.0.6-jammy-arm64.tar.gz"
        );
        assert!(RedisComponent::valkey_asset(Arch::Armv7).is_err());
        for asset in [
            RedisComponent::windows_asset(),
            RedisComponent::valkey_asset(Arch::X86_64).unwrap(),
            RedisComponent::valkey_asset(Arch::Aarch64).unwrap(),
        ] {
            assert_eq!(
                RedisComponent::pinned_sha256(&asset).map(str::len),
                Some(64),
                "{asset}"
            );
        }
    }

    #[test]
    fn managed_binary_layout() {
        let win = HostPath::from_windows(r"C:\ProgramData\NapCatQQ Desktop\components\Redis");
        assert_eq!(
            RedisComponent::managed_binary_path_for_os(&win, Os::Windows).as_posix(),
            "/c/ProgramData/NapCatQQ Desktop/components/Redis/redis-server.exe"
        );
        let remote = RedisComponent::default_remote_install_dir("/home/u");
        assert_eq!(remote.as_posix(), "/home/u/ncd/tools/valkey");
        assert_eq!(
            RedisComponent::managed_binary_path_for_os(&remote, Os::Linux).as_posix(),
            "/home/u/ncd/tools/valkey/bin/valkey-server"
        );
    }

    #[test]
    fn linux_needs_tar_windows_needs_nothing() {
        let comp = RedisComponent::new(HostPath::from_posix("/x"));
        assert_eq!(
            comp.requirements(Os::Linux, Locality::Remote),
            vec![Requirement::host_command("tar", "tar")]
        );
        assert!(comp.requirements(Os::Windows, Locality::Local).is_empty());
    }
}
