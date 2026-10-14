//! 仅扫描 Desktop 自有缓存。下载/上传/解压持有共享租约，清理与迁移取得独占租约。
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ncd_config::DataPaths;
use ncd_domain::cache_cleanup::{CacheCategory, CacheCategoryUsage, CacheCleanupResult, CacheScan};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

use super::cache_fs::{plain_ancestors, redirected, remove_plain_file};

pub(super) struct ProcessLease {
    #[cfg(windows)]
    _file: std::fs::File,
    #[cfg(windows)]
    _directories: super::cache_fs::PlainDirectories,
}

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const LEGACY_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const BUSY: &str = "安装、更新、数据迁移或缓存清理正在进行，请稍后重试";
const MAX_WARNINGS: usize = 20;

#[derive(Clone)]
pub struct DownloadCacheService {
    paths: DataPaths,
    system_temp: PathBuf,
    gate: Arc<RwLock<()>>,
}

pub struct DownloadCacheLease {
    _process: Option<ProcessLease>,
    _gate: OwnedRwLockReadGuard<()>,
}

pub struct CacheMaintenanceLease {
    _process: Option<ProcessLease>,
    _gate: OwnedRwLockWriteGuard<()>,
}

impl DownloadCacheService {
    pub fn new(data_root: &Path) -> Self {
        Self {
            paths: DataPaths::new(data_root),
            system_temp: std::env::temp_dir(),
            gate: Arc::new(RwLock::new(())),
        }
    }

    pub async fn lease(&self) -> Result<DownloadCacheLease, String> {
        let lease = self.gate.clone().read_owned().await;
        self.ensure_current_root()?;
        Ok(DownloadCacheLease {
            _gate: lease,
            _process: self.process_lease(false)?,
        })
    }

    pub fn maintenance(&self) -> Result<CacheMaintenanceLease, String> {
        let lease = self
            .gate
            .clone()
            .try_write_owned()
            .map_err(|_| BUSY.to_string())?;
        self.ensure_current_root()?;
        Ok(CacheMaintenanceLease {
            _gate: lease,
            _process: self.process_lease(true)?,
        })
    }

    #[cfg(not(windows))]
    fn process_lease(&self, _exclusive: bool) -> Result<Option<ProcessLease>, String> {
        Ok(None)
    }

    #[cfg(windows)]
    fn process_lease(&self, exclusive: bool) -> Result<Option<ProcessLease>, String> {
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        };
        // ProgramData 可由不同 Windows 会话共享；不能只依赖单进程的 RwLock。
        // 放 tmp 下：整树迁移会跳过 tmp，不会复制正在独占使用的租约文件。
        let directory = self.paths.tmp_dir();
        let directories = super::cache_fs::PlainDirectories::create(&directory)
            .map_err(|e| format!("准备缓存租约目录失败：{e}"))?;
        let path = directory.join(".ncd-download-cache.lock");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if redirected(&metadata) || !metadata.is_file() => {
                return Err("缓存租约文件是链接或非普通文件，请检查数据目录".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(file) => drop(file),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(format!("创建缓存租约失败：{error}")),
                }
            }
            Err(error) => return Err(format!("检查缓存租约失败：{error}")),
        }
        let file = OpenOptions::new()
            .read(true)
            // 独占由共享模式控制，不需要修改租约内容或申请写权限。
            .share_mode(if exclusive { 0 } else { FILE_SHARE_READ })
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|error| {
                if matches!(error.raw_os_error(), Some(32 | 33)) {
                    "同一数据目录中的其他 Desktop 实例正在安装或维护，请稍后重试".to_string()
                } else {
                    format!("打开缓存租约失败：{error}")
                }
            })?;
        let metadata = file
            .metadata()
            .map_err(|e| format!("读取缓存租约失败：{e}"))?;
        if redirected(&metadata) || !metadata.is_file() {
            return Err("缓存租约文件是链接或非普通文件，请检查数据目录".into());
        }
        directories
            .validate_child(&file)
            .map_err(|e| format!("缓存租约路径已变化：{e}"))?;
        Ok(Some(ProcessLease {
            _file: file,
            _directories: directories,
        }))
    }

    fn ensure_current_root(&self) -> Result<(), String> {
        if self
            .paths
            .root()
            .join(ncd_domain::DataRootRetiredMarker::FILE_NAME)
            .exists()
        {
            Err("数据目录已迁移，请重启 Desktop 后再操作".into())
        } else {
            Ok(())
        }
    }

    pub async fn scan(&self) -> Result<CacheScan, String> {
        self.ensure_current_root()?;
        let busy =
            self.gate.clone().try_write_owned().is_err() || self.process_lease(true).is_err();
        let lease = self
            .gate
            .clone()
            .try_read_owned()
            .map_err(|_| BUSY.to_string())?;
        self.ensure_current_root()?;
        let lease = DownloadCacheLease {
            _gate: lease,
            _process: self.process_lease(false)?,
        };
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            let alive = live_pids();
            CacheScan {
                categories: CacheCategory::ALL
                    .into_iter()
                    .map(|category| service.collect(category, &alive).usage)
                    .collect(),
                busy,
            }
        })
        .await
        .map_err(|e| format!("扫描缓存失败：{e}"))
    }

    pub async fn clean(
        &self,
        categories: Vec<CacheCategory>,
    ) -> Result<CacheCleanupResult, String> {
        if categories.is_empty() {
            return Err("请选择要清理的缓存类别".into());
        }
        let lease = self.maintenance()?;
        let service = self.clone();
        // 租约随阻塞任务存活；IPC 请求取消也不会提前放开正在删除文件的锁。
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            let alive = live_pids();
            let mut result = CacheCleanupResult::default();
            for category in CacheCategory::ALL
                .into_iter()
                .filter(|c| categories.contains(c))
            {
                let collected = service.collect(category, &alive);
                result.skipped_files = result.skipped_files.saturating_add(collected.usage.skipped);
                for warning in collected.usage.warnings {
                    warn(&mut result.warnings, warning);
                }
                for candidate in collected.files {
                    match remove_plain_file(&candidate.path, candidate.bytes, candidate.modified) {
                        Ok(bytes) => {
                            result.deleted_files = result.deleted_files.saturating_add(1);
                            result.deleted_bytes = result.deleted_bytes.saturating_add(bytes);
                        }
                        Err(error)
                            if error.kind() == std::io::ErrorKind::NotFound
                                || error.kind() == std::io::ErrorKind::WouldBlock
                                || matches!(error.raw_os_error(), Some(32 | 33)) =>
                        {
                            result.skipped_files = result.skipped_files.saturating_add(1);
                            warn(
                                &mut result.warnings,
                                format!("{}：文件被占用、已变化或已移除", candidate.label),
                            );
                        }
                        Err(error) => {
                            result.failed_files = result.failed_files.saturating_add(1);
                            warn(
                                &mut result.warnings,
                                if error.kind() == std::io::ErrorKind::PermissionDenied {
                                    format!("{}：权限不足或文件为只读，已保留", candidate.label)
                                } else {
                                    format!("{}：{error}", candidate.label)
                                },
                            );
                        }
                    }
                }
            }
            result
        })
        .await
        .map_err(|e| format!("清理缓存失败：{e}"))
    }

    fn roots(&self, category: CacheCategory) -> Vec<CacheRoot> {
        let root = self.paths.root();
        match category {
            CacheCategory::Installers => vec![
                CacheRoot::new(
                    self.paths.installer_cache_dir().join("qq"),
                    "QQ 安装包",
                    Rule::Qq,
                ),
                CacheRoot::new(
                    self.paths.installer_cache_dir().join("vcredist"),
                    "VC++ 安装包",
                    Rule::VcRedist,
                ),
                CacheRoot::new(root.join("runtime/cache/qq"), "历史 QQ 安装包", Rule::Qq),
                CacheRoot::new(
                    root.join("runtime/cache/vcredist"),
                    "历史 VC++ 安装包",
                    Rule::VcRedist,
                ),
            ],
            CacheCategory::DesktopUpdates => vec![
                CacheRoot::new(
                    self.paths.update_cache_dir(),
                    "Desktop 更新包",
                    Rule::Update,
                ),
                CacheRoot::new(
                    root.join("runtime/tmp"),
                    "历史 Desktop 更新包",
                    Rule::Update,
                ),
            ],
            CacheCategory::DownloadResidues => vec![
                CacheRoot::new(
                    self.paths.download_cache_dir(),
                    "组件下载暂存",
                    Rule::Managed,
                ),
                CacheRoot::new(self.system_temp.clone(), "历史临时下载", Rule::Legacy),
            ],
        }
    }

    fn collect(&self, category: CacheCategory, alive: &HashSet<u32>) -> Collected {
        let mut collected = Collected {
            usage: CacheCategoryUsage {
                category,
                files: 0,
                bytes: 0,
                skipped: 0,
                warnings: vec![],
            },
            files: vec![],
        };
        let now = SystemTime::now();
        for root in self.roots(category) {
            match fs::symlink_metadata(&root.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                _ => {}
            }
            if let Err(error) = plain_ancestors(&root.path) {
                collected.skip(format!("{}：{error}", root.label));
                continue;
            }
            let mut pending = vec![(root.path.clone(), 0)];
            while let Some((directory, depth)) = pending.pop() {
                // 每一层都校验祖先，避免扫描进入目录联接。
                if let Err(error) = plain_ancestors(&directory) {
                    collected.skip(format!("{}：{error}", root.label));
                    continue;
                }
                let entries = match fs::read_dir(&directory) {
                    Ok(entries) => entries,
                    Err(error) => {
                        collected.skip(format!("{}：{error}", root.label));
                        continue;
                    }
                };
                for entry in entries {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(error) => {
                            collected.skip(format!("{}：{error}", root.label));
                            continue;
                        }
                    };
                    let path = entry.path();
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if !root.rule.matches(&name) {
                        continue;
                    }
                    let metadata = match fs::symlink_metadata(&path) {
                        Ok(metadata) => metadata,
                        Err(error) => {
                            collected.skip(format!("{} / {name}：{error}", root.label));
                            continue;
                        }
                    };
                    if redirected(&metadata) {
                        collected.skip(format!("{} / {name}：已跳过链接或重解析点", root.label));
                        continue;
                    }
                    if metadata.is_dir() && matches!(root.rule, Rule::Managed) {
                        if depth < 32 {
                            pending.push((path, depth + 1));
                        } else {
                            collected.skip(format!("{}：目录层级过深", root.label));
                        }
                        continue;
                    }
                    if !metadata.is_file() {
                        continue;
                    }
                    let modified = metadata.modified().ok();
                    let age = modified.and_then(|time| now.duration_since(time).ok());
                    let protected = match root.rule {
                        Rule::Update => age.is_none_or(|age| age < DAY),
                        Rule::Legacy => {
                            age.is_none_or(|age| age < LEGACY_AGE)
                                || legacy_pid(&name).is_none_or(|pid| alive.contains(&pid))
                        }
                        _ => false,
                    };
                    if protected {
                        collected.usage.skipped = collected.usage.skipped.saturating_add(1);
                        continue;
                    }
                    collected.usage.files = collected.usage.files.saturating_add(1);
                    collected.usage.bytes = collected.usage.bytes.saturating_add(metadata.len());
                    collected.files.push(Candidate {
                        path,
                        bytes: metadata.len(),
                        modified,
                        label: format!("{} / {name}", root.label),
                    });
                }
            }
        }
        collected
    }
}

struct Candidate {
    path: PathBuf,
    bytes: u64,
    modified: Option<SystemTime>,
    label: String,
}
struct Collected {
    usage: CacheCategoryUsage,
    files: Vec<Candidate>,
}
impl Collected {
    fn skip(&mut self, message: String) {
        self.usage.skipped = self.usage.skipped.saturating_add(1);
        warn(&mut self.usage.warnings, message);
    }
}
fn warn(warnings: &mut Vec<String>, message: String) {
    if warnings.len() < MAX_WARNINGS {
        warnings.push(message);
    }
}

struct CacheRoot {
    path: PathBuf,
    label: &'static str,
    rule: Rule,
}
impl CacheRoot {
    fn new(path: PathBuf, label: &'static str, rule: Rule) -> Self {
        Self { path, label, rule }
    }
}
#[derive(Clone, Copy)]
enum Rule {
    Qq,
    VcRedist,
    Update,
    Managed,
    Legacy,
}
impl Rule {
    fn matches(self, name: &str) -> bool {
        let base = download_base(name);
        match self {
            Self::Qq => base
                .strip_prefix("QQNT-")
                .and_then(|s| s.strip_suffix(".exe"))
                .is_some_and(valid_version),
            Self::VcRedist => base == "VC_redist.x64.exe",
            Self::Update => base == "NapCatQQ-Desktop-x64.msi",
            Self::Managed => true,
            Self::Legacy => legacy_pid(name).is_some(),
        }
    }
}

fn download_base(name: &str) -> &str {
    if let Some(base) = name.strip_suffix(".part") {
        return base;
    }
    if let Some((base, index)) = name.rsplit_once(".chunk-") {
        if !index.is_empty() && index.bytes().all(|c| c.is_ascii_digit()) {
            return base;
        }
    }
    name
}
fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}
fn pid(value: &str) -> Option<u32> {
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let pid = value.parse::<u32>().ok()?;
    (pid > 0).then_some(pid)
}

/// 严格限定已知下载命名，不能用通用 ncd-* 清空配置导入、重启脚本等临时产物。
fn legacy_pid(name: &str) -> Option<u32> {
    let base = download_base(name);
    let stem = [".tar.gz", ".tar.xz", ".zip", ".deb", ".rpm"]
        .into_iter()
        .find_map(|ext| base.strip_suffix(ext));
    if let Some(stem) = stem {
        for prefix in ["ncd-nodejs-", "ncd-uv-", "ncd-mingit-", "ncd-qq-"] {
            if let Some(rest) = stem.strip_prefix(prefix) {
                let (version, id) = rest.rsplit_once('-')?;
                return valid_version(version).then(|| pid(id)).flatten();
            }
        }
        for prefix in [
            "ncd-napcat-win-",
            "ncd-napcat-",
            "ncd-snowluma-win-",
            "ncd-snowluma-",
            "ncd-koishi-",
        ] {
            if let Some(rest) = stem.strip_prefix(prefix) {
                let (id, stamp) = rest.split_once('-')?;
                return stamp.parse::<u64>().ok().and_then(|_| pid(id));
            }
        }
        if let Some(rest) = stem.strip_prefix("ncd-maibot-") {
            let (slot_and_pid, stamp) = rest.rsplit_once('-')?;
            let (slot, id) = slot_and_pid.rsplit_once('-')?;
            return (valid_version(slot) && stamp.parse::<u64>().is_ok())
                .then(|| pid(id))
                .flatten();
        }
        if let Some(rest) = stem.strip_prefix("ncd-python-") {
            let (id, asset) = rest.split_once('-')?;
            return asset.starts_with("cpython-").then(|| pid(id)).flatten();
        }
        if let Some(rest) = stem.strip_prefix("ncd-") {
            let (id, asset) = rest.split_once('-')?;
            if asset.starts_with("Redis-") || asset.starts_with("valkey-") {
                return pid(id);
            }
        }
    }
    if let Some(rest) = base.strip_prefix("ncd-watch-") {
        let (id, version) = rest.split_once('-')?;
        let version = version.strip_prefix('v').unwrap_or(version);
        return (!version.is_empty()
            && version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit())))
        .then(|| pid(id))
        .flatten();
    }
    None
}

fn live_pids() -> HashSet<u32> {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, ProcessRefreshKind::new());
    let mut alive: HashSet<_> = system.processes().keys().map(|pid| pid.as_u32()).collect();
    alive.insert(std::process::id());
    alive
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
