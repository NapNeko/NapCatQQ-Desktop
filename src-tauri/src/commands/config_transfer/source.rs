use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

use ncd_runtime::app_framework::{FrameworkConfigBackup, FrameworkConfigRecovery};
use serde_json::Value;
use zip::read::ZipArchive;

use super::ConfigFrontendPreferences;
use super::registry::{
    TRANSFER_FILES, TransferEntry, TransferKind, identify_source_path, validate_relative_path,
};

pub(super) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub(super) const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 4096;

#[derive(Debug)]
pub(super) struct RawEntry {
    pub descriptor: TransferEntry,
    pub value: Value,
}

pub(super) struct SourceSnapshot {
    pub entries: Vec<RawEntry>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

pub(super) struct StagingDir {
    pub path: PathBuf,
}

impl StagingDir {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("ncd-config-import-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).map_err(|error| format!("创建临时目录失败: {error}"))?;
        Ok(Self { path })
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn add_budget(total: &mut u64, size: u64, name: &str) -> Result<(), String> {
    if size > MAX_FILE_BYTES {
        return Err(format!("配置文件 {name} 超出 16 MiB 大小限制"));
    }
    *total = total
        .checked_add(size)
        .filter(|sum| *sum <= MAX_TOTAL_BYTES)
        .ok_or_else(|| "配置包超出 64 MiB 总大小限制".to_string())?;
    Ok(())
}

fn regular_file_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("配置来源不能是符号链接: {}", path.display()))
        }
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(format!("配置来源不是普通文件: {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("检查配置来源失败 {}: {error}", path.display())),
    }
}

fn read_json(root: &Path, path: &Path, total: &mut u64) -> Result<Value, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("解析来源目录失败: {error}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("解析配置路径失败: {error}"))?;
    if !canonical.starts_with(&root) {
        return Err(format!("配置来源路径越界: {}", path.display()));
    }
    let file =
        File::open(path).map_err(|error| format!("读取 {} 失败: {error}", path.display()))?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_FILE_BYTES {
        return Err(format!("配置文件 {} 超出 16 MiB 大小限制", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("读取 {} 失败: {error}", path.display()))?;
    add_budget(total, bytes.len() as u64, &path.to_string_lossy())?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{} 不是合法 JSON，已中止导入: {error}", path.display()))
}

fn collect_snapshots(
    root: &Path,
    aliases: bool,
    total: &mut u64,
    entries: &mut Vec<RawEntry>,
    seen: &mut HashSet<String>,
    framework: bool,
) -> Result<(), String> {
    let directories: &[&str] = if framework && aliases {
        &["config/framework-configs", "framework-configs"]
    } else if framework {
        &["config/framework-configs"]
    } else if aliases {
        &["config/app-adopts", "app-adopts"]
    } else {
        &["config/app-adopts"]
    };
    for relative in directories {
        let directory = root.join(relative);
        if !directory.exists() {
            continue;
        }
        let mut files: Vec<_> = fs::read_dir(&directory)
            .map_err(|error| format!("读取领养快照目录失败: {error}"))?
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?;
        files.sort_by_key(|file| file.file_name());
        for file in files {
            let leaf = file.file_name().to_string_lossy().into_owned();
            let Some(descriptor) = identify_source_path(&format!("{relative}/{leaf}"))? else {
                continue;
            };
            if !regular_file_exists(&file.path())? {
                continue;
            }
            if !seen.insert(descriptor.data_relative.clone()) {
                return Err(format!(
                    "同一配置目标出现重复来源: {}",
                    descriptor.data_relative
                ));
            }
            let mut value = read_json(root, &file.path(), total)?;
            if framework && value.get("backup").is_some() {
                let recovery: FrameworkConfigRecovery = serde_json::from_value(value)
                    .map_err(|e| format!("{leaf} 框架配置恢复副本无效: {e}"))?;
                if !aliases && !recovery.pending {
                    continue;
                }
                value = serde_json::to_value(recovery.backup).map_err(|e| e.to_string())?;
            }
            entries.push(RawEntry { descriptor, value });
        }
    }
    Ok(())
}

fn metadata_warnings(root: &Path, total: &mut u64) -> Result<Vec<String>, String> {
    let path = root.join("export_meta.json");
    if !regular_file_exists(&path)? {
        return Ok(vec!["来源没有导出元数据，将按旧版配置包兼容导入。".into()]);
    }
    let meta = read_json(root, &path, total)?;
    let version = meta
        .get("exportFormatVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| "export_meta.json 缺少有效的导出格式版本".to_string())?;
    if !matches!(version, "v1" | "v2") {
        return Err(format!(
            "不支持的配置包格式版本 {version}，请使用支持该版本的 Desktop 导入"
        ));
    }
    for key in ["files", "entries"] {
        if let Some(value) = meta.get(key) {
            let Some(values) = value
                .as_array()
                .filter(|values| values.iter().all(Value::is_string))
            else {
                return Err(format!("export_meta.json 的 {key} 字段必须是字符串数组"));
            };
            if key == "entries" {
                for name in values.iter().filter_map(Value::as_str) {
                    validate_relative_path(name)?;
                }
            }
        }
    }
    if meta
        .get("exportedAtUnix")
        .is_some_and(|value| value.as_u64().is_none())
    {
        return Err("export_meta.json 的导出时间无效".into());
    }
    Ok(if version == "v1" {
        vec!["来源使用 v1 导出格式，将兼容导入已包含的配置。".into()]
    } else {
        Vec::new()
    })
}

pub(super) fn discover_source(root: &Path) -> Result<SourceSnapshot, String> {
    let mut total = 0;
    let mut warnings = metadata_warnings(root, &mut total)?;
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = HashSet::new();
    for spec in TRANSFER_FILES {
        let mut found = None;
        for name in [spec.archive_name, spec.data_relative] {
            if name == spec.data_relative && name == spec.archive_name && found.is_some() {
                continue;
            }
            let path = root.join(name);
            if !regular_file_exists(&path)? {
                continue;
            }
            if found.is_some() {
                return Err(format!("同一配置目标出现重复来源: {}", spec.data_relative));
            }
            found = Some(path);
        }
        if let Some(path) = found {
            let value = read_json(root, &path, &mut total)?;
            let descriptor = TransferEntry::from(spec);
            seen.insert(descriptor.data_relative.clone());
            entries.push(RawEntry { descriptor, value });
        } else {
            skipped.push(spec.label.to_string());
        }
    }
    collect_snapshots(root, true, &mut total, &mut entries, &mut seen, false)?;
    collect_snapshots(root, true, &mut total, &mut entries, &mut seen, true)?;
    if entries.iter().any(|entry| {
        matches!(
            entry.descriptor.kind,
            TransferKind::Instances | TransferKind::Adopt
        )
    }) {
        warnings.push(
            "应用实例将恢复安装路径与关联；框架与插件程序需独立安装，配置仅写入已识别且停止运行的框架目录。".into(),
        );
    }
    if entries
        .iter()
        .any(|entry| entry.descriptor.kind == TransferKind::Framework)
    {
        warnings.push("无法立即恢复的框架配置会保留待恢复副本；安装完成、停止实例或连接远端后，可在配置备份中单独重试。".into());
    }
    Ok(SourceSnapshot {
        entries,
        skipped,
        warnings,
    })
}

pub(super) fn collect_export(
    root: &Path,
    frontend: Option<&ConfigFrontendPreferences>,
) -> Result<Vec<RawEntry>, String> {
    let mut total = 0;
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for spec in TRANSFER_FILES {
        let descriptor = TransferEntry::from(spec);
        let value = if spec.kind == TransferKind::Frontend && frontend.is_some() {
            let value = serde_json::to_value(frontend)
                .map_err(|error| format!("序列化浏览器偏好失败: {error}"))?;
            add_budget(
                &mut total,
                serde_json::to_vec(&value)
                    .map_err(|error| error.to_string())?
                    .len() as u64,
                spec.archive_name,
            )?;
            value
        } else {
            let path = root.join(spec.data_relative);
            if !regular_file_exists(&path)? {
                continue;
            }
            read_json(root, &path, &mut total)?
        };
        seen.insert(descriptor.data_relative.clone());
        entries.push(RawEntry { descriptor, value });
    }
    collect_snapshots(root, false, &mut total, &mut entries, &mut seen, false)?;
    collect_snapshots(root, false, &mut total, &mut entries, &mut seen, true)?;
    Ok(entries)
}

pub(super) fn merge_framework_backups(
    entries: &mut Vec<RawEntry>,
    backups: Vec<FrameworkConfigBackup>,
) -> Result<(), String> {
    for backup in backups {
        let name = format!("config/framework-configs/{}.json", backup.instance_id);
        let descriptor = identify_source_path(&name)?.ok_or("无法识别框架配置备份条目")?;
        let value = serde_json::to_value(backup).map_err(|e| e.to_string())?;
        entries.retain(|entry| entry.descriptor.data_relative != descriptor.data_relative);
        entries.push(RawEntry { descriptor, value });
    }
    entries.sort_by(|a, b| a.descriptor.archive_name.cmp(&b.descriptor.archive_name));
    Ok(())
}

pub(super) fn extract_zip_to_dir(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|error| format!("打开 ZIP 失败: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("读取 ZIP 失败: {error}"))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err("ZIP 条目数量超出限制".into());
    }
    let mut targets = HashSet::new();
    let mut total = 0;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取 ZIP 条目失败: {error}"))?;
        let name = entry.name().to_string();
        let relative = if entry.is_dir() {
            name.strip_suffix('/').unwrap_or(&name)
        } else {
            &name
        };
        validate_relative_path(relative).map_err(|error| format!("非法 ZIP 路径: {error}"))?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("ZIP 配置来源不能是符号链接: {name}"));
        }
        if entry.is_dir() {
            if TRANSFER_FILES.iter().any(|spec| {
                spec.archive_name.starts_with(&format!("{relative}/"))
                    || spec.data_relative.starts_with(&format!("{relative}/"))
            }) || matches!(
                relative,
                "config/app-adopts"
                    | "app-adopts"
                    | "config/framework-configs"
                    | "framework-configs"
            ) {
                fs::create_dir_all(dest.join(relative))
                    .map_err(|error| format!("创建 ZIP 目录失败: {error}"))?;
            }
            continue;
        }
        let target = if name == "export_meta.json" {
            name.clone()
        } else if let Some(descriptor) = identify_source_path(&name)? {
            descriptor.data_relative
        } else {
            continue;
        };
        if !targets.insert(target.clone()) {
            return Err(format!("ZIP 同一配置目标出现重复条目: {target}"));
        }
        if entry.size() > MAX_FILE_BYTES {
            return Err(format!("ZIP 配置条目 {name} 超出 16 MiB 大小限制"));
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("解压 {name} 失败: {error}"))?;
        add_budget(&mut total, bytes.len() as u64, &name)?;
        let out_path = dest.join(&name);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建 ZIP 目录失败: {error}"))?;
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out_path)
            .map_err(|error| format!("写出 {name} 失败: {error}"))?;
        std::io::Write::write_all(&mut out, &bytes)
            .map_err(|error| format!("写出 {name} 失败: {error}"))?;
    }
    Ok(())
}

pub(super) fn resolve_import_staging(
    source: &Path,
) -> Result<(PathBuf, String, Option<StagingDir>), String> {
    if source.is_dir() {
        return Ok((source.into(), "directory".into(), None));
    }
    if source.is_file()
        && source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        let staging = StagingDir::new()?;
        extract_zip_to_dir(source, &staging.path)?;
        return Ok((staging.path.clone(), "zip".into(), Some(staging)));
    }
    Err("请选择配置 ZIP 包或包含 Desktop 配置的文件夹".into())
}
