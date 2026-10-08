// 截图附件按应用运行留存，草稿与重试引用的文件不在运行中淘汰。
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use ncd_domain::chat_screenshot::ChatScreenshotAttachment;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use uuid::Uuid;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;
const MAX_PREVIEW_BYTES: usize = 256 * 1024;
const MAX_RUN_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const STALE_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const SESSION_MARKER: &str = ".session";

pub struct ChatScreenshotCache {
    data_root: PathBuf,
    cache_root: PathBuf,
    run_root: PathBuf,
    usage: Mutex<CacheUsage>,
}

#[derive(Default)]
struct CacheUsage {
    initialized: bool,
    bytes: u64,
    saved: u32,
    _lease: Option<File>,
}

impl ChatScreenshotCache {
    pub fn new(data_root: &Path) -> Self {
        let cache_root = data_root.join("state").join("chat").join("screenshots");
        let run_root = cache_root.join(Uuid::new_v4().to_string());
        Self {
            data_root: data_root.to_path_buf(),
            cache_root,
            run_root,
            usage: Mutex::new(CacheUsage::default()),
        }
    }

    pub fn save(
        &self,
        width: u32,
        height: u32,
        png: &[u8],
        preview: &[u8],
    ) -> Result<ChatScreenshotAttachment, String> {
        validate_png(width, height, png, preview)?;
        let bytes = (png.len() as u64)
            .checked_add(preview.len() as u64)
            .ok_or("截图附件大小无效")?;
        let mut usage = self.usage.lock().map_err(|_| "截图缓存状态不可用")?;
        usage
            .bytes
            .checked_add(bytes)
            .filter(|bytes| *bytes <= MAX_RUN_BYTES)
            .ok_or("本次截图缓存已达 256 MiB，可使用复制或另存为")?;
        let _budget = self.prepare(&mut usage)?;
        let (total, run_bytes) = cache_bytes(&self.cache_root, &self.run_root)?;
        if total
            .checked_add(bytes)
            .is_none_or(|bytes| bytes > MAX_CACHE_BYTES)
        {
            return Err("截图缓存已达 512 MiB，请关闭其他桌面端并清理过期截图后重试".into());
        }
        // 回滚若被外部文件锁暂时阻止，下一次保存也把遗留文件计入运行预算。
        let next_run_bytes = usage
            .bytes
            .max(run_bytes)
            .checked_add(bytes)
            .filter(|bytes| *bytes <= MAX_RUN_BYTES)
            .ok_or("本次截图缓存已达 256 MiB，可使用复制或另存为")?;
        ensure_plain_directory(&self.run_root)?;
        let id = Uuid::new_v4();
        let number = usage.saved.checked_add(1).ok_or("截图数量已达上限")?;
        let name = format!("截图-{number}.png");
        let path = self.run_root.join(format!("{id}.png"));
        let preview_path = self.run_root.join(format!("{id}.preview.png"));
        let attachment = ChatScreenshotAttachment {
            path: path.to_str().ok_or("截图路径包含不支持的字符")?.to_string(),
            preview_path: preview_path
                .to_str()
                .ok_or("截图预览路径包含不支持的字符")?
                .to_string(),
            name,
            width,
            height,
            clipboard_error: None,
        };
        let mut pair = CreatedPair::default();
        pair.write(&path, png)?;
        pair.write(&preview_path, preview)?;
        usage.bytes = next_run_bytes;
        usage.saved = number;
        pair.committed = true;
        Ok(attachment)
    }

    fn prepare(&self, usage: &mut CacheUsage) -> Result<BudgetLease, String> {
        ensure_plain_directory(&self.data_root)?;
        let mut directory = self.data_root.clone();
        for part in ["state", "chat", "screenshots"] {
            directory.push(part);
            create_plain_directory(&directory)?;
        }
        let budget = BudgetLease::open(&self.cache_root)?;
        if usage.initialized {
            return Ok(budget);
        }
        cleanup_stale_sessions(&self.cache_root, &self.run_root)?;
        fs::create_dir(&self.run_root).map_err(|error| format!("创建截图会话目录失败：{error}"))?;
        let marker = self.run_root.join(SESSION_MARKER);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // 打开的运行标记拒绝共享，旧会话清理不能读写或删除仍存活的会话。
            options.share_mode(0);
        }
        let lease_result = (|| -> io::Result<File> {
            let mut lease = options.open(&marker)?;
            write!(lease, "{}", std::process::id())?;
            lease.sync_data()?;
            Ok(lease)
        })();
        match lease_result {
            Ok(lease) => {
                usage._lease = Some(lease);
                usage.initialized = true;
                Ok(budget)
            }
            Err(error) => {
                let _ = fs::remove_file(marker);
                let _ = fs::remove_dir(&self.run_root);
                Err(format!("创建截图运行标记失败：{error}"))
            }
        }
    }
}

fn validate_png(width: u32, height: u32, png: &[u8], preview: &[u8]) -> Result<(), String> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err("截图尺寸无效".into());
    }
    if png.len() > MAX_PNG_BYTES {
        return Err("截图超过 16 MiB，请缩小截图区域".into());
    }
    if preview.len() > MAX_PREVIEW_BYTES {
        return Err("截图预览超过 256 KiB".into());
    }
    let dimensions = png_dimensions(png).ok_or("截图 PNG 数据无效")?;
    if dimensions != (width, height) {
        return Err("截图 PNG 尺寸与附件信息不符".into());
    }
    let (preview_width, preview_height) = png_dimensions(preview).ok_or("截图预览 PNG 数据无效")?;
    if preview_width == 0 || preview_height == 0 || preview_width > 512 || preview_height > 512 {
        return Err("截图预览尺寸无效".into());
    }
    Ok(())
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 33
        || &bytes[..8] != PNG_SIGNATURE
        || bytes[8..12] != 13u32.to_be_bytes()
        || &bytes[12..16] != b"IHDR"
    {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

fn create_plain_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => ensure_plain_directory(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(format!("创建截图缓存目录失败：{error}")),
            }
            ensure_plain_directory(path)
        }
        Err(error) => Err(format!("检查截图缓存目录失败：{error}")),
    }
}

fn ensure_plain_directory(path: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("读取截图目录失败：{error}"))?;
    if is_redirected(&metadata) || !metadata.is_dir() {
        return Err("截图缓存路径是链接或非目录，请检查数据目录".into());
    }
    Ok(())
}

fn is_redirected(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn cleanup_stale_sessions(cache_root: &Path, current_run: &Path) -> Result<(), String> {
    let now = SystemTime::now();
    for session in session_directories(cache_root)? {
        if session == current_run {
            continue;
        }
        let metadata = match fs::symlink_metadata(&session) {
            Ok(metadata) if !is_redirected(&metadata) && metadata.is_dir() => metadata,
            _ => continue,
        };
        if !is_stale(&metadata, now) {
            continue;
        }
        let Some(files) = plain_session_files(&session)? else {
            continue;
        };
        if files.iter().any(|(_, metadata)| !is_stale(metadata, now)) {
            continue;
        }
        let marker = session.join(SESSION_MARKER);
        let mut marker_file = match File::open(&marker) {
            Ok(file) => file,
            // Windows 的存活会话持有拒绝共享的标记，无法读取就保持整个会话。
            Err(_) => continue,
        };
        let mut marker_text = String::new();
        if (&mut marker_file)
            .take(32)
            .read_to_string(&mut marker_text)
            .is_err()
        {
            continue;
        }
        let Ok(pid) = marker_text.trim().parse::<u32>() else {
            continue;
        };
        if process_running(pid) {
            continue;
        }
        drop(marker_file);
        // 只逐个移除经过非链接检查的普通文件，不递归删除目录或链接目标。
        let mut complete = true;
        for (path, _) in files
            .iter()
            .filter(|(path, _)| path.file_name().is_none_or(|name| name != SESSION_MARKER))
        {
            if !remove_plain_file(path) {
                complete = false;
            }
        }
        if complete && remove_plain_file(&marker) {
            let _ = fs::remove_dir(&session);
        }
    }
    Ok(())
}

fn cache_bytes(cache_root: &Path, current_run: &Path) -> Result<(u64, u64), String> {
    let mut bytes = 0u64;
    let mut run_bytes = 0u64;
    for session in session_directories(cache_root)? {
        let Some(files) = plain_session_files(&session)? else {
            if session == current_run {
                return Err("当前截图会话包含链接或目录，请检查截图缓存".into());
            }
            // 不沿着可疑会话中的链接统计或清理，避免触碰缓存根以外的文件。
            continue;
        };
        for (_, metadata) in files {
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or("截图缓存大小无效")?;
            if session == current_run {
                run_bytes = run_bytes
                    .checked_add(metadata.len())
                    .ok_or("截图会话大小无效")?;
            }
            if bytes > MAX_CACHE_BYTES {
                return Err("截图缓存已达 512 MiB，请关闭其他桌面端并清理过期截图后重试".into());
            }
        }
    }
    Ok((bytes, run_bytes))
}

fn session_directories(cache_root: &Path) -> Result<Vec<PathBuf>, String> {
    ensure_plain_directory(cache_root)?;
    let entries = fs::read_dir(cache_root).map_err(|error| format!("读取截图缓存失败：{error}"))?;
    let mut sessions = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= 4096 {
            return Err("截图缓存会话过多，请清理过期截图后重试".into());
        }
        let entry = entry.map_err(|error| format!("读取截图缓存条目失败：{error}"))?;
        let name = entry.file_name();
        if name
            .to_str()
            .is_none_or(|name| Uuid::parse_str(name).is_err())
        {
            continue;
        }
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("读取截图会话信息失败：{error}")),
        };
        if metadata.is_dir() && !is_redirected(&metadata) {
            sessions.push(entry.path());
        }
    }
    Ok(sessions)
}

fn plain_session_files(session: &Path) -> Result<Option<Vec<(PathBuf, Metadata)>>, String> {
    ensure_plain_directory(session)?;
    let entries = fs::read_dir(session).map_err(|error| format!("读取截图会话失败：{error}"))?;
    let mut files = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= 32768 {
            return Err("截图缓存文件过多，请保存草稿后重启桌面端".into());
        }
        let entry = entry.map_err(|error| format!("读取截图文件条目失败：{error}"))?;
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("读取截图文件信息失败：{error}")),
        };
        if is_redirected(&metadata) || !metadata.is_file() {
            return Ok(None);
        }
        files.push((path, metadata));
    }
    Ok(Some(files))
}

fn is_stale(metadata: &Metadata, now: SystemTime) -> bool {
    metadata
        .modified()
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age > STALE_AGE)
}

fn process_running(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), ProcessRefreshKind::new());
    system.process(pid).is_some()
}

fn remove_plain_file(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !is_redirected(&metadata) => {
            fs::remove_file(path).is_ok()
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => true,
        _ => false,
    }
}

#[derive(Default)]
struct CreatedPair {
    paths: Vec<PathBuf>,
    committed: bool,
}

impl CreatedPair {
    fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("创建截图文件失败：{error}"))?;
        self.paths.push(path.to_path_buf());
        file.write_all(bytes)
            .and_then(|()| file.sync_data())
            .map_err(|error| format!("写入截图文件失败：{error}"))
    }
}

impl Drop for CreatedPair {
    fn drop(&mut self) {
        if !self.committed {
            for path in &self.paths {
                let _ = remove_plain_file(path);
            }
        }
    }
}

struct BudgetLease {
    _file: File,
    remove_on_drop: Option<PathBuf>,
}

impl BudgetLease {
    fn open(root: &Path) -> Result<Self, String> {
        let path = root.join(".budget");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.is_file() || is_redirected(&metadata) {
                return Err("截图缓存锁路径无效，请检查数据目录".into());
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .open(&path)
                .map_err(|error| format!("其他截图正在保存，请稍后重试：{error}"))?;
            Ok(Self {
                _file: file,
                remove_on_drop: None,
            })
        }
        #[cfg(not(windows))]
        {
            for attempt in 0..2 {
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(mut file) => {
                        if let Err(error) = write!(file, "{}", std::process::id()) {
                            drop(file);
                            let _ = remove_plain_file(&path);
                            return Err(format!("创建截图缓存锁失败：{error}"));
                        }
                        return Ok(Self {
                            _file: file,
                            remove_on_drop: Some(path),
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempt == 0 => {
                        let metadata = fs::symlink_metadata(&path)
                            .map_err(|error| format!("读取截图缓存锁失败：{error}"))?;
                        if is_redirected(&metadata) || !metadata.is_file() {
                            return Err("截图缓存锁路径无效".into());
                        }
                        let pid = File::open(&path)
                            .and_then(|file| {
                                let mut text = String::new();
                                file.take(32).read_to_string(&mut text)?;
                                Ok(text.trim().parse::<u32>().ok())
                            })
                            .map_err(|error| format!("读取截图缓存锁失败：{error}"))?;
                        let live = pid.map(process_running).unwrap_or_else(|| {
                            metadata
                                .modified()
                                .ok()
                                .and_then(|modified| {
                                    SystemTime::now().duration_since(modified).ok()
                                })
                                .is_none_or(|age| age < Duration::from_secs(30))
                        });
                        if live || !remove_plain_file(&path) {
                            return Err("其他截图正在保存，请稍后重试".into());
                        }
                    }
                    Err(error) => return Err(format!("获取截图缓存锁失败：{error}")),
                }
            }
            Err("其他截图正在保存，请稍后重试".into())
        }
    }
}

impl Drop for BudgetLease {
    fn drop(&mut self) {
        if let Some(path) = &self.remove_on_drop {
            let _ = remove_plain_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use std::fs::FileTimes;
    use tempfile::TempDir;

    fn png() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a6VQAAAAASUVORK5CYII=")
            .unwrap()
    }

    fn age_path(path: &Path, modified: SystemTime) {
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            OpenOptions::new()
                .read(true)
                .access_mode(FILE_WRITE_ATTRIBUTES)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)
                .unwrap()
        };
        #[cfg(not(windows))]
        let file = File::open(path).unwrap();
        file.set_times(FileTimes::new().set_modified(modified))
            .unwrap();
    }

    #[test]
    fn construction_and_invalid_input_never_create_cache_directories() {
        let root = TempDir::new().unwrap();
        let cache = ChatScreenshotCache::new(root.path());
        assert!(!cache.cache_root.exists());
        let valid = png();
        assert!(cache.save(0, 1, &valid, &valid).is_err());
        assert!(cache.save(2, 1, &valid, &valid).is_err());
        assert!(cache.save(1, 1, b"not png", &valid).is_err());
        let mut oversized = valid.clone();
        oversized.resize(MAX_PNG_BYTES + 1, 0);
        assert!(cache.save(1, 1, &oversized, &valid).is_err());
        let mut oversized_preview = valid.clone();
        oversized_preview.resize(MAX_PREVIEW_BYTES + 1, 0);
        assert!(cache.save(1, 1, &valid, &oversized_preview).is_err());
        let mut large_preview = valid.clone();
        large_preview[16..20].copy_from_slice(&513u32.to_be_bytes());
        assert!(cache.save(1, 1, &valid, &large_preview).is_err());
        assert!(!cache.cache_root.exists());
    }

    #[test]
    fn successful_save_publishes_both_files_and_retains_them_after_cache_drop() {
        let root = TempDir::new().unwrap();
        let cache = ChatScreenshotCache::new(root.path());
        let bytes = png();
        let attachment = cache.save(1, 1, &bytes, &bytes).unwrap();
        assert_eq!((attachment.width, attachment.height), (1, 1));
        assert!(attachment.name.starts_with("截图-"));
        assert_eq!(
            Path::new(&attachment.path).parent(),
            Some(cache.run_root.as_path())
        );
        assert_eq!(fs::read(&attachment.path).unwrap(), bytes);
        assert_eq!(fs::read(&attachment.preview_path).unwrap(), bytes);
        drop(cache);
        assert!(Path::new(&attachment.path).is_file());
        assert!(Path::new(&attachment.preview_path).is_file());
    }

    #[test]
    fn repeated_saves_get_unique_names_without_overwriting_previous_attachments() {
        let root = TempDir::new().unwrap();
        let cache = ChatScreenshotCache::new(root.path());
        let bytes = png();
        let first = cache.save(1, 1, &bytes, &bytes).unwrap();
        let second = cache.save(1, 1, &bytes, &bytes).unwrap();
        assert_ne!(first.path, second.path);
        assert_ne!(first.preview_path, second.preview_path);
        assert_ne!(first.name, second.name);
        assert_eq!(fs::read(first.path).unwrap(), bytes);
        assert_eq!(fs::read(second.path).unwrap(), bytes);
    }

    #[test]
    fn failed_second_write_rolls_back_first_without_removing_existing_destination() {
        let root = TempDir::new().unwrap();
        let original = root.path().join("original.png");
        let existing_preview = root.path().join("preview.png");
        fs::write(&existing_preview, b"preserve existing file").unwrap();
        {
            let mut pair = CreatedPair::default();
            pair.write(&original, b"new screenshot").unwrap();
            assert!(
                pair.write(&existing_preview, b"replacement preview")
                    .is_err()
            );
        }
        assert!(!original.exists());
        assert_eq!(
            fs::read(existing_preview).unwrap(),
            b"preserve existing file"
        );
    }

    #[test]
    fn full_current_run_budget_refuses_new_writes_and_preserves_older_drafts() {
        let root = TempDir::new().unwrap();
        let cache = ChatScreenshotCache::new(root.path());
        let bytes = png();
        let first = cache.save(1, 1, &bytes, &bytes).unwrap();
        cache.usage.lock().unwrap().bytes = MAX_RUN_BYTES;
        let error = cache.save(1, 1, &bytes, &bytes).unwrap_err();
        assert!(error.contains("256 MiB"));
        assert_eq!(fs::read(first.path).unwrap(), bytes);
        assert_eq!(
            plain_session_files(&cache.run_root).unwrap().unwrap().len(),
            3
        );
    }

    #[test]
    fn another_save_preserves_active_sessions_even_when_their_files_are_older_than_seven_days() {
        let root = TempDir::new().unwrap();
        let active = ChatScreenshotCache::new(root.path());
        let bytes = png();
        let attachment = active.save(1, 1, &bytes, &bytes).unwrap();
        let old = SystemTime::now() - STALE_AGE - Duration::from_secs(60);
        age_path(Path::new(&attachment.path), old);
        age_path(Path::new(&attachment.preview_path), old);
        active
            .usage
            .lock()
            .unwrap()
            ._lease
            .as_ref()
            .unwrap()
            .set_times(FileTimes::new().set_modified(old))
            .unwrap();
        age_path(&active.run_root, old);
        let second = ChatScreenshotCache::new(root.path());
        second.save(1, 1, &bytes, &bytes).unwrap();
        assert!(active.run_root.is_dir());
        assert_eq!(fs::read(attachment.path).unwrap(), bytes);
        assert!(process_running(std::process::id()));
    }

    #[test]
    fn cleanup_removes_only_inactive_stale_sessions_and_preserves_young_ones() {
        let root = TempDir::new().unwrap();
        let cache = ChatScreenshotCache::new(root.path());
        fs::create_dir_all(&cache.cache_root).unwrap();
        let stale = cache.cache_root.join(Uuid::new_v4().to_string());
        let young = cache.cache_root.join(Uuid::new_v4().to_string());
        for session in [&stale, &young] {
            fs::create_dir(session).unwrap();
            fs::write(session.join(SESSION_MARKER), u32::MAX.to_string()).unwrap();
            fs::write(session.join("image.png"), png()).unwrap();
        }
        let old = SystemTime::now() - STALE_AGE - Duration::from_secs(60);
        for path in [
            &stale.join(SESSION_MARKER),
            &stale.join("image.png"),
            &stale,
        ] {
            age_path(path, old);
        }
        cleanup_stale_sessions(&cache.cache_root, &cache.run_root).unwrap();
        assert!(!stale.exists());
        assert!(young.join("image.png").is_file());
    }

    fn link_directory(target: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).unwrap();
            true
        }
        #[cfg(windows)]
        {
            match std::os::windows::fs::symlink_dir(target, link) {
                Ok(()) => true,
                Err(error) if error.raw_os_error() == Some(1314) => false,
                Err(error) => panic!("create test symlink: {error}"),
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (target, link);
            false
        }
    }

    #[test]
    fn save_refuses_linked_parent_without_writing_into_its_target() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        if !link_directory(outside.path(), &root.path().join("state")) {
            return;
        }
        let cache = ChatScreenshotCache::new(root.path());
        let bytes = png();
        assert!(cache.save(1, 1, &bytes, &bytes).is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn cleanup_does_not_follow_linked_session_or_delete_its_target() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let cache = ChatScreenshotCache::new(root.path());
        fs::create_dir_all(&cache.cache_root).unwrap();
        let original = outside.path().join("keep.png");
        fs::write(&original, png()).unwrap();
        fs::write(outside.path().join(SESSION_MARKER), u32::MAX.to_string()).unwrap();
        let linked = cache.cache_root.join(Uuid::new_v4().to_string());
        if !link_directory(outside.path(), &linked) {
            return;
        }
        cleanup_stale_sessions(&cache.cache_root, &cache.run_root).unwrap();
        assert!(linked.exists());
        assert!(original.is_file());
        assert!(outside.path().join(SESSION_MARKER).is_file());
    }
}
