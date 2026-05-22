use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::errors::ConfigError;
use crate::kinds::SchemaVersion;
use crate::models::BackupInfo;
use crate::report::MigrationReport;
use crate::traits::{ConfigStore, JsonTransaction, TransactionReport};

#[derive(Debug, Clone)]
pub struct LocalConfigStore {
    root: PathBuf,
}

impl LocalConfigStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn app_config_path(&self) -> PathBuf {
        self.config_dir().join("config.json")
    }

    pub fn bot_config_path(&self) -> PathBuf {
        self.config_dir().join("bot.json")
    }

    pub fn napcat_config_dir(&self) -> PathBuf {
        self.root.join("NapCatQQ").join("config")
    }

    fn ensure_within_root(&self, path: &Path) -> Result<(), ConfigError> {
        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(ConfigError::OutsideAllowedRoots(path.display().to_string()));
        }

        let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let root = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());
        if target.starts_with(&root) || path.starts_with(&self.root) {
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
        &self.root
    }

    fn config_dir(&self) -> PathBuf {
        self.root.join("runtime").join("config")
    }

    fn backup_dir(&self) -> PathBuf {
        self.root
            .join("runtime")
            .join("tmp")
            .join("migration-backup")
    }

    fn migration_report_path(&self) -> PathBuf {
        self.config_dir().join("migration-report.json")
    }

    fn load_schema_version(&self) -> Result<SchemaVersion, ConfigError> {
        let report = self.config_dir().join("migration-report.json");
        if report.exists() {
            if let Ok(payload) = self.read_json(&report) {
                if let Some(version) = payload.get("schema_version").and_then(Value::as_u64) {
                    return Ok(SchemaVersion::new(version as u16));
                }
            }
        }

        let bot = self.bot_config_path();
        if !bot.exists() {
            return Ok(SchemaVersion::V1);
        }

        let payload = self.read_json(&bot)?;
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
            Ok(()) => Ok(()),
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
        self.write_json_atomic(&self.migration_report_path(), &payload)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_json_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let store = LocalConfigStore::new(temp.path());
        let target = store.config_dir().join("config.json");
        store
            .write_json_atomic(&target, &serde_json::json!({"a": 1}))
            .unwrap();

        let payload = store.read_json(&target).unwrap();
        assert_eq!(payload["a"], 1);
    }
}
