use async_trait::async_trait;

use crate::bot_config::{BotConfig, BotConfigError};

#[async_trait]
pub trait BotConfigRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<BotConfig>, BotConfigError>;
    async fn get(&self, qq_id: u64) -> Result<Option<BotConfig>, BotConfigError>;
    async fn upsert(&self, config: BotConfig) -> Result<(), BotConfigError>;
    async fn delete(&self, qq_id: u64) -> Result<bool, BotConfigError>;
    async fn count(&self) -> Result<usize, BotConfigError>;
}
