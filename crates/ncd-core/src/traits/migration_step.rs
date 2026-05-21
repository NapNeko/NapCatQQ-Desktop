use serde_json::Value;

use crate::errors::MigrationError;
use crate::kinds::SchemaVersion;

pub trait MigrationStep: Send + Sync {
    fn id(&self) -> &'static str;
    fn from(&self) -> SchemaVersion;
    fn to(&self) -> SchemaVersion;
    fn apply(&self, payload: &mut Value) -> Result<(), MigrationError>;
}
