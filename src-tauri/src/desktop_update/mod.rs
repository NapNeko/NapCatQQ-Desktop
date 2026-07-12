//! Desktop 自更新：GitHub Release MSI 路径（对齐 legacy + CI 产物命名）。
//!
//! 不走 tauri-plugin-updater 签名包；下载 MSI 后用 msiexec 静默升级，
//! 资产名与 build-msi.yml 一致：`NapCatQQ-Desktop-{ver}-x64.msi` /
//! 别名 `NapCatQQ-Desktop-x64.msi`。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use ncd_domain::release_snapshot::ReleaseInfo;
use ncd_domain::SchemaVersion;
use ncd_network::{download_with_resume, DownloadConfig, NoopProgressSink};
use ncd_runtime::release::fetch_release_snapshot;
use ncd_update::{AvailableUpdate, UpdateChannel, UpdateError, UpdateProvider};
use semver::Version;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// 与 CI collect 步骤 / legacy UpdateManager 一致的 MSI 命名。
pub const MSI_VERSIONED_NAME_FMT: &str = "NapCatQQ-Desktop-{version}-x64.msi";
pub const MSI_ALIAS_NAME: &str = "NapCatQQ-Desktop-x64.msi";
const MINIMUM_MSI_SIZE_BYTES: u64 = 1024 * 1024;

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

/// 从 ReleaseInfo 选出 MSI 下载 URL + 可选 sha256。
pub fn pick_desktop_msi(info: &ReleaseInfo) -> Result<(String, String, Option<String>), UpdateError> {
    let version_plain = info.version.trim().trim_start_matches(['v', 'V']);
    let versioned = MSI_VERSIONED_NAME_FMT.replace("{version}", version_plain);
    let tag = if info.tag.trim().is_empty() {
        format!("v{version_plain}")
    } else {
        info.tag.clone()
    };

    let asset_name = if info.assets.iter().any(|a| a.name == versioned) {
        versioned
    } else if info.assets.iter().any(|a| a.name == MSI_ALIAS_NAME) {
        MSI_ALIAS_NAME.to_string()
    } else if info.assets.iter().any(|a| {
        a.name.ends_with(".msi")
            && a.name.contains("NapCatQQ-Desktop")
            && a.name.contains("x64")
    }) {
        info.assets
            .iter()
            .find(|a| {
                a.name.ends_with(".msi")
                    && a.name.contains("NapCatQQ-Desktop")
                    && a.name.contains("x64")
            })
            .map(|a| a.name.clone())
            .unwrap_or(versioned)
    } else {
        // API 未列 assets 时仍拼规范 URL（CI 会上传 versioned + alias）
        versioned
    };

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
        "https://github.com/NapNeko/NapCatQQ-Desktop/releases/download/{tag}/{asset_name}"
    );
    Ok((url, asset_name, sha))
}

pub fn release_info_to_available_update(
    info: &ReleaseInfo,
) -> Result<AvailableUpdate, UpdateError> {
    let version = Version::parse(info.version.trim().trim_start_matches(['v', 'V'])).map_err(
        |e| UpdateError::check_failed(format!("desktop release version parse: {e}")),
    )?;
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
        // 1ase notes/manifest 读取
        schema_version: SchemaVersion::CURRENT,
        notes: info.release_notes.clone(),
        pub_date,
        download_url,
        // MSI 路径：字段复用为期望 SHA256 hex（无 Ed25519 签名时为空）
        signature: sha.unwrap_or_default(),
    })
}

pub struct GithubMsiUpdateProvider {
    data_root: PathBuf,
    /// 检查时注入 PAT 的闭包结果缓存：命令层构造时读一次即可
    github_token: Option<String>,
}

impl GithubMsiUpdateProvider {
    pub fn new(data_root: impl Into<PathBuf>, github_token: Option<String>) -> Self {
        Self {
            data_root: data_root.into(),
            github_token,
        }
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
            return Err(UpdateError::SignatureFailed {
                reason: format!("msi sha256 mismatch: expected {expected}, actual {actual}"),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl UpdateProvider for GithubMsiUpdateProvider {
    async fn check(&self, channel: UpdateChannel) -> Result<Option<AvailableUpdate>, UpdateError> {
        // 当前仅 Stable；Beta/Nightly 与 Stable 同一 GitHub list 过滤（后续可分 pre-release）
        let _ = channel;
        let snap =
            fetch_release_snapshot(&self.data_root, self.github_token.as_deref()).await;
        let Some(info) = snap.desktop_latest.as_ref() else {
            return Err(UpdateError::check_failed(
                "无法获取 Desktop 最新版本（网络或 GitHub 不可达）",
            ));
        };
        let update = release_info_to_available_update(info)?;
        Ok(Some(update))
    }

    async fn download_and_install(&self, update: &AvailableUpdate) -> Result<(), UpdateError> {
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

    let sink = Arc::new(NoopProgressSink);
    let cfg = DownloadConfig {
        mirror_url: Some(update.download_url.clone()),
        ..Default::default()
    };
    download_with_resume(
        &update.download_url,
        &dest,
        sink,
        CancellationToken::new(),
        cfg,
    )
    .await
    .map_err(|e| UpdateError::install_failed(format!("download msi: {e}")))?;

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

    let expected = update.signature.trim();
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
    let log_path = app_root.join("msi_install.log");

    launch_msiexec_elevated(&dest, &log_path)?;
    info!(
        target: "ncd_tauri::desktop_update",
        msi = %dest.display(),
        log = %log_path.display(),
        "msiexec launched; caller should exit app"
    );
    Ok(())
}

/// 对齐 legacy：ShellExecuteW runas + msiexec /i /quiet /norestart /l*v
#[cfg(windows)]
#[allow(unsafe_code)] // Windows FFI: ShellExecuteW 启动 msiexec 必须用 unsafe
fn launch_msiexec_elevated(msi_path: &Path, log_path: &Path) -> Result<(), UpdateError> {
    use std::os::windows::ffi::OsStrExt;
    use std::ffi::OsStr;

    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let msiexec = PathBuf::from(system_root).join("System32").join("msiexec.exe");
    if !msiexec.is_file() {
        return Err(UpdateError::install_failed(format!(
            "msiexec not found: {}",
            msiexec.display()
        )));
    }

    let arguments = format!(
        "/i \"{}\" /quiet /norestart /l*v \"{}\"",
        msi_path.display(),
        log_path.display()
    );

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
    }
}
