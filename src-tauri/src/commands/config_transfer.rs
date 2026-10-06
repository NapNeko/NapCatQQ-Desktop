//! 配置导入导出命令
//!
//! Registered Desktop configuration is validated before ZIP export or one atomic import transaction.
//! Browser preferences are explicitly allowlisted; secrets and runtime/history data stay separate.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ncd_runtime::app_framework::{FrameworkConfigBackup, FrameworkConfigRecovery};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use ts_rs::TS;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::AppState;

mod registry;
mod source;
mod validation;

use registry::TransferKind;
use source::{collect_export, discover_source, merge_framework_backups, resolve_import_staging};
use validation::{MemorySecretStore, normalize_entry, validate_framework_links};

#[cfg(test)]
use source::extract_zip_to_dir;

const EXPORT_FORMAT_VERSION: &str = "v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct ConfigFrontendPreferences {
    pub version: u32,
    pub storage: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct ConfigExportResult {
    /// 写出的 ZIP 绝对路径
    pub export_path: String,
    /// 成功打入包内的人类可读名
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frontend_preferences: Option<ConfigFrontendPreferences>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub framework_pending: Option<Vec<String>>,
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn build_export_meta(files: &[String], entries: &[String]) -> serde_json::Value {
    serde_json::json!({
        "exportFormatVersion": EXPORT_FORMAT_VERSION,
        "exportedAtUnix": unix_ts(),
        "files": files,
        "entries": entries,
    })
}

fn add_bytes_to_zip<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(name, options)
        .map_err(|e| format!("ZIP 写入 {name} 失败: {e}"))?;
    zip.write_all(bytes)
        .map_err(|e| format!("ZIP 写出 {name} 失败: {e}"))?;
    Ok(())
}

/// 导出当前配置为 ZIPdest_path 为完整 .zip 路径;父目录不存在则创建
#[tauri::command]
pub async fn export_config(
    state: State<'_, AppState>,
    dest_path: String,
    frontend_preferences: Option<ConfigFrontendPreferences>,
) -> Result<ConfigExportResult, String> {
    state.migrate_gate.ensure_idle()?;
    let _framework_guard = state.app_manager.framework_config_transfer_guard().await;
    let instances = state.app_manager.list_instances().await;
    let servers = state.server_manager.list_servers().await;
    let frameworks = state
        .app_manager
        .export_framework_config_backups(&instances, Some(&servers))
        .await?;
    let dest = PathBuf::from(&dest_path);
    export_config_with_frameworks(
        &state.data_root,
        &dest,
        frontend_preferences,
        frameworks,
        Some(&instances),
        Some(&servers),
    )
}

#[cfg(test)]
fn export_config_to_path(
    data_root: &Path,
    dest: &Path,
    frontend_preferences: Option<ConfigFrontendPreferences>,
) -> Result<ConfigExportResult, String> {
    export_config_with_frameworks(
        data_root,
        dest,
        frontend_preferences,
        Vec::new(),
        None,
        None,
    )
}

fn export_config_with_frameworks(
    data_root: &Path,
    dest: &Path,
    frontend_preferences: Option<ConfigFrontendPreferences>,
    frameworks: Vec<FrameworkConfigBackup>,
    instances: Option<&[ncd_domain::AppInstance]>,
    servers: Option<&[ncd_runtime::ServerProfile]>,
) -> Result<ConfigExportResult, String> {
    if !dest
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err("导出目标必须是 .zip 文件路径".to_string());
    }
    let mut entries = collect_export(data_root, frontend_preferences.as_ref())?;
    if let Some(instances) = instances {
        entries.retain(|entry| {
            entry.descriptor.kind != TransferKind::Instances
                && (entry.descriptor.kind != TransferKind::Framework
                    || instances
                        .iter()
                        .any(|i| entry.value["instance_id"] == i.id.as_str()))
        });
        let spec = registry::TRANSFER_FILES
            .iter()
            .find(|spec| spec.kind == TransferKind::Instances)
            .ok_or("缺少实例备份注册项")?;
        entries.push(source::RawEntry {
            descriptor: spec.into(),
            value: serde_json::json!({"version": 1, "instances": instances}),
        });
    }
    if let Some(servers) = servers {
        entries.retain(|entry| entry.descriptor.kind != TransferKind::Servers);
        let spec = registry::TRANSFER_FILES
            .iter()
            .find(|spec| spec.kind == TransferKind::Servers)
            .ok_or("缺少远端备份注册项")?;
        entries.push(source::RawEntry {
            descriptor: spec.into(),
            value: serde_json::to_value(servers).map_err(|e| e.to_string())?,
        });
    }
    merge_framework_backups(&mut entries, frameworks)?;
    if entries.is_empty() {
        return Err("当前没有可导出的配置文件".to_string());
    }
    let simulation = MemorySecretStore::default();
    validate_framework_links(&entries)?;
    let mut labels = Vec::new();
    let mut paths = Vec::new();
    let mut serialized = Vec::new();
    let mut total = 0_u64;

    for entry in entries {
        normalize_entry(&entry.descriptor, entry.value.clone(), &simulation)?;
        let bytes = serde_json::to_vec_pretty(&entry.value)
            .map_err(|error| format!("序列化 {} 失败: {error}", entry.descriptor.archive_name))?;
        if bytes.len() as u64 > source::MAX_FILE_BYTES {
            return Err(format!(
                "配置文件 {} 超出 16 MiB 大小限制",
                entry.descriptor.archive_name
            ));
        }
        total += bytes.len() as u64;
        serialized.push((entry.descriptor.archive_name.clone(), bytes));
        labels.push(entry.descriptor.label);
        paths.push(entry.descriptor.archive_name);
    }
    let meta = build_export_meta(&labels, &paths);
    let metadata = serde_json::to_vec_pretty(&meta)
        .map_err(|error| format!("序列化导出元数据失败: {error}"))?;
    total += metadata.len() as u64;
    if total > source::MAX_TOTAL_BYTES {
        return Err("配置包超出 64 MiB 总大小限制".into());
    }
    serialized.push(("export_meta.json".into(), metadata));

    // Build a sibling archive, then rename only after all JSON and ZIP writes have succeeded.
    let parent = dest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("创建导出目录失败: {error}"))?;
    let name = dest
        .file_name()
        .ok_or_else(|| "导出目标必须包含文件名".to_string())?;
    let temporary = ExportTempFile {
        path: parent.join(format!(
            ".{}.ncd-export-{}.tmp",
            name.to_string_lossy(),
            uuid::Uuid::new_v4()
        )),
    };
    let file = File::options()
        .write(true)
        .create_new(true)
        .open(&temporary.path)
        .map_err(|error| format!("创建临时 ZIP 失败: {error}"))?;
    let mut zip = ZipWriter::new(file);
    for (name, bytes) in serialized {
        add_bytes_to_zip(&mut zip, &name, &bytes)?;
    }
    let file = zip
        .finish()
        .map_err(|error| format!("完成 ZIP 失败: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("同步 ZIP 失败: {error}"))?;
    drop(file);
    replace_export_archive(&temporary.path, dest)?;

    Ok(ConfigExportResult {
        export_path: dest.to_string_lossy().to_string(),
        files: labels,
    })
}

// Windows 的 rename 不能覆盖既有文件：先把旧文件挪作兄弟临时名，新 ZIP 落位后再删；
// 中途失败把旧文件挪回去，目标始终是完整的一份。
fn replace_export_archive(temporary: &Path, dest: &Path) -> Result<(), String> {
    if !dest.exists() {
        return fs::rename(temporary, dest).map_err(|error| format!("导出 ZIP 落盘失败: {error}"));
    }
    let name = dest
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let swapped = dest.with_file_name(format!(".{name}.ncd-replaced-{}.tmp", uuid::Uuid::new_v4()));
    fs::rename(dest, &swapped)
        .map_err(|error| format!("替换导出 ZIP 失败，原文件已保留: {error}"))?;
    match fs::rename(temporary, dest) {
        Ok(()) => {
            let _ = fs::remove_file(&swapped);
            Ok(())
        }
        Err(error) => {
            let _ = fs::rename(&swapped, dest);
            Err(format!("替换导出 ZIP 失败，原文件已保留: {error}"))
        }
    }
}

struct ExportTempFile {
    path: PathBuf,
}

impl Drop for ExportTempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Validate an import source without changing production files, caches, or credentials.
#[tauri::command]
pub async fn preview_config_import(source_path: String) -> Result<ConfigImportPreview, String> {
    let source = PathBuf::from(&source_path);
    if !source.exists() {
        return Err("导入来源不存在".to_string());
    }

    let (staging, kind, _guard) = resolve_import_staging(&source)?;
    let snapshot = match discover_source(&staging) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return Ok(ConfigImportPreview {
                source_path: source.to_string_lossy().into(),
                source_kind: kind,
                files_found: Vec::new(),
                warnings: vec![error],
                can_import: false,
            });
        }
    };
    let found: Vec<_> = snapshot
        .entries
        .iter()
        .map(|entry| entry.descriptor.label.clone())
        .collect();
    let mut warnings = snapshot.warnings;
    let simulation = MemorySecretStore::default();
    let mut can_import = !found.is_empty();
    if found.is_empty() {
        warnings.push("未找到可识别的 Desktop 配置文件".into());
    }
    if let Err(error) = validate_framework_links(&snapshot.entries) {
        warnings.push(error);
        can_import = false;
    }
    for entry in snapshot.entries {
        if let Err(error) = normalize_entry(&entry.descriptor, entry.value, &simulation) {
            warnings.push(error);
            can_import = false;
        }
    }

    Ok(ConfigImportPreview {
        source_path: source.to_string_lossy().to_string(),
        source_kind: kind,
        files_found: found.clone(),
        warnings,
        can_import,
    })
}

/// 校验并归一化 app config(config.json)非对象 / 不像应用配置直接拒,绝不覆盖
/// 生产配置;通过则走 migrate_app_config 归一化到当前版本。
/// 同时返回可选的 app-settings 种子(旧 WebHook/Email 字段)。
fn normalize_app_config_import(
    value: serde_json::Value,
) -> Result<(serde_json::Value, Option<serde_json::Value>), String> {
    if !ncd_runtime::app_config_migration::looks_like_app_config(&value) {
        return Err("config.json 不像应用配置(非对象或缺少已知配置段),已中止导入".to_string());
    }
    let seed = ncd_runtime::app_config_migration::app_settings_from_legacy_config(&value);
    let app_settings_payload = if seed.has_any() {
        Some(
            serde_json::to_value(&seed.settings)
                .map_err(|e| format!("序列化 app-settings 失败: {e}"))?,
        )
    } else {
        None
    };
    let payload = ncd_runtime::app_config_migration::migrate_app_config(value).payload;
    Ok((payload, app_settings_payload))
}

/// 校验并归一化 bot config(bot.json):迁移 → 反序列化 Vec<BotConfig> → 逐个
/// validate + QQ 去重任一非法即中止,返回迁移后的强类型化 payload(而非原样透传)
fn normalize_bot_config_import(
    value: serde_json::Value,
    secrets: &dyn ncd_traits::SecretStore,
) -> Result<serde_json::Value, String> {
    use std::collections::HashSet;
    let migrated = ncd_runtime::bot_config_migration::migrate_bot_config(value, secrets)
        .map_err(|e| format!("bot.json 迁移/解析失败,已中止导入: {e}"))?;
    let bots_payload = migrated
        .payload
        .get("bots")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let bots: Vec<ncd_domain::BotConfig> = serde_json::from_value(bots_payload)
        .map_err(|e| format!("bot.json 不是合法 Bot 配置,已中止导入: {e}"))?;
    let mut seen = HashSet::new();
    for bot in &bots {
        bot.validate()
            .map_err(|e| format!("bot.json 含非法 Bot 配置,已中止导入: {e}"))?;
        bot.validate_runtime_matrix()
            .map_err(|e| format!("bot.json 含当前不支持的运行组合,已中止导入: {e}"))?;
        if !seen.insert(bot.bot.qq_id) {
            return Err(format!(
                "bot.json 含重复 QQ 号 {},已中止导入",
                bot.bot.qq_id
            ));
        }
    }
    Ok(migrated.payload)
}

/// 校验并归一化 servers.json:支持旧版 wrapper schema,最终写成当前 ServerProfile 数组
fn normalize_servers_import(value: serde_json::Value) -> Result<serde_json::Value, String> {
    let migrated = ncd_runtime::server_profile_migration::migrate_server_profiles_payload(value)
        .map_err(|e| format!("servers.json 迁移/解析失败,已中止导入: {e}"))?;
    Ok(migrated.payload)
}

/// 校验 app-settings.json:反序列化为 AppSettings 并 normalize,再写回 JSON
fn normalize_app_settings_import(value: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut settings: ncd_domain::AppSettings = serde_json::from_value(value)
        .map_err(|e| format!("app-settings.json 不是合法应用设置,已中止导入: {e}"))?;
    settings.normalize();
    serde_json::to_value(&settings).map_err(|e| format!("序列化 app-settings 失败: {e}"))
}

#[derive(Debug)]
struct PreparedImport {
    txn: ncd_traits::JsonTransaction,
    files: Vec<String>,
    skipped: Vec<String>,
    frontend_preferences: Option<ConfigFrontendPreferences>,
    framework_backups: Vec<FrameworkConfigBackup>,
}

/// Collect and validate every source before running real legacy credential migration.
fn prepare_import_transaction(
    staging: &Path,
    data_root: &Path,
    secrets: &dyn ncd_traits::SecretStore,
) -> Result<PreparedImport, String> {
    let snapshot = discover_source(staging)?;
    if snapshot.entries.is_empty() {
        return Err("来源里没有可识别的 Desktop 配置文件".into());
    }
    validate_framework_links(&snapshot.entries)?;
    let simulation = MemorySecretStore::default();
    let explicit_settings = snapshot
        .entries
        .iter()
        .any(|entry| entry.descriptor.kind == TransferKind::AppSettings);
    let mut txn = ncd_traits::JsonTransaction::new();
    let mut files = Vec::new();
    let mut pending_app_settings: Option<serde_json::Value> = None;
    let mut frontend_preferences = None;
    let mut bot_source = None;
    let mut framework_backups = Vec::new();
    for entry in snapshot.entries {
        if entry.descriptor.kind == TransferKind::BotConfig {
            bot_source = Some(entry.value.clone());
        }
        let (normalized, seed) = normalize_entry(&entry.descriptor, entry.value, &simulation)?;
        if entry.descriptor.kind == TransferKind::AppConfig {
            pending_app_settings = seed;
        }
        if entry.descriptor.kind == TransferKind::Frontend {
            frontend_preferences = Some(
                serde_json::from_value(normalized.clone())
                    .map_err(|error| format!("读取已校验浏览器偏好失败: {error}"))?,
            );
        }
        if entry.descriptor.kind == TransferKind::Framework {
            framework_backups.push(
                serde_json::from_value(normalized.clone())
                    .map_err(|e| format!("框架备份无法读取: {e}"))?,
            );
        }
        txn = txn.write(data_root.join(&entry.descriptor.data_relative), normalized);
        files.push(entry.descriptor.label);
    }

    // A source settings file is authoritative even when the target file does not exist yet.
    if let Some(settings_payload) = pending_app_settings.filter(|_| !explicit_settings) {
        let app_settings_path = data_root
            .join("config")
            .join(ncd_runtime::app_config_migration::APP_SETTINGS_FILE);
        if !app_settings_path.is_file() {
            txn = txn.write(app_settings_path, settings_payload);
            files.push("离线通知设置(app-settings)".to_string());
        }
    }

    // Reuse the already collected source, so validation cannot be bypassed by a subsequent file change.
    if let Some(value) = bot_source {
        let payload = normalize_bot_config_import(value, secrets)?;
        if let Some(write) = txn
            .writes
            .iter_mut()
            .find(|write| write.path == data_root.join("config/bot.json"))
        {
            write.payload = payload;
        }
    }
    Ok(PreparedImport {
        txn,
        files,
        skipped: snapshot.skipped,
        frontend_preferences,
        framework_backups,
    })
}

#[cfg(test)]
fn build_import_transaction(
    staging: &Path,
    data_root: &Path,
    secrets: &dyn ncd_traits::SecretStore,
) -> Result<(ncd_traits::JsonTransaction, Vec<String>, Vec<String>), String> {
    let prepared = prepare_import_transaction(staging, data_root, secrets)?;
    Ok((prepared.txn, prepared.files, prepared.skipped))
}

/// 从 ZIP 或目录导入配置:全量强类型校验通过后,一次性事务原子写回当前数据根
/// 任一文件语义非法即整体中止,绝不发生"改了一半"的半导入漂移(旧实现逐文件
/// write_json_atomic 会半成功);apply_transaction 自带备份,写失败整体回滚
#[tauri::command]
pub async fn import_config(
    app: AppHandle,
    state: State<'_, AppState>,
    source_path: String,
) -> Result<ConfigImportResult, String> {
    use ncd_runtime::LocalConfigStore;
    use ncd_traits::ConfigStore;

    state.migrate_gate.ensure_idle()?;
    let _framework_guard = state.app_manager.framework_config_transfer_guard().await;
    let source = PathBuf::from(&source_path);
    let (staging, _kind, _guard) = resolve_import_staging(&source)?;

    // Legacy password fields are normalized without importing credentials into this machine's keyring.
    let secrets = MemorySecretStore::default();
    let PreparedImport {
        mut txn,
        files,
        skipped,
        frontend_preferences,
        framework_backups,
    } = prepare_import_transaction(&staging, &state.data_root, &secrets)?;

    let mut instances: Option<Vec<ncd_domain::AppInstance>> = imported_cache_payload(
        &txn,
        &state.data_root,
        "config/app-instances.json",
        Some("instances"),
    )?;
    let current_instances = state.app_manager.list_instances().await;
    if let Some(imported) = instances.as_mut() {
        normalize_imported_instance_states(imported, &current_instances)?;
        if let Some(write) = txn
            .writes
            .iter_mut()
            .find(|w| w.path == state.data_root.join("config/app-instances.json"))
        {
            write.payload = serde_json::json!({"version": 1, "instances": imported});
        }
    }
    let targets = instances.as_deref().unwrap_or(&current_instances);
    let imported_servers: Option<Vec<ncd_runtime::ServerProfile>> =
        imported_cache_payload(&txn, &state.data_root, "config/servers.json", None)?;
    let servers = match imported_servers {
        Some(servers) => servers,
        None => state.server_manager.list_servers().await,
    };
    let framework_restore = state
        .app_manager
        .prepare_framework_config_restore(&framework_backups, targets, Some(&servers))
        .await?;
    stage_framework_recoveries(
        &mut txn,
        &state.data_root,
        &framework_backups,
        &framework_restore.restored_ids,
    )?;
    let chat_preferences = imported_cache_payload(
        &txn,
        &state.data_root,
        "config/chat-desktop.json",
        Some("accounts"),
    )?;
    let workspace =
        imported_cache_payload(&txn, &state.data_root, "onebot-debug/workspace.json", None)?;
    let collections = imported_cache_payload(
        &txn,
        &state.data_root,
        "onebot-debug/collections.json",
        None,
    )?;
    let bots = imported_cache_payload(&txn, &state.data_root, "config/bot.json", Some("bots"))?;

    // Commit while each file owner's write gate is held; failed transactions keep every cache unchanged.
    let store = LocalConfigStore::new(&state.data_root);
    let json_commit = ncd_runtime::desktop::replace_app_settings_with(
        &state.data_root,
        &state.app_settings,
        || {
            store
                .apply_transaction(txn)
                .map_err(|e| format!("写入配置失败(已回滚): {e}"))
        },
    );
    let commit = framework_restore.plan.commit_with(json_commit);
    state
        .app_manager
        .replace_instances_with(
            instances,
            state.chat.replace_preferences_with(
                chat_preferences,
                state.onebot_debug.replace_config_with(
                    workspace,
                    collections,
                    state.bot_manager.replace_bot_configs_with(bots, commit),
                ),
            ),
        )
        .await?;
    state
        .app_manager
        .framework_configs_restored(&framework_restore.restored_ids);

    if let Err(error) = app.emit("config-imported", &files) {
        tracing::warn!(%error, "failed to notify windows after configuration import");
    }

    Ok(ConfigImportResult {
        files,
        skipped,
        frontend_preferences,
        framework_pending: Some(framework_restore.pending),
    })
}

fn normalize_imported_instance_states(
    imported: &mut [ncd_domain::AppInstance],
    current: &[ncd_domain::AppInstance],
) -> Result<(), String> {
    use ncd_domain::AppInstanceState;
    for active in current.iter().filter(|i| {
        matches!(
            i.state,
            AppInstanceState::Running | AppInstanceState::Installing
        )
    }) {
        if !imported.iter().any(|i| {
            i.id == active.id
                && i.framework_id == active.framework_id
                && i.host_id == active.host_id
                && i.install_dir == active.install_dir
        }) {
            return Err(format!(
                "{} 正在运行或安装，导入会移除或改变其目录，请先停止实例或等待安装完成",
                active.display_name
            ));
        }
    }
    for instance in imported {
        if let Some(active) = current.iter().find(|i| {
            i.id == instance.id
                && i.host_id == instance.host_id
                && i.install_dir == instance.install_dir
                && matches!(
                    i.state,
                    AppInstanceState::Running | AppInstanceState::Installing
                )
        }) {
            instance.state = active.state;
        } else {
            instance.state = match instance.state {
                AppInstanceState::Running => AppInstanceState::Stopped,
                AppInstanceState::Installing => AppInstanceState::NotInstalled,
                state => state,
            };
        }
    }
    Ok(())
}

fn stage_framework_recoveries(
    txn: &mut ncd_traits::JsonTransaction,
    root: &Path,
    backups: &[FrameworkConfigBackup],
    restored: &[String],
) -> Result<(), String> {
    for backup in backups {
        let path = root.join(format!(
            "config/framework-configs/{}.json",
            backup.instance_id
        ));
        if restored.contains(&backup.instance_id) {
            txn.writes.retain(|w| w.path != path);
            txn.deletes.push(path);
            continue;
        }
        let recovery = FrameworkConfigRecovery {
            backup: backup.clone(),
            pending: true,
        };
        let value = serde_json::to_value(recovery).map_err(|e| e.to_string())?;
        if let Some(write) = txn.writes.iter_mut().find(|w| w.path == path) {
            write.payload = value;
        } else {
            txn.writes.push(ncd_traits::JsonWrite {
                path,
                payload: value,
            });
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn list_pending_framework_config_restores(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    state.app_manager.pending_framework_config_restores().await
}

#[tauri::command]
pub async fn retry_framework_config_restore(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ConfigImportResult, String> {
    use ncd_traits::ConfigStore;
    state.migrate_gate.ensure_idle()?;
    let _framework_guard = state.app_manager.framework_config_transfer_guard().await;
    let backups: Vec<_> = state
        .app_manager
        .framework_config_recoveries()
        .await?
        .into_iter()
        .filter(|r| r.pending)
        .map(|r| r.backup)
        .collect();
    let targets = state.app_manager.list_instances().await;
    let servers = state.server_manager.list_servers().await;
    let restore = state
        .app_manager
        .prepare_framework_config_restore(&backups, &targets, Some(&servers))
        .await?;
    let mut txn = ncd_traits::JsonTransaction::new();
    stage_framework_recoveries(&mut txn, &state.data_root, &backups, &restore.restored_ids)?;
    let store = ncd_runtime::LocalConfigStore::new(&state.data_root);
    restore
        .plan
        .commit_with(async {
            store
                .apply_transaction(txn)
                .map_err(|e| format!("写入框架恢复状态失败: {e}"))
        })
        .await?;
    state
        .app_manager
        .framework_configs_restored(&restore.restored_ids);
    let files: Vec<_> = restore
        .restored_ids
        .iter()
        .map(|id| format!("框架配置 ({id})"))
        .collect();
    if !files.is_empty()
        && let Err(error) = app.emit("config-imported", &files)
    {
        tracing::warn!(%error, "failed to notify windows after framework configuration restore");
    }
    Ok(ConfigImportResult {
        files,
        skipped: Vec::new(),
        frontend_preferences: None,
        framework_pending: Some(restore.pending),
    })
}

fn imported_cache_payload<T: serde::de::DeserializeOwned>(
    txn: &ncd_traits::JsonTransaction,
    data_root: &Path,
    relative: &str,
    field: Option<&str>,
) -> Result<Option<T>, String> {
    let path = data_root.join(relative);
    let Some(write) = txn.writes.iter().find(|write| write.path == path) else {
        return Ok(None);
    };
    let payload = match field {
        Some(field) => write
            .payload
            .get(field)
            .ok_or_else(|| format!("已校验配置 {relative} 缺少 {field}"))?,
        None => &write.payload,
    };
    serde_json::from_value(payload.clone())
        .map(Some)
        .map_err(|error| format!("构建导入缓存 {relative} 失败: {error}"))
}

#[cfg(test)]
#[path = "config_transfer/regression_tests.rs"]
mod regression_tests;

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

    fn valid_app_settings_json() -> String {
        serde_json::to_string(&ncd_domain::AppSettings::default()).unwrap()
    }

    #[test]
    fn build_import_transaction_validates_all_and_batches_writes() {
        let staging = tempfile::tempdir().unwrap();
        write_file(staging.path(), "config.json", VALID_CONFIG);
        write_file(staging.path(), "bot.json", VALID_BOT);
        write_file(
            staging.path(),
            "app-settings.json",
            &valid_app_settings_json(),
        );
        write_file(staging.path(), "servers.json", "[]");
        let (_d, secrets) = force_fallback_secrets();
        let data_root = tempfile::tempdir().unwrap();

        let (txn, files, skipped) =
            build_import_transaction(staging.path(), data_root.path(), &secrets).unwrap();
        // 四个文件一次性进同一个 transaction,而非逐文件落盘
        assert_eq!(txn.writes.len(), 4, "files={files:?} skipped={skipped:?}");
        assert_eq!(files.len(), 4);
        assert_eq!(skipped.len(), registry::TRANSFER_FILES.len() - 4);
    }

    #[test]
    fn build_import_transaction_aborts_when_any_file_is_semantically_invalid() {
        let staging = tempfile::tempdir().unwrap();
        // config 合法,但 servers.json 语义非法:整体必须中止,不构造事务
        write_file(staging.path(), "config.json", VALID_CONFIG);
        write_file(staging.path(), "servers.json", r#"{"not":"an array"}"#);
        let (_d, secrets) = force_fallback_secrets();
        let data_root = tempfile::tempdir().unwrap();

        let err = build_import_transaction(staging.path(), data_root.path(), &secrets).unwrap_err();
        assert!(
            err.contains("servers.json"),
            "应报 servers.json 非法: {err}"
        );
    }

    #[test]
    fn build_import_transaction_migrates_legacy_servers_json() {
        let staging = tempfile::tempdir().unwrap();
        write_file(
            staging.path(),
            "servers.json",
            r#"{
              "schema_version": 1,
              "servers": [{
                "id": "legacy-s1",
                "name": "Legacy Server",
                "credentials": {
                  "host": "10.0.0.8",
                  "port": 22022,
                  "username": "ubuntu",
                  "auth_method": "password"
                }
              }]
            }"#,
        );
        let (_d, secrets) = force_fallback_secrets();
        let data_root = tempfile::tempdir().unwrap();

        let (txn, files, skipped) =
            build_import_transaction(staging.path(), data_root.path(), &secrets).unwrap();

        assert_eq!(txn.writes.len(), 1);
        assert_eq!(files, vec!["远端服务器档案".to_string()]);
        // config / bot / app-settings 缺失
        assert_eq!(
            skipped.len(),
            registry::TRANSFER_FILES.len() - 1,
            "skipped={skipped:?}"
        );
        let payload = &txn.writes[0].payload;
        assert!(payload.is_array());
        assert_eq!(payload[0]["id"], "legacy-s1");
        assert_eq!(payload[0]["authMethod"], "password");
    }

    #[test]
    fn build_import_transaction_rejects_non_object_app_config() {
        let staging = tempfile::tempdir().unwrap();
        write_file(staging.path(), "config.json", r#"[1,2,3]"#);
        let (_d, secrets) = force_fallback_secrets();
        let data_root = tempfile::tempdir().unwrap();

        let err = build_import_transaction(staging.path(), data_root.path(), &secrets).unwrap_err();
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
        // bot / app-settings / servers 缺失
        assert_eq!(
            skipped.len(),
            registry::TRANSFER_FILES.len() - 1,
            "skipped={skipped:?}"
        );
    }
}
