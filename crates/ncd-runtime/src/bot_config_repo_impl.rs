use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::task;

use crate::bot_config_migration::{BOT_CONFIG_COMPAT_VERSION, migrate_bot_config};
use ncd_domain::bot_config::{BotConfig, BotConfigError};
use ncd_domain::errors::ConfigError;
use ncd_traits::{BotConfigRepo, ConfigStore, JsonTransaction, SecretStore};

pub struct LocalBotConfigRepo<S: ConfigStore + 'static> {
    store: Arc<S>,
    secrets: Arc<dyn SecretStore + Send + Sync>,
    /// 串行化 upsert / delete 的 read-modify-write 序列,防止并发写入互相覆盖。
    /// 写锁覆盖整个 RMW(含读盘/迁移/校验),Desktop 最多 4 Bot 足够。
    write_lock: Mutex<()>,
    /// 进程内权威快照:list/get/count 命中后不再反复读 bot.json。
    /// 仅由本 repo 的 upsert/delete 在落盘成功后更新;外部直接改盘不会自动失效
    /// (Desktop 单写者,测试若绕过 repo 写盘需新建 repo 实例)。
    cache: Mutex<Option<Vec<BotConfig>>>,
}

impl<S: ConfigStore + 'static> LocalBotConfigRepo<S> {
    pub fn new(store: Arc<S>, secrets: Arc<dyn SecretStore + Send + Sync>) -> Self {
        Self {
            store,
            secrets,
            write_lock: Mutex::new(()),
            cache: Mutex::new(None),
        }
    }

    /// 丢弃进程内缓存;下次 list/get 从盘重载。
    /// 测试若绕过 repo 直接改 bot.json,必须先调本方法,否则仍读到旧快照。
    pub async fn invalidate_cache(&self) {
        *self.cache.lock().await = None;
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

    /// 读盘 + migrate + validate。不碰 cache;调用方决定是否写入 cache。
    async fn load_from_disk(&self) -> Result<Vec<BotConfig>, BotConfigError> {
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

    /// 保证 cache 已填充。miss 时抢 write_lock 再 double-check,避免与 upsert
    /// 并发时用旧盘数据盖掉新 cache。调用方若已持 write_lock 请用
    /// [Self::fill_cache_under_write_lock]。
    async fn ensure_cache(&self) -> Result<(), BotConfigError> {
        if self.cache.lock().await.is_some() {
            return Ok(());
        }
        let _guard = self.write_lock.lock().await;
        self.fill_cache_under_write_lock().await
    }

    /// 调用方必须已持有 write_lock(tokio Mutex 不可重入)。
    /// 返回 cache 的 owned 快照供 RMW 修改;只在需要所有权时 clone 一次。
    async fn snapshot_under_write_lock(&self) -> Result<Vec<BotConfig>, BotConfigError> {
        self.fill_cache_under_write_lock().await?;
        self.cache
            .lock()
            .await
            .clone()
            .ok_or_else(|| BotConfigError::Storage("bot config cache missing after fill".into()))
    }

    async fn fill_cache_under_write_lock(&self) -> Result<(), BotConfigError> {
        if self.cache.lock().await.is_some() {
            return Ok(());
        }
        let bots = self.load_from_disk().await?;
        *self.cache.lock().await = Some(bots);
        Ok(())
    }
}

#[async_trait::async_trait]
impl<S: ConfigStore + 'static> BotConfigRepo for LocalBotConfigRepo<S> {
    async fn list(&self) -> Result<Vec<BotConfig>, BotConfigError> {
        // trait 要求 owned Vec;cache 命中时 clone 一次是 API 边界成本
        self.ensure_cache().await?;
        self.cache
            .lock()
            .await
            .clone()
            .ok_or_else(|| BotConfigError::Storage("bot config cache missing after ensure".into()))
    }

    async fn get(&self, qq_id: u64) -> Result<Option<BotConfig>, BotConfigError> {
        // 只 clone 命中的那一条,不要为 find 整表 into_iter
        self.ensure_cache().await?;
        let guard = self.cache.lock().await;
        Ok(guard
            .as_ref()
            .and_then(|bots| bots.iter().find(|c| c.bot.qq_id == qq_id).cloned()))
    }

    async fn upsert(&self, config: BotConfig) -> Result<(), BotConfigError> {
        config.validate()?;
        config.validate_runtime_matrix()?;

        // 持写锁覆盖整个 read-modify-write 流程,防止并发 upsert/delete 丢更新
        let _guard = self.write_lock.lock().await;

        let mut bots = self.snapshot_under_write_lock().await?;
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

        task::spawn_blocking(move || -> Result<(), BotConfigError> {
            store.apply_transaction(transaction)?;
            Ok(())
        })
        .await
        .map_err(|error| BotConfigError::Storage(error.to_string()))??;

        *self.cache.lock().await = Some(bots);
        Ok(())
    }

    async fn delete(&self, qq_id: u64) -> Result<bool, BotConfigError> {
        let _guard = self.write_lock.lock().await;

        let mut bots = self.snapshot_under_write_lock().await?;
        let original_len = bots.len();
        bots.retain(|config| config.bot.qq_id != qq_id);
        if bots.len() == original_len {
            return Ok(false);
        }

        let payload = Self::build_root_payload(&bots)?;
        let path = self.bot_path();
        let store = Arc::clone(&self.store);
        let transaction = JsonTransaction::new().write(path, payload);

        task::spawn_blocking(move || -> Result<(), BotConfigError> {
            store.apply_transaction(transaction)?;
            Ok(())
        })
        .await
        .map_err(|error| BotConfigError::Storage(error.to_string()))??;

        *self.cache.lock().await = Some(bots);
        Ok(true)
    }

    async fn count(&self) -> Result<usize, BotConfigError> {
        // 与 list 同一权威集(迁移/校验/去重后),但只读 len,不 clone Vec
        self.ensure_cache().await?;
        Ok(self.cache.lock().await.as_ref().map_or(0, Vec::len))
    }
}
