use std::path::{Path, PathBuf};

use serde_json::Value;

use ncd_domain::errors::ConfigError;
use ncd_domain::kinds::SchemaVersion;
use ncd_domain::models::BackupInfo;
use ncd_domain::report::MigrationReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonWrite {
    pub path: PathBuf,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct JsonTransaction {
    pub writes: Vec<JsonWrite>,
    pub deletes: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TransactionReport {
    pub backup: Option<BackupInfo>,
    pub written: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
}

impl JsonTransaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write(mut self, path: impl Into<PathBuf>, payload: Value) -> Self {
        self.writes.push(JsonWrite {
            path: path.into(),
            payload,
        });
        self
    }

    pub fn delete(mut self, path: impl Into<PathBuf>) -> Self {
        self.deletes.push(path.into());
        self
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.deletes.is_empty()
    }

    /// Merge another transaction into this one.
    /// Writes and deletes from other are appended to self.
    pub fn merge(&mut self, other: JsonTransaction) {
        self.writes.extend(other.writes);
        self.deletes.extend(other.deletes);
    }
}

pub trait ConfigStore: Send + Sync {
    fn root(&self) -> &Path;
    fn config_dir(&self) -> PathBuf;
    fn backup_dir(&self) -> PathBuf;
    fn migration_report_path(&self) -> PathBuf;
    fn load_schema_version(&self) -> Result<SchemaVersion, ConfigError>;
    fn read_json(&self, path: &Path) -> Result<Value, ConfigError>;
    fn write_json_atomic(&self, path: &Path, payload: &Value) -> Result<(), ConfigError>;
    fn apply_transaction(
        &self,
        transaction: JsonTransaction,
    ) -> Result<TransactionReport, ConfigError>;
    fn save_migration_report(&self, report: &MigrationReport) -> Result<(), ConfigError>;
}
