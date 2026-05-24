use std::path::PathBuf;

use crate::bot_config::BotConfig;
use crate::ids::BotId;
use crate::traits::config_store::JsonTransaction;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("invalid config for rendering: {0}")]
    InvalidConfig(String),
    #[error("renderer not implemented for this backend")]
    NotImplemented,
}

impl From<serde_json::Error> for RenderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}

/// Renders a `BotConfig` into backend-specific JSON files (e.g. onebot11.json, napcat.json)
/// as a `JsonTransaction` that can be atomically committed via `ConfigStore`.
pub trait BackendConfigRenderer: Send + Sync {
    /// Render the bot config into a transaction of JSON file writes.
    ///
    /// The returned `JsonTransaction` contains paths relative to the renderer's configured
    /// output directory. The caller commits the transaction via `ConfigStore::apply_transaction`.
    fn render(&self, bot_id: &BotId, config: &BotConfig) -> Result<JsonTransaction, RenderError>;

    /// List the paths that would be written/deleted for a given bot.
    /// Used by delete operations to know which derived files to clean up.
    fn output_paths(&self, bot_id: &BotId) -> Vec<PathBuf>;
}
