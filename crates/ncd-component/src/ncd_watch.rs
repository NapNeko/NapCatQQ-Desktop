//! NcdWatchComponent:远端主机侧监控进程(ncd-watch)
//!
//! 装到 $HOME/ncd-watch:Desktop 本机拉 release 再 SFTP 上传、写默认配置、
//! 尽量拉起 systemd --user。不启停 Bot;配置同步(notify.json / desktop_present)
//! 由 Desktop 侧另做。正式路径不依赖本机 cargo 产物。

use std::path::PathBuf;

use async_trait::async_trait;
use ncd_host::{Arch, Host, HostCommand, HostPath, Locality, Os, shell_single_quote};
use ncd_network::build_mirror_urls;

use crate::context::{ActionCtx, ProgressKind, ProgressLogLevel};
use crate::download::DownloadHelper;
use crate::error::ActionError;
use crate::traits::Component;
use crate::types::{ComponentId, DetectedVersion, LaunchArgs, VerifyReport};

const INSTALL_DIR_NAME: &str = "ncd-watch";
const BIN_NAME: &str = "ncd-watch";
/// 与 GitHub Release asset / workflow 命名一致
const RELEASE_REPO: &str = "NapNeko/NapCatQQ-Desktop";

/// 远端 ncd-watch 可执行文件的位置：`root` 给了就用它（远端库存里选的目录），否则 `$HOME/ncd-watch`。
/// 和组件自己的 `bin_path` 同一套布局；Desktop 心跳拿它判断这台机装没装
pub fn ncd_watch_bin_path(home: &str, root: Option<&str>) -> HostPath {
    let root = root
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(|r| HostPath::from_posix(r.to_string()))
        .unwrap_or_else(|| HostPath::from_posix(format!("{home}/{INSTALL_DIR_NAME}")));
    root.join("bin").join(BIN_NAME)
}

/// 远端 ncd-watch 组件
#[derive(Debug, Clone)]
pub struct NcdWatchComponent {
    /// 远端 $HOME(探测得到);None 时 install 失败
    pub remote_home: Option<String>,
    /// release tag(如 watch-v0.2.0);place_binary 时按远端 uname 拼 URL
    pub release_tag: Option<String>,
    /// 显式下载 URL(无 tag 时用;开发回退)
    pub download_url: Option<String>,
    /// 可选 SHA256(与当前 download_url / 探测到的 arch asset 对应)
    pub expected_sha256: Option<String>,
    /// 写入远端的版本标签(detect 失败时展示;成功时以 --version 为准)
    pub version_label: String,
    /// 覆盖默认 `$HOME/ncd-watch`
    pub install_root: Option<HostPath>,
}

impl NcdWatchComponent {
    pub fn new(remote_home: Option<String>) -> Self {
        Self {
            remote_home,
            release_tag: None,
            download_url: None,
            expected_sha256: None,
            version_label: env!("CARGO_PKG_VERSION").to_string(),
            install_root: None,
        }
    }

    pub fn with_install_root(mut self, root: HostPath) -> Self {
        self.install_root = Some(root);
        self
    }

    pub fn with_release_tag(mut self, tag: impl Into<String>) -> Self {
        let t = tag.into();
        self.release_tag = if t.trim().is_empty() { None } else { Some(t) };
        self
    }

    pub fn with_download_url(mut self, url: impl Into<String>) -> Self {
        self.download_url = Some(url.into());
        self
    }

    pub fn with_sha256(mut self, sha256: impl Into<String>) -> Self {
        let s = sha256.into();
        self.expected_sha256 = if s.trim().is_empty() { None } else { Some(s) };
        self
    }

    pub fn with_version_label(mut self, v: impl Into<String>) -> Self {
        self.version_label = v.into();
        self
    }

    pub fn info() -> crate::types::ComponentInfo {
        crate::types::ComponentInfo {
            id: ComponentId::NcdWatch,
            display_name: "NCD Watch".to_string(),
            description: "本控制台关闭后仍可告警".to_string(),
            repo_url: Some(format!("https://github.com/{RELEASE_REPO}")),
            supported_targets: vec![
                crate::types::SupportedTarget::new(Os::Linux, Locality::Remote),
                crate::types::SupportedTarget::new(Os::Windows, Locality::Remote),
            ],
            // 与 Desktop 同属产品侧配套：远端脱管后的告警进程，不是 QQ/Node 一类框架运行时依赖
            category: crate::types::ComponentCategory::SelfApp,
            uninstall: crate::types::UninstallSupport::Supported,
        }
    }

    fn require_home(&self) -> Result<&str, ActionError> {
        self.remote_home
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ActionError::InvalidConfig {
                reason: "ncd-watch 需要远端 $HOME".into(),
            })
    }

    fn root_path(&self) -> Result<HostPath, ActionError> {
        if let Some(root) = &self.install_root {
            return Ok(root.clone());
        }
        let home = self.require_home()?;
        Ok(HostPath::from_posix(format!("{home}/{INSTALL_DIR_NAME}")))
    }

    fn bin_path(&self) -> Result<HostPath, ActionError> {
        Ok(self.root_path()?.join("bin").join(BIN_NAME))
    }

    fn config_watch_path(&self) -> Result<HostPath, ActionError> {
        Ok(self.root_path()?.join("config").join("watch.json"))
    }

    /// 安装时写入的版本戳。release 文件名/tag 可能是 0.2.0，但 clap 读的是
    /// workspace CARGO_PKG_VERSION(常仍是 0.1.0)，detect 必须以戳为准。
    fn installed_version_path(&self) -> Result<HostPath, ActionError> {
        Ok(self.root_path()?.join("config").join("installed_version"))
    }

    async fn write_installed_version_stamp(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.installed_version_path()?;
        let ver = self.version_label.trim();
        if ver.is_empty() {
            return Ok(());
        }
        let body = format!("{}\n", normalize_detected_version(ver));
        host.write_file(&path, body.as_bytes())
            .await
            .map_err(|e| ActionError::install_step("write installed_version", e.to_string()))?;
        Ok(())
    }

    async fn read_installed_version_stamp(&self, host: &dyn Host) -> Option<String> {
        let path = self.installed_version_path().ok()?;
        let exists = host.exists(&path).await.ok()?;
        if !exists {
            return None;
        }
        let bytes = host.read_file(&path).await.ok()?;
        let text = String::from_utf8_lossy(&bytes);
        let line = text.lines().next().unwrap_or("").trim();
        if line.is_empty() {
            return None;
        }
        Some(normalize_detected_version(line))
    }

    async fn ensure_layout(&self, host: &dyn Host) -> Result<(), ActionError> {
        let root = self.root_path()?;
        for sub in ["bin", "config", "state", "logs"] {
            host.create_dir_all(&root.join(sub))
                .await
                .map_err(|e| ActionError::install_step("mkdir", e.to_string()))?;
        }
        Ok(())
    }

    async fn place_binary(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        let dest = self.bin_path()?;

        // Host::arch() 远端写死 X86_64;装 musl 必须以 uname 为准
        let arch = match probe_remote_arch(host).await {
            Ok(a) => a,
            Err(err) => {
                ctx.emit(ProgressKind::Log {
                    level: ProgressLogLevel::Warn,
                    message: format!("uname -m 失败,回退 host.arch(): {err}"),
                })
                .await;
                host.arch()
            }
        };

        let (url, sha) = if let Some(tag) = self
            .release_tag
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let url = ncd_watch_release_download_url_for_tag(tag, arch).ok_or_else(|| {
                ActionError::InvalidConfig {
                    reason: format!("无法为架构 {arch:?} 拼 ncd-watch 下载 URL(tag={tag})"),
                }
            })?;
            // expected_sha256 按 build 时 host.arch() 预填;uname 结果若不同则丢弃 hash,
            // 避免用 x86_64 digest 校验 aarch64 二进制。
            let sha = if arch == host.arch() {
                self.expected_sha256.clone().filter(|s| !s.is_empty())
            } else {
                None
            };
            (url, sha)
        } else {
            let url = self
                .download_url
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ActionError::InvalidConfig {
                    reason: "未提供 ncd-watch 下载 URL(release 快照缺失或架构不支持)".into(),
                })?
                .to_string();
            (url, self.expected_sha256.clone().filter(|s| !s.is_empty()))
        };

        // 正式路径:Desktop 本机下载 release → SFTP 上传(与 NC/SL 一致)。
        // 不用远端 wget/curl 直下 GitHub:远端常无外网/无代理。
        let local_tmp = ctx.download_path(format!(
            "ncd-watch-{}-{}",
            std::process::id(),
            self.version_label.replace(['/', '\\', ' '], "_")
        ))?;
        if local_tmp.exists() {
            let _ = tokio::fs::remove_file(&local_tmp).await;
        }

        let mirrors = build_mirror_urls(&url, None);
        ctx.emit(ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: format!("本机下载 ncd-watch ← {url}"),
        })
        .await;

        let helper = DownloadHelper::new()?;
        helper
            .download_with_mirrors(&mirrors, &local_tmp, sha.as_deref(), ctx, 2)
            .await
            .inspect_err(|_| {
                let _ = std::fs::remove_file(&local_tmp);
            })?;

        ctx.emit(ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: format!("上传 ncd-watch → {}", dest.as_posix()),
        })
        .await;
        let placed = install_binary_file(host, &local_tmp, &dest).await;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        placed
    }

    async fn write_default_watch_json(&self, host: &dyn Host) -> Result<(), ActionError> {
        let path = self.config_watch_path()?;
        if host
            .exists(&path)
            .await
            .map_err(|e| ActionError::install_step("exists watch.json", e.to_string()))?
        {
            return Ok(());
        }
        // 与 ncd-watch::WatchConfig::default 字段对齐(避免 component 依赖 ncd-watch crate)
        let body = r#"{
  "protocol": 1,
  "features": ["process_watch", "docker_watch", "webhook"],
  "probeIntervalSecs": 15,
  "desktopPresentTtlSecs": 90,
  "debounceSecs": 0,
  "notifyWhileDesktopPresent": false
}
"#;
        host.write_file(&path, body.as_bytes())
            .await
            .map_err(|e| ActionError::install_step("write watch.json", e.to_string()))?;
        Ok(())
    }

    async fn write_and_start_systemd(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
    ) -> Result<(), ActionError> {
        let home = self.require_home()?;
        // 自定义安装根(库存选的目录)时 unit 也要指向它,否则服务跑的还是旧路径
        let bin = self.bin_path()?.as_posix().to_string();
        let root = self.root_path()?.as_posix().to_string();
        let unit_dir = HostPath::from_posix(format!("{home}/.config/systemd/user"));
        host.create_dir_all(&unit_dir)
            .await
            .map_err(|e| ActionError::install_step("mkdir systemd user", e.to_string()))?;
        let unit_path = unit_dir.join("ncd-watch.service");
        let unit = unit_text(&bin, &root);
        host.write_file(&unit_path, unit.as_bytes())
            .await
            .map_err(|e| ActionError::install_step("write unit", e.to_string()))?;

        let script = start_script(&bin, &root);
        ctx.emit(ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: "启动/重启 ncd-watch(systemd --user 或 nohup)".into(),
        })
        .await;
        let out = host
            .run_to_string(HostCommand::new("sh").arg("-c").arg(&script))
            .await
            .map_err(|e| ActionError::install_step("start watch", e.to_string()))?;
        if !out.success() {
            return Err(ActionError::install_step(
                "start watch",
                format!(
                    "ncd-watch 未能启动(exit={:?}): {}",
                    out.exit_code,
                    out.stderr.trim()
                ),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl Component for NcdWatchComponent {
    fn id(&self) -> ComponentId {
        ComponentId::NcdWatch
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        &[(Os::Linux, Locality::Remote)]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        let bin = match self.bin_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };
        if !host
            .exists(&bin)
            .await
            .map_err(|e| ActionError::other(e.to_string()))?
        {
            return Ok(None);
        }
        let bin_s = bin.as_posix().to_string();

        // 优先 installed_version 戳(与 release tag/文件名一致);
        // clap --version 跟 workspace CARGO_PKG_VERSION,发版 tag 不等于 crate 版本时会漂。
        if let Some(stamped) = self.read_installed_version_stamp(host).await {
            return Ok(Some(DetectedVersion {
                version: stamped,
                source: format!("{bin_s} (installed_version)"),
            }));
        }

        let out = host
            .run_to_string(HostCommand::new(&bin_s).arg("--version"))
            .await;
        let version = match out {
            Ok(o) if o.success() => {
                let line = o.stdout.lines().next().unwrap_or("").trim();
                if line.is_empty() {
                    self.version_label.clone()
                } else {
                    normalize_detected_version(line)
                }
            }
            _ => self.version_label.clone(),
        };
        Ok(Some(DetectedVersion {
            version,
            source: bin_s,
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 4 }).await;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "创建目录".into(),
        })
        .await;
        self.ensure_layout(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        // 覆盖安装/更新:先停再换。二进制本身走同目录 rename,旧进程没退干净也不会 ETXTBSY
        let stop = stop_script();
        ctx.emit(ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: "停止 ncd-watch 以便覆盖二进制".into(),
        })
        .await;
        let _ = host
            .run_to_string(HostCommand::new("sh").arg("-c").arg(&stop))
            .await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "下载并上传二进制".into(),
        })
        .await;
        self.place_binary(host, ctx).await?;
        self.write_installed_version_stamp(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 3,
            message: "写入默认 watch.json".into(),
        })
        .await;
        self.write_default_watch_json(host).await?;
        ctx.emit(ProgressKind::StepEnd { step: 3, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 4,
            message: "配置并重启服务".into(),
        })
        .await;
        self.write_and_start_systemd(host, ctx).await?;
        ctx.emit(ProgressKind::StepEnd { step: 4, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        // install 已含 stop + download/upload + restart
        self.install(host, ctx).await
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 2 }).await;
        let root = self.root_path()?;
        ctx.emit(ProgressKind::StepBegin {
            step: 1,
            message: "停止服务".into(),
        })
        .await;
        let stop = format!(
            "systemctl --user disable --now ncd-watch.service 2>/dev/null || true; {}",
            stop_script()
        );
        let _ = host
            .run_to_string(HostCommand::new("sh").arg("-c").arg(&stop))
            .await;
        ctx.emit(ProgressKind::StepEnd { step: 1, ok: true }).await;

        ctx.emit(ProgressKind::StepBegin {
            step: 2,
            message: "删除安装目录".into(),
        })
        .await;
        host.remove_dir_all(&root)
            .await
            .map_err(|e| ActionError::install_step("remove", e.to_string()))?;
        ctx.emit(ProgressKind::StepEnd { step: 2, ok: true }).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let bin = self.bin_path()?;
        let exists = host
            .exists(&bin)
            .await
            .map_err(|e| ActionError::other(e.to_string()))?;
        Ok(
            VerifyReport::ok().with_check(
                "binary exists",
                exists,
                Some(bin.as_posix().to_string()),
            ),
        )
    }

    fn launch_command(
        &self,
        _host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let bin = self.bin_path()?.as_posix().to_string();
        let root = self.root_path()?.as_posix().to_string();
        let cmd = HostCommand::new(bin).arg("--root").arg(root).arg("run");
        Ok(args.apply_to(cmd))
    }
}

/// clap `--version` / 杂讯输出 → 裸 semver
pub fn normalize_detected_version(raw: &str) -> String {
    let line = raw.lines().next().unwrap_or(raw).trim();
    if line.is_empty() {
        return String::new();
    }
    let token = line
        .strip_prefix("ncd-watch ")
        .or_else(|| line.strip_prefix("ncd-watch"))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(line);
    let token = token
        .strip_prefix("watch-v")
        .or_else(|| token.strip_prefix("watch-V"))
        .or_else(|| token.strip_prefix('v'))
        .or_else(|| token.strip_prefix('V'))
        .unwrap_or(token);
    if token.contains(' ') {
        token
            .split_whitespace()
            .rev()
            .find(|p| p.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .unwrap_or(token)
            .to_string()
    } else {
        token.to_string()
    }
}

/// 远端 uname -m → Arch。
/// Host::arch() 对远端写死 X86_64,装 musl 二进制必须以 uname 为准。
pub async fn probe_remote_arch(host: &dyn Host) -> Result<Arch, ActionError> {
    if host.os() == Os::Windows {
        return Ok(host.arch());
    }
    let out = host
        .run_to_string(HostCommand::new("uname").arg("-m"))
        .await
        .map_err(|e| ActionError::other(format!("uname -m: {e}")))?;
    if !out.success() {
        return Err(ActionError::other(format!(
            "uname -m failed: exit={:?} {}",
            out.exit_code,
            out.stderr.trim()
        )));
    }
    let m = out
        .stdout
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match m.as_str() {
        "x86_64" | "amd64" => Ok(Arch::X86_64),
        "aarch64" | "arm64" => Ok(Arch::Aarch64),
        "armv7l" | "armv7" => Ok(Arch::Armv7),
        "i386" | "i686" | "x86" => Ok(Arch::X86),
        other => Err(ActionError::other(format!(
            "不支持的远端架构 uname -m={other}(仅 x86_64/aarch64 musl 发版)"
        ))),
    }
}

/// musl 目标 triple(仅 x86_64 / aarch64;其它架构不发版)
pub fn ncd_watch_musl_target(arch: Arch) -> Option<&'static str> {
    match arch {
        Arch::X86_64 => Some("x86_64-unknown-linux-musl"),
        Arch::Aarch64 => Some("aarch64-unknown-linux-musl"),
        Arch::X86 | Arch::Armv7 => None,
    }
}

/// GitHub Release 直链
///
/// 优先由调用方传入 release 快照里的 watch-v* tag;未传时回退
/// watch-v{CARGO_PKG_VERSION}(开发/旧路径)。
pub fn ncd_watch_release_download_url(arch: Arch) -> Option<String> {
    ncd_watch_release_download_url_for_tag(&format!("watch-v{}", env!("CARGO_PKG_VERSION")), arch)
}

/// 从 tag(如 watch-v0.2.0)拼 musl asset 直链
pub fn ncd_watch_release_download_url_for_tag(tag: &str, arch: Arch) -> Option<String> {
    let triple = ncd_watch_musl_target(arch)?;
    let tag = tag.trim();
    if tag.is_empty() {
        return None;
    }
    let ver = ncd_watch_version_from_tag(tag);
    if ver.is_empty() {
        return None;
    }
    Some(format!(
        "https://github.com/{RELEASE_REPO}/releases/download/{tag}/ncd-watch-{ver}-{triple}"
    ))
}

/// watch-v0.2.0 → 0.2.0;已是裸版本则原样
pub fn ncd_watch_version_from_tag(tag: &str) -> String {
    let t = tag.trim();
    if let Some(rest) = t
        .strip_prefix("watch-v")
        .or_else(|| t.strip_prefix("watch-V"))
    {
        return rest.to_string();
    }
    if let Some(rest) = t.strip_prefix('v').or_else(|| t.strip_prefix('V')) {
        if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return rest.to_string();
        }
    }
    t.to_string()
}

/// release asset 文件名:ncd-watch-{ver}-{triple}
pub fn ncd_watch_asset_name(tag: &str, arch: Arch) -> Option<String> {
    let triple = ncd_watch_musl_target(arch)?;
    let ver = ncd_watch_version_from_tag(tag);
    if ver.is_empty() {
        return None;
    }
    Some(format!("ncd-watch-{ver}-{triple}"))
}

/// 同目录暂存名;rename 只在同一文件系统上原子
const STAGING_SUFFIX: &str = ".new";

/// 先传到同目录 `ncd-watch.new` 再 rename 覆盖。直接 SFTP create 截断正在跑的
/// 可执行文件会 ETXTBSY(旧进程收到 SIGTERM 后还要跑完这一轮探活才退)
async fn install_binary_file(
    host: &dyn Host,
    local: &std::path::Path,
    bin: &HostPath,
) -> Result<(), ActionError> {
    let staging = HostPath::from_posix(format!("{}{STAGING_SUFFIX}", bin.as_posix()));
    if let Err(error) = host.upload(local, &staging).await {
        let _ = host.remove_file(&staging).await;
        return Err(ActionError::install_step("upload", error.to_string()));
    }

    let chmod = HostCommand::new("chmod").arg("+x").arg(staging.as_posix());
    let chmod_err = match host.run_to_string(chmod).await {
        Ok(out) if out.success() => None,
        Ok(out) => Some(format!("exit={:?} {}", out.exit_code, out.stderr.trim())),
        Err(e) => Some(e.to_string()),
    };
    if let Some(detail) = chmod_err {
        let _ = host.remove_file(&staging).await;
        return Err(ActionError::install_step("chmod", detail));
    }

    if let Err(e) = host.rename(&staging, bin).await {
        let _ = host.remove_file(&staging).await;
        return Err(ActionError::install_step("replace", e.to_string()));
    }
    Ok(())
}

/// 停当前用户的 ncd-watch 并最多等 5s。不用 `pkill -f <路径>`:那条命令行本身就在
/// `sh -c` 参数里,会把执行脚本的 shell 一起杀掉
fn stop_script() -> String {
    format!(
        "systemctl --user stop ncd-watch.service 2>/dev/null || true; \
         pkill -x -u \"$(id -u)\" {BIN_NAME} 2>/dev/null || true; \
         i=0; \
         while pgrep -x -u \"$(id -u)\" {BIN_NAME} >/dev/null 2>&1 && [ \"$i\" -lt 5 ]; do \
           sleep 1; i=$((i+1)); \
         done; \
         true"
    )
}

/// systemd --user 只有用户总线连得上才用(root / 无 logind 会话的 SSH exec 连不上),
/// 否则或 systemd 这条失败时回退 nohup。最后确认进程真在跑,不在就 exit 3
/// 让调用方报出来,免得「更新了但 watch 没在跑」
fn start_script(bin: &str, root: &str) -> String {
    let q_bin = shell_single_quote(bin);
    let q_root = shell_single_quote(root);
    // enable --now 在已 active 时不会换二进制;更新后必须 restart
    format!(
        "if command -v systemctl >/dev/null 2>&1 \
            && systemctl --user show-environment >/dev/null 2>&1 \
            && systemctl --user daemon-reload \
            && systemctl --user enable ncd-watch.service >/dev/null 2>&1 \
            && systemctl --user restart ncd-watch.service; then \
           :; \
         else \
           pkill -x -u \"$(id -u)\" {BIN_NAME} 2>/dev/null || true; \
           nohup {q_bin} --root {q_root} run >/dev/null 2>&1 </dev/null & \
         fi; \
         sleep 1; \
         pgrep -x -u \"$(id -u)\" {BIN_NAME} >/dev/null 2>&1 || exit 3"
    )
}

fn unit_text(bin: &str, root: &str) -> String {
    // HOME 里带空格时不加引号 systemd 会把路径拆成几个参数,服务起不来
    let unit_bin = systemd_quote(bin);
    let unit_root = systemd_quote(root);
    format!(
        "[Unit]\n\
         Description=NapCatQQ Desktop remote watch\n\
         After=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={unit_bin} --root {unit_root} run\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         Environment=RUST_LOG=info\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}

/// systemd unit 里 ExecStart 的一个参数:双引号包起来,里面的 \ 和 " 转义,
/// % 与 $ 写两遍,免得被当成 specifier / 环境变量展开
fn systemd_quote(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    for c in arg.chars() {
        match c {
            '\\' | '"' => {
                out.push('\\');
                out.push(c);
            }
            '%' | '$' => {
                out.push(c);
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 开发期本机 cargo 产物探测，正式安装和更新不走这条。
pub fn discover_local_ncd_watch_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidates = [
        dir.join("ncd-watch.exe"),
        dir.join("ncd-watch"),
        dir.join("ncd_watch.exe"),
        dir.parent()
            .map(|p| p.join("ncd-watch.exe"))
            .unwrap_or_default(),
        dir.parent()
            .map(|p| p.join("ncd-watch"))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_and_info() {
        let c = NcdWatchComponent::new(Some("/home/u".into()));
        assert_eq!(c.id(), ComponentId::NcdWatch);
        assert_eq!(NcdWatchComponent::info().id, ComponentId::NcdWatch);
        assert_eq!(
            c.bin_path().unwrap().as_posix(),
            "/home/u/ncd-watch/bin/ncd-watch"
        );
    }

    #[test]
    fn systemd_quote_keeps_spaced_path_as_one_argument() {
        assert_eq!(
            systemd_quote("/home/a b/ncd-watch"),
            "\"/home/a b/ncd-watch\""
        );
        assert_eq!(systemd_quote(r#"/x"y\z%h$H"#), r#""/x\"y\\z%%h$$H""#);
    }

    #[test]
    fn info_should_use_self_app_category() {
        assert_eq!(
            NcdWatchComponent::info().category,
            crate::types::ComponentCategory::SelfApp
        );
    }

    #[test]
    fn normalize_detected_version_strips_clap() {
        assert_eq!(normalize_detected_version("ncd-watch 0.2.0"), "0.2.0");
        assert_eq!(normalize_detected_version("0.2.0"), "0.2.0");
        assert_eq!(normalize_detected_version("watch-v0.2.0"), "0.2.0");
    }

    #[test]
    fn version_label_defaults_to_workspace_pkg() {
        let c = NcdWatchComponent::new(Some("/home/u".into()));
        // workspace 当前 0.1.0;发版后 CI 会改 ncd-watch crate version,不在此断言具体数字
        assert!(!c.version_label.is_empty());
        let stamped = c.with_version_label("0.2.0").version_label.clone();
        assert_eq!(normalize_detected_version(&stamped), "0.2.0");
    }

    #[test]
    fn musl_url_x86_64() {
        let url = ncd_watch_release_download_url(Arch::X86_64).expect("x86_64");
        let ver = env!("CARGO_PKG_VERSION");
        assert!(url.contains(&format!("watch-v{ver}")));
        assert!(url.ends_with(&format!("ncd-watch-{ver}-x86_64-unknown-linux-musl")));
    }

    #[test]
    fn musl_url_from_snapshot_tag() {
        let url = ncd_watch_release_download_url_for_tag("watch-v0.2.0", Arch::Aarch64).unwrap();
        assert_eq!(
            url,
            "https://github.com/NapNeko/NapCatQQ-Desktop/releases/download/watch-v0.2.0/ncd-watch-0.2.0-aarch64-unknown-linux-musl"
        );
        assert_eq!(ncd_watch_version_from_tag("watch-v0.2.0"), "0.2.0");
        assert_eq!(
            ncd_watch_asset_name("watch-v0.2.0", Arch::X86_64).as_deref(),
            Some("ncd-watch-0.2.0-x86_64-unknown-linux-musl")
        );
    }

    #[test]
    fn musl_url_rejects_x86() {
        assert!(ncd_watch_release_download_url(Arch::X86).is_none());
    }

    #[test]
    fn discover_does_not_panic() {
        let _ = discover_local_ncd_watch_binary();
    }

    #[test]
    fn stop_script_never_uses_pkill_f() {
        let s = stop_script();
        assert!(!s.contains("pkill -f"), "{s}");
        assert!(s.contains("pkill -x -u \"$(id -u)\" ncd-watch"), "{s}");
        assert!(s.contains("pgrep -x -u \"$(id -u)\" ncd-watch"), "{s}");
    }

    #[test]
    fn start_script_checks_user_bus_and_falls_back_to_nohup() {
        let s = start_script("/home/u/ncd-watch/bin/ncd-watch", "/home/u/ncd-watch");
        assert!(!s.contains("pkill -f"), "{s}");
        let bus = s
            .find("systemctl --user show-environment")
            .expect("user bus probe");
        let restart = s.find("systemctl --user restart").expect("restart");
        assert!(bus < restart, "先探用户总线再 restart: {s}");
        assert!(s.contains("else"), "{s}");
        assert!(
            s.contains("nohup '/home/u/ncd-watch/bin/ncd-watch' --root '/home/u/ncd-watch' run"),
            "{s}"
        );
        assert!(s.trim_end().ends_with("|| exit 3"), "{s}");
    }

    #[test]
    fn scripts_quote_spaced_paths() {
        let s = start_script("/home/a b/ncd-watch/bin/ncd-watch", "/home/a b/ncd-watch");
        assert!(s.contains("'/home/a b/ncd-watch/bin/ncd-watch'"), "{s}");
        assert!(s.contains("--root '/home/a b/ncd-watch'"), "{s}");
        let unit = unit_text("/home/a b/ncd-watch/bin/ncd-watch", "/home/a b/ncd-watch");
        assert!(
            unit.contains(
                "ExecStart=\"/home/a b/ncd-watch/bin/ncd-watch\" --root \"/home/a b/ncd-watch\" run"
            ),
            "{unit}"
        );
    }

    #[test]
    fn custom_install_root_is_used_for_unit_and_launch() {
        let c = NcdWatchComponent::new(Some("/home/u".into()))
            .with_install_root(HostPath::from_posix("/opt/w"));
        let host = RecordingHost::default();
        let cmd = c
            .launch_command(&host, &LaunchArgs::default())
            .expect("launch");
        assert_eq!(cmd.program, "/opt/w/bin/ncd-watch");
        assert_eq!(cmd.args, ["--root", "/opt/w", "run"]);
        let bin = c.bin_path().unwrap();
        let root = c.root_path().unwrap();
        let unit = unit_text(bin.as_posix(), root.as_posix());
        assert!(
            unit.contains("ExecStart=\"/opt/w/bin/ncd-watch\" --root \"/opt/w\" run"),
            "{unit}"
        );
    }

    #[tokio::test]
    async fn install_binary_file_uploads_to_staging_then_renames() {
        let host = RecordingHost::default();
        let bin = HostPath::from_posix("/home/u/ncd-watch/bin/ncd-watch");
        install_binary_file(&host, std::path::Path::new("local-bin"), &bin)
            .await
            .expect("install");
        let log = host.log.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "upload /home/u/ncd-watch/bin/ncd-watch.new".to_string(),
                "run chmod +x /home/u/ncd-watch/bin/ncd-watch.new".to_string(),
                "rename /home/u/ncd-watch/bin/ncd-watch.new -> /home/u/ncd-watch/bin/ncd-watch"
                    .to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn install_binary_file_cleans_staging_when_rename_fails() {
        let host = RecordingHost {
            fail_rename: true,
            ..RecordingHost::default()
        };
        let bin = HostPath::from_posix("/home/u/ncd-watch/bin/ncd-watch");
        let err = install_binary_file(&host, std::path::Path::new("local-bin"), &bin)
            .await
            .expect_err("rename failure must surface");
        assert!(err.to_string().contains("replace"), "{err}");
        let log = host.log.lock().unwrap().clone();
        assert_eq!(
            log.last().map(String::as_str),
            Some("remove /home/u/ncd-watch/bin/ncd-watch.new")
        );
    }

    #[tokio::test]
    async fn install_binary_file_cleans_incomplete_upload() {
        let host = RecordingHost {
            fail_upload: true,
            ..RecordingHost::default()
        };
        let bin = HostPath::from_posix("/home/u/ncd-watch/bin/ncd-watch");
        let err = install_binary_file(&host, std::path::Path::new("local-bin"), &bin)
            .await
            .expect_err("upload failure must surface");
        assert!(err.to_string().contains("upload"), "{err}");
        assert_eq!(
            host.log.lock().unwrap().last().map(String::as_str),
            Some("remove /home/u/ncd-watch/bin/ncd-watch.new")
        );
    }

    #[tokio::test]
    async fn service_start_failure_is_reported_as_install_failure() {
        let host = RecordingHost {
            fail_start: true,
            ..RecordingHost::default()
        };
        let component = NcdWatchComponent::new(Some("/home/u".into()));
        let (mut ctx, _rx) = ActionCtx::new();
        let err = component
            .write_and_start_systemd(&host, &mut ctx)
            .await
            .expect_err("a stopped watch must not be reported as successfully installed");
        assert!(err.to_string().contains("start watch"), "{err}");
    }

    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_host::{
        ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell, ShellKind,
    };
    use std::sync::Mutex;

    struct NoopShell;
    impl HostShell for NoopShell {
        fn kind(&self) -> ShellKind {
            ShellKind::Bash
        }
        fn escape(&self, arg: &str) -> String {
            arg.to_string()
        }
        fn line_separator(&self) -> &'static str {
            "\n"
        }
    }
    static NOOP_SHELL: NoopShell = NoopShell;

    #[derive(Default)]
    struct RecordingHost {
        log: Mutex<Vec<String>>,
        fail_rename: bool,
        fail_upload: bool,
        fail_start: bool,
    }

    impl RecordingHost {
        fn push(&self, entry: String) {
            self.log.lock().unwrap().push(entry);
        }
    }

    #[async_trait]
    impl Host for RecordingHost {
        fn os(&self) -> Os {
            Os::Linux
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Remote
        }
        fn id(&self) -> &str {
            "recording"
        }
        fn shell(&self) -> &dyn HostShell {
            &NOOP_SHELL
        }
        async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
            Ok(())
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
            self.push(format!("remove {}", path.as_posix()));
            Ok(())
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), HostError> {
            self.push(format!("rename {} -> {}", from.as_posix(), to.as_posix()));
            if self.fail_rename {
                return Err(HostError::Unsupported {
                    operation: "rename",
                });
            }
            Ok(())
        }
        async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
            Ok(false)
        }
        async fn upload(&self, _: &std::path::Path, remote: &HostPath) -> Result<(), HostError> {
            self.push(format!("upload {}", remote.as_posix()));
            if self.fail_upload {
                return Err(HostError::Unsupported {
                    operation: "upload",
                });
            }
            Ok(())
        }
        async fn download(&self, _: &HostPath, _: &std::path::Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
            self.push(format!("run {} {}", cmd.program, cmd.args.join(" ")));
            Ok(CommandOutput {
                exit_code: Some(if self.fail_start && cmd.program == "sh" {
                    3
                } else {
                    0
                }),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }
}
