//! 配置导入导出命令。
//!
//! 导出：将应用配置 / Bot 配置 / 远端档案（不含密钥）打成 ZIP，含 `export_meta.json`。
//! 导入：从 ZIP 或扁平目录读取 `config.json` / `bot.json` / `servers.json`，校验后原子写回。
//! 预览：`preview_config_import` 只扫描来源，不写盘，供导入向导展示。

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::AppState;

const EXPORT_FORMAT_VERSION: &str = "v1";

/// 参与导入导出的配置文件，相对 data_root 的路径 + 用途描述。
const TRANSFER_FILES: &[(&str, &str, &str)] = &[
    ("runtime/config/config.json", "config.json", "应用配置"),
    ("runtime/config/bot.json", "bot.json", "Bot 配置"),
    ("config/servers.json", "servers.json", "远端服务器档案"),
];

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct ConfigExportResult {
    /// 写出的 ZIP 绝对路径。
    pub export_path: String,
    /// 成功打入包内的人类可读名。
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct ConfigImportPreview {
    pub source_path: String,
    /// `zip` | `directory`
    pub source_kind: String,
    pub files_found: Vec<String>,
    pub warnings: Vec<String>,
    pub can_import: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct ConfigImportResult {
    pub files: Vec<String>,
    pub skipped: Vec<String>,
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn build_export_meta(files: &[String]) -> serde_json::Value {
    serde_json::json!({
        "exportFormatVersion": EXPORT_FORMAT_VERSION,
        "exportedAtUnix": unix_ts(),
        "files": files,
    })
}

fn add_json_to_zip<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(name, options)
        .map_err(|e| format!("ZIP 写入 {name} 失败: {e}"))?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| format!("序列化 {name} 失败: {e}"))?;
    zip.write_all(&bytes)
        .map_err(|e| format!("ZIP 写出 {name} 失败: {e}"))?;
    Ok(())
}

/// 导出当前配置为 ZIP。`dest_path` 为完整 `.zip` 路径；父目录不存在则创建。
#[tauri::command]
pub async fn export_config(
    state: State<'_, AppState>,
    dest_path: String,
) -> Result<ConfigExportResult, String> {
    let dest = PathBuf::from(&dest_path);
    if dest.extension().and_then(|s| s.to_str()) != Some("zip") {
        return Err("导出目标必须是 .zip 文件路径".to_string());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }

    let data_root = &state.data_root;
    let mut labels = Vec::new();
    let file = File::create(&dest).map_err(|e| format!("创建 ZIP 失败: {e}"))?;
    let mut zip = ZipWriter::new(file);

    for (rel, zip_name, label) in TRANSFER_FILES {
        let src = data_root.join(rel);
        if !src.is_file() {
            continue;
        }
        let text = fs::read_to_string(&src).map_err(|e| format!("读取 {label} 失败: {e}"))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("{label} 不是合法 JSON: {e}"))?;
        add_json_to_zip(&mut zip, zip_name, &value)?;
        labels.push((*label).to_string());
    }

    if labels.is_empty() {
        return Err("当前没有可导出的配置文件".to_string());
    }

    let meta = build_export_meta(&labels);
    add_json_to_zip(&mut zip, "export_meta.json", &meta)?;

    zip.finish().map_err(|e| format!("完成 ZIP 失败: {e}"))?;

    Ok(ConfigExportResult {
        export_path: dest.to_string_lossy().to_string(),
        files: labels,
    })
}

struct StagingDir {
    path: PathBuf,
}

impl StagingDir {
    fn new() -> Result<Self, String> {
        let base = std::env::temp_dir().join(format!("ncd-config-import-{}", unix_ts()));
        fs::create_dir_all(&base).map_err(|e| format!("创建临时目录失败: {e}"))?;
        Ok(Self { path: base })
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn extract_zip_to_dir(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| format!("打开 ZIP 失败: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("读取 ZIP 失败: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败: {e}"))?;
        let name = entry.name().to_string();
        if entry.is_dir() || name.contains("..") {
            continue;
        }
        let file_name = Path::new(&name)
            .file_name()
            .ok_or_else(|| format!("非法 ZIP 路径: {name}"))?;
        let out_path = dest.join(file_name);
        let mut out = File::create(&out_path).map_err(|e| format!("写出 {name} 失败: {e}"))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("解压 {name} 失败: {e}"))?;
    }
    Ok(())
}

fn resolve_import_staging(source: &Path) -> Result<(PathBuf, String, Option<StagingDir>), String> {
    if source.is_dir() {
        return Ok((source.to_path_buf(), "directory".to_string(), None));
    }
    if source.is_file() {
        let ext = source.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("zip") {
            let staging = StagingDir::new()?;
            extract_zip_to_dir(source, &staging.path)?;
            return Ok((staging.path.clone(), "zip".to_string(), Some(staging)));
        }
    }
    Err("请选择配置 ZIP 包或包含 config.json / bot.json / servers.json 的文件夹".to_string())
}

fn scan_staging(staging: &Path) -> (Vec<String>, Vec<String>) {
    let mut found = Vec::new();
    let mut skipped = Vec::new();
    for (_, zip_name, label) in TRANSFER_FILES {
        let p = staging.join(zip_name);
        if p.is_file() {
            found.push((*label).to_string());
        } else {
            skipped.push((*label).to_string());
        }
    }
    (found, skipped)
}

/// 扫描导入来源（ZIP 或目录），不写盘。
#[tauri::command]
pub async fn preview_config_import(source_path: String) -> Result<ConfigImportPreview, String> {
    let source = PathBuf::from(&source_path);
    if !source.exists() {
        return Err("导入来源不存在".to_string());
    }

    let (staging, kind, _guard) = resolve_import_staging(&source)?;
    let (found, _skipped) = scan_staging(&staging);
    let mut warnings = Vec::new();
    if found.is_empty() {
        warnings.push("未找到 config.json、bot.json 或 servers.json".to_string());
    }

    Ok(ConfigImportPreview {
        source_path: source.to_string_lossy().to_string(),
        source_kind: kind,
        files_found: found.clone(),
        warnings,
        can_import: !found.is_empty(),
    })
}

/// 从 ZIP 或目录导入配置，原子写回当前数据根。
#[tauri::command]
pub async fn import_config(
    state: State<'_, AppState>,
    source_path: String,
) -> Result<ConfigImportResult, String> {
    use ncd_runtime::{ConfigStore, LocalConfigStore};

    let source = PathBuf::from(&source_path);
    let (staging, _kind, _guard) = resolve_import_staging(&source)?;

    let store = LocalConfigStore::new(&state.data_root);
    let mut files = Vec::new();
    let mut skipped = Vec::new();

    for (rel, zip_name, label) in TRANSFER_FILES {
        let src = staging.join(zip_name);
        if !src.is_file() {
            skipped.push((*label).to_string());
            continue;
        }

        let text = fs::read_to_string(&src).map_err(|e| format!("读取 {label} 失败: {e}"))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("{label} 不是合法 JSON，已中止导入: {e}"))?;

        let dst = state.data_root.join(rel);
        store
            .write_json_atomic(&dst, &value)
            .map_err(|e| format!("写入 {label} 失败: {e}"))?;
        files.push((*label).to_string());
    }

    if files.is_empty() {
        return Err(
            "来源里没有可识别的配置文件（config.json / bot.json / servers.json）".to_string(),
        );
    }

    Ok(ConfigImportResult { files, skipped })
}