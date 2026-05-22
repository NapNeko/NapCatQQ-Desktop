use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
use crate::bot_config::{BotConfig, BotConfigError};
use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::ids::BotId;
use crate::traits::{BackendConfigRenderer, BotConfigRepo, ConfigStore};

// ─── 常量 ──────────────────────────────────────────────────────────────────────

/// Desktop 单机上限：最多同时托管 4 个 Bot。
const MAX_BOTS: usize = 4;

// ─── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum BotManagerError {
    #[error("bot {0} not found")]
    BotNotFound(BotId),

    #[error("bot {0} already exists")]
    BotAlreadyExists(BotId),

    #[error("bot limit reached (max {MAX_BOTS})")]
    BotLimitReached,

    #[error("bot {bot_id} is in state {state:?}, cannot {action}")]
    InvalidState {
        bot_id: BotId,
        state: BotActorState,
        action: &'static str,
    },

    #[error(transparent)]
    Actor(#[from] BotActorError),

    #[error(transparent)]
    Config(#[from] BotConfigError),

    #[error("renderer error: {0}")]
    Render(String),

    #[error("task join failed: {0}")]
    TaskJoinFailed(String),
}

impl From<crate::traits::RenderError> for BotManagerError {
    fn from(err: crate::traits::RenderError) -> Self {
        Self::Render(err.to_string())
    }
}

// ─── Batch 结果 ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct BatchResult {
    pub succeeded: Vec<BotId>,
    pub failed: Vec<(BotId, BotManagerError)>,
}

#[derive(Debug)]
pub struct BootstrapResult {
    pub started: BatchResult,
    pub skipped: Vec<BotId>,
}

// ─── BotManager ────────────────────────────────────────────────────────────────

/// 编排层：统一管理所有 Bot 的生命周期。
///
/// - 每个 Bot 对应一个 `BotActorHandle`（状态机）。
/// - `BotConfigRepo` 负责持久化配置。
/// - `BackendConfigRenderer` 负责生成后端运行时配置文件。
/// - `BroadcastEventBus` 负责事件广播给前端。
///
/// `BotManager` 自身不持有可变业务状态，所有可变状态都封装在
/// `actors` map（由 `RwLock` 保护）和各 `BotActorHandle` 内部。
pub struct BotManager<R: BotConfigRepo, S: ConfigStore + 'static> {
    repo: Arc<R>,
    store: Arc<S>,
    renderer: Arc<dyn BackendConfigRenderer>,
    event_bus: Arc<BroadcastEventBus>,
    actors: RwLock<HashMap<BotId, BotActorHandle>>,
}

impl<R: BotConfigRepo, S: ConfigStore + 'static> BotManager<R, S> {
    pub fn new(
        repo: Arc<R>,
        store: Arc<S>,
        renderer: Arc<dyn BackendConfigRenderer>,
        event_bus: Arc<BroadcastEventBus>,
    ) -> Self {
        Self {
            repo,
            store,
            renderer,
            event_bus,
            actors: RwLock::new(HashMap::new()),
        }
    }

    // ─── bootstrap ─────────────────────────────────────────────────────────

    /// 启动时从持久化配置恢复所有 Bot Actor，并自动启动标记了 `auto_start` 的 Bot。
    ///
    /// 返回 `BootstrapResult`，其中 `skipped` 包含超出 4 开上限而未注册的 Bot ID。
    pub async fn bootstrap(&self) -> Result<BootstrapResult, BotManagerError> {
        let configs = self.repo.list().await?;

        let mut skipped: Vec<BotId> = Vec::new();

        // 恢复 Actor（不超过 MAX_BOTS），超出的记入 skipped
        {
            let mut actors = self.actors.write().await;
            for config in &configs {
                let bot_id = BotId::new(config.bot.qq_id.to_string());
                if actors.len() >= MAX_BOTS {
                    skipped.push(bot_id);
                    continue;
                }
                if !actors.contains_key(&bot_id) {
                    let handle = BotActorHandle::spawn(bot_id.clone());
                    actors.insert(bot_id, handle);
                }
            }
        }

        // 自动启动（只针对已注册的 actor，skipped 的不会被启动）
        let auto_start_ids: Vec<BotId> = configs
            .iter()
            .filter(|c| c.advanced.auto_start)
            .map(|c| BotId::new(c.bot.qq_id.to_string()))
            .filter(|id| !skipped.contains(id))
            .collect();

        let started = if auto_start_ids.is_empty() {
            BatchResult {
                succeeded: Vec::new(),
                failed: Vec::new(),
            }
        } else {
            self.batch_start(&auto_start_ids).await?
        };

        Ok(BootstrapResult { started, skipped })
    }

    // ─── 单 Bot 操作 ──────────────────────────────────────────────────────

    /// 启动指定 Bot。
    ///
    /// 前置条件：Actor 已存在且处于可启动状态（Stopped / Crashed）。
    pub async fn start_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let snapshot = handle.request_start().await?;
        self.publish_state_change(&snapshot, "start_requested");
        Ok(snapshot)
    }

    /// 停止指定 Bot。
    pub async fn stop_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let snapshot = handle.request_stop().await?;
        self.publish_state_change(&snapshot, "stop_requested");
        Ok(snapshot)
    }

    // ─── 批量操作 ─────────────────────────────────────────────────────────

    /// 批量启动。并发调度所有目标 Bot，收集成功/失败。
    pub async fn batch_start(&self, bot_ids: &[BotId]) -> Result<BatchResult, BotManagerError> {
        let mut handles = Vec::with_capacity(bot_ids.len());

        {
            let actors = self.actors.read().await;
            for bot_id in bot_ids {
                if let Some(actor) = actors.get(bot_id) {
                    handles.push((bot_id.clone(), actor.clone()));
                }
            }
        }

        let mut tasks = Vec::with_capacity(handles.len());
        for (bot_id, actor) in handles {
            let event_bus = Arc::clone(&self.event_bus);
            let id_for_join = bot_id.clone();
            tasks.push((id_for_join, tokio::spawn(async move {
                match actor.request_start().await {
                    Ok(snapshot) => {
                        event_bus.publish(DomainEvent::bot_state_changed(
                            snapshot.clone(),
                            "batch_start",
                        ));
                        Ok(bot_id)
                    }
                    Err(err) => Err((bot_id, BotManagerError::from(err))),
                }
            })));
        }

        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };

        for (fallback_id, task) in tasks {
            match task.await {
                Ok(Ok(bot_id)) => result.succeeded.push(bot_id),
                Ok(Err((bot_id, err))) => result.failed.push((bot_id, err)),
                Err(join_err) => {
                    result.failed.push((
                        fallback_id,
                        BotManagerError::TaskJoinFailed(join_err.to_string()),
                    ));
                }
            }
        }

        Ok(result)
    }

    /// 批量停止。
    pub async fn batch_stop(&self, bot_ids: &[BotId]) -> Result<BatchResult, BotManagerError> {
        let mut handles = Vec::with_capacity(bot_ids.len());

        {
            let actors = self.actors.read().await;
            for bot_id in bot_ids {
                if let Some(actor) = actors.get(bot_id) {
                    handles.push((bot_id.clone(), actor.clone()));
                }
            }
        }

        let mut tasks = Vec::with_capacity(handles.len());
        for (bot_id, actor) in handles {
            let event_bus = Arc::clone(&self.event_bus);
            let id_for_join = bot_id.clone();
            tasks.push((id_for_join, tokio::spawn(async move {
                match actor.request_stop().await {
                    Ok(snapshot) => {
                        event_bus.publish(DomainEvent::bot_state_changed(
                            snapshot.clone(),
                            "batch_stop",
                        ));
                        Ok(bot_id)
                    }
                    Err(err) => Err((bot_id, BotManagerError::from(err))),
                }
            })));
        }

        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };

        for (fallback_id, task) in tasks {
            match task.await {
                Ok(Ok(bot_id)) => result.succeeded.push(bot_id),
                Ok(Err((bot_id, err))) => result.failed.push((bot_id, err)),
                Err(join_err) => {
                    result.failed.push((
                        fallback_id,
                        BotManagerError::TaskJoinFailed(join_err.to_string()),
                    ));
                }
            }
        }

        Ok(result)
    }

    /// 批量删除：先停止运行中的 Bot，再 shutdown Actor，最后删除持久化配置。
    pub async fn batch_delete(&self, bot_ids: &[BotId]) -> Result<BatchResult, BotManagerError> {
        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };

        for bot_id in bot_ids {
            match self.delete_bot_internal(bot_id).await {
                Ok(()) => result.succeeded.push(bot_id.clone()),
                Err(err) => result.failed.push((bot_id.clone(), err)),
            }
        }

        Ok(result)
    }

    // ─── 配置管理 ─────────────────────────────────────────────────────────

    /// 新增或更新 Bot 配置。
    ///
    /// 策略：**先持久化 bot.json（source of truth），再写派生文件**。
    /// - 如果 bot.json 写入失败，派生文件不会被写入，状态完全未变。
    /// - 如果派生文件写入失败，bot.json 已是最新，派生文件可在下次启动时重新生成，
    ///   不会造成不可恢复的不一致。
    ///
    /// - 新增时：检查 4 开上限，持久化，写派生文件，创建 Actor。
    /// - 更新时：持久化，写派生文件，热推送（通过 restart 通知 Actor 重新加载）。
    pub async fn upsert_bot_config(
        &self,
        config: BotConfig,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        let is_new = {
            let actors = self.actors.read().await;
            !actors.contains_key(&bot_id)
        };

        if is_new {
            let current_count = {
                let actors = self.actors.read().await;
                actors.len()
            };
            if current_count >= MAX_BOTS {
                return Err(BotManagerError::BotLimitReached);
            }
        }

        // 1. 先持久化 bot.json（source of truth）
        self.repo.upsert(config.clone()).await?;

        // 2. 再渲染并写入派生配置文件（失败不会造成不可恢复的不一致）
        let txn = self.renderer.render(&bot_id, &config)?;
        if !txn.is_empty() {
            let store = Arc::clone(&self.store);
            tokio::task::spawn_blocking(move || store.apply_transaction(txn))
                .await
                .map_err(|e| BotManagerError::Render(e.to_string()))?
                .map_err(|e| BotManagerError::Render(e.to_string()))?;
        }

        // 3. 创建或热推送 Actor
        if is_new {
            let handle = BotActorHandle::spawn(bot_id.clone());
            let snapshot = handle.snapshot();
            {
                let mut actors = self.actors.write().await;
                actors.insert(bot_id, handle);
            }
            self.publish_state_change(&snapshot, "bot_created");
            Ok(snapshot)
        } else {
            let handle = self.get_actor(&bot_id).await?;
            let current = handle.snapshot();
            if current.state == BotActorState::Running
                || current.state == BotActorState::Starting
            {
                let snapshot = handle.request_restart().await?;
                self.publish_state_change(&snapshot, "config_hot_reload");
                Ok(snapshot)
            } else {
                self.event_bus.publish(DomainEvent::bot_state_changed(
                    current.clone(),
                    "config_updated",
                ));
                Ok(current)
            }
        }
    }

    /// 删除 Bot 配置及其 Actor。如果 Bot 正在运行，先停止。
    pub async fn delete_bot_config(
        &self,
        bot_id: &BotId,
    ) -> Result<(), BotManagerError> {
        self.delete_bot_internal(bot_id).await
    }

    // ─── 查询 ─────────────────────────────────────────────────────────────

    /// 获取所有 Bot 的当前快照。
    pub async fn list_snapshots(&self) -> Vec<BotActorSnapshot> {
        let actors = self.actors.read().await;
        actors.values().map(|h| h.snapshot()).collect()
    }

    /// 获取指定 Bot 的当前快照。
    pub async fn get_snapshot(
        &self,
        bot_id: &BotId,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        Ok(handle.snapshot())
    }

    /// 当前托管的 Bot 数量。
    pub async fn bot_count(&self) -> usize {
        self.actors.read().await.len()
    }

    /// 当前活跃（Starting / Running / Stopping）的 Bot 数量。
    pub async fn active_count(&self) -> usize {
        let actors = self.actors.read().await;
        actors
            .values()
            .filter(|h| h.snapshot().state.is_active())
            .count()
    }

    // ─── 内部方法 ─────────────────────────────────────────────────────────

    async fn get_actor(&self, bot_id: &BotId) -> Result<BotActorHandle, BotManagerError> {
        let actors = self.actors.read().await;
        actors
            .get(bot_id)
            .cloned()
            .ok_or_else(|| BotManagerError::BotNotFound(bot_id.clone()))
    }

    fn publish_state_change(&self, snapshot: &BotActorSnapshot, reason: &str) {
        self.event_bus
            .publish(DomainEvent::bot_state_changed(snapshot.clone(), reason));
    }

    /// 内部删除流程：持久化删除 → 停止 → shutdown → 移除内存 Actor。
    ///
    /// 策略：**先删持久化（source of truth），再清理内存**。
    /// - 如果 repo.delete 失败，Actor 保持不变，可重试。
    /// - 如果 repo.delete 成功但 shutdown 失败，持久化已删除，
    ///   下次 bootstrap 不会恢复此 Bot，内存态在进程结束时自然清理。
    async fn delete_bot_internal(&self, bot_id: &BotId) -> Result<(), BotManagerError> {
        // 1. 先删持久化配置（source of truth）
        let qq_id: u64 = bot_id
            .as_str()
            .parse()
            .map_err(|_| BotManagerError::BotNotFound(bot_id.clone()))?;

        self.repo.delete(qq_id).await?;

        // 2. 再停止和 shutdown Actor（持久化已删，失败也不会导致 "复活"）
        let maybe_handle = {
            let actors = self.actors.read().await;
            actors.get(bot_id).cloned()
        };

        if let Some(handle) = maybe_handle {
            let current = handle.snapshot();
            if current.state.is_active() {
                let _ = handle.request_stop().await;
            }
            let _ = handle.shutdown().await;
        }

        // 3. 最后移除内存态 Actor
        {
            let mut actors = self.actors.write().await;
            actors.remove(bot_id);
        }

        self.event_bus.publish(DomainEvent::BotStateChanged {
            snapshot: BotActorSnapshot::new(bot_id.clone()),
            reason: Some("bot_deleted".to_string()),
        });

        Ok(())
    }
}
