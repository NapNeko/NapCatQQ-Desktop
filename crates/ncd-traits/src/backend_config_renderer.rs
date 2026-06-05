use std::path::PathBuf;

use ncd_domain::bot_config::BotConfig;
use ncd_domain::ids::BotId;
use crate::config_store::JsonTransaction;

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

    /// 渲染时合并已有派生文件中的"未知字段"。
    ///
    /// 用于"用户在派生文件里加了我们 schema 不识别的字段"的场景：默认实装直接
    /// 调 `render`，把已有内容丢弃；NapCat / SnowLuma renderer 各自重写这个方法
    /// 用 deep merge 把 `existing[path]` 里 schema 之外的字段合进来。
    ///
    /// `existing` 的 key 是 `output_paths` 返回的路径之一；不在 map 里的路径
    /// 当作"派生文件不存在"处理（直接走 render）。
    fn render_with_existing(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        existing: &std::collections::HashMap<PathBuf, serde_json::Value>,
    ) -> Result<JsonTransaction, RenderError> {
        let _ = existing;
        self.render(bot_id, config)
    }

    /// Baseline JSON for drift detection (may differ from [`Self::render`] on disk write).
    ///
    /// Default: same as `render`. SnowLuma overrides to avoid injecting install-default
    /// listeners when Desktop `connect` is empty — otherwise opening WebUI after first
    /// boot looks like a false external change.
    fn render_for_drift(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<JsonTransaction, RenderError> {
        self.render(bot_id, config)
    }

    /// List the paths that would be written/deleted for a given bot.
    /// Used by delete operations to know which derived files to clean up.
    fn output_paths(&self, bot_id: &BotId) -> Vec<PathBuf>;
}
