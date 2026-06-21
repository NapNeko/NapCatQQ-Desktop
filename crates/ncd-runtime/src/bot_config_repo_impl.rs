use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::task;

use crate::bot_config::{BotConfig, BotConfigError};
use crate::bot_config_migration::{BOT_CONFIG_COMPAT_VERSION, migrate_bot_config};
use crate::errors::ConfigError;
use crate::traits::{BotConfigRepo, ConfigStore, JsonTransaction, SecretStore};

pub struct LocalBotConfigRepo<S: ConfigStore + 'static> {
    store: Arc<S>,
    secrets: Arc<dyn SecretStore + Send + Sync>,
    /// 串行化 upsert / delete 的 read-modify-write 序列,防止并发写入互相覆盖
    /// list / get / count 是只读的,不需要持锁
    ///
    /// 注意:写锁覆盖整个 RMW 流程(含 list 内部的迁移/校验/反序列化),并发写吞吐受限
    /// 当前场景(Desktop 单用户,最多 4 Bot)足够;若未来需要更高并发,可考虑拆分
    /// "读当前快照" 和 "CAS 提交" 两步以缩小临界区
    write_lock: Mutex<()>,
}

impl<S: ConfigStore + 'static> LocalBotConfigRepo<S> {
    pub fn new(store: Arc<S>, secrets: Arc<dyn SecretStore + Send + Sync>) -> Self {
        Self {
            store,
            secrets,
            write_lock: Mutex::new(()),
        }
    }

    fn bot_path(&self) -> PathBuf {
        Self::bot_path_for(&*self.store)
    }

    fn bot_path_for(store: &S) -> PathBuf {
        store.config_dir().join("bot.json")
    }

    fn build_root_payload(bots: &[BotConfig]) -> Result<Value, BotConfigError> {
        Ok(json!({
            "info": {"configVersion": BOT_CONFIG_COMPAT_VERSION},
            "bots": serde_json::to_value(bots)?,
        }))
    }
}

#[async_trait::async_trait]
impl<S: ConfigStore + 'static> BotConfigRepo for LocalBotConfigRepo<S> {
    async fn list(&self) -> Result<Vec<BotConfig>, BotConfigError> {
        let store = Arc::clone(&self.store);
        let secrets = Arc::clone(&self.secrets);

        task::spawn_blocking(move || {
            let path = Self::bot_path_for(&*store);
            let payload = match store.read_json(&path) {
                Ok(payload) => payload,
                Err(ConfigError::NotFound(_)) => return Ok(Vec::new()),
                Err(error) => return Err(BotConfigError::from(error)),
            };

            let migrated = migrate_bot_config(payload, &*secrets)?;
            let bots_payload = migrated
                .payload
                .get("bots")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
            let bots: Vec<BotConfig> = serde_json::from_value(bots_payload)?;

            for config in &bots {
                config.validate()?;
            }

            let mut seen_ids = std::collections::HashSet::new();
            for config in &bots {
                if !seen_ids.insert(config.bot.qq_id) {
                    return Err(BotConfigError::DuplicateQqId(config.bot.qq_id));
                }
            }

            Ok(bots)
        })
        .await
        .map_err(|error| BotConfigError::Storage(error.to_string()))?
    }

    async fn get(&self, qq_id: u64) -> Result<Option<BotConfig>, BotConfigError> {
        Ok(self
            .list()
            .await?
            .into_iter()
            .find(|config| config.bot.qq_id == qq_id))
    }

    async fn upsert(&self, config: BotConfig) -> Result<(), BotConfigError> {
        config.validate()?;
        config.validate_runtime_matrix()?;

        // 持写锁覆盖整个 read-modify-write 流程,防止并发 upsert/delete 丢更新
        let _guard = self.write_lock.lock().await;

        let mut bots = self.list().await?;
        if let Some(existing) = bots
            .iter_mut()
            .find(|existing| existing.bot.qq_id == config.bot.qq_id)
        {
            *existing = config;
        } else {
            bots.push(config);
        }

        let payload = Self::build_root_payload(&bots)?;
        let path = self.bot_path();
        let store = Arc::clone(&self.store);
        let transaction = JsonTransaction::new().write(path, payload);

        task::spawn_blocking(move || {
            store.apply_transaction(transaction)?;
            Ok(())
        })
        .await
        .map_err(|error| BotConfigError::Storage(error.to_string()))?
    }

    async fn delete(&self, qq_id: u64) -> Result<bool, BotConfigError> {
        let _guard = self.write_lock.lock().await;

        let mut bots = self.list().await?;
        let original_len = bots.len();
        bots.retain(|config| config.bot.qq_id != qq_id);
        if bots.len() == original_len {
            return Ok(false);
        }

        let payload = Self::build_root_payload(&bots)?;
        let path = self.bot_path();
        let store = Arc::clone(&self.store);
        let transaction = JsonTransaction::new().write(path, payload);

        task::spawn_blocking(move || {
            store.apply_transaction(transaction)?;
            Ok(true)
        })
        .await
        .map_err(|error| BotConfigError::Storage(error.to_string()))?
    }

    async fn count(&self) -> Result<usize, BotConfigError> {
        // Reuse list() to guarantee count reflects the canonical set of bots
        // (after migration, validation, and dedup) — not the raw JSON array length.
        Ok(self.list().await?.len())
    }
}
