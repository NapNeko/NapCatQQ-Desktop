//! 配置导入导出命令。
//!
//! 导出：将应用配置 / Bot 配置 / 远端档案（不含密钥）打成 ZIP，含 export_meta.json。
//! 导入：从 ZIP 或扁平目录读取 config.json / bot.json / servers.json，校验后原子写回。
//! 预览：preview_config_import 只扫描来源，不写盘，供导入向导展示。

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
    /// zip | directory
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

/// 导出当前配置为 ZIP。dest_path 为完整 .zip 路径；父目录不存在则创建。
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

/// 校验并归一化 app config(config.json)。非对象 / 不像应用配置直接拒,绝不覆盖
/// 生产配置;通过则走 migrate_app_config 归一化到当前版本。
fn normalize_app_config_import(value: serde_json::Value) -> Result<serde_json::Value, String> {
    if !ncd_runtime::app_config_migration::looks_like_app_config(&value) {
        return Err("config.json 不像应用配置(非对象或缺少已知配置段),已中止导入".to_string());
    }
    Ok(ncd_runtime::app_config_migration::migrate_app_config(value).payload)
}

/// 校验并归一化 bot config(bot.json):迁移 → 反序列化 Vec<BotConfig> → 逐个
/// validate + QQ 去重。任一非法即中止,返回迁移后的强类型化 payload(而非原样透传)。
fn normalize_bot_config_import(
    value: serde_json::Value,
    secrets: &dyn ncd_runtime::SecretStore,
) -> Result<serde_json::Value, String> {
    use std::collections::HashSet;
    let migrated = ncd_runtime::bot_config_migration::migrate_bot_config(value, secrets)
        .map_err(|e| format!("bot.json 迁移/解析失败,已中止导入: {e}"))?;
    let bots_payload = migrated
        .payload
        .get("bots")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let bots: Vec<ncd_runtime::BotConfig> = serde_json::from_value(bots_payload)
        .map_err(|e| format!("bot.json 不是合法 Bot 配置,已中止导入: {e}"))?;
    let mut seen = HashSet::new();
    for bot in &bots {
        bot.validate()
            .map_err(|e| format!("bot.json 含非法 Bot 配置,已中止导入: {e}"))?;
        bot.validate_runtime_matrix()
            .map_err(|e| format!("bot.json 含当前不支持的运行组合,已中止导入: {e}"))?;
        if !seen.insert(bot.bot.qq_id) {
            return Err(format!("bot.json 含重复 QQ 号 {},已中止导入", bot.bot.qq_id));
        }
    }
    Ok(migrated.payload)
}

/// 校验并归一化 servers.json:必须是 ServerProfile 数组。重新序列化,丢弃多余字段。
fn normalize_servers_import(value: serde_json::Value) -> Result<serde_json::Value, String> {
    let servers: Vec<ncd_runtime::ServerProfile> = serde_json::from_value(value)
        .map_err(|e| format!("servers.json 不是合法服务器档案数组,已中止导入: {e}"))?;
    serde_json::to_value(&servers).map_err(|e| format!("servers.json 归一化失败: {e}"))
}

/// 读 staging 里的配置文件,全量强类型反序列化 + 迁移 + validate,通过后构造一个
/// 一次性 JsonTransaction。任一文件语义非法即整体中止(返回 Err),绝不半导入;调用方
/// 对返回的 transaction 走 ConfigStore::apply_transaction 原子提交(失败自动回滚)。
fn build_import_transaction(
    staging: &Path,
    data_root: &Path,
    secrets: &dyn ncd_runtime::SecretStore,
) -> Result<(ncd_runtime::JsonTransaction, Vec<String>, Vec<String>), String> {
    let mut txn = ncd_runtime::JsonTransaction::new();
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
            .map_err(|e| format!("{label} 不是合法 JSON,已中止导入: {e}"))?;

        let normalized = match *zip_name {
            "config.json" => normalize_app_config_import(value)?,
            "bot.json" => normalize_bot_config_import(value, secrets)?,
            "servers.json" => normalize_servers_import(value)?,
            other => return Err(format!("未知导入文件: {other}")),
        };
        txn = txn.write(data_root.join(rel), normalized);
        files.push((*label).to_string());
    }

    if files.is_empty() {
        return Err(
            "来源里没有可识别的配置文件(config.json / bot.json / servers.json)".to_string(),
        );
    }
    Ok((txn, files, skipped))
}

/// 从 ZIP 或目录导入配置:全量强类型校验通过后,一次性事务原子写回当前数据根。
/// 任一文件语义非法即整体中止,绝不发生"改了一半"的半导入漂移(旧实现逐文件
/// write_json_atomic 会半成功);apply_transaction 自带备份,写失败整体回滚。
#[tauri::command]
pub async fn import_config(
    state: State<'_, AppState>,
    source_path: String,
) -> Result<ConfigImportResult, String> {
    use ncd_runtime::{ConfigStore, LocalConfigStore, SecretStoreImpl};

    let source = PathBuf::from(&source_path);
    let (staging, _kind, _guard) = resolve_import_staging(&source)?;

    let secrets = SecretStoreImpl::new(state.data_root.join("secrets"));
    let (txn, files, skipped) = build_import_transaction(&staging, &state.data_root, &secrets)?;

    let store = LocalConfigStore::new(&state.data_root);
    store
        .apply_transaction(txn)
        .map_err(|e| format!("写入配置失败(已回滚): {e}"))?;

    Ok(ConfigImportResult { files, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn force_fallback_secrets() -> (tempfile::TempDir, ncd_runtime::SecretStoreImpl) {
        let dir = tempfile::tempdir().unwrap();
        let store =
            ncd_runtime::SecretStoreImpl::new_with_force_fallback(dir.path().to_path_buf(), true);
        (dir, store)
    }

    fn write_file(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    const VALID_CONFIG: &str = r#"{"Info":{"ConfigVersion":"v2.0"}}"#;
    const VALID_BOT: &str =
        r#"{"bots":[{"bot":{"QQID":"10001","name":"X"},"connect":{},"advanced":{}}]}"#;

    #[test]
    fn build_import_transaction_validates_all_and_batches_writes() {
        let staging = tempfile::tempdir().unwrap();
        write_file(staging.path(), "config.json", VALID_CONFIG);
        write_file(staging.path(), "bot.json", VALID_BOT);
        write_file(staging.path(), "servers.json", "[]");
        let (_d, secrets) = force_fallback_secrets();
        let data_root = tempfile::tempdir().unwrap();

        let (txn, files, skipped) =
            build_import_transaction(staging.path(), data_root.path(), &secrets).unwrap();
        // 三个文件一次性进同一个 transaction,而非逐文件落盘。
        assert_eq!(txn.writes.len(), 3);
        assert_eq!(files.len(), 3);
        assert!(skipped.is_empty());
    }

    #[test]
    fn build_import_transaction_aborts_when_any_file_is_semantically_invalid() {
        let staging = tempfile::tempdir().unwrap();
        // config 合法,但 servers.json 语义非法(不是数组):整体必须中止,不构造事务。
        write_file(staging.path(), "config.json", VALID_CONFIG);
        write_file(staging.path(), "servers.json", r#"{"not":"an array"}"#);
        let (_d, secrets) = force_fallback_secrets();
        let data_root = tempfile::tempdir().unwrap();

        let err =
            build_import_transaction(staging.path(), data_root.path(), &secrets).unwrap_err();
        assert!(err.contains("servers.json"), "应报 servers.json 非法: {err}");
    }

    #[test]
    fn build_import_transaction_rejects_non_object_app_config() {
        let staging = tempfile::tempdir().unwrap();
        write_file(staging.path(), "config.json", r#"[1,2,3]"#);
        let (_d, secrets) = force_fallback_secrets();
        let data_root = tempfile::tempdir().unwrap();

        let err =
            build_import_transaction(staging.path(), data_root.path(), &secrets).unwrap_err();
        assert!(err.contains("config.json"), "应报 config.json 非法: {err}");
    }

    #[test]
    fn build_import_transaction_skips_missing_files() {
        let staging = tempfile::tempdir().unwrap();
        write_file(staging.path(), "config.json", VALID_CONFIG);
        let (_d, secrets) = force_fallback_secrets();
        let data_root = tempfile::tempdir().unwrap();

        let (txn, files, skipped) =
            build_import_transaction(staging.path(), data_root.path(), &secrets).unwrap();
        assert_eq!(txn.writes.len(), 1);
        assert_eq!(files, vec!["应用配置".to_string()]);
        assert_eq!(skipped.len(), 2);
    }
}