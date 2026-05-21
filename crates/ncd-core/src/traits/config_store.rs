use crate::errors::ConfigError;
use crate::kinds::SchemaVersion;
use crate::report::MigrationReport;

pub trait ConfigStore: Send + Sync {
    fn load_schema_version(&self) -> Result<SchemaVersion, ConfigError>;
    fn save_migration_report(&self, report: &MigrationReport) -> Result<(), ConfigError>;
}
