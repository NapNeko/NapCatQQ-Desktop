use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::sync::RwLock;

use crate::app_config::WebUiPollerSettings;
use crate::backend_config_renderer::output_paths_for_backend;
use crate::bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
use crate::bot_config::{BackendType, BotConfig, BotConfigError};
use crate::events::{BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter};
use crate::ids::BotId;
use crate::napcat_login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
use crate::napcat_webui_client::NapCatWebUiClient;
use crate::offline_notifier::OfflineNotifier;
use crate::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, StopMode,
};
use crate::runtime_launch_plan::{RuntimeLaunchPlanError, RuntimeLaunchPlanner};
use crate::traits::{BackendConfigRenderer, BotConfigRepo, ConfigStore, JsonTransaction};

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

    #[error("runtime backend error: {0}")]
    Runtime(#[from] BotBackendError),

    #[error("task join failed: {0}")]
    TaskJoinFailed(String),
}

impl From<crate::traits::RenderError> for BotManagerError {
    fn from(err: crate::traits::RenderError) -> Self {
        Self::Render(err.to_string())
    }
}

impl From<RuntimeLaunchPlanError> for BotManagerError {
    fn from(err: RuntimeLaunchPlanError) -> Self {
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
pub struct BotManager<R: BotConfigRepo + 'static, S: ConfigStore + 'static> {
    repo: Arc<R>,
    store: Arc<S>,
    renderer: Arc<dyn BackendConfigRenderer>,
    backend: Arc<dyn BotBackend>,
    launch_planner: Arc<dyn RuntimeLaunchPlanner>,
    event_bus: Arc<BroadcastEventBus>,
    actors: Arc<RwLock<HashMap<BotId, BotActorHandle>>>,
    /// per-Bot WebUI 登录轮询组件，由 `run_napcat_login_listener` 在收到
    /// `NapCatWebuiAvailable` 事件时插入；`BotProcessExited` / `delete_bot`
    /// / `shutdown_all` 时移除并 `dispose()`。
    login_pollers: Arc<RwLock<HashMap<BotId, NapCatLoginPoller>>>,
    /// NapCat WebUI HTTP 客户端依赖，可注入 mock 用于测试。
    webui_client: Arc<dyn NapCatWebUiClient>,
    /// 离线通知通道依赖，可注入 mock；默认 wiring 走 `NoopOfflineNotifier`。
    offline_notifier: Arc<dyn OfflineNotifier>,
    /// App 级 Poller 设置，热更新通过 `poller_settings.write()` 即可生效，
    /// 下次 `handle_webui_available` 创建 Poller 时读取最新值。
    poller_settings: Arc<RwLock<WebUiPollerSettings>>,
}

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> Clone for BotManager<R, S> {
    fn clone(&self) -> Self {
        Self {
            repo: Arc::clone(&self.repo),
            store: Arc::clone(&self.store),
            renderer: Arc::clone(&self.renderer),
            backend: Arc::clone(&self.backend),
            launch_planner: Arc::clone(&self.launch_planner),
            event_bus: Arc::clone(&self.event_bus),
            actors: Arc::clone(&self.actors),
            login_pollers: Arc::clone(&self.login_pollers),
            webui_client: Arc::clone(&self.webui_client),
            offline_notifier: Arc::clone(&self.offline_notifier),
            poller_settings: Arc::clone(&self.poller_settings),
        }
    }
}

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub fn new(
        repo: Arc<R>,
        store: Arc<S>,
        renderer: Arc<dyn BackendConfigRenderer>,
        backend: Arc<dyn BotBackend>,
        launch_planner: Arc<dyn RuntimeLaunchPlanner>,
        event_bus: Arc<BroadcastEventBus>,
        webui_client: Arc<dyn NapCatWebUiClient>,
        offline_notifier: Arc<dyn OfflineNotifier>,
        poller_settings: Arc<RwLock<WebUiPollerSettings>>,
    ) -> Self {
        Self {
            repo,
            store,
            renderer,
            backend,
            launch_planner,
            event_bus,
            actors: Arc::new(RwLock::new(HashMap::new())),
            login_pollers: Arc::new(RwLock::new(HashMap::new())),
            webui_client,
            offline_notifier,
            poller_settings,
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
        let config = self.get_required_bot_config(bot_id).await?;
        self.render_backend_config(bot_id, &config).await?;

        let starting = handle.request_start().await?;
        self.publish_state_change(&starting, "start_requested");

        let runtime_config = self.build_runtime_config(bot_id, &config);
        let runtime_config = match self.launch_planner.build_plan(bot_id, &config).await {
            Ok(plan) => plan.into_runtime_config(runtime_config),
            Err(err) => {
                let message = err.to_string();
                let crashed = handle.mark_crashed(message.clone()).await?;
                self.publish_state_change(&crashed, "start_failed");
                self.event_bus.publish(DomainEvent::bot_error(
                    bot_id.clone(),
                    message,
                    Some("NapCat 运行时组件缺失或 SnowLuma 启动链路未接入".to_string()),
                ));
                return Err(BotManagerError::Render(err.to_string()));
            }
        };
        match self
            .backend
            .start(&BotStartCtx {
                config: runtime_config,
            })
            .await
        {
            Ok(status) => {
                self.event_bus
                    .publish(DomainEvent::bot_status_changed(status, "runtime_start"));
                let running = handle.confirm_running().await?;
                self.publish_state_change(&running, "start_completed");
                Ok(running)
            }
            Err(err) => {
                let message = err.to_string();
                let crashed = handle.mark_crashed(message.clone()).await?;
                self.publish_state_change(&crashed, "start_failed");
                self.event_bus.publish(DomainEvent::bot_error(
                    bot_id.clone(),
                    message,
                    Some("请在运行时设置中配置 NapCat/SnowLuma 启动命令后重试".to_string()),
                ));
                Err(err.into())
            }
        }
    }

    /// 停止指定 Bot。
    pub async fn stop_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let stopping = handle.request_stop().await?;
        self.publish_state_change(&stopping, "stop_requested");

        let status = self.backend.status(bot_id.clone()).await?;
        if status.state == BotActorState::Stopped {
            let stopped = match stopping.state {
                BotActorState::Stopping => handle.confirm_stopped().await?,
                _ => stopping,
            };
            self.publish_state_change(&stopped, "stop_completed");
            return Ok(stopped);
        }

        match self.backend.stop(bot_id.clone(), StopMode::Force).await {
            Ok(()) => {
                let status = self.backend.status(bot_id.clone()).await?;
                self.event_bus
                    .publish(DomainEvent::bot_status_changed(status, "runtime_stop"));
                let stopped = handle.confirm_stopped().await?;
                self.publish_state_change(&stopped, "stop_completed");
                Ok(stopped)
            }
            Err(err) => {
                let message = err.to_string();
                let crashed = handle.mark_crashed(message.clone()).await?;
                self.publish_state_change(&crashed, "stop_failed");
                self.event_bus.publish(DomainEvent::bot_error(
                    bot_id.clone(),
                    message,
                    Some("请检查进程是否仍在运行，必要时手动结束后重试".to_string()),
                ));
                Err(err.into())
            }
        }
    }

    /// 重启指定 Bot。
    ///
    /// 6 状态分支语义：
    /// - `Running | Starting`：`actor.request_restart()`（标记 `pending_restart` 并转 `Stopping`）
    ///   → `backend.stop(Force)` → 等 actor 经 `confirm_stopped` 转入 `Starting` → `start_bot`
    /// - `Stopped | Crashed`：直接 `start_bot`
    /// - `Stopping`：`actor.request_restart()` 标 `pending_restart` → 等 actor 转入 `Starting` → `start_bot`
    /// - `Repairing`：返回 `BotManagerError::InvalidState`
    ///
    /// 设计：复用 `BotActor` 现有的 `pending_restart` 机制，**不**新增状态机分支。
    /// 错误返回给调用方；`RestartHandle::restart_bot` impl 会把
    /// 错误转为 `DomainEvent::bot_error` 发布给前端。
    pub async fn restart_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let snap = handle.snapshot();

        match snap.state {
            BotActorState::Running | BotActorState::Starting => {
                let stopping = handle.request_restart().await?;
                self.publish_state_change(&stopping, "restart_requested");
                self.backend.stop(bot_id.clone(), StopMode::Force).await?;
                self.wait_until_state(bot_id, BotActorState::Starting, Duration::from_secs(10))
                    .await?;
                self.start_bot(bot_id).await
            }
            BotActorState::Stopped | BotActorState::Crashed => self.start_bot(bot_id).await,
            BotActorState::Stopping => {
                let stopping = handle.request_restart().await?;
                self.publish_state_change(&stopping, "restart_requested");
                self.wait_until_state(bot_id, BotActorState::Starting, Duration::from_secs(10))
                    .await?;
                self.start_bot(bot_id).await
            }
            BotActorState::Repairing => Err(BotManagerError::InvalidState {
                bot_id: bot_id.clone(),
                state: snap.state,
                action: "restart",
            }),
        }
    }

    /// 监听 actor 的 `watch::Receiver` 直到进入指定 `target` 状态或超时。
    ///
    /// 用 `watch::Receiver::borrow_and_update` 先消化已有快照，再 `changed()`
    /// 等下次更新；超时返回 `BotManagerError::Render`，邮箱关闭则返回
    /// `BotManagerError::Actor(MailboxClosed)`。
    async fn wait_until_state(
        &self,
        bot_id: &BotId,
        target: BotActorState,
        timeout: Duration,
    ) -> Result<(), BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let mut rx = handle.subscribe();
        tokio::time::timeout(timeout, async {
            loop {
                if rx.borrow_and_update().state == target {
                    return Ok::<(), BotManagerError>(());
                }
                if rx.changed().await.is_err() {
                    return Err(BotManagerError::Actor(BotActorError::MailboxClosed));
                }
            }
        })
        .await
        .map_err(|_| BotManagerError::Render(format!("wait_until_state {target:?} timeout")))?
    }

    // ─── 批量操作 ─────────────────────────────────────────────────────────

    /// 批量启动。并发调度所有目标 Bot，收集成功/失败。
    pub async fn batch_start(&self, bot_ids: &[BotId]) -> Result<BatchResult, BotManagerError> {
        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };
        let mut tasks = FuturesUnordered::new();

        for bot_id in bot_ids {
            let manager = self.clone();
            let bot_id = bot_id.clone();
            let task_bot_id = bot_id.clone();
            let handle =
                tokio::spawn(async move { manager.start_bot(&task_bot_id).await.map(|_| ()) });
            tasks.push(async move {
                let outcome = match handle.await {
                    Ok(outcome) => outcome,
                    Err(err) => Err(BotManagerError::TaskJoinFailed(err.to_string())),
                };
                (bot_id, outcome)
            });
        }

        while let Some((bot_id, outcome)) = tasks.next().await {
            match outcome {
                Ok(()) => result.succeeded.push(bot_id),
                Err(err) => result.failed.push((bot_id, err)),
            }
        }

        Ok(result)
    }

    /// 批量停止。
    pub async fn batch_stop(&self, bot_ids: &[BotId]) -> Result<BatchResult, BotManagerError> {
        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };
        let mut tasks = FuturesUnordered::new();

        for bot_id in bot_ids {
            let manager = self.clone();
            let bot_id = bot_id.clone();
            let task_bot_id = bot_id.clone();
            let handle =
                tokio::spawn(async move { manager.stop_bot(&task_bot_id).await.map(|_| ()) });
            tasks.push(async move {
                let outcome = match handle.await {
                    Ok(outcome) => outcome,
                    Err(err) => Err(BotManagerError::TaskJoinFailed(err.to_string())),
                };
                (bot_id, outcome)
            });
        }

        while let Some((bot_id, outcome)) = tasks.next().await {
            match outcome {
                Ok(()) => result.succeeded.push(bot_id),
                Err(err) => result.failed.push((bot_id, err)),
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
        let mut txn = self.renderer.render(&bot_id, &config)?;
        let target_backend = config.bot.backend_type;
        let current_paths =
            output_paths_for_backend(target_backend, self.store.config_dir(), &bot_id);
        let all_paths = {
            let mut paths = self.renderer.output_paths(&bot_id);
            paths.sort();
            paths.dedup();
            paths
        };
        for path in all_paths
            .into_iter()
            .filter(|path| !current_paths.contains(path))
        {
            txn = txn.delete(path);
        }
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
            if current.state == BotActorState::Running || current.state == BotActorState::Starting {
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
    pub async fn delete_bot_config(&self, bot_id: &BotId) -> Result<(), BotManagerError> {
        self.delete_bot_internal(bot_id).await
    }

    // ─── 查询 ─────────────────────────────────────────────────────────────

    /// 获取所有 Bot 的当前快照。
    pub async fn list_snapshots(&self) -> Vec<BotActorSnapshot> {
        let actors = self.actors.read().await;
        actors.values().map(|h| h.snapshot()).collect()
    }

    /// 获取指定 Bot 的当前快照。
    pub async fn get_snapshot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        Ok(handle.snapshot())
    }

    /// 获取指定 Bot 的当前配置。
    pub async fn get_bot_config(
        &self,
        bot_id: &BotId,
    ) -> Result<Option<BotConfig>, BotManagerError> {
        let qq_id: u64 = bot_id
            .as_str()
            .parse()
            .map_err(|_| BotManagerError::BotNotFound(bot_id.clone()))?;
        self.repo.get(qq_id).await.map_err(BotManagerError::from)
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

    /// 拉取指定 Bot 的最近 `lines` 行日志快照。
    ///
    /// 返回 [`LogSnapshot`]，包含已截尾的日志行 + 总行数。供 UI 在 BotLogPage
    /// 初次开页时一次性加载历史，再叠加 `bot_log_appended` 实时事件。对齐
    /// legacy `NapCatQQProcessLog.get_log_content` 行为：本地是内存 deque
    /// 快照（进程存活期间累计的全量），进程被 stop / 重启时缓冲清零。
    pub async fn tail_log(
        &self,
        bot_id: &BotId,
        lines: usize,
    ) -> Result<crate::runtime_backend::LogSnapshot, BotManagerError> {
        // Actor 不存在直接返回空，UI 不需要为此报错。
        if !self.actors.read().await.contains_key(bot_id) {
            return Ok(crate::runtime_backend::LogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            });
        }
        let opts = crate::runtime_backend::TailOpts { lines };
        self.backend
            .tail_log(bot_id.clone(), opts)
            .await
            .map_err(BotManagerError::from)
    }

    // ─── 内部方法 ─────────────────────────────────────────────────────────

    async fn get_actor(&self, bot_id: &BotId) -> Result<BotActorHandle, BotManagerError> {
        let actors = self.actors.read().await;
        actors
            .get(bot_id)
            .cloned()
            .ok_or_else(|| BotManagerError::BotNotFound(bot_id.clone()))
    }

    async fn get_required_bot_config(&self, bot_id: &BotId) -> Result<BotConfig, BotManagerError> {
        let qq_id: u64 = bot_id
            .as_str()
            .parse()
            .map_err(|_| BotManagerError::BotNotFound(bot_id.clone()))?;
        self.repo
            .get(qq_id)
            .await?
            .ok_or_else(|| BotManagerError::BotNotFound(bot_id.clone()))
    }

    async fn render_backend_config(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<(), BotManagerError> {
        let txn = self.renderer.render(bot_id, config)?;
        if txn.is_empty() {
            return Ok(());
        }
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || store.apply_transaction(txn))
            .await
            .map_err(|e| BotManagerError::Render(e.to_string()))?
            .map_err(|e| BotManagerError::Render(e.to_string()))?;
        Ok(())
    }

    fn build_runtime_config(&self, bot_id: &BotId, config: &BotConfig) -> BotRuntimeConfig {
        BotRuntimeConfig::default_path(self.store.root(), bot_id.clone())
            .with_runtime_defaults(self.store.root())
            .with_bot_config(config)
    }

    fn publish_state_change(&self, snapshot: &BotActorSnapshot, reason: &str) {
        self.event_bus
            .publish(DomainEvent::bot_state_changed(snapshot.clone(), reason));
    }

    /// 应用退出收口：尝试停止所有运行中的 Bot 并 shutdown 它们的 Actor。
    ///
    /// 用法：Tauri `WindowEvent::CloseRequested` 时调用，避免 QQ.exe 残留。
    ///
    /// 行为：
    /// - 对所有处于 active 状态的 Bot 调用 `stop_bot`（内部走 `kill_process_tree`）。
    /// - 不论 stop 是否成功，都会 shutdown 对应的 actor 释放邮箱。
    /// - 任何错误只记录到返回值，不会阻塞其它 Bot 的清理。
    pub async fn shutdown_all(&self) -> BatchResult {
        let snapshots: Vec<BotActorSnapshot> = {
            let actors = self.actors.read().await;
            actors.values().map(|h| h.snapshot()).collect()
        };

        let active_ids: Vec<BotId> = snapshots
            .iter()
            .filter(|s| s.state.is_active() || matches!(s.state, BotActorState::Running))
            .map(|s| s.bot_id.clone())
            .collect();

        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };

        for bot_id in &active_ids {
            match self.stop_bot(bot_id).await {
                Ok(_) => result.succeeded.push(bot_id.clone()),
                Err(err) => result.failed.push((bot_id.clone(), err)),
            }
        }

        // 关闭所有 actor 邮箱，释放后台任务。
        let handles: Vec<BotActorHandle> = {
            let actors = self.actors.read().await;
            actors.values().cloned().collect()
        };
        for handle in handles {
            let _ = handle.shutdown().await;
        }
        {
            let mut actors = self.actors.write().await;
            actors.clear();
        }

        // dispose 所有 NapCatLoginPoller，取消其后台轮询任务。
        {
            let mut pollers = self.login_pollers.write().await;
            for (_, poller) in pollers.drain() {
                poller.dispose();
            }
        }

        result
    }

    /// 订阅运行时事件总线，将 `BotProcessExited` 转换为 actor 状态机转移：
    /// - 进程正常或异常退出 → 调用 `confirm_stopped` / `mark_crashed`，
    ///   防止 UI 残留假 Running。
    ///
    /// 返回的 future 由调用方在合适的运行时上 spawn（例如
    /// `tauri::async_runtime::spawn`）。它**不依赖** tokio current handle，
    /// 因此可以在 Tauri `setup` 回调里安全启动；用 `tokio::spawn` 在
    /// 没有 tokio 运行时上下文的位置直接跑会 panic。
    pub async fn run_runtime_event_listener(self) {
        let mut subscription = self.event_bus.subscribe(EventFilter::kind(
            crate::events::DomainEventKind::BotProcessExited,
        ));
        while let Some(event) = subscription.next().await {
            if let DomainEvent::BotProcessExited {
                bot_id,
                exit_code,
                reason,
            } = event
            {
                self.handle_process_exited(bot_id, exit_code, reason).await;
            }
        }
    }

    /// 在当前 tokio 运行时上 spawn 事件监听任务。
    ///
    /// 仅在调用方已处于 tokio 运行时上下文（`#[tokio::test]` 或被
    /// `tauri::async_runtime::spawn` 包过的 future）中使用；在 Tauri
    /// `setup` 这种无 tokio handle 的位置请改用：
    ///
    /// ```ignore
    /// let manager = bot_manager.clone();
    /// tauri::async_runtime::spawn(async move {
    ///     (*manager).clone().run_runtime_event_listener().await;
    /// });
    /// ```
    pub fn spawn_runtime_event_listener(&self) {
        let manager = self.clone();
        tokio::spawn(manager.run_runtime_event_listener());
    }

    async fn handle_process_exited(
        &self,
        bot_id: BotId,
        exit_code: Option<i32>,
        reason: Option<String>,
    ) {
        let handle = {
            let actors = self.actors.read().await;
            actors.get(&bot_id).cloned()
        };
        let Some(handle) = handle else {
            return;
        };

        let snapshot = handle.snapshot();
        match snapshot.state {
            // 主动停止流程：进程退出意味着 stop 完成。
            BotActorState::Stopping => {
                if let Ok(updated) = handle.confirm_stopped().await {
                    self.publish_state_change(&updated, "process_exited");
                }
            }
            // 还在运行中却收到退出事件：进程被外部 kill 或自身崩溃。
            BotActorState::Running | BotActorState::Starting => {
                let detail = match (exit_code, reason.as_deref()) {
                    (Some(code), _) if code == 0 => "process exited with code 0".to_string(),
                    (Some(code), _) => format!("process exited with code {code}"),
                    (None, Some(reason)) => format!("process terminated: {reason}"),
                    (None, None) => "process terminated by signal".to_string(),
                };
                if let Ok(updated) = handle.mark_crashed(detail.clone()).await {
                    self.publish_state_change(&updated, "process_exited");
                    self.event_bus.publish(DomainEvent::bot_error(
                        bot_id,
                        detail,
                        Some("Bot 进程已退出，请检查日志或手动重启。".to_string()),
                    ));
                }
            }
            // 已是 Stopped / Crashed / Repairing：不再做转移，避免无效转移报错。
            _ => {}
        }
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

        // 2. 再删除派生配置文件（NapCat / SnowLuma 两套路径都清理）
        let mut txn = JsonTransaction::new();
        for path in output_paths_for_backend(BackendType::NapCat, self.store.config_dir(), bot_id) {
            txn = txn.delete(path);
        }
        for path in output_paths_for_backend(BackendType::SnowLuma, self.store.config_dir(), bot_id)
        {
            txn = txn.delete(path);
        }
        if !txn.is_empty() {
            let store = Arc::clone(&self.store);
            tokio::task::spawn_blocking(move || store.apply_transaction(txn))
                .await
                .map_err(|e| BotManagerError::Render(e.to_string()))?
                .map_err(|e| BotManagerError::Render(e.to_string()))?;
        }

        // 3. 再停止和 shutdown Actor（持久化已删，失败也不会导致 "复活"）
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

        // 4. 移除内存态 Actor
        {
            let mut actors = self.actors.write().await;
            actors.remove(bot_id);
        }

        // 5. dispose 该 Bot 的 NapCatLoginPoller（若存在）。
        self.dispose_poller(bot_id).await;

        self.event_bus.publish(DomainEvent::BotStateChanged {
            snapshot: BotActorSnapshot::new(bot_id.clone()),
            reason: Some("bot_deleted".to_string()),
        });

        Ok(())
    }

    /// 处理 `NapCatWebuiAvailable` 事件：为给定 Bot 创建/替换 `NapCatLoginPoller`。
    ///
    /// 行为：
    /// - `repo.get(bot_id)` 不到对应配置时**直接 return**（不报错），避免在
    ///   配置删除后还接到延迟的 WebuiAvailable 事件时崩溃。
    /// - 从 `poller_settings.read().await` 取最新值组装 `PollerConfig`：
    ///   - `login_check_interval` ← `settings.bot_login_check_interval_ms`
    ///   - `unlogged_interval` 固定 1s
    ///   - `auth_refresh_period` 30 min；`auth_refresh_throttle` 5s；`http_timeout` 5s
    ///   - `offline_auto_restart` ← `bot_cfg.bot.offline_auto_restart`
    ///   - `offline_notice_enabled = bot_cfg.advanced.offline_notice
    ///       && (settings.offline_webhook_notice || settings.offline_email_notice)`
    /// - 旧 Poller 先 `dispose()`（取消其 `CancellationToken` 并触发 `Drop` 兜底）
    ///   再插入新实例，保证不会同时存在两个 Poller 抢同一 BotId 的事件。
    ///
    /// `restart_handle` 通过 `Arc::clone(self) as Arc<dyn RestartHandle>` 注入，
    /// 利用本类型的 `impl RestartHandle for BotManager`（见文件末尾）。
    pub async fn handle_webui_available(
        self: &Arc<Self>,
        bot_id: BotId,
        port: u16,
        token: String,
    ) {
        // 1. 取 BotConfig；解析失败或不存在时静默 return（事件可能晚到）。
        let qq_id: u64 = match bot_id.as_str().parse() {
            Ok(v) => v,
            Err(_) => return,
        };
        let bot_cfg = match self.repo.get(qq_id).await {
            Ok(Some(cfg)) => cfg,
            _ => return,
        };

        // 2. 读最新 poller settings 拼装 PollerConfig。
        let settings = self.poller_settings.read().await.clone();
        let cfg = PollerConfig {
            login_check_interval: Duration::from_millis(settings.bot_login_check_interval_ms),
            unlogged_interval: Duration::from_secs(1),
            auth_refresh_period: Duration::from_secs(30 * 60),
            auth_refresh_throttle: Duration::from_secs(5),
            http_timeout: Duration::from_secs(5),
            offline_auto_restart: bot_cfg.bot.offline_auto_restart,
            offline_notice_enabled: bot_cfg.advanced.offline_notice
                && (settings.offline_webhook_notice || settings.offline_email_notice),
        };

        // 3. 注入依赖。`restart_handle` 把 BotManager 自身作为 RestartHandle。
        let deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http: Arc::clone(&self.webui_client),
            notifier: Arc::clone(&self.offline_notifier),
            restart_handle: Arc::clone(self) as Arc<dyn RestartHandle>,
        };

        // 4. 替换旧 Poller，再插入新实例。
        let mut pollers = self.login_pollers.write().await;
        if let Some(old) = pollers.remove(&bot_id) {
            old.dispose();
        }
        let poller = NapCatLoginPoller::spawn(bot_id.clone(), port, token, cfg, deps);
        pollers.insert(bot_id, poller);
    }

    /// 移除并取消指定 Bot 的 `NapCatLoginPoller`。多次调用幂等。
    ///
    /// 由 `run_napcat_login_listener` 在 `BotProcessExited` 事件到达时调用，
    /// 也由 `delete_bot_internal` / `shutdown_all` 在生命周期收尾时调用。
    pub async fn dispose_poller(&self, bot_id: &BotId) {
        let mut pollers = self.login_pollers.write().await;
        if let Some(poller) = pollers.remove(bot_id) {
            poller.dispose();
        }
    }

    /// 监听 `NapCatWebuiAvailable` 与 `BotProcessExited` 两路事件，分别驱动
    /// Poller 的创建与回收。
    ///
    /// - `Arc<Self>` 作为 receiver：`handle_webui_available` 需要把
    ///   `Arc<BotManager<R, S>>` 转成 `Arc<dyn RestartHandle>` 注入 `PollerDeps`。
    /// - `tokio::select!` 同时消费两路 subscription；任一路关闭都会让 `else =>`
    ///   分支退出循环，避免半挂死。
    /// - 调用方（Tauri `setup` 或测试）通过 `tauri::async_runtime::spawn` /
    ///   `tokio::spawn` 启动；与 `run_runtime_event_listener` 风格一致。
    pub async fn run_napcat_login_listener(self: Arc<Self>) {
        let mut webui_sub = self
            .event_bus
            .subscribe(EventFilter::kind(DomainEventKind::NapCatWebuiAvailable));
        let mut exit_sub = self
            .event_bus
            .subscribe(EventFilter::kind(DomainEventKind::BotProcessExited));
        loop {
            tokio::select! {
                ev = webui_sub.next() => match ev {
                    Some(DomainEvent::NapCatWebuiAvailable { bot_id, port, token }) => {
                        self.handle_webui_available(bot_id, port, token).await;
                    }
                    Some(_) => continue,
                    None => break,
                },
                ev = exit_sub.next() => match ev {
                    Some(DomainEvent::BotProcessExited { bot_id, .. }) => {
                        self.dispose_poller(&bot_id).await;
                    }
                    Some(_) => continue,
                    None => break,
                },
            }
        }
    }
}
// ─── RestartHandle 实现 ────────────────────────────────────────────────────────

/// `BotManager` 实现 `RestartHandle`，让 `NapCatLoginPoller` 可以在踢线 +
/// `offline_auto_restart=true` 分支调用 `restart_bot` 而不直接持有
/// `BotManager` 引用（避免循环依赖）。
///
/// 失败处理：把错误转成 `DomainEvent::bot_error` 发布到事件总线，附中文
/// 提示「自动重启失败，请手动启动 Bot」。Poller 不感知失败。
#[async_trait]
impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> RestartHandle for BotManager<R, S> {
    async fn restart_bot(&self, bot_id: &BotId) {
        if let Err(err) = BotManager::restart_bot(self, bot_id).await {
            self.event_bus.publish(DomainEvent::bot_error(
                bot_id.clone(),
                err.to_string(),
                Some("自动重启失败，请手动启动 Bot".to_string()),
            ));
        }
    }
}
