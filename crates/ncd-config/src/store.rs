use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use ncd_domain::errors::ConfigError;
use ncd_domain::kinds::SchemaVersion;
use ncd_domain::migration::BackupInfo;
use ncd_domain::migration::MigrationReport;
use ncd_traits::{ConfigStore, JsonTransaction, TransactionReport};

use crate::data_paths::{DataPaths, MAX_JSON_BAK_FILES, MAX_MIGRATION_BACKUPS};

#[derive(Debug, Clone)]
pub struct LocalConfigStore {
    paths: DataPaths,
}

impl LocalConfigStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            paths: DataPaths::new(root),
        }
    }

    pub fn app_config_path(&self) -> PathBuf {
        self.paths.app_config_path()
    }

    pub fn bot_config_path(&self) -> PathBuf {
        self.paths.bot_config_path()
    }

    pub fn napcat_config_dir(&self) -> PathBuf {
        self.paths.napcat_config_dir()
    }

    pub fn paths(&self) -> &DataPaths {
        &self.paths
    }

    fn ensure_within_root(&self, path: &Path) -> Result<(), ConfigError> {
        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(ConfigError::OutsideAllowedRoots(path.display().to_string()));
        }

        let root = self.paths.root();
        let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let root_canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if target.starts_with(&root_canon) || path.starts_with(root) {
            Ok(())
        } else {
            Err(ConfigError::OutsideAllowedRoots(path.display().to_string()))
        }
    }

    fn unique_sibling(path: &Path, marker: &str) -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        path.with_file_name(format!(
            "{}.{}.{}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            marker,
            id
        ))
    }

    fn create_backup_root(&self) -> Result<PathBuf, ConfigError> {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let backup = self.backup_dir().join(format!("migration-{}", id));
        fs::create_dir_all(&backup).map_err(to_io_error)?;
        prune_migration_backups(&self.backup_dir(), MAX_MIGRATION_BACKUPS);
        Ok(backup)
    }

    fn snapshot_existing(
        &self,
        backup_root: &Path,
        path: &Path,
    ) -> Result<Option<PathBuf>, ConfigError> {
        if !path.exists() || !path.is_file() {
            return Ok(None);
        }

        let file_name = path.file_name().ok_or_else(|| {
            ConfigError::InvalidPayloadDetail(format!("missing file name: {}", path.display()))
        })?;
        let target = backup_root.join(file_name);
        fs::copy(path, &target).map_err(to_io_error)?;
        Ok(Some(target))
    }
}

impl ConfigStore for LocalConfigStore {
    fn root(&self) -> &Path {
        self.paths.root()
    }

    fn config_dir(&self) -> PathBuf {
        self.paths.config_dir()
    }

    fn backup_dir(&self) -> PathBuf {
        self.paths.migration_backup_dir()
    }

    fn migration_report_path(&self) -> PathBuf {
        self.paths.migration_report_path()
    }

    fn load_schema_version(&self) -> Result<SchemaVersion, ConfigError> {
        // 新布局优先;旧 runtime/config 仅作 schema 探测,避免收敛前误判为空
        let report_candidates = [
            self.paths.migration_report_path(),
            self.paths
                .legacy_runtime_config_dir()
                .join("migration-report.json"),
        ];
        for report in report_candidates {
            if report.exists() {
                if let Ok(payload) = self.read_json(&report) {
                    if let Some(version) = payload.get("schema_version").and_then(Value::as_u64) {
                        return Ok(SchemaVersion::new(version as u16));
                    }
                }
            }
        }

        let bot_candidates = [self.bot_config_path(), self.paths.legacy_bot_config_path()];
        let mut bot_payload = None;
        for bot in bot_candidates {
            if bot.exists() {
                bot_payload = Some(self.read_json(&bot)?);
                break;
            }
        }
        let Some(payload) = bot_payload else {
            return Ok(SchemaVersion::V1);
        };

        if payload
            .get("schemaVersion")
            .or_else(|| payload.get("schema_version"))
            .and_then(Value::as_u64)
            .is_some_and(|version| version >= SchemaVersion::V3.get() as u64)
        {
            return Ok(SchemaVersion::V3);
        }

        if payload
            .get("info")
            .and_then(|info| info.get("configVersion"))
            .is_some()
        {
            return Ok(SchemaVersion::V2);
        }

        Ok(SchemaVersion::V1)
    }

    fn read_json(&self, path: &Path) -> Result<Value, ConfigError> {
        self.ensure_within_root(path)?;
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ConfigError::NotFound(path.display().to_string()));
            }
            Err(error) => return Err(to_io_error(error)),
        };
        serde_json::from_str(&text).map_err(|error| ConfigError::Json(error.to_string()))
    }

    fn write_json_atomic(&self, path: &Path, payload: &Value) -> Result<(), ConfigError> {
        self.ensure_within_root(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(to_io_error)?;
        }

        let temp = Self::unique_sibling(path, "tmp");
        let backup = Self::unique_sibling(path, "bak");
        let bytes = serde_json::to_vec_pretty(payload)
            .map_err(|error| ConfigError::Json(error.to_string()))?;
        fs::write(&temp, bytes).map_err(to_io_error)?;

        let mut moved_to_backup = false;
        if path.exists() && !backup.exists() {
            fs::rename(path, &backup).map_err(to_io_error)?;
            moved_to_backup = true;
        }

        match fs::rename(&temp, path) {
            Ok(()) => {
                prune_json_bak_files(path, MAX_JSON_BAK_FILES);
                Ok(())
            }
            Err(error) => {
                let _ = fs::remove_file(&temp);
                if moved_to_backup && backup.exists() && !path.exists() {
                    let _ = fs::rename(&backup, path);
                }
                Err(to_io_error(error))
            }
        }
    }

    fn apply_transaction(
        &self,
        transaction: JsonTransaction,
    ) -> Result<TransactionReport, ConfigError> {
        if transaction.is_empty() {
            return Ok(TransactionReport::default());
        }

        let backup_root = self.create_backup_root()?;
        let mut backup_files = Vec::new();
        for write in &transaction.writes {
            self.ensure_within_root(&write.path)?;
            if let Some(snapshot) = self.snapshot_existing(&backup_root, &write.path)? {
                backup_files.push(snapshot);
            }
        }
        for delete in &transaction.deletes {
            self.ensure_within_root(delete)?;
            if let Some(snapshot) = self.snapshot_existing(&backup_root, delete)? {
                backup_files.push(snapshot);
            }
        }

        let mut written = Vec::new();
        for write in transaction.writes {
            if let Err(error) = self.write_json_atomic(&write.path, &write.payload) {
                restore_transaction_state(&backup_root, &written, &[]);
                return Err(error);
            }
            written.push(write.path);
        }

        let mut deleted = Vec::new();
        for delete in transaction.deletes {
            if delete.exists() {
                if let Err(error) = fs::remove_file(&delete).map_err(to_io_error) {
                    restore_transaction_state(&backup_root, &written, &deleted);
                    return Err(error);
                }
                deleted.push(delete);
            }
        }

        Ok(TransactionReport {
            backup: Some(BackupInfo {
                root: backup_root,
                files: backup_files,
            }),
            written,
            deleted,
        })
    }

    fn save_migration_report(&self, report: &MigrationReport) -> Result<(), ConfigError> {
        let mut payload =
            serde_json::to_value(report).map_err(|error| ConfigError::Json(error.to_string()))?;
        if let Value::Object(map) = &mut payload {
            map.insert(
                "schema_version".to_string(),
                Value::from(SchemaVersion::CURRENT.get()),
            );
        }

        // 内容不变则不重写,避免每次启动堆 migration-report.bak.*
        let path = self.migration_report_path();
        if path.is_file() {
            if let Ok(existing) = self.read_json(&path) {
                if existing == payload {
                    return Ok(());
                }
            }
        }

        self.write_json_atomic(&path, &payload)
    }
}

fn restore_transaction_state(backup_root: &Path, written: &[PathBuf], deleted: &[PathBuf]) {
    for path in deleted.iter().rev() {
        if let Some(file_name) = path.file_name() {
            let backup = backup_root.join(file_name);
            if backup.exists() {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::copy(&backup, path);
            }
        }
    }

    for path in written.iter().rev() {
        if let Some(file_name) = path.file_name() {
            let backup = backup_root.join(file_name);
            if backup.exists() {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::copy(&backup, path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn to_io_error(error: std::io::Error) -> ConfigError {
    ConfigError::Io(error.to_string())
}

/// 保留同文件最新 keep 个 `name.bak.*`,其余删除。
pub fn prune_json_bak_files(path: &Path, keep: usize) {
    let Some(parent) = path.parent() else {
        return;
    };
    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let prefix = format!("{file_name}.bak.");
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    let mut baks: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if !p.is_file() {
                return None;
            }
            let name = p.file_name()?.to_str()?;
            if !name.starts_with(&prefix) {
                return None;
            }
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, p))
        })
        .collect();
    if baks.len() <= keep {
        return;
    }
    baks.sort_by_key(|b| std::cmp::Reverse(b.0));
    for (_, path) in baks.into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

/// 保留最新 keep 个 migration-* 目录。
pub fn prune_migration_backups(backup_dir: &Path, keep: usize) {
    if !backup_dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(backup_dir) else {
        return;
    };
    let mut dirs: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if !p.is_dir() {
                return None;
            }
            let name = p.file_name()?.to_str()?;
            if !name.starts_with("migration-") {
                return None;
            }
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, p))
        })
        .collect();
    if dirs.len() <= keep {
        return;
    }
    dirs.sort_by_key(|b| std::cmp::Reverse(b.0));
    for (_, path) in dirs.into_iter().skip(keep) {
        let _ = fs::remove_dir_all(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_json_atomically() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let store = LocalConfigStore::new(temp.path());
        let target = store.config_dir().join("config.json");
        store
            .write_json_atomic(&target, &serde_json::json!({"a": 1}))
            .unwrap();

        let payload = store.read_json(&target).unwrap();
        assert_eq!(payload["a"], 1);
    }

    #[test]
    fn write_json_atomic_prunes_old_bak_files() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let store = LocalConfigStore::new(temp.path());
        let path = store.config_dir().join("sample.json");

        for i in 0..6 {
            store
                .write_json_atomic(&path, &serde_json::json!({ "n": i }))
                .unwrap();
        }

        let prefix = "sample.json.bak.";
        let bak_count = fs::read_dir(store.config_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with(prefix))
            })
            .count();
        assert!(bak_count <= MAX_JSON_BAK_FILES, "bak_count={bak_count}");
        assert_eq!(store.read_json(&path).unwrap()["n"], 5);
    }

    #[test]
    fn save_migration_report_skips_identical_rewrite() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let store = LocalConfigStore::new(temp.path());
        let report = MigrationReport::clean();
        store.save_migration_report(&report).unwrap();

        store.save_migration_report(&report).unwrap();
        let bak_count = fs::read_dir(store.config_dir())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("migration-report.json.bak."))
            })
            .count();
        assert_eq!(bak_count, 0);
    }
}
