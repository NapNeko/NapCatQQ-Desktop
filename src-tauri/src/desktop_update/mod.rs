//! Desktop 自更新：GitHub Release MSI 路径（对齐 legacy + CI 产物命名）。
//!
//! 不走 tauri-plugin-updater 签名包；下载 MSI 后用 msiexec 安装
//!（`/passive` 进度条 + `/norestart` 不重启系统），资产名与 build-msi.yml
//! 一致：`NapCatQQ-Desktop-{ver}-x64.msi` / 别名 `NapCatQQ-Desktop-x64.msi`。
//!
//! 装完自启：在退出前 spawn 独立 helper，等本进程退出后再启动新 exe
//!（避免文件锁；不依赖整机 reboot）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use ncd_domain::SchemaVersion;
use ncd_domain::release_snapshot::ReleaseInfo;
use ncd_domain::{ProgressEvent, ProgressKind, ProgressLogLevel};
use ncd_network::{
    DownloadConfig, DownloadProgressSink, DownloadStage, NoopProgressSink, ProgressUpdate,
    download_with_resume,
};
use ncd_runtime::release::fetch_release_snapshot;
use ncd_runtime::{BroadcastEventBus, DomainEvent, EventBus};
use ncd_update::{AvailableUpdate, UpdateChannel, UpdateError, UpdateProvider};
use semver::Version;
use tokio_util::sync::CancellationToken;
use tracing::info;

// 跨边界类型 re-export：定义在 ncd-update（ts-rs 路径与 AvailableUpdate 一致）
pub use ncd_update::{DesktopUpdateNoticeKind, DesktopUpdateStartupNotice};

/// 与 CI collect 步骤 / legacy UpdateManager 一致的 MSI 命名。
pub const MSI_VERSIONED_NAME_FMT: &str = "NapCatQQ-Desktop-{version}-x64.msi";
pub const MSI_ALIAS_NAME: &str = "NapCatQQ-Desktop-x64.msi";
/// 安装树主程序（与 autostart / WiX 一致）
pub const MAIN_EXE_NAME: &str = "NapCatQQ-Desktop.exe";
const MINIMUM_MSI_SIZE_BYTES: u64 = 1024 * 1024;
/// 仅允许本仓库 GitHub Releases 下载路径，防止 IPC 篡改 download_url。
const TRUSTED_RELEASE_HOST: &str = "github.com";
const TRUSTED_RELEASE_OWNER: &str = "NapNeko";
const TRUSTED_RELEASE_REPO: &str = "NapCatQQ-Desktop";

/// 去掉 v/V 前缀，便于版本字符串比较。
pub fn normalize_version_label(raw: &str) -> String {
    raw.trim().trim_start_matches(['v', 'V']).to_string()
}

/// 判断 FileVersion/ProductVersion 是否已到达目标版本。
/// MSI 常写成 `3.1.6.0`，目标可能是 `3.1.6`：前缀匹配即可。
pub fn file_version_matches_target(file_version: &str, target: &str) -> bool {
    let file = normalize_version_label(file_version);
    let target = normalize_version_label(target);
    if file.is_empty() || target.is_empty() {
        return false;
    }
    file == target || file.starts_with(&format!("{target}."))
}

/// 产品版本（build.rs 从 tauri.conf.json 注入；回退 workspace crate 版本仅开发兜底）。
pub fn product_version_str() -> &'static str {
    option_env!("NCD_PRODUCT_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub fn product_version() -> Result<Version, UpdateError> {
    Version::parse(product_version_str().trim_start_matches(['v', 'V'])).map_err(|e| {
        UpdateError::Internal(format!(
            "invalid product version {}: {e}",
            product_version_str()
        ))
    })
}

/// 是否为可信 Desktop MSI 下载 URL（固定 owner/repo + releases/download + .msi）。
pub fn is_trusted_desktop_msi_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url.trim()) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    if parsed.host_str() != Some(TRUSTED_RELEASE_HOST) {
        return false;
    }
    let segments: Vec<&str> = match parsed.path_segments() {
        Some(s) => s.filter(|p| !p.is_empty()).collect(),
        None => return false,
    };
    // /NapNeko/NapCatQQ-Desktop/releases/download/<tag>/<file.msi>
    match segments.as_slice() {
        [owner, repo, "releases", "download", tag, file]
            if *owner == TRUSTED_RELEASE_OWNER
                && *repo == TRUSTED_RELEASE_REPO
                && !tag.is_empty()
                && is_desktop_msi_asset_name(file) =>
        {
            true
        }
        _ => false,
    }
}

fn is_desktop_msi_asset_name(name: &str) -> bool {
    let name = name.trim();
    if !name.ends_with(".msi") || !name.contains("NapCatQQ-Desktop") {
        return false;
    }
    // versioned / alias 都以 x64.msi 结尾；拒绝奇怪拼接
    name.ends_with("x64.msi") || name == MSI_ALIAS_NAME
}

/// 从 ReleaseInfo 选出 MSI 下载 URL + 可选 sha256。
pub fn pick_desktop_msi(
    info: &ReleaseInfo,
) -> Result<(String, String, Option<String>), UpdateError> {
    let version_plain = info.version.trim().trim_start_matches(['v', 'V']);
    let versioned = MSI_VERSIONED_NAME_FMT.replace("{version}", version_plain);
    let tag = if info.tag.trim().is_empty() {
        format!("v{version_plain}")
    } else {
        info.tag.clone()
    };

    // 优先 versioned 名，再 alias，再收紧的模糊匹配；API 无 assets 时仍拼规范 URL
    let asset_name = info
        .assets
        .iter()
        .find(|a| a.name == versioned)
        .map(|a| a.name.clone())
        .or_else(|| {
            info.assets
                .iter()
                .find(|a| a.name == MSI_ALIAS_NAME)
                .map(|a| a.name.clone())
        })
        .or_else(|| {
            info.assets
                .iter()
                .find(|a| is_desktop_msi_asset_name(&a.name))
                .map(|a| a.name.clone())
        })
        .unwrap_or(versioned);

    let sha = info
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .and_then(|a| {
            let s = a.sha256.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_ascii_lowercase())
            }
        });

    let url = format!(
        "https://{TRUSTED_RELEASE_HOST}/{TRUSTED_RELEASE_OWNER}/{TRUSTED_RELEASE_REPO}/releases/download/{tag}/{asset_name}"
    );
    if !is_trusted_desktop_msi_url(&url) {
        return Err(UpdateError::check_failed(format!(
            "constructed download URL failed trust check: {url}"
        )));
    }
    Ok((url, asset_name, sha))
}

pub fn release_info_to_available_update(
    info: &ReleaseInfo,
) -> Result<AvailableUpdate, UpdateError> {
    let version = Version::parse(info.version.trim().trim_start_matches(['v', 'V']))
        .map_err(|e| UpdateError::check_failed(format!("desktop release version parse: {e}")))?;
    let (download_url, _name, sha) = pick_desktop_msi(info)?;
    let pub_date = if info.published_at > 0 {
        Utc.timestamp_opt(info.published_at as i64, 0)
            .single()
            .unwrap_or_else(Utc::now)
    } else {
        Utc::now()
    };
    Ok(AvailableUpdate {
        v: ncd_update::types::UPDATE_PROTOCOL_VERSION,
        version,
        // MSI 路径暂固定当前 schema；跨版本迁移元数据以后可从 release notes/manifest 读
        schema_version: SchemaVersion::CURRENT,
        notes: info.release_notes.clone(),
        pub_date,
        download_url,
        signature: String::new(),
        content_sha256: sha.unwrap_or_default(),
    })
}

/// 把 ncd-network 下载进度翻成 component_action_progress，复用组件页任务队列 UI。
pub struct DesktopUpdateProgressSink {
    event_bus: BroadcastEventBus,
    task_id: String,
    /// 当前 step（检查=1，下载=2）；下载开始前 set_step(2)
    step: std::sync::atomic::AtomicU32,
}

impl DesktopUpdateProgressSink {
    pub fn new(event_bus: BroadcastEventBus, task_id: impl Into<String>, step: u32) -> Self {
        Self {
            event_bus,
            task_id: task_id.into(),
            step: std::sync::atomic::AtomicU32::new(step),
        }
    }

    pub fn set_step(&self, step: u32) {
        self.step.store(step, std::sync::atomic::Ordering::Relaxed);
    }

    fn current_step(&self) -> u32 {
        self.step.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn publish(&self, kind: ProgressKind) {
        self.event_bus
            .publish(DomainEvent::component_action_progress(
                self.task_id.clone(),
                ProgressEvent::new(kind),
            ));
    }

    pub fn emit_started(&self, total_steps: u32) {
        self.publish(ProgressKind::Started { total_steps });
    }

    pub fn emit_step_begin(&self, step: u32, message: impl Into<String>) {
        self.publish(ProgressKind::StepBegin {
            step,
            message: message.into(),
        });
    }

    pub fn emit_step_end(&self, step: u32, ok: bool) {
        self.publish(ProgressKind::StepEnd { step, ok });
    }

    pub fn emit_log(&self, level: ProgressLogLevel, message: impl Into<String>) {
        self.publish(ProgressKind::Log {
            level,
            message: message.into(),
        });
    }

    pub fn emit_finished(&self, ok: bool) {
        self.publish(ProgressKind::Finished { ok });
    }
}

#[async_trait]
impl DownloadProgressSink for DesktopUpdateProgressSink {
    async fn tick(&self, update: ProgressUpdate) {
        let pct = match (update.total, update.downloaded) {
            (Some(t), d) if t > 0 => ((d as f64 / t as f64) * 100.0).clamp(0.0, 99.0) as u8,
            _ => 0,
        };
        let stage_id = match update.stage {
            DownloadStage::Racing => "racing",
            DownloadStage::Streaming => "streaming",
            DownloadStage::SwitchingMirror => "switching_mirror",
            DownloadStage::Resuming => "resuming",
        };
        let message = if update.message.trim().is_empty() {
            match (update.total, update.speed_bps) {
                (Some(t), Some(bps)) => format!(
                    "下载 {} / {} · {}/s",
                    fmt_bytes(update.downloaded),
                    fmt_bytes(t),
                    fmt_bytes(bps)
                ),
                (Some(t), None) => {
                    format!("下载 {} / {}", fmt_bytes(update.downloaded), fmt_bytes(t))
                }
                (None, Some(bps)) => {
                    format!(
                        "下载 {} · {}/s",
                        fmt_bytes(update.downloaded),
                        fmt_bytes(bps)
                    )
                }
                (None, None) => format!("下载 {}", fmt_bytes(update.downloaded)),
            }
        } else {
            update.message
        };

        self.publish(ProgressKind::StepProgress {
            step: self.current_step(),
            percent: pct,
            message,
            speed_bps: update.speed_bps,
            downloaded_bytes: Some(update.downloaded),
            total_bytes: update.total,
            download_stage: Some(stage_id.to_string()),
            docker_layers: None,
        });
    }
}

fn fmt_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

pub struct GithubMsiUpdateProvider {
    data_root: PathBuf,
    /// 检查时注入 PAT 的闭包结果缓存：命令层构造时读一次即可
    github_token: Option<String>,
    /// 下载进度（install 路径注入；check 路径为 None）
    progress: Option<Arc<dyn DownloadProgressSink>>,
    cancel: CancellationToken,
}

impl GithubMsiUpdateProvider {
    pub fn new(data_root: impl Into<PathBuf>, github_token: Option<String>) -> Self {
        Self {
            data_root: data_root.into(),
            github_token,
            progress: None,
            cancel: CancellationToken::new(),
        }
    }

    pub fn with_progress(mut self, sink: Arc<dyn DownloadProgressSink>) -> Self {
        self.progress = Some(sink);
        self
    }

    pub fn with_cancel(mut self, token: CancellationToken) -> Self {
        self.cancel = token;
        self
    }

    fn staging_dir(&self) -> PathBuf {
        self.data_root.join("runtime").join("tmp")
    }

    fn staged_msi_path(&self) -> PathBuf {
        self.staging_dir().join(MSI_ALIAS_NAME)
    }

    async fn verify_sha256_if_needed(
        path: &Path,
        expected: Option<&str>,
    ) -> Result<(), UpdateError> {
        let Some(expected) = expected.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(());
        };
        use sha2::{Digest, Sha256};
        use tokio::io::AsyncReadExt;
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| UpdateError::install_failed(format!("open msi for hash: {e}")))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = file
                .read(&mut buf)
                .await
                .map_err(|e| UpdateError::install_failed(format!("read msi for hash: {e}")))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let actual = hex::encode(hasher.finalize());
        if !expected.eq_ignore_ascii_case(&actual) {
            let _ = tokio::fs::remove_file(path).await;
            return Err(UpdateError::install_failed(format!(
                "msi content sha256 mismatch: expected {expected}, actual {actual}"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl UpdateProvider for GithubMsiUpdateProvider {
    async fn check(&self, channel: UpdateChannel) -> Result<Option<AvailableUpdate>, UpdateError> {
        // 产品策略：暂不开放更新渠道，一律按正式 Desktop release 拉（ignore channel）
        let _ = channel;
        // 用户主动「检查更新」:跳过 release 磁盘 TTL,避免半小时内假缓存挡检查
        let snap =
            fetch_release_snapshot(&self.data_root, self.github_token.as_deref(), true).await;
        let Some(info) = snap.desktop_latest.as_ref() else {
            // 与「已是最新」区分：无快照 = 检查失败，由上层变成 Err 字符串，UI 勿显示「无需更新」
            return Err(UpdateError::check_failed(
                "无法获取 Desktop 最新版本（网络不可达、缓存为空或尚无正式 Release）",
            ));
        };
        let update = release_info_to_available_update(info)?;
        Ok(Some(update))
    }

    async fn download_and_install(&self, update: &AvailableUpdate) -> Result<(), UpdateError> {
        if !is_trusted_desktop_msi_url(&update.download_url) {
            return Err(UpdateError::install_failed(format!(
                "拒绝非本仓库 GitHub Release 的下载地址: {}",
                update.download_url
            )));
        }

        #[cfg(not(windows))]
        {
            let _ = update;
            return Err(UpdateError::install_failed(
                "Desktop MSI 自更新仅支持 Windows",
            ));
        }

        #[cfg(windows)]
        {
            download_and_install_windows(self, update).await
        }
    }
}

#[cfg(windows)]
async fn download_and_install_windows(
    provider: &GithubMsiUpdateProvider,
    update: &AvailableUpdate,
) -> Result<(), UpdateError> {
    let staging = provider.staging_dir();
    tokio::fs::create_dir_all(&staging)
        .await
        .map_err(|e| UpdateError::install_failed(format!("create staging dir: {e}")))?;

    let dest = provider.staged_msi_path();
    if dest.exists() {
        let _ = tokio::fs::remove_file(&dest).await;
    }

    info!(
        target: "ncd_tauri::desktop_update",
        version = %update.version,
        url = %update.download_url,
        dest = %dest.display(),
        "downloading desktop MSI"
    );

    if provider.cancel.is_cancelled() {
        return Err(UpdateError::Cancelled);
    }

    let sink: Arc<dyn DownloadProgressSink> = provider
        .progress
        .clone()
        .unwrap_or_else(|| Arc::new(NoopProgressSink));
    let cfg = DownloadConfig {
        mirror_url: Some(update.download_url.clone()),
        ..Default::default()
    };
    download_with_resume(
        &update.download_url,
        &dest,
        sink,
        provider.cancel.clone(),
        cfg,
    )
    .await
    .map_err(|e| match e {
        ncd_network::NetworkError::Cancelled => UpdateError::Cancelled,
        other => UpdateError::install_failed(format!("download msi: {other}")),
    })?;

    if provider.cancel.is_cancelled() {
        return Err(UpdateError::Cancelled);
    }

    let meta = tokio::fs::metadata(&dest)
        .await
        .map_err(|e| UpdateError::install_failed(format!("stat msi: {e}")))?;
    if meta.len() < MINIMUM_MSI_SIZE_BYTES {
        let _ = tokio::fs::remove_file(&dest).await;
        return Err(UpdateError::install_failed(format!(
            "MSI 过小（{} bytes），可能下载不完整",
            meta.len()
        )));
    }

    let expected = update.content_sha256.trim();
    GithubMsiUpdateProvider::verify_sha256_if_needed(
        &dest,
        if expected.is_empty() {
            None
        } else {
            Some(expected)
        },
    )
    .await?;

    let app_root = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    // 日志放 data_root staging，避免 Program Files 无写权限；探测可写后立刻 drop 句柄
    let preferred_log = provider
        .staging_dir()
        .join(format!("msi_install_{}.log", update.version));
    let _ = tokio::fs::create_dir_all(provider.staging_dir()).await;
    let log_path = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&preferred_log)
        .await
    {
        Ok(file) => {
            drop(file);
            preferred_log
        }
        Err(_) => app_root.join("msi_install.log"),
    };

    // Microsoft docs: /passive = 进度条无交互；/norestart = 不重启系统
    // https://learn.microsoft.com/windows/win32/msi/standard-installer-command-line-options
    launch_msiexec_elevated(&dest, &log_path)?;
    info!(
        target: "ncd_tauri::desktop_update",
        msi = %dest.display(),
        log = %log_path.display(),
        "msiexec launched; caller should exit app"
    );
    Ok(())
}

/// 组装 msiexec 参数（纯函数，便于单测）。
///
/// `/passive`：显示进度条，无取消/错误对话框（MS 标准选项，等价旧 `/qb!-`）。
/// `/norestart`：禁止安装后重启系统（应用自启由 helper 负责）。
pub fn build_msiexec_install_args(msi_path: &Path, log_path: &Path) -> String {
    format!(
        "/i \"{}\" /passive /norestart /l*v \"{}\"",
        msi_path.display(),
        log_path.display()
    )
}

/// 解析升级后应启动的主程序路径：优先 InstallDir，再 current_exe。
pub fn resolve_post_update_exe() -> Result<PathBuf, UpdateError> {
    if let Some(dir) = crate::product_registry::read_install_dir() {
        let candidate = dir.join(MAIN_EXE_NAME);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    std::env::current_exe().map_err(|e| UpdateError::install_failed(format!("current_exe: {e}")))
}

/// 生成「等本进程退出 + 目标版本落盘后再启动」的 PowerShell 脚本（纯函数，可测）。
///
/// ShellExecute 异步启动 msiexec，不能只等旧 PID 退出就 Start-Process，
/// 否则会在安装写文件中途拉起旧/半更新 exe。helper 会：
/// 1) 等旧 Desktop PID 退出；
/// 2) 等主程序 ProductVersion/FileVersion 匹配目标版本；
/// 3) Start-Process 失败（文件锁）则重试。
pub fn build_relaunch_helper_script(pid: u32, exe: &Path, expected_version: &str) -> String {
    // 单引号路径 / 版本：PowerShell 里 ' 用 '' 转义
    let exe_ps = exe.to_string_lossy().replace('\'', "''");
    let ver_ps = normalize_version_label(expected_version).replace('\'', "''");
    format!(
        r#"$ErrorActionPreference = 'SilentlyContinue'
$pidToWait = {pid}
$exe = '{exe_ps}'
$expected = '{ver_ps}'
function Get-ExeVersion([string]$path) {{
  try {{
    $info = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($path)
    if ($info.ProductVersion) {{ return [string]$info.ProductVersion }}
    if ($info.FileVersion) {{ return [string]$info.FileVersion }}
  }} catch {{}}
  return ''
}}
function Version-Matches([string]$fileVer, [string]$target) {{
  if (-not $fileVer -or -not $target) {{ return $false }}
  $f = $fileVer.Trim().TrimStart('v','V')
  $t = $target.Trim().TrimStart('v','V')
  return ($f -eq $t) -or ($f.StartsWith($t + '.'))
}}
$deadline = (Get-Date).AddMinutes(8)
while ((Get-Date) -lt $deadline) {{
  $p = Get-Process -Id $pidToWait -ErrorAction SilentlyContinue
  if (-not $p) {{ break }}
  Start-Sleep -Milliseconds 400
}}
$deadline2 = (Get-Date).AddMinutes(12)
while ((Get-Date) -lt $deadline2) {{
  if (-not (Test-Path -LiteralPath $exe)) {{
    Start-Sleep -Milliseconds 500
    continue
  }}
  $ver = Get-ExeVersion $exe
  if (-not (Version-Matches $ver $expected)) {{
    Start-Sleep -Milliseconds 700
    continue
  }}
  try {{
    Start-Process -FilePath $exe -ErrorAction Stop
    exit 0
  }} catch {{
    Start-Sleep -Milliseconds 800
  }}
}}
"#
    )
}

/// 在退出前启动独立 helper：等当前 PID 退出且 exe 版本匹配后再启动。
///
/// 失败只记日志，不阻断已启动的 msiexec（安装仍可完成，用户可手动打开）。
pub fn spawn_post_install_relaunch_helper(expected_version: &str) {
    #[cfg(windows)]
    {
        if let Err(err) = spawn_post_install_relaunch_helper_windows(expected_version) {
            tracing::warn!(
                target: "ncd_tauri::desktop_update",
                error = %err,
                "failed to spawn post-install relaunch helper"
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = expected_version;
    }
}

#[cfg(windows)]
fn spawn_post_install_relaunch_helper_windows(expected_version: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let exe = resolve_post_update_exe().map_err(|e| e.to_string())?;
    let pid = std::process::id();
    let script = build_relaunch_helper_script(pid, &exe, expected_version);

    let staging = std::env::temp_dir().join(format!("ncd-desktop-relaunch-{pid}.ps1"));
    std::fs::write(&staging, script.as_bytes())
        .map_err(|e| format!("write relaunch script: {e}"))?;

    // CREATE_NO_WINDOW：不弹黑框；DETACHED：不随父进程退出
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-WindowStyle",
        "Hidden",
        "-File",
        &staging.to_string_lossy(),
    ])
    .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());

    cmd.spawn()
        .map_err(|e| format!("spawn powershell relaunch helper: {e}"))?;
    info!(
        target: "ncd_tauri::desktop_update",
        pid,
        expected = %normalize_version_label(expected_version),
        exe = %exe.display(),
        script = %staging.display(),
        "post-install relaunch helper started"
    );
    Ok(())
}

/// 启动时消费 resume / 失败记录，生成一次性用户提示。
///
/// 只读 `update-resume.json` / `update-failures.jsonl`，不构造 Mock provider、
/// 不触网，避免把测试桩带进生产启动路径。
pub async fn consume_startup_update_notice(
    data_root: &Path,
    current_version: &str,
) -> Option<DesktopUpdateStartupNotice> {
    let store = ncd_update::ResumeStore::new(data_root);
    if let Ok(Some(point)) = store.load().await {
        // 成功路径清 resume；未完成也清，避免每次启动刷 incomplete
        //（用户可到关于页重试；失败细节仍可能在 failures 里）
        let _ = store.clear().await;
        let cur = normalize_version_label(current_version);
        let target = normalize_version_label(&point.to_version);
        if !target.is_empty() && cur == target {
            return Some(DesktopUpdateStartupNotice {
                v: 1,
                kind: DesktopUpdateNoticeKind::Success,
                from_version: Some(point.from_version),
                to_version: Some(point.to_version.clone()),
                message: format!("已更新到 v{target}"),
            });
        }
        return Some(DesktopUpdateStartupNotice {
            v: 1,
            kind: DesktopUpdateNoticeKind::Incomplete,
            from_version: Some(point.from_version),
            to_version: Some(point.to_version.clone()),
            message: format!(
                "上次更新目标为 v{target}，当前仍为 v{cur}。若安装未完成，请到「设置 · 关于」重试，或查看安装日志。"
            ),
        });
    }

    let failures_path = data_root.join("update-failures.jsonl");
    if let Ok(content) = tokio::fs::read_to_string(&failures_path).await {
        let mut last_install: Option<ncd_update::RecordedFailure> = None;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(f) = serde_json::from_str::<ncd_update::RecordedFailure>(line) {
                if matches!(f.phase, ncd_update::UpdatePhase::Install) {
                    last_install = Some(f);
                }
            }
        }
        if let Some(last) = last_install {
            let _ = clear_update_failures_file(data_root).await;
            return Some(DesktopUpdateStartupNotice {
                v: 1,
                kind: DesktopUpdateNoticeKind::Failure,
                from_version: None,
                to_version: last.target_version.clone(),
                message: format!("上次 Desktop 更新失败：{}", last.error),
            });
        }
    }
    None
}

async fn clear_update_failures_file(data_root: &Path) -> Result<(), std::io::Error> {
    let path = data_root.join("update-failures.jsonl");
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// ShellExecuteW runas + msiexec /i /passive /norestart /l*v
#[cfg(windows)]
#[allow(unsafe_code)] // Windows FFI: ShellExecuteW 启动 msiexec 必须用 unsafe
fn launch_msiexec_elevated(msi_path: &Path, log_path: &Path) -> Result<(), UpdateError> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let msiexec = PathBuf::from(system_root)
        .join("System32")
        .join("msiexec.exe");
    if !msiexec.is_file() {
        return Err(UpdateError::install_failed(format!(
            "msiexec not found: {}",
            msiexec.display()
        )));
    }

    let arguments = build_msiexec_install_args(msi_path, log_path);

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    let file = wide(msiexec.as_os_str());
    let params = wide(OsStr::new(&arguments));
    let cwd = msi_path
        .parent()
        .map(|p| wide(p.as_os_str()))
        .unwrap_or_else(|| wide(OsStr::new(".")));

    // 已是管理员则 open，否则 runas 触发 UAC（与 legacy manager 一致）
    let verb = if is_running_as_admin() {
        wide(OsStr::new("open"))
    } else {
        wide(OsStr::new("runas"))
    };

    // SAFETY: file/params/cwd/verb 均为 NUL 结尾的宽字符串；句柄 None 表示无父窗口。
    // ShellExecuteW 返回值 > 32 表示已成功把启动请求交给 shell。
    let result = unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(file.as_ptr()),
            windows::core::PCWSTR(params.as_ptr()),
            windows::core::PCWSTR(cwd.as_ptr()),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };

    let code = result.0 as isize;
    if code <= 32 {
        return Err(UpdateError::install_failed(format!(
            "ShellExecuteW msiexec failed with code {code}"
        )));
    }
    Ok(())
}

#[allow(unsafe_code)] // Windows FFI: IsUserAnAdmin 判断是否已管理员
#[cfg(windows)]
fn is_running_as_admin() -> bool {
    // 与 legacy ctypes IsUserAnAdmin 同语义；失败当非管理员走 UAC
    // SAFETY: shell32 IsUserAnAdmin 无参数、无输出缓冲，仅读进程令牌。
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn IsUserAnAdmin() -> i32;
    }
    // SAFETY: 见上；返回非 0 即管理员。
    unsafe { IsUserAnAdmin() != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::release_snapshot::{ReleaseAsset, ReleaseInfo};

    fn sample_info(version: &str, assets: Vec<(&str, &str)>) -> ReleaseInfo {
        ReleaseInfo {
            version: version.to_string(),
            tag: format!("v{version}"),
            published_at: 1_700_000_000,
            html_url: format!(
                "https://github.com/NapNeko/NapCatQQ-Desktop/releases/tag/v{version}"
            ),
            release_notes: "notes".into(),
            assets: assets
                .into_iter()
                .map(|(name, sha)| ReleaseAsset {
                    name: name.to_string(),
                    sha256: sha.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn pick_prefers_versioned_msi_name() {
        let info = sample_info(
            "3.0.1",
            vec![
                ("NapCatQQ-Desktop-3.0.1-x64.msi", "abc"),
                ("NapCatQQ-Desktop-x64.msi", "def"),
            ],
        );
        let (url, name, sha) = pick_desktop_msi(&info).unwrap();
        assert_eq!(name, "NapCatQQ-Desktop-3.0.1-x64.msi");
        assert!(url.ends_with("/v3.0.1/NapCatQQ-Desktop-3.0.1-x64.msi"));
        assert_eq!(sha.as_deref(), Some("abc"));
    }

    #[test]
    fn pick_falls_back_to_alias() {
        let info = sample_info("3.0.1", vec![("NapCatQQ-Desktop-x64.msi", "deadbeef")]);
        let (url, name, sha) = pick_desktop_msi(&info).unwrap();
        assert_eq!(name, MSI_ALIAS_NAME);
        assert!(url.contains("NapCatQQ-Desktop-x64.msi"));
        assert_eq!(sha.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn release_to_available_update_parses_semver() {
        let info = sample_info("3.1.0", vec![("NapCatQQ-Desktop-3.1.0-x64.msi", "")]);
        let u = release_info_to_available_update(&info).unwrap();
        assert_eq!(u.version, Version::new(3, 1, 0));
        assert!(u.download_url.contains("3.1.0"));
        assert!(is_trusted_desktop_msi_url(&u.download_url));
        assert!(u.signature.is_empty());
        assert!(u.content_sha256.is_empty());
    }

    #[test]
    fn release_puts_digest_in_content_sha256_not_signature() {
        let info = sample_info(
            "3.1.0",
            vec![(
                "NapCatQQ-Desktop-3.1.0-x64.msi",
                "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            )],
        );
        let u = release_info_to_available_update(&info).unwrap();
        assert!(u.signature.is_empty());
        assert_eq!(
            u.content_sha256,
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        );
    }

    #[test]
    fn pick_uses_versioned_name_when_assets_empty() {
        let info = sample_info("3.0.2", vec![]);
        let (url, name, sha) = pick_desktop_msi(&info).unwrap();
        assert_eq!(name, "NapCatQQ-Desktop-3.0.2-x64.msi");
        assert!(url.ends_with("/v3.0.2/NapCatQQ-Desktop-3.0.2-x64.msi"));
        assert!(sha.is_none());
    }

    #[test]
    fn trusted_url_accepts_github_release_msi() {
        assert!(is_trusted_desktop_msi_url(
            "https://github.com/NapNeko/NapCatQQ-Desktop/releases/download/v3.0.1/NapCatQQ-Desktop-3.0.1-x64.msi"
        ));
        assert!(is_trusted_desktop_msi_url(
            "https://github.com/NapNeko/NapCatQQ-Desktop/releases/download/v3.0.1/NapCatQQ-Desktop-x64.msi"
        ));
    }

    #[test]
    fn trusted_url_rejects_foreign_host_or_path() {
        assert!(!is_trusted_desktop_msi_url(
            "https://evil.example/NapNeko/NapCatQQ-Desktop/releases/download/v1/x.msi"
        ));
        assert!(!is_trusted_desktop_msi_url(
            "http://github.com/NapNeko/NapCatQQ-Desktop/releases/download/v1/NapCatQQ-Desktop-x64.msi"
        ));
        assert!(!is_trusted_desktop_msi_url(
            "https://github.com/other/NapCatQQ-Desktop/releases/download/v1/NapCatQQ-Desktop-x64.msi"
        ));
        assert!(!is_trusted_desktop_msi_url(
            "https://github.com/NapNeko/NapCatQQ-Desktop/releases/download/v1/readme.txt"
        ));
    }

    #[test]
    fn release_info_to_available_update_errs_on_bad_semver() {
        let info = sample_info("not-a-version", vec![]);
        assert!(release_info_to_available_update(&info).is_err());
    }

    #[test]
    fn msiexec_args_use_passive_and_norestart() {
        let args = build_msiexec_install_args(
            Path::new(r"C:\tmp\NapCatQQ-Desktop-x64.msi"),
            Path::new(r"C:\tmp\msi.log"),
        );
        assert!(
            args.contains("/passive"),
            "expected /passive for progress UI"
        );
        assert!(args.contains("/norestart"));
        assert!(!args.contains("/quiet"), "quiet hides all UI; issue #126");
        assert!(args.contains(r"C:\tmp\NapCatQQ-Desktop-x64.msi"));
        assert!(args.contains(r"C:\tmp\msi.log"));
    }

    #[test]
    fn file_version_matches_target_accepts_msi_four_part() {
        assert!(file_version_matches_target("3.1.6.0", "3.1.6"));
        assert!(file_version_matches_target("v3.1.6", "3.1.6"));
        assert!(!file_version_matches_target("3.1.5.0", "3.1.6"));
        assert!(!file_version_matches_target("", "3.1.6"));
    }

    #[test]
    fn relaunch_helper_script_waits_pid_version_and_starts_exe() {
        let script = build_relaunch_helper_script(
            4242,
            Path::new(r"C:\Program Files\NapCatQQ Desktop\NapCatQQ-Desktop.exe"),
            "3.1.6",
        );
        assert!(script.contains("$pidToWait = 4242"));
        assert!(script.contains("$expected = '3.1.6'"));
        assert!(script.contains("Get-Process -Id $pidToWait"));
        assert!(script.contains("GetVersionInfo"));
        assert!(script.contains("Version-Matches"));
        assert!(script.contains("Start-Process -FilePath $exe -ErrorAction Stop"));
        assert!(script.contains("NapCatQQ-Desktop.exe"));
        // PowerShell 单引号转义
        let script_q = build_relaunch_helper_script(1, Path::new(r"C:\a'b\app.exe"), "1.0.0");
        assert!(script_q.contains(r"C:\a''b\app.exe"));
    }

    #[tokio::test]
    async fn consume_startup_notice_success_when_version_matches() {
        let dir = tempfile::tempdir().unwrap();
        let store = ncd_update::ResumeStore::new(dir.path());
        store
            .save(&ncd_update::UpdateResumePoint::new("3.1.4", "3.1.5"))
            .await
            .unwrap();
        let notice = consume_startup_update_notice(dir.path(), "3.1.5")
            .await
            .expect("notice");
        assert_eq!(notice.kind, DesktopUpdateNoticeKind::Success);
        assert!(notice.message.contains("3.1.5"));
        // resume 应被消费
        assert!(store.load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn consume_startup_notice_incomplete_when_version_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let store = ncd_update::ResumeStore::new(dir.path());
        store
            .save(&ncd_update::UpdateResumePoint::new("3.1.4", "3.1.5"))
            .await
            .unwrap();
        let notice = consume_startup_update_notice(dir.path(), "3.1.4")
            .await
            .expect("notice");
        assert_eq!(notice.kind, DesktopUpdateNoticeKind::Incomplete);
        assert!(store.load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn consume_startup_notice_failure_from_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-failures.jsonl");
        let line = serde_json::json!({
            "timestamp": "2026-07-22T00:00:00Z",
            "target_version": "3.1.5",
            "phase": "install",
            "error": "download msi: timeout"
        });
        tokio::fs::write(&path, format!("{line}\n")).await.unwrap();
        let notice = consume_startup_update_notice(dir.path(), "3.1.4")
            .await
            .expect("notice");
        assert_eq!(notice.kind, DesktopUpdateNoticeKind::Failure);
        assert!(notice.message.contains("timeout"));
        assert!(!path.exists());
    }
}
