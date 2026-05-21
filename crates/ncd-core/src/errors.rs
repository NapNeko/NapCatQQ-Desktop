use crate::kinds::SchemaVersion;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("migration error: {0}")]
    Migration(#[from] MigrationError),
    #[error("secret error: {0}")]
    Secret(#[from] SecretError),
    #[error("path error: {0}")]
    Path(#[from] PathError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid schema version: {0:?}")]
    InvalidSchemaVersion(SchemaVersion),
    #[error("invalid config payload")]
    InvalidPayload,
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("migration failed: {0}")]
    Failed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("secret store unavailable")]
    Unavailable,
    #[error("secret not found")]
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("path is outside allowed roots: {0}")]
    OutsideAllowedRoots(String),
    #[error("path is invalid: {0}")]
    Invalid(String),
}
