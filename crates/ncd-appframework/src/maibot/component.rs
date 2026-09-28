//! 每实例一份 MaiBot 源码 + 项目内 `.venv`。
//!
//! 首装：解析 uv → 定适配器版本并下载 → 按适配器声明的范围挑 MaiBot 版本、下载源码 →
//! 解压到实例目录里的暂存目录，剥掉源码包的顶级目录挪进来 → `uv sync --locked` →
//! 种最小配置（两个口、WebUI token、协议确认）。
//! 更新：下载同上，换代码但留下 config / data / plugins / logs / .venv / 协议确认；
//! 适配器换代码、留 config.toml。协议确认只在首装时写，条款改了要用户重新同意，这里不替用户做主。
//!
//! 下载都在桌面端本机做（镜像竞速），再 upload 到主机：远端常常连不上 GitHub。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use ncd_component::{
    ActionCtx, ActionError, Component, ComponentId, DetectOutcome, DetectedVersion,
    DownloadHelper, LaunchArgs, ProgressKind, Requirement, UnusableInstall, VerifyReport,
    probe_remote_arch,
};
use ncd_host::{Arch, ArchiveKind, Host, HostCommand, HostPath, Locality, Os};
use ncd_network::build_mirror_urls;
use rand::Rng;

use super::config::write_bot_config_ports;
use super::manifest::{
    ADAPTER_DIR, ADAPTER_MANIFEST_FILE, ADAPTER_REPO, BOT_CONFIG, BOT_PY, CONFIG_PY,
    MAIBOT_MIN_FREE_KB, MAIBOT_MIN_GLIBC, MAIBOT_PYTHON_REQUIRES, MAIBOT_REPO,
    MAIBOT_UV_VERSION_RANGE, PINNED_ADAPTER_TAG, PINNED_MAIBOT_TAG, PRESERVED_ON_UPDATE,
    PYPROJECT, PYPROJECT_NAME, PYTHON_SCRATCH_DIR, STAGE_DIR, WEBUI_JSON,
};
use super::release::{
    ArchiveExt, adapter_host_range, archive_url, fetch_stable_release_tags, host_compatible,
    pick_maibot_tag,
};
use super::terms::write_confirmations;
use crate::ports::PortUsage;
use crate::uv_tooling::{
    LinuxLibc, ensure_python, probe_free_kb, probe_linux_libc, read_uv_marker, resolve_uv,
    venv_python, write_uv_marker,
};

const SUPPORTED: &[(Os, Locality)] = &[
    (Os::Windows, Locality::Local),
    (Os::Linux, Locality::Local),
    (Os::Linux, Locality::Remote),
];

/// 依赖里有 faiss / scipy / pandas / pyarrow，首次 sync 几百 MB，给足时间
const SYNC_TIMEOUT: Duration = Duration::from_secs(40 * 60);
const PORT_PROBE_TRIES: u16 = 32;

#[derive(Debug, Clone)]
pub struct MaiBotComponent {
    pub install_dir: HostPath,
    /// WebUI 口 = 实例 `port`
    pub webui_port: u16,
    pub uv_bin: Option<HostPath>,
}

/// 下好、解开、还没挪进实例目录的一份源码
struct StagedSource {
    /// 暂存目录里剥掉顶级目录之后的根
    root: HostPath,
    tag: String,
}

impl MaiBotComponent {
    pub fn new(install_dir: HostPath, webui_port: u16) -> Self {
        Self {
            install_dir,
            webui_port,
            uv_bin: None,
        }
    }

    pub fn with_uv_bin(mut self, uv_bin: Option<HostPath>) -> Self {
        self.uv_bin = uv_bin;
        self
    }

    pub fn venv_python(&self, os: Os) -> HostPath {
        venv_python(&self.install_dir, os)
    }

    fn stage_dir(&self) -> HostPath {
        self.install_dir.join(STAGE_DIR)
    }

    async fn preferred_uvs(&self, host: &dyn Host) -> Vec<HostPath> {
        let mut out = Vec::with_capacity(2);
        if let Some(p) = &self.uv_bin {
            out.push(p.clone());
        }
        if let Some(p) = read_uv_marker(host, &self.install_dir).await {
            out.push(p);
        }
        out
    }

    /// 下载源码包到桌面端临时目录 → 传到主机暂存目录 → 解压 → 返回唯一的顶级目录
    async fn stage_archive(
        &self,
        host: &dyn Host,
        ctx: &ActionCtx,
        step: u32,
        repo: &str,
        tag: &str,
        slot: &str,
    ) -> Result<StagedSource, ActionError> {
        let (ext, kind) = match host.locality() {
            Locality::Local => (ArchiveExt::Zip, ArchiveKind::Zip),
            Locality::Remote => (ArchiveExt::TarGz, ArchiveKind::TarGz),
        };
        let url = archive_url(repo, tag, ext);
        let mirrors = build_mirror_urls(&url, None);
        ctx.info(format!("下载 {repo} {tag}，候选镜像 {} 个", mirrors.len()))
            .await;
        let local_tmp = std::env::temp_dir().join(format!(
            "ncd-maibot-{slot}-{}-{}.{}",
            std::process::id(),
            unix_ms(),
            ext.as_str()
        ));
        let downloaded = DownloadHelper::new()?
            .download_with_mirrors_no_chunk(&mirrors, &local_tmp, None, ctx, step)
            .await;
        if let Err(e) = downloaded {
            let _ = tokio::fs::remove_file(&local_tmp).await;
            return Err(e);
        }

        let slot_dir = self.stage_dir().join(slot);
        let _ = host.remove_dir_all(&slot_dir).await;
        host.create_dir_all(&slot_dir).await?;
        let archive = self.stage_dir().join(format!("{slot}.{}", ext.as_str()));
        let uploaded = host.upload(&local_tmp, &archive).await;
        let _ = tokio::fs::remove_file(&local_tmp).await;
        uploaded?;
        host.extract_archive(&archive, &slot_dir, kind).await?;
        let _ = host.remove_file(&archive).await;

        let dirs: Vec<String> = host
            .list_dir(&slot_dir)
            .await?
            .into_iter()
            .filter(|e| e.is_dir)
            .map(|e| e.name)
            .collect();
        let [top] = dirs.as_slice() else {
            return Err(ActionError::install_step(
                "extract",
                format!("{repo} 源码包结构不对：顶层应只有一个目录，实际 {dirs:?}"),
            ));
        };
        Ok(StagedSource {
            root: slot_dir.join(top),
            tag: tag.to_string(),
        })
    }

    /// 先定适配器，再按它声明的兼容范围挑 MaiBot（原因见 release.rs）。任一步拿不到就回落内置组合
    async fn stage_sources(
        &self,
        host: &dyn Host,
        ctx: &ActionCtx,
    ) -> Result<(StagedSource, StagedSource), ActionError> {
        begin(ctx, 2, "下载 NapCat 适配器").await;
        let adapter_tag = match fetch_stable_release_tags(ADAPTER_REPO).await {
            Ok(tags) if !tags.is_empty() => tags[0].clone(),
            Ok(_) => PINNED_ADAPTER_TAG.to_string(),
            Err(e) => {
                ctx.warn(format!("{e}；改用内置版本")).await;
                PINNED_ADAPTER_TAG.to_string()
            }
        };
        let mut adapter = self
            .stage_archive(host, ctx, 2, ADAPTER_REPO, &adapter_tag, "adapter")
            .await?;
        let (min, max) = self.adapter_range(host, &adapter).await?;
        ctx.info(format!("适配器 {} 兼容 MaiBot {min} ~ {max}", adapter.tag))
            .await;

        let picked = match fetch_stable_release_tags(MAIBOT_REPO).await {
            Ok(tags) => pick_maibot_tag(&tags, &min, &max),
            Err(e) => {
                ctx.warn(format!("{e}；改用内置版本")).await;
                None
            }
        };
        let maibot_tag = match picked {
            Some(tag) => tag,
            None => {
                if adapter.tag != PINNED_ADAPTER_TAG {
                    ctx.warn(format!(
                        "没有和适配器 {} 兼容的 MaiBot 正式版，改装内置组合 {PINNED_MAIBOT_TAG} + {PINNED_ADAPTER_TAG}",
                        adapter.tag
                    ))
                    .await;
                    adapter = self
                        .stage_archive(host, ctx, 2, ADAPTER_REPO, PINNED_ADAPTER_TAG, "adapter")
                        .await?;
                    let (min, max) = self.adapter_range(host, &adapter).await?;
                    if !host_compatible(PINNED_MAIBOT_TAG, &min, &max) {
                        ctx.warn("内置组合的版本范围对不上，适配器可能不会被加载").await;
                    }
                } else if !host_compatible(PINNED_MAIBOT_TAG, &min, &max) {
                    ctx.warn("内置组合的版本范围对不上，适配器可能不会被加载").await;
                }
                PINNED_MAIBOT_TAG.to_string()
            }
        };
        end(ctx, 2).await;
        begin(ctx, 3, format!("下载 MaiBot {maibot_tag} 源码")).await;
        let maibot = self
            .stage_archive(host, ctx, 3, MAIBOT_REPO, &maibot_tag, "maibot")
            .await?;
        end(ctx, 3).await;
        Ok((maibot, adapter))
    }

    async fn adapter_range(
        &self,
        host: &dyn Host,
        adapter: &StagedSource,
    ) -> Result<(String, String), ActionError> {
        let text = read_text(host, &adapter.root.join(ADAPTER_MANIFEST_FILE))
            .await?
            .unwrap_or_default();
        adapter_host_range(&text).ok_or_else(|| {
            ActionError::install_step(
                "adapter-manifest",
                format!("适配器 {} 的 _manifest.json 缺少 host_application", adapter.tag),
            )
        })
    }

    /// 源码顶层条目挪进实例目录。更新时用户数据那几项原样留下
    async fn place_maibot(
        &self,
        host: &dyn Host,
        staged: &StagedSource,
        updating: bool,
    ) -> Result<(), ActionError> {
        for entry in host.list_dir(&staged.root).await? {
            if updating && PRESERVED_ON_UPDATE.contains(&entry.name.as_str()) {
                continue;
            }
            let dest = self.install_dir.join(&entry.name);
            remove_existing(host, &dest, entry.is_dir).await?;
            host.rename(&staged.root.join(&entry.name), &dest).await?;
        }
        Ok(())
    }

    /// 适配器整目录换代码，只留用户的 config.toml
    async fn place_adapter(&self, host: &dyn Host, staged: &StagedSource) -> Result<(), ActionError> {
        let dest = self.install_dir.join(ADAPTER_DIR);
        if !host.exists(&dest).await? {
            if let Some(parent) = dest.parent() {
                host.create_dir_all(&parent).await?;
            }
            host.rename(&staged.root, &dest).await?;
            return Ok(());
        }
        for entry in host.list_dir(&dest).await? {
            if entry.name == "config.toml" {
                continue;
            }
            remove_existing(host, &dest.join(&entry.name), entry.is_dir).await?;
        }
        for entry in host.list_dir(&staged.root).await? {
            if entry.name == "config.toml" {
                continue;
            }
            host.rename(&staged.root.join(&entry.name), &dest.join(&entry.name))
                .await?;
        }
        Ok(())
    }
}

impl MaiBotComponent {
    async fn run_step(
        &self,
        host: &dyn Host,
        ctx: &mut ActionCtx,
        step: u32,
        name: &str,
        cmd: HostCommand,
    ) -> Result<(), ActionError> {
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        ctx.emit(ProgressKind::StepBegin {
            step,
            message: name.to_string(),
        })
        .await;
        let cmd = cmd
            .timeout(SYNC_TIMEOUT)
            .cancel_token(ctx.cancel_token())
            .env("UV_NO_PROGRESS", "1")
            .env("PYTHONUTF8", "1");
        let log_ctx = ctx.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let drain = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                log_ctx.info(line).await;
            }
        });
        let out = host
            .run_streaming(
                cmd,
                Box::new(move |_src, line| {
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        let _ = tx.send(trimmed.to_string());
                    }
                }),
            )
            .await;
        drain.abort();
        let out = out.map_err(|e| ActionError::install_step(name, e.to_string()))?;
        if !out.success() {
            ctx.emit(ProgressKind::StepEnd { step, ok: false }).await;
            return Err(ActionError::install_step(
                name,
                format!(
                    "exit={:?}: {} MaiBot 需要 Python {MAIBOT_PYTHON_REQUIRES}，确认 uv 能下载托管的解释器、能连上依赖源。",
                    out.exit_code,
                    out.stderr.trim().lines().last().unwrap_or_default()
                ),
            ));
        }
        ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
        Ok(())
    }

    /// 装之前看一眼 Linux 主机：glibc 旧了 uv 会退回源码编译，几十分钟后才报一串编译错误；
    /// 依赖只有 x86_64 / aarch64 的轮子。探测不出来的不拦，真装不上 uv 自己会报
    async fn preflight(&self, host: &dyn Host, ctx: &ActionCtx) -> Result<(), ActionError> {
        if host.os() != Os::Linux {
            return Ok(());
        }
        let arch = match host.locality() {
            Locality::Remote => probe_remote_arch(host).await.ok(),
            Locality::Local => Some(host.arch()),
        };
        let unsupported = match arch {
            Some(Arch::X86) => Some("32 位 x86"),
            Some(Arch::Armv7) => Some("32 位 ARM"),
            _ => None,
        };
        if let Some(what) = unsupported {
            return Err(ActionError::install_step(
                "preflight",
                format!("MaiBot 的依赖只有 x86_64 / aarch64 的 Linux 包，这台主机是 {what}"),
            ));
        }
        let (major, minor) = MAIBOT_MIN_GLIBC;
        match probe_linux_libc(host).await {
            LinuxLibc::Musl => {
                return Err(ActionError::install_step(
                    "preflight",
                    "这台主机用的是 musl（Alpine 一类），MaiBot 的依赖装不上，换 Debian / Ubuntu 系的系统",
                ));
            }
            LinuxLibc::Glibc { major: a, minor: b } if (a, b) < (major, minor) => {
                return Err(ActionError::install_step(
                    "preflight",
                    format!(
                        "这台主机的 glibc 是 {a}.{b}，MaiBot 的依赖（pyarrow 等）要 {major}.{minor} 以上：\
                         Ubuntu 20.04、Debian 10、CentOS 8 及更新的系统可以装"
                    ),
                ));
            }
            _ => {}
        }
        if let Some(kb) = probe_free_kb(host, &self.install_dir).await
            && kb < MAIBOT_MIN_FREE_KB
        {
            ctx.warn(format!(
                "安装目录所在分区只剩 {:.1} GB，依赖加 uv 缓存要两三 GB，可能装不完",
                kb as f64 / 1024.0 / 1024.0
            ))
            .await;
        }
        Ok(())
    }

    async fn provision(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        let updating = host.exists(&self.install_dir.join(BOT_PY)).await?;
        ctx.emit(ProgressKind::Started { total_steps: 7 }).await;

        begin(ctx, 1, "检查主机、解析 uv").await;
        self.preflight(host, ctx).await?;
        let preferred = self.preferred_uvs(host).await;
        let uv = resolve_uv(host, &preferred).await?;
        ctx.info(format!("uv {}: {}", uv.version, uv.uv_bin.as_posix()))
            .await;
        host.create_dir_all(&self.install_dir).await?;
        write_uv_marker(host, &self.install_dir, &uv).await?;
        end(ctx, 1).await;

        let (maibot, adapter) = self.stage_sources(host, ctx).await?;
        ctx.info(format!("安装 MaiBot {} + NapCat 适配器 {}", maibot.tag, adapter.tag))
            .await;

        begin(ctx, 4, if updating { "替换源码（保留配置、数据与插件）" } else { "放置源码" }).await;
        self.place_maibot(host, &maibot, updating).await?;
        self.place_adapter(host, &adapter).await?;
        let _ = host.remove_dir_all(&self.stage_dir()).await;
        end(ctx, 4).await;

        begin(ctx, 5, format!("准备 Python {MAIBOT_PYTHON_REQUIRES}")).await;
        ensure_python(
            host,
            &uv.uv_bin,
            MAIBOT_PYTHON_REQUIRES,
            &self.install_dir.join(PYTHON_SCRATCH_DIR),
            ctx,
            5,
        )
        .await?;
        end(ctx, 5).await;

        let sync = HostCommand::new(uv.uv_bin.as_posix())
            .arg("sync")
            .arg("--locked")
            .arg("--no-dev")
            .arg("--no-install-project")
            .arg("--python")
            .arg(MAIBOT_PYTHON_REQUIRES)
            .working_dir(self.install_dir.clone())
            .env("UV_PROJECT_ENVIRONMENT", ".venv");
        self.run_step(host, ctx, 6, "同步 Python 依赖（uv sync，首次要下几百 MB）", sync)
            .await?;

        begin(ctx, 7, "预置端口、WebUI token 与协议确认").await;
        self.seed(host, ctx, updating).await?;
        end(ctx, 7).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    /// 只补缺的文件，已有的一律不碰（更新时用户在 WebUI 里改过的都在这些文件里）
    async fn seed(&self, host: &dyn Host, ctx: &ActionCtx, updating: bool) -> Result<(), ActionError> {
        let bot_config = self.install_dir.join(BOT_CONFIG);
        if !host.exists(&bot_config).await? {
            let config_py = read_text(host, &self.install_dir.join(CONFIG_PY))
                .await?
                .unwrap_or_default();
            let version = read_config_version(&config_py).ok_or_else(|| {
                ActionError::install_step(
                    "seed-config",
                    format!("{CONFIG_PY} 里找不到 CONFIG_VERSION，上游代码结构可能变了"),
                )
            })?;
            let legacy = self.pick_legacy_port(host).await;
            let text = write_bot_config_ports(None, &version, self.webui_port, legacy)
                .map_err(|e| ActionError::install_step("seed-config", e))?;
            host.create_dir_all(&self.install_dir.join("config")).await?;
            host.write_file(&bot_config, text.as_bytes()).await?;
            ctx.info(format!(
                "WebUI 端口 {}，旧版消息服务端口 {legacy}",
                self.webui_port
            ))
            .await;
        }
        let webui_json = self.install_dir.join(WEBUI_JSON);
        if !host.exists(&webui_json).await? {
            host.create_dir_all(&self.install_dir.join("data")).await?;
            host.write_file(&webui_json, render_webui_json(&generate_webui_token()).as_bytes())
                .await?;
        }
        // 用户在新建对话框里同意过才走到首装；更新后条款变了要重新问，不能在这里补写
        if !updating {
            write_confirmations(host, &self.install_dir, true)
                .await
                .map_err(|e| ActionError::install_step("terms", e.to_string()))?;
        }
        Ok(())
    }

    async fn pick_legacy_port(&self, host: &dyn Host) -> u16 {
        let candidates = legacy_port_candidates(self.webui_port);
        if let Some(usage) = PortUsage::probe(host).await
            && let Some(port) = candidates.iter().copied().find(|p| usage.is_free(*p))
        {
            return port;
        }
        // 探不出来就给第一个候选，真被占了上游启动日志会说是哪个口
        candidates.first().copied().unwrap_or(self.webui_port.wrapping_add(1))
    }

    async fn read_project(&self, host: &dyn Host) -> Result<Option<(String, String)>, ActionError> {
        let Some(text) = read_text(host, &self.install_dir.join(PYPROJECT)).await? else {
            return Ok(None);
        };
        let Ok(root) = toml::from_str::<toml::Table>(&text) else {
            return Ok(None);
        };
        let project = root.get("project").and_then(toml::Value::as_table);
        let name = project
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or_default();
        let version = project
            .and_then(|p| p.get("version"))
            .and_then(toml::Value::as_str)
            .unwrap_or("installed");
        Ok(Some((name.to_string(), version.to_string())))
    }

    /// 实例 venv 的解释器直接跑 bot.py（保留 Runner：WebUI 里的「重启」靠它接退出码 42）
    pub async fn resolve_launch_command(
        &self,
        host: &dyn Host,
        args: &LaunchArgs,
    ) -> Result<HostCommand, ActionError> {
        let python = self.venv_python(host.os());
        if !host.exists(&python).await? {
            return Err(ActionError::other(format!(
                "实例虚拟环境不存在（{}），请先安装 / 重新安装",
                python.as_posix()
            )));
        }
        let uv_dir = self
            .preferred_uvs(host)
            .await
            .into_iter()
            .find_map(|p| p.parent())
            .filter(|d| !d.as_posix().is_empty() && d.as_posix() != ".");
        Ok(self.launch_with(host.os(), host.locality(), args, uv_dir.as_ref()))
    }

    fn launch_with(
        &self,
        os: Os,
        locality: Locality,
        args: &LaunchArgs,
        uv_dir: Option<&HostPath>,
    ) -> HostCommand {
        let mut cmd = HostCommand::new(self.venv_python(os).as_posix())
            .arg(BOT_PY)
            .working_dir(self.install_dir.clone())
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .long_running();
        // 上游启动时把这几个环境变量迁进配置、WEBUI_HOST 还会并进监听地址；用户机器上常有别的程序留下的
        // PORT=3000 之类，会顶掉桌面端分好的口。HostCommand 只能加不能删，置空即可：上游把空串当没设
        for key in ["HOST", "PORT", "WEBUI_HOST", "WEBUI_PORT"] {
            cmd = cmd.env(key, "");
        }
        // 上游装插件依赖时只认 PATH 上的 uv，找不到就退回 `python -m pip`，而 uv 建的 venv 里没有 pip
        if let Some(dir) = uv_dir {
            cmd = cmd.env("PATH", path_with_uv(dir, os, locality));
        }
        args.apply_to(cmd)
    }
}

fn path_with_uv(uv_dir: &HostPath, os: Os, locality: Locality) -> String {
    let prefix = vec![uv_dir.render_for(os)];
    match locality {
        Locality::Local => crate::node_tooling::local_path_env(&prefix, os),
        // 远端守护进程给一份确定的 PATH，和 Karin 一样
        Locality::Remote => {
            let mut parts = prefix;
            parts.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(String::from));
            parts.join(":")
        }
    }
}

async fn begin(ctx: &ActionCtx, step: u32, message: impl Into<String>) {
    ctx.emit(ProgressKind::StepBegin {
        step,
        message: message.into(),
    })
    .await;
}

async fn end(ctx: &ActionCtx, step: u32) {
    ctx.emit(ProgressKind::StepEnd { step, ok: true }).await;
}

/// 照上游 `_read_config_constant` 认：行首 `CONFIG_VERSION: str = "x.y.z"`
pub fn read_config_version(config_py: &str) -> Option<String> {
    read_version_constant(config_py, "CONFIG_VERSION")
}

/// 同上，按常量名取（`MODEL_CONFIG_VERSION` 也在同一个文件里）
pub fn read_version_constant(config_py: &str, name: &str) -> Option<String> {
    config_py.lines().find_map(|line| {
        let rest = line.strip_prefix(name)?.trim_start();
        let rest = rest.strip_prefix(':')?.trim_start();
        let rest = rest.strip_prefix("str")?.trim_start();
        let rest = rest.strip_prefix('=')?.trim_start();
        let rest = rest.strip_prefix('"')?;
        let v = &rest[..rest.find('"')?];
        (!v.is_empty()).then(|| v.to_string())
    })
}

/// 满足上游自定义 token 的口令规则（至少 10 位，含大写、小写、特殊符号），`Ncd_` 前缀就占齐了
pub fn generate_webui_token() -> String {
    let body: String = rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();
    format!("Ncd_{body}")
}

/// `configured` 让上游不在每次启动时换 token；不写 `first_setup_completed`，首次登录照样走配置向导
pub fn render_webui_json(token: &str) -> String {
    let v = serde_json::json!({
        "access_token": token,
        "first_setup_completed": false,
        "token_source": "configured",
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&v).unwrap_or_else(|_| "{}".to_string())
    )
}

/// 旧版消息口挨着 WebUI 口找：先往上再往下，避开 0 和 WebUI 口本身
pub fn legacy_port_candidates(webui_port: u16) -> Vec<u16> {
    let up = (1..=PORT_PROBE_TRIES).filter_map(|i| webui_port.checked_add(i));
    let down = (1..=PORT_PROBE_TRIES).filter_map(|i| webui_port.checked_sub(i));
    up.chain(down).filter(|p| *p != 0).collect()
}

#[async_trait]
impl Component for MaiBotComponent {
    fn id(&self) -> ComponentId {
        ComponentId::MaiBot
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        SUPPORTED
    }

    fn requirements(&self, _os: Os, _locality: Locality) -> Vec<Requirement> {
        vec![Requirement::component_version(
            ComponentId::Uv,
            MAIBOT_UV_VERSION_RANGE,
        )]
    }

    async fn detect(&self, host: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(self.detect_outcome(host).await?.into_installed())
    }

    async fn detect_outcome(&self, host: &dyn Host) -> Result<DetectOutcome, ActionError> {
        if !host.exists(&self.install_dir.join(BOT_PY)).await? {
            return Ok(DetectOutcome::NotInstalled);
        }
        let Some((name, version)) = self.read_project(host).await? else {
            return Ok(DetectOutcome::NotInstalled);
        };
        if name != PYPROJECT_NAME {
            return Ok(DetectOutcome::NotInstalled);
        }
        if !host.exists(&self.venv_python(host.os())).await? {
            return Ok(DetectOutcome::Unusable(UnusableInstall {
                source: self.install_dir.as_posix().to_string(),
                version: Some(version),
                reason: "源码已就位但依赖未同步，重新安装可修复".to_string(),
            }));
        }
        Ok(DetectOutcome::Installed(DetectedVersion {
            version,
            source: self.install_dir.join(PYPROJECT).as_posix().to_string(),
        }))
    }

    async fn install(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        self.provision(host, ctx).await
    }

    async fn update(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.check_target(host)?;
        self.provision(host, ctx).await
    }

    async fn uninstall(&self, host: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        ctx.emit(ProgressKind::Started { total_steps: 1 }).await;
        begin(ctx, 1, format!("删除 {}", self.install_dir.as_posix())).await;
        if host.exists(&self.install_dir).await? {
            host.remove_dir_all(&self.install_dir).await?;
        }
        end(ctx, 1).await;
        ctx.emit(ProgressKind::Finished { ok: true }).await;
        Ok(())
    }

    async fn verify(&self, host: &dyn Host) -> Result<VerifyReport, ActionError> {
        let mut report = VerifyReport::ok();
        for (name, path) in [
            (BOT_PY, self.install_dir.join(BOT_PY)),
            ("python", self.venv_python(host.os())),
            ("napcat-adapter", self.install_dir.join(ADAPTER_DIR).join(ADAPTER_MANIFEST_FILE)),
        ] {
            let ok = host.exists(&path).await?;
            report = report.with_check(name, ok, Some(path.as_posix().to_string()));
        }
        Ok(report)
    }

    fn launch_command(&self, host: &dyn Host, args: &LaunchArgs) -> Result<HostCommand, ActionError> {
        Ok(self.launch_with(host.os(), host.locality(), args, None))
    }
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, ActionError> {
    if !host.exists(path).await? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

async fn remove_existing(host: &dyn Host, path: &HostPath, is_dir: bool) -> Result<(), ActionError> {
    if !host.exists(path).await? {
        return Ok(());
    }
    if is_dir {
        host.remove_dir_all(path).await?;
    } else {
        host.remove_file(path).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_version_is_read_like_upstream() {
        let src = "MODEL_CONFIG_VERSION: str = \"1.17.9\"\nCONFIG_VERSION_OLD: str = \"1.0.0\"\nCONFIG_VERSION: str = \"8.14.40\"\n";
        assert_eq!(read_config_version(src).as_deref(), Some("8.14.40"));
        assert_eq!(read_config_version("    CONFIG_VERSION: str = \"1\"\n"), None, "上游只认行首");
        assert_eq!(read_config_version("CONFIG_VERSION = \"1\"\n"), None);
        assert_eq!(read_version_constant(src, "MODEL_CONFIG_VERSION").as_deref(), Some("1.17.9"));
    }

    #[test]
    fn webui_token_meets_upstream_custom_token_rule() {
        let t = generate_webui_token();
        assert!(t.len() >= 10);
        assert!(t.chars().any(|c| c.is_ascii_uppercase()));
        assert!(t.chars().any(|c| c.is_ascii_lowercase()));
        assert!(t.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?/".contains(c)));
        assert_ne!(t, generate_webui_token());
    }

    #[test]
    fn webui_json_is_configured_and_keeps_setup_wizard() {
        let v: serde_json::Value = serde_json::from_str(&render_webui_json("Ncd_x")).unwrap();
        assert_eq!(v["access_token"], "Ncd_x");
        assert_eq!(v["token_source"], "configured");
        assert_eq!(v["first_setup_completed"], false);
        assert_eq!(super::super::config::read_webui_token(Some(&render_webui_json("Ncd_x"))), "Ncd_x");
    }

    #[test]
    fn legacy_port_candidates_stay_near_webui_and_skip_it() {
        let c = legacy_port_candidates(23001);
        assert_eq!(c[0], 23002);
        assert!(!c.contains(&23001));
        let top = legacy_port_candidates(u16::MAX);
        assert_eq!(top[0], u16::MAX - 1, "顶到头就往下找");
        assert!(!legacy_port_candidates(1).contains(&0));
    }

    #[test]
    fn launch_runs_bot_py_with_uv_on_path() {
        let comp = MaiBotComponent::new(HostPath::from_posix("/home/u/ncd/apps/maibot/m1"), 23001);
        let uv = HostPath::from_posix("/home/u/ncd/tools/uv");
        let cmd = comp.launch_with(Os::Linux, Locality::Remote, &LaunchArgs::default(), Some(&uv));
        assert_eq!(cmd.program, "/home/u/ncd/apps/maibot/m1/.venv/bin/python");
        assert_eq!(cmd.args, vec!["bot.py".to_string()]);
        let path = cmd
            .environment
            .iter()
            .find(|(k, _)| *k == "PATH")
            .map(|(_, v)| v.as_str());
        assert_eq!(path, Some("/home/u/ncd/tools/uv:/usr/local/bin:/usr/bin:/bin"));
        assert!(
            !cmd.environment.iter().any(|(k, _)| k == "MAIBOT_WORKER_PROCESS"),
            "要让 bot.py 以 Runner 身份起"
        );
        for key in ["HOST", "PORT", "WEBUI_HOST", "WEBUI_PORT"] {
            assert_eq!(cmd.environment.get(key).map(String::as_str), Some(""), "{key} 置空，不让外面的值顶掉桌面端分的口");
        }

        let bare = comp.launch_with(Os::Linux, Locality::Remote, &LaunchArgs::default(), None);
        assert!(!bare.environment.iter().any(|(k, _)| k == "PATH"));
    }

    #[test]
    fn requirements_pin_uv() {
        let comp = MaiBotComponent::new(HostPath::from_posix("/x"), 1);
        assert_eq!(
            comp.requirements(Os::Windows, Locality::Local),
            vec![Requirement::component_version(ComponentId::Uv, ">=0.4")]
        );
        assert_eq!(comp.id(), ComponentId::MaiBot);
    }
}
