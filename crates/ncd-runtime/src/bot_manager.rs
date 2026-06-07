use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{FuturesUnordered, StreamExt};
use rand::RngCore;
use tokio::sync::RwLock;

use crate::app_config::WebUiPollerSettings;
use crate::backend_config_renderer::output_paths_for_backend;
use crate::bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
use crate::bot_config::{BackendType, BotConfig, BotConfigError};
use crate::events::{BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter};
use crate::ids::BotId;
use crate::kinds::BotFlavor;
use crate::napcat::endpoint_table::{NapCatEndpoint, NapCatEndpointTable};
use crate::napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
use crate::napcat::offline_notifier::OfflineNotifier;
use crate::napcat::webui_client::NapCatWebUiClient;
use crate::native_deployment_adapter::DockerDeploymentBackend;
use crate::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, StopMode,
};
use crate::runtime_launch_plan::{RuntimeLaunchPlanError, RuntimeLaunchPlanner};
use crate::traits::{
    BackendConfigRenderer, BotConfigRepo, ConfigStore, JsonTransaction, SecretStore,
};
use ncd_domain::{DeploymentType, RuntimeTarget};

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
/// - 每个 Bot 对应一个 `BotActorHandle`（状态机）。
/// - `BotConfigRepo` 负责持久化配置。
/// - `BackendConfigRenderer` 负责生成后端运行时配置文件。
/// - `BroadcastEventBus` 负责事件广播给前端。
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
    /// / `shutdown_all` 时移除并 `dispose`。
    login_pollers: Arc<RwLock<HashMap<BotId, NapCatLoginPoller>>>,
    /// per-Bot NapCat WebUI 端点 (port + token) 的内存表。
    /// NapCat 多 bot 启动时端口会自动 +1，webui.json 不能区分，必须从每个 bot
    /// 自己的 stdout 抓 `WebUi User Panel Url`。本表的写/删时机与 `login_pollers`
    /// 完全对齐，供配置热推送在保存配置时反查 (port, token)。
    napcat_endpoints: NapCatEndpointTable,
    /// NapCat WebUI HTTP 客户端依赖，可注入 mock 用于测试。
    webui_client: Arc<dyn NapCatWebUiClient>,
    /// 离线通知通道依赖，可注入 mock；默认 wiring 走 `NoopOfflineNotifier`。
    offline_notifier: Arc<dyn OfflineNotifier>,
    /// App 级 Poller 设置，热更新通过 `poller_settings.write()` 即可生效
    /// 下次 `handle_webui_available` 创建 Poller 时读取最新值。
    poller_settings: Arc<RwLock<WebUiPollerSettings>>,
    /// SnowLuma flavor backend（可选）。`None` 时所有 bot 走 `backend`（NapCat 路径）。
    /// 由 wiring 阶段构造并通过 `with_snowluma_backend` 注入。
    snowluma_backend: Option<Arc<dyn BotBackend>>,
    /// SnowLuma 全局 daemon 句柄（可选），用于 `shutdown_all` 关闭 daemon、
    /// `run_snowluma_listener` 监听 daemon Crashed 级联级 actor。
    snowluma_daemon: Option<Arc<crate::snowluma::SnowLumaDaemon>>,
    /// 把 BotConfig 的 runtime_target 解析成 host（本机 / 远端 SSH）。
    /// None 时走旧路径（backend 自带的本机 host，行为同历史版本）；生产侧由
    /// `with_host_resolver` 注入 TauriHostResolver 后,启动时按 target 取 host。
    host_resolver: Option<Arc<dyn crate::host_resolver::HostResolver>>,
    /// Docker NapCat WebUI token 的凭据存储。DockerDeployment 要求上层显式传入
    /// token，不能从 QQ 号或容器名派生；这里按 Bot 持久化，保证重启后 token 稳定。
    docker_webui_secret_store: Option<Arc<dyn SecretStore + Send + Sync>>,
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
            napcat_endpoints: self.napcat_endpoints.clone(),
            webui_client: Arc::clone(&self.webui_client),
            offline_notifier: Arc::clone(&self.offline_notifier),
            poller_settings: Arc::clone(&self.poller_settings),
            snowluma_backend: self.snowluma_backend.clone(),
            snowluma_daemon: self.snowluma_daemon.clone(),
            host_resolver: self.host_resolver.clone(),
            docker_webui_secret_store: self.docker_webui_secret_store.clone(),
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
            napcat_endpoints: NapCatEndpointTable::new(),
            webui_client,
            offline_notifier,
            poller_settings,
            snowluma_backend: None,
            snowluma_daemon: None,
            host_resolver: None,
            docker_webui_secret_store: None,
        }
    }

    /// 注入 HostResolver（wiring 阶段调用），让启动时按 runtime_target 选本机/远端 host。
    /// 不注入时走旧路径（全本机）。链式 builder 风格。
    pub fn with_host_resolver(
        mut self,
        resolver: Arc<dyn crate::host_resolver::HostResolver>,
    ) -> Self {
        self.host_resolver = Some(resolver);
        self
    }

    /// 注入 Docker WebUI token 的 SecretStore。DockerDeployment 不生成生产默认 token，
    /// runtime 层负责按 Bot 持久化后显式传入 deploy 层。
    pub fn with_docker_webui_secret_store(
        mut self,
        store: Arc<dyn SecretStore + Send + Sync>,
    ) -> Self {
        self.docker_webui_secret_store = Some(store);
        self
    }

    /// 注入 SnowLuma flavor 路由依赖（ wiring 阶段调用）。
    /// 链式 builder 风格，便于 setup 代码组装。
    pub fn with_snowluma(
        mut self,
        backend: Arc<dyn BotBackend>,
        daemon: Arc<crate::snowluma::SnowLumaDaemon>,
    ) -> Self {
        self.snowluma_backend = Some(backend);
        self.snowluma_daemon = Some(daemon);
        self
    }

    /// 热更新 App 级轮询设置。运行中的 Poller 在下次创建（启动 / 重启）时
    /// 从 `poller_settings` 读最新值；已在跑的 Poller 不强制重建，避免抖动。
    /// 设置页 `set_app_settings` 写盘后调用此方法让内存值同步。
    pub async fn update_poller_settings(&self, settings: WebUiPollerSettings) {
        *self.poller_settings.write().await = settings;
    }

    /// 按 flavor 选择 backend：`SnowLuma` 时优先用注入的 SL backend，否则
    /// 回落到默认 `backend`（向后兼容：未注入 SL 时与历史行为一致）。
    fn backend_for(&self, flavor: BotFlavor) -> Arc<dyn BotBackend> {
        match flavor {
            BotFlavor::SnowLuma => self
                .snowluma_backend
                .clone()
                .unwrap_or_else(|| Arc::clone(&self.backend)),
            _ => Arc::clone(&self.backend),
        }
    }

    fn docker_webui_secret_key(qq_id: u64) -> String {
        format!("bot:{qq_id}:napcat_docker_webui_token")
    }

    fn generate_docker_webui_token() -> String {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    fn get_or_create_docker_webui_token(&self, qq_id: u64) -> Result<String, BotManagerError> {
        Self::get_or_create_docker_webui_token_from_store(
            self.docker_webui_secret_store.as_ref(),
            qq_id,
        )
    }

    /// stop / restart / delete 必须按完整 BotConfig 路由 backend；`backend_for_config`
    /// 失败时不得静默回落本机 baked backend，否则远端 Docker 会假停/假删。
    async fn backend_for_lifecycle(
        &self,
        config: &BotConfig,
    ) -> Result<Arc<dyn BotBackend>, BotManagerError> {
        self.backend_for_config(config).await
    }

    fn get_or_create_docker_webui_token_from_store(
        store: Option<&Arc<dyn SecretStore + Send + Sync>>,
        qq_id: u64,
    ) -> Result<String, BotManagerError> {
        let store = store.ok_or_else(|| {
            BotManagerError::Render("Docker 部署需要 WebUI token secret store".to_string())
        })?;
        let key = Self::docker_webui_secret_key(qq_id);
        if let Some(existing) = store
            .get(&key)
            .map_err(|e| BotManagerError::Render(e.to_string()))?
        {
            let trimmed = existing.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        let token = Self::generate_docker_webui_token();
        store
            .put(&key, &token)
            .map_err(|e| BotManagerError::Render(e.to_string()))?;
        Ok(token)
    }

    /// 按 BotConfig 的 deployment_type + runtime_target 选/造 backend。
    ///
    /// 路由矩阵:
    /// - Docker: 按 runtime_target 解析 host(本机 Docker Desktop / 远端 SSH),
    ///   现造 DockerDeploymentBackend。这是远端 + 容器化的真正可用路径。
    /// - Native + Local: 走现有 baked backend(零行为变化,最低风险)。
    /// - Native + Server(id): 远端原生启动需要远端 runtime 路径规划,launch_planner
    ///   当前只产本机路径,贸然跑会用错路径。明确报错引导用户改用 Docker 方式,
    ///   而不是静默跑错(诚实降级,非 bug)。
    ///
    /// 无 host_resolver 注入时(过渡期/测试)一律回落 baked backend,行为同历史。
    async fn backend_for_config(
        &self,
        config: &BotConfig,
    ) -> Result<Arc<dyn BotBackend>, BotManagerError> {
        let flavor = map_backend_flavor(config.bot.backend_type);
        let resolver = match &self.host_resolver {
            Some(r) => r,
            // 没注入 resolver:维持历史行为(全本机 native)。
            None => return Ok(self.backend_for(flavor)),
        };

        match config.bot.deployment_type {
            DeploymentType::Docker => {
                if flavor != BotFlavor::NapCat {
                    return Err(BotManagerError::Render(
                        "Docker 部署当前仅支持 NapCat 底座,SnowLuma 容器化待后续支持".to_string(),
                    ));
                }
                // 本机(Windows)不支持 Docker:Docker Desktop 安装链路太麻烦,产品上
                // 不支持本机容器化。Docker 只允许配合远端 SSH 主机。前端已挡,这里
                // 做后端兜底防御。
                if config.bot.runtime_target.is_local() {
                    return Err(BotManagerError::Render(
                        "本机暂不支持 Docker 部署。请将启动方式改为「直接运行」,\
                         或把运行宿主切换为远程 SSH 主机后再用 Docker。"
                            .to_string(),
                    ));
                }
                let host = resolver
                    .resolve(&config.bot.runtime_target)
                    .await
                    .map_err(BotManagerError::Render)?;
                let token = self.get_or_create_docker_webui_token(config.bot.qq_id)?;
                let deployment = Arc::new(ncd_deploy::DockerDeployment::with_webui_token(token));
                let backend_id = BotId::new(format!("docker-{}", config.bot.qq_id));
                Ok(Arc::new(DockerDeploymentBackend::new(
                    deployment, host, backend_id, flavor,
                )))
            }
            DeploymentType::Native => match &config.bot.runtime_target {
                RuntimeTarget::Local => Ok(self.backend_for(flavor)),
                RuntimeTarget::Server(_) => Err(BotManagerError::Render(
                    "远端原生启动暂未支持(需远端 runtime 安装与路径规划)。\
                     请将启动方式改为 Docker,即可在远端以容器运行。"
                        .to_string(),
                )),
            },
        }
    }

    // ─── bootstrap ─────────────────────────────────────────────────────────

    /// 启动时从持久化配置恢复所有 Bot Actor，并自动启动标记了 `auto_start` 的 Bot。
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

    /// 启动前检测派生配置文件 drift。
    ///
    /// 返回 `None`：派生文件不存在或跟 BotConfig 完全一致,可以直接启动。
    /// 返回 `Some(drift)`：有差异,前端应弹 ConfigDriftDialog 让用户抉择。
    pub async fn detect_config_drift(
        &self,
        bot_id: &BotId,
    ) -> Result<Option<crate::config_drift::ConfigDrift>, BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        let drift = crate::config_drift::detect_drift(bot_id, &config, self.renderer.as_ref())
            .await
            .map_err(|e| BotManagerError::Render(e.to_string()))?;
        if drift.is_clean() {
            Ok(None)
        } else {
            Ok(Some(drift))
        }
    }

    /// 带用户决议启动 Bot。
    ///
    /// decisions 来自前端 ConfigDriftDialog,包含：
    /// - `AcceptExternal { file, path, value }`：渲染输出时把对应 path 覆盖为外部值
    /// - `DropAdded { file, path }`：不保留新增字段（覆盖时这些字段不出现在 existing 里即可）
    /// - `KeepAdded` / `UseInternal`：无需特别处理(默认行为)
    pub async fn start_bot_with_decisions(
        &self,
        bot_id: &BotId,
        decisions: &[crate::config_drift::DriftDecision],
    ) -> Result<BotActorSnapshot, BotManagerError> {
        use crate::config_drift::DriftDecision;

        // 把 AcceptExternal 的决议转成 overrides map
        let mut overrides: std::collections::HashMap<String, Vec<(String, serde_json::Value)>> =
            std::collections::HashMap::new();
        for d in decisions {
            if let DriftDecision::AcceptExternal { file, path, value } = d {
                overrides
                    .entry(file.clone())
                    .or_default()
                    .push((path.clone(), value.clone()));
            }
        }

        // DropAdded 的决议：从 existing 文件里删掉对应 key 再渲染。这里不做
        // 额外处理——因为 render_with_existing 只合并 existing 里在 known_keys
        // **之外**的顶层字段。如果要 drop 某个顶层扩展字段,需要从 existing map
        // 里主动 remove。但当前 render_backend_config 是从磁盘现读的 existing,
        // 要 drop 的字段仍然在磁盘文件里。
        //
        // 处理方式：把 DropAdded 转成"用 null 覆盖"→ set_value_at_dot_path 遇到
        // null 会 remove 该 key,效果等价于"用户选择丢弃"。
        for d in decisions {
            if let DriftDecision::DropAdded { file, path } = d {
                overrides
                    .entry(file.clone())
                    .or_default()
                    .push((path.clone(), serde_json::Value::Null));
            }
        }

        let handle = self.get_actor(bot_id).await?;
        let config = self.get_required_bot_config(bot_id).await?;
        self.render_backend_config(bot_id, &config, &overrides)
            .await?;

        let (starting, advanced) = handle.request_start_transition().await?;
        if !advanced {
            return Ok(starting);
        }
        self.publish_state_change(&starting, "start_requested");
        self.start_runtime_from_starting(bot_id, &handle, &config)
            .await
    }

    /// 启动指定 Bot（无 drift 决议版本,等价于全部 UseInternal）。
    /// 前置条件：Actor 已存在且处于可启动状态（Stopped / Crashed）。
    pub async fn start_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let config = self.get_required_bot_config(bot_id).await?;
        self.render_backend_config(bot_id, &config, &std::collections::HashMap::new())
            .await?;

        let (starting, advanced) = handle.request_start_transition().await?;
        if !advanced {
            return Ok(starting);
        }
        self.publish_state_change(&starting, "start_requested");
        self.start_runtime_from_starting(bot_id, &handle, &config)
            .await
    }

    /// 停止指定 Bot。
    pub async fn stop_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let stopping = handle.request_stop().await?;
        self.publish_state_change(&stopping, "stop_requested");

        // 按 deployment_type + runtime_target 选 backend；路由失败必须向上报错。
        let cfg = self.get_required_bot_config(bot_id).await?;
        let backend = self.backend_for_lifecycle(&cfg).await?;

        let status = backend.status(bot_id.clone()).await?;
        if status.state == BotActorState::Stopped {
            let stopped = match stopping.state {
                BotActorState::Stopping => handle.confirm_stopped().await?,
                _ => stopping,
            };
            self.publish_state_change(&stopped, "stop_completed");
            if stopped.state == BotActorState::Starting {
                let config = self.get_required_bot_config(bot_id).await?;
                return self
                    .start_runtime_from_starting(bot_id, &handle, &config)
                    .await;
            }
            return Ok(stopped);
        }

        match backend.stop(bot_id.clone(), StopMode::Force).await {
            Ok(()) => {
                let status = backend.status(bot_id.clone()).await?;
                self.event_bus
                    .publish(DomainEvent::bot_status_changed(status, "runtime_stop"));
                let stopped = handle.confirm_stopped().await?;
                self.publish_state_change(&stopped, "stop_completed");
                if stopped.state == BotActorState::Starting {
                    let config = self.get_required_bot_config(bot_id).await?;
                    return self
                        .start_runtime_from_starting(bot_id, &handle, &config)
                        .await;
                }
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
    /// 6 状态分支语义：
    /// - `Running | Starting`：`actor.request_restart()`（标记 `pending_restart` 并转 `Stopping`）
    /// → `backend.stop(Force)` → 等 actor 经 `confirm_stopped` 转入 `Starting` → `start_bot`
    /// - `Stopped | Crashed`：直接 `start_bot`
    /// - `Stopping`：`actor.request_restart()` 标 `pending_restart` → 等 actor 转入 `Starting` → `start_bot`
    /// - `Repairing`：返回 `BotManagerError::InvalidState`
    /// 设计：复用 `BotActor` 现有的 `pending_restart` 机制，不新增状态机分支。
    /// 错误返回给调用方；`RestartHandle::restart_bot` impl 会把
    /// 错误转为 `DomainEvent::bot_error` 发布给前端。
    pub async fn restart_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let snap = handle.snapshot();
        let config = self.get_required_bot_config(bot_id).await.ok();

        match snap.state {
            BotActorState::Running | BotActorState::Starting => {
                let stopping = handle.request_restart().await?;
                self.publish_state_change(&stopping, "restart_requested");
                let cfg = match &config {
                    Some(cfg) => cfg.clone(),
                    None => self.get_required_bot_config(bot_id).await?,
                };
                let backend = self.backend_for_lifecycle(&cfg).await?;
                backend.stop(bot_id.clone(), StopMode::Force).await?;
                match handle.confirm_stopped().await {
                    Ok(s) => self.publish_state_change(&s, "restart_stopped"),
                    Err(crate::bot_actor::BotActorError::InvalidTransition { .. }) => {}
                    Err(e) => return Err(e.into()),
                }
                let current = handle.snapshot();
                if current.state != BotActorState::Starting {
                    return Ok(current);
                }
                self.start_runtime_from_starting(bot_id, &handle, &cfg)
                    .await
            }
            BotActorState::Stopped | BotActorState::Crashed => self.start_bot(bot_id).await,
            BotActorState::Stopping => {
                let stopping = handle.request_restart().await?;
                self.publish_state_change(&stopping, "restart_requested");
                Ok(stopping)
            }
            BotActorState::Repairing => Err(BotManagerError::InvalidState {
                bot_id: bot_id.clone(),
                state: snap.state,
                action: "restart",
            }),
        }
    }

    /// 监听 actor 的 `watch::Receiver` 直到进入指定 `target` 状态或超时。
    /// 用 `watch::Receiver::borrow_and_update` 先消化已有快照，再 `changed`
    /// 等下次更新；超时返回 `BotManagerError::Render`，邮箱关闭则返回
    /// `BotManagerError::Actor(MailboxClosed)`。
    ///
    /// 当前 restart 路径全部走 fast-path（`confirm_stopped` 直接推进），不再
    /// 用这个 helper；保留是为了将来真正需要等异步状态转移时可以复用。
    #[allow(dead_code)]
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
        let mut seen = HashSet::new();

        for bot_id in bot_ids {
            if !seen.insert(bot_id.clone()) {
                continue;
            }
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
    /// 策略：先持久化 bot.json（source of truth），再写派生文件。
    /// - 如果 bot.json 写入失败，派生文件不会被写入，状态完全未变。
    /// - 如果派生文件写入失败，bot.json 已是最新，派生文件可在下次启动时重新生成
    /// 不会造成不可恢复的不一致。
    /// - 新增时：检查 4 开上限，持久化，写派生文件，创建 Actor。
    /// - 更新时：持久化，写派生文件，热推送（通过 restart 通知 Actor 重新加载）。
    /// - 如果 backend_type 发生切换（NapCat ↔ SnowLuma），必须用**旧** backend
    ///   停掉运行中的进程，再用**新** backend 启动，避免老进程留尸。
    pub async fn upsert_bot_config(
        &self,
        config: BotConfig,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        self.upsert_bot_config_with_overrides(config, &std::collections::HashMap::new())
            .await
    }

    /// 带 drift overrides 的 upsert。前端保存时如果检测到 drift 并确认了决议,
    /// 把 overrides 带进来;无 drift 时传空 map。
    pub async fn upsert_bot_config_with_overrides(
        &self,
        config: BotConfig,
        overrides: &std::collections::HashMap<String, Vec<(String, serde_json::Value)>>,
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

        // 0. 读旧 config 拿原 backend_type，用于检测 backend 切换走特殊路径。
        //    新建 bot 没有旧 config，这步返回 None。
        let previous_backend_type: Option<BackendType> = if is_new {
            None
        } else {
            self.repo
                .get(config.bot.qq_id)
                .await
                .ok()
                .flatten()
                .map(|c| c.bot.backend_type)
        };

        // 1. 先持久化 bot.json（source of truth）
        self.repo.upsert(config.clone()).await?;

        // 2. 渲染派生配置文件（走 render_backend_config：读 existing + merge unknown + apply overrides）
        self.render_backend_config(&bot_id, &config, overrides)
            .await?;
        // 清理不再需要的旧 backend 派生文件（例如 NapCat→SL 时删除 onebot11/napcat 文件）
        let target_backend = config.bot.backend_type;
        let current_paths =
            output_paths_for_backend(target_backend, self.store.config_dir(), &bot_id);
        let all_paths = {
            let mut paths = self.renderer.output_paths(&bot_id);
            paths.sort();
            paths.dedup();
            paths
        };
        let delete_paths: Vec<_> = all_paths
            .into_iter()
            .filter(|path| !current_paths.contains(path))
            .collect();
        if !delete_paths.is_empty() {
            let mut txn = crate::traits::config_store::JsonTransaction::new();
            for path in delete_paths {
                txn = txn.delete(path);
            }
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
                let backend_switched = matches!(
                    previous_backend_type,
                    Some(prev) if prev != target_backend
                );
                if backend_switched {
                    // backend 切换:必须停旧 + 起新,没法热推送(NapCat ↔ SL 协议完全不同)
                    let prev_flavor = map_backend_flavor(
                        previous_backend_type.expect("backend_switched => previous Some"),
                    );
                    let snapshot = self
                        .restart_bot_with_backend_switch(&bot_id, prev_flavor)
                        .await?;
                    self.publish_state_change(&snapshot, "config_hot_reload");
                    Ok(snapshot)
                } else {
                    // 同 backend 运行中:派生文件已写盘(step 2),尝试通过 WebUI 热推送。
                    // NapCat: POST /api/OB11Config/SetConfig (需要 port + auth,当前实装延后)
                    // SnowLuma: POST /api/config/:uin (用 daemon 共享 client)
                    // 热推送失败不阻塞保存流程,只给前端一个 warning;下次重启生效。
                    if target_backend == BackendType::SnowLuma {
                        if let Some(daemon) = &self.snowluma_daemon {
                            if let Ok(client) = daemon.current_client().await {
                                let uin = config.bot.qq_id.to_string();
                                let payload = self
                                    .renderer
                                    .render(&bot_id, &config)
                                    .ok()
                                    .and_then(|txn| txn.writes.into_iter().next())
                                    .map(|w| w.payload);
                                if let Some(payload) = payload {
                                    match client.update_onebot_config(&uin, &payload).await {
                                        Ok(reloaded) => {
                                            let msg = if reloaded {
                                                "config_hot_reloaded"
                                            } else {
                                                "config_saved_pending_reload"
                                            };
                                            self.event_bus.publish(DomainEvent::bot_state_changed(
                                                current.clone(),
                                                msg,
                                            ));
                                        }
                                        Err(_) => {
                                            // 热推送失败,配置已写盘下次重启生效
                                            self.event_bus.publish(DomainEvent::bot_state_changed(
                                                current.clone(),
                                                "config_updated",
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // NapCat 热推送：endpoint 表里有 (port, token) 才能继续
                        // （bot 已经把 WebUI 端点报到 stdout 上）。没有就只写盘
                        // 等下次启动生效。配置 payload 直接复用 renderer 写入
                        // onebot11_{bot}.json 的内容——NapCat WebUI
                        // /api/OB11Config/SetConfig 期望的 schema 与该文件一致。
                        let endpoint = self.napcat_endpoints.snapshot(&bot_id).await;
                        let onebot_payload =
                            self.renderer.render(&bot_id, &config).ok().and_then(|txn| {
                                txn.writes
                                    .into_iter()
                                    .find(|w| {
                                        w.path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .is_some_and(|n| n.starts_with("onebot11_"))
                                    })
                                    .map(|w| w.payload)
                            });
                        match (endpoint, onebot_payload) {
                            (Some(ep), Some(payload)) => {
                                let reason = self.push_napcat_hot_reload(ep, payload).await;
                                self.event_bus.publish(DomainEvent::bot_state_changed(
                                    current.clone(),
                                    reason,
                                ));
                            }
                            // 端点没拿到（bot 还没 ready）或者渲染异常 → 配置已落盘，
                            // 等下次重启再生效。
                            _ => {
                                self.event_bus.publish(DomainEvent::bot_state_changed(
                                    current.clone(),
                                    "config_saved_pending_reload",
                                ));
                            }
                        }
                    }
                    Ok(current)
                }
            } else {
                self.event_bus.publish(DomainEvent::bot_state_changed(
                    current.clone(),
                    "config_updated",
                ));
                Ok(current)
            }
        }
    }

    /// 切换 backend_type 时的专用 restart：用 `previous_flavor` 对应的 backend
    /// 停掉老进程，再用 `start_bot`（自动按新 config 选 backend）启动新进程。
    ///
    /// 与普通 `restart_bot` 的关键差异：
    /// - stop 阶段不再用 `self.backend`（写死 NapCat backend），而是用
    ///   `backend_for(previous_flavor)`，避免切换 NapCat → SnowLuma 时老进程留尸。
    /// - stop 返回后**直接** `confirm_stopped` 推进 actor 到 Starting，不依赖
    ///   异步 `BotProcessExited` 事件链。原因：
    ///   1. `backend.stop` 是同步 await 的，返回时进程树已被 force kill
    ///   2. 切换 backend 时 actor 上层不一定能立刻收到旧 backend 的 exit 事件
    ///      （例如旧 backend processes map 已被 stop 主动 remove，spawn_exit_watcher
    ///      持有的 child handle 还在等 wait 完成，wait_until_state 会 10s 超时）
    async fn restart_bot_with_backend_switch(
        &self,
        bot_id: &BotId,
        previous_flavor: BotFlavor,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let stopping = handle.request_restart().await?;
        self.publish_state_change(&stopping, "restart_requested");
        // 用旧 flavor 的 backend 停老进程
        self.backend_for(previous_flavor)
            .stop(bot_id.clone(), StopMode::Force)
            .await?;
        // confirm_stopped 可能跟 exit watcher listener 竞争(参见 restart_bot 注释)
        match handle.confirm_stopped().await {
            Ok(s) => self.publish_state_change(&s, "restart_stopped"),
            Err(crate::bot_actor::BotActorError::InvalidTransition { .. }) => {}
            Err(e) => return Err(e.into()),
        }
        // start_bot 内部按当前 config 重新选 backend，自动用新 flavor 启动。
        self.start_bot(bot_id).await
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

    /// 批量返回所有 Bot 的 backend_type，用于 UI 列表页一次性拿 flavor map。
    /// 避免 BotListPage 对每个 bot 单独调 `get_bot_config` 造成 N+1。
    /// key 为 BotId.to_string()（即 QQID 数字字符串）。
    pub async fn list_bot_flavors(
        &self,
    ) -> Result<
        std::collections::HashMap<String, ncd_domain::bot_config::BackendType>,
        BotManagerError,
    > {
        let configs = self.repo.list().await?;
        let mut out = std::collections::HashMap::with_capacity(configs.len());
        for cfg in configs {
            out.insert(cfg.bot.qq_id.to_string(), cfg.bot.backend_type);
        }
        Ok(out)
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

    /// 本机 runtime_target 且处于活跃态的 Bot 数量（退出拦截用）。
    pub async fn count_local_active_bots(&self) -> Result<usize, BotManagerError> {
        let configs = self.repo.list().await?;
        let snapshots = self.list_snapshots().await;
        Ok(snapshots
            .iter()
            .filter(|s| s.state.is_active())
            .filter(|s| {
                configs
                    .iter()
                    .find(|c| c.bot.qq_id.to_string() == s.bot_id.as_str())
                    .map(|c| c.bot.runtime_target == RuntimeTarget::Local)
                    .unwrap_or(false)
            })
            .count())
    }

    /// 拉取指定 Bot 的最近 `lines` 行日志快照。
    /// 返回 [`LogSnapshot`]，包含已截尾的日志行 + 总行数。供 UI 在 BotLogPage
    /// 初次开页时一次性加载历史，再叠加 `bot_log_appended` / `snowluma_daemon_log`
    /// 实时事件。对齐 legacy `NapCatQQProcessLog.get_log_content` 行为：本地是
    /// 内存 deque 快照（进程存活期间累计的全量），进程被 stop / 重启时缓冲清零。
    ///
    /// 必须按 bot 当前配置的 backend 路由，不能写死走默认 NapCat backend：
    /// 否则 SnowLuma flavor 的 bot 会去 NapCat backend 拉历史，拿到的是磁盘
    /// 归档里 NC 旧日志，配置切换 NC → SL 后用户看到的依然是 NC 的内容。
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
        // 按 bot 当前 backend 选 backend；读不到 config 时退回默认 NapCat backend。
        let qq_id: u64 = bot_id.as_str().parse().unwrap_or(0);
        let flavor = match self.repo.get(qq_id).await {
            Ok(Some(cfg)) => map_backend_flavor(cfg.bot.backend_type),
            _ => BotFlavor::NapCat,
        };
        let opts = crate::runtime_backend::TailOpts { lines };
        self.backend_for(flavor)
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

    /// 渲染派生配置文件。
    ///
    /// 调用 `renderer.render_with_existing` 而不是 `render`，让 NapCat / SnowLuma
    /// renderer 把磁盘上派生文件里**用户加的扩展字段**（如 `imageDownloadProxy`、
    /// `autoTimeSync`）合并进新输出，避免每次启动覆盖时丢掉用户的手改。
    ///
    /// `overrides` 来自前端 ConfigDriftDialog 的 `AcceptExternal` 决议：先按
    /// 默认 BotConfig 渲染输出，再用 overrides 把对应 JSON path 的值换成外部值。
    /// 没有决议时传空 map。
    async fn render_backend_config(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        overrides: &std::collections::HashMap<String, Vec<(String, serde_json::Value)>>,
    ) -> Result<(), BotManagerError> {
        // 1. 把现有派生文件读进来（不存在的跳过）
        let mut existing: std::collections::HashMap<std::path::PathBuf, serde_json::Value> =
            std::collections::HashMap::new();
        for path in self.renderer.output_paths(bot_id) {
            match tokio::fs::read(&path).await {
                Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(value) => {
                        existing.insert(path, value);
                    }
                    Err(_) => {
                        // 派生文件被人改坏了，无法 parse；当作"不存在"处理，下面
                        // render_with_existing 会写一份干净的覆盖。
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(BotManagerError::Render(format!(
                        "read derived file {} failed: {e}",
                        path.display()
                    )));
                }
            }
        }

        // 2. 渲染（合并 unknown 顶层字段）
        let mut txn = self
            .renderer
            .render_with_existing(bot_id, config, &existing)?;
        if txn.is_empty() {
            return Ok(());
        }

        // 3. 应用 overrides：按文件名找 write 项，按 dot-path 替换值
        if !overrides.is_empty() {
            for write in txn.writes.iter_mut() {
                let Some(file_name) = write
                    .path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                else {
                    continue;
                };
                if let Some(file_overrides) = overrides.get(&file_name) {
                    for (path, value) in file_overrides {
                        set_value_at_dot_path(&mut write.payload, path, value.clone())
                            .map_err(|e| {
                                BotManagerError::Render(format!(
                                    "应用 drift 决议到 {file_name} 的 {path} 失败: {e}"
                                ))
                            })?;
                    }
                }
            }
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

    async fn start_runtime_from_starting(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        config: &BotConfig,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let runtime_config = if config.bot.deployment_type == DeploymentType::Docker {
            self.build_runtime_config(bot_id, config)
        } else {
            let base = self.build_runtime_config(bot_id, config);
            match self.launch_planner.build_plan(bot_id, config).await {
                Ok(plan) => plan.into_runtime_config(base),
                Err(err) => {
                    let message = err.to_string();
                    let hint = match &err {
                        RuntimeLaunchPlanError::SnowLumaNodeMissing(path) => Some(format!(
                            "未在 {} 找到 SnowLuma daemon 二进制。请安装 SnowLuma 运行时组件，或在 Bot 配置中把后端类型切换为 NapCat。",
                            path.display()
                        )),
                        RuntimeLaunchPlanError::SnowLumaInvalidStartMode(detail) => Some(format!(
                            "SnowLuma 启动参数无效：{detail}。请在 Bot 配置中检查启动模式。"
                        )),
                        RuntimeLaunchPlanError::MissingFile { .. } => {
                            Some("NapCat 运行时组件缺失，请先在「设置」页安装运行时。".to_string())
                        }
                        _ => Some("启动计划构造失败：请检查后端类型与运行时安装状态。".to_string()),
                    };
                    let crashed = handle.mark_crashed(message.clone()).await?;
                    self.publish_state_change(&crashed, "start_failed");
                    self.event_bus
                        .publish(DomainEvent::bot_error(bot_id.clone(), message, hint));
                    return Err(BotManagerError::Render(err.to_string()));
                }
            }
        };

        let backend = match self.backend_for_config(config).await {
            Ok(b) => b,
            Err(err) => {
                let message = err.to_string();
                let crashed = handle.mark_crashed(message.clone()).await?;
                self.publish_state_change(&crashed, "start_failed");
                self.event_bus
                    .publish(DomainEvent::bot_error(bot_id.clone(), message, None));
                return Err(err);
            }
        };
        match backend
            .start(&BotStartCtx {
                config: runtime_config,
            })
            .await
        {
            Ok(status) => {
                self.event_bus
                    .publish(DomainEvent::bot_status_changed(status, "runtime_start"));
                // 防快速退出竞态:backend.start Ok 只代表 spawn / compose up 成功,
                // 进程可能在 confirm_running 之前就崩了。Starting 阶段的
                // BotProcessExited 被 handle_process_exited 有意忽略(无法区分本轮新
                // 进程退出与 restart fast-path 旧进程退出),所以这里启动后立即复查一次
                // backend.status:若已落到 Stopped / Crashed,直接按崩溃收口,避免 actor
                // 与 UI 停在假 Running。复查本身报错时不阻断(查不到不代表没起来)。
                if let Ok(observed) = backend.status(bot_id.clone()).await {
                    if matches!(
                        observed.state,
                        BotActorState::Stopped | BotActorState::Crashed
                    ) {
                        let detail = observed
                            .extra
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                            .unwrap_or_else(|| "进程启动后立即退出".to_string());
                        let crashed = handle.mark_crashed(detail.clone()).await?;
                        self.publish_state_change(&crashed, "start_failed");
                        self.event_bus.publish(DomainEvent::bot_error(
                            bot_id.clone(),
                            detail.clone(),
                            Some("Bot 启动后立即退出,请检查启动命令、运行时依赖与日志。".to_string()),
                        ));
                        return Err(BotManagerError::Render(detail));
                    }
                }
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

    fn publish_state_change(&self, snapshot: &BotActorSnapshot, reason: &str) {
        self.event_bus
            .publish(DomainEvent::bot_state_changed(snapshot.clone(), reason));
    }

    /// 应用退出收口：尝试停止所有运行中的 Bot 并 shutdown 它们的 Actor。
    /// 用法：Tauri `WindowEvent::CloseRequested` 时调用，避免 QQ.exe 残留。
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

        // SnowLuma daemon 优雅关闭。
        if let Some(daemon) = self.snowluma_daemon.as_ref() {
            daemon.shutdown().await;
        }

        result
    }

    /// 长任务：监听 `SnowLumaDaemonStateChanged` 事件，daemon 转 `Crashed` 时
    /// 把所有 SnowLuma flavor 且 active 的 actor 级联转 `Crashed`，并各发一次
    /// `BotError`。
    /// 调用方应在 setup 阶段 `tokio::spawn(manager.clone().run_snowluma_listener)`。
    pub async fn run_snowluma_listener(self: Arc<Self>) {
        use crate::snowluma::DaemonState;

        let mut sub = self.event_bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaDaemonStateChanged,
        ));
        loop {
            let evt = match sub.next().await {
                Some(e) => e,
                None => break,
            };
            let DomainEvent::SnowLumaDaemonStateChanged { state, reason, .. } = evt else {
                continue;
            };
            if state != DaemonState::Crashed {
                continue;
            }
            // 收到 Crashed → 把 SL flavor active actor 转 Crashed
            let snapshots: Vec<BotActorSnapshot> = {
                let actors = self.actors.read().await;
                actors.values().map(|h| h.snapshot()).collect()
            };
            // MVP：通过 repo 反查每个 active bot 的 flavor，避免在 actor snapshot 上扩字段。
            for snap in snapshots {
                if !matches!(snap.state, BotActorState::Starting | BotActorState::Running) {
                    continue;
                }
                let bot_id = snap.bot_id.clone();
                let cfg = match self.get_required_bot_config(&bot_id).await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if !matches!(cfg.bot.backend_type, BackendType::SnowLuma) {
                    continue;
                }
                let handle = match self.get_actor(&bot_id).await {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                let reason_str = reason
                    .clone()
                    .unwrap_or_else(|| "snowluma daemon crashed".to_string());
                if let Ok(crashed) = handle.mark_crashed(reason_str.clone()).await {
                    self.publish_state_change(&crashed, "snowluma_daemon_crashed");
                }
                self.event_bus.publish(DomainEvent::bot_error(
                    bot_id,
                    reason_str,
                    Some("SnowLuma daemon 已崩溃，请重启 App".to_string()),
                ));
            }
        }
    }

    /// 订阅运行时事件总线，将 `BotProcessExited` 转换为 actor 状态机转移：
    /// - 进程正常或异常退出 → 调用 `confirm_stopped` / `mark_crashed`
    /// 防止 UI 残留假 Running。
    /// 返回的 future 由调用方在合适的运行时上 spawn（例如
    /// `tauri::async_runtime::spawn`）。它不依赖 tokio current handle
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
    /// 仅在调用方已处于 tokio 运行时上下文（`#[tokio::test]` 或被
    /// `tauri::async_runtime::spawn` 包过的 future）中使用；在 Tauri
    /// `setup` 这种无 tokio handle 的位置请改用：
    /// ```ignore
    /// let manager = bot_manager.clone()
    /// tauri::async_runtime::spawn(async move {
    /// (*manager).clone().run_runtime_event_listener().await
    /// })
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
            // 已运行中却收到退出事件：进程被外部 kill 或自身崩溃。
            BotActorState::Running => {
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
            // Starting：可能是 restart 路径里 fast-path confirm_stopped 后转过来的，
            // 旧 backend 的 spawn_exit_watcher 还在 wait 旧 child handle，wait 返回时
            // 发出来的 exit 事件其实指向的是上一轮已被 force kill 的进程。如果在这里
            // mark_crashed 会把刚 Starting 的新进程误标崩溃。直接忽略最稳。
            // 真正"启动失败"的情况由 start_bot 里 backend.start 的 Err 分支处理。
            BotActorState::Starting => {}
            // 已是 Stopped / Crashed / Repairing：不再做转移，避免无效转移报错。
            _ => {}
        }
    }

    /// 内部删除流程：停止旧 runtime → 持久化删除 → shutdown → 移除内存 Actor。
    /// 停止运行中 Bot 必须发生在 repo.delete 前；否则旧 config/backend identity
    /// 丢失后只能按新默认路由停进程，远端或 Docker 场景会留下真实 runtime。
    async fn delete_bot_internal(&self, bot_id: &BotId) -> Result<(), BotManagerError> {
        let qq_id: u64 = bot_id
            .as_str()
            .parse()
            .map_err(|_| BotManagerError::BotNotFound(bot_id.clone()))?;
        let previous_config = self.repo.get(qq_id).await?;

        let maybe_handle = {
            let actors = self.actors.read().await;
            actors.get(bot_id).cloned()
        };

        if let Some(handle) = &maybe_handle {
            let current = handle.snapshot();
            if current.state.is_active() {
                let stopping = handle.request_stop().await?;
                self.publish_state_change(&stopping, "delete_stop_requested");
                let backend = match &previous_config {
                    Some(cfg) => self.backend_for_lifecycle(cfg).await?,
                    None => self.backend_for(BotFlavor::NapCat),
                };
                if let Err(err) = backend.stop(bot_id.clone(), StopMode::Force).await {
                    let message = err.to_string();
                    self.event_bus.publish(DomainEvent::bot_error(
                        bot_id.clone(),
                        message,
                        Some("删除前停止 Bot 失败，配置与 Actor 已保留，可重试删除。".to_string()),
                    ));
                    return Err(err.into());
                }
                match handle.confirm_stopped().await {
                    Ok(stopped) => self.publish_state_change(&stopped, "delete_stop_completed"),
                    Err(crate::bot_actor::BotActorError::InvalidTransition { .. }) => {}
                    Err(err) => return Err(err.into()),
                }
            }
        }

        self.repo.delete(qq_id).await?;

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

        if let Some(handle) = maybe_handle {
            let _ = handle.shutdown().await;
        }

        {
            let mut actors = self.actors.write().await;
            actors.remove(bot_id);
        }

        self.dispose_poller(bot_id).await;

        self.event_bus.publish(DomainEvent::BotStateChanged {
            snapshot: BotActorSnapshot::new(bot_id.clone()),
            reason: Some("bot_deleted".to_string()),
        });

        Ok(())
    }

    /// 处理 `NapCatWebuiAvailable` 事件：为给定 Bot 创建/替换 `NapCatLoginPoller`。
    /// 行为：
    /// - `repo.get(bot_id)` 不到对应配置时直接 return（不报错），避免在
    /// 配置删除后还接到延迟的 WebuiAvailable 事件时崩溃。
    /// - 从 `poller_settings.read().await` 取最新值组装 `PollerConfig`：
    /// - `login_check_interval` ← `settings.bot_login_check_interval_ms`
    /// - `unlogged_interval` 固定 1s
    /// - `auth_refresh_period` 30 min；`auth_refresh_throttle` 5s；`http_timeout` 5s
    /// - `offline_auto_restart` ← `bot_cfg.bot.offline_auto_restart`
    /// - `offline_notice_enabled = bot_cfg.advanced.offline_notice
    /// && (settings.offline_webhook_notice || settings.offline_email_notice)`
    /// - 旧 Poller 先 `dispose`（取消其 `CancellationToken` 并触发 `Drop` 兜底）
    /// 再插入新实例，保证不会同时存在两个 Poller 抢同一 BotId 的事件。
    /// `restart_handle` 通过 `Arc::clone(self) as Arc<dyn RestartHandle>` 注入
    /// 利用本类型的 `impl RestartHandle for BotManager`（见文件末尾）。
    pub async fn handle_webui_available(self: &Arc<Self>, bot_id: BotId, port: u16, token: String) {
        // 0. 先把 (port, token) 落进 endpoint 表，让保存配置时的热推送可查。
        //    NapCat 多 bot 时 6099 会被先到的占住，后到的自动 +1，token 也是
        //    每进程随机的，必须按 bot 隔离记忆。本步在 BotConfig 解析之前做：
        //    即便 BotConfig 不再存在（事件晚到 / 配置已删），表里多一条孤儿
        //    记录也无害——dispose_poller 会清掉，最坏情况是占一个 BotId 槽位。
        self.napcat_endpoints
            .insert(
                bot_id.clone(),
                NapCatEndpoint {
                    port,
                    token: token.clone(),
                },
            )
            .await;

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

    /// 把一份 OneBot 配置 payload 通过 NapCat WebUI 热推送给运行中的 bot。
    ///
    /// 调用链：login（fetch_credential）→ check_login_status → set_ob11_config。
    /// 任何一步失败都不阻塞保存——已经写盘了，最差就是等下次重启生效。返回
    /// 一个 `BotStateChanged` 的 reason 字符串，让前端区分提示：
    /// - `config_hot_reloaded`：推送成功，配置已生效。
    /// - `config_saved_pending_login`：QQ 还没扫码，待登录后下次启动生效。
    /// - `config_saved_pending_reload`：网络 / 401 / 业务错误，等下次重启生效。
    async fn push_napcat_hot_reload(
        &self,
        endpoint: NapCatEndpoint,
        payload: serde_json::Value,
    ) -> &'static str {
        let NapCatEndpoint { port, token } = endpoint;
        let credential = match self.webui_client.fetch_credential(port, &token).await {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(
                    port,
                    error = ?err,
                    "napcat hot reload: fetch_credential failed; saved without push"
                );
                return "config_saved_pending_reload";
            }
        };
        // QQ 未登录时 set_ob11_config 会返回 NotLogin；提前查一把可以让
        // 前端拿到更准确的语义（避免把"等扫码"误显示成"推送失败"）。
        match self
            .webui_client
            .check_login_status(port, &credential)
            .await
        {
            Ok(data) if !data.is_login => return "config_saved_pending_login",
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(
                    port,
                    error = ?err,
                    "napcat hot reload: check_login_status failed; pushing anyway"
                );
            }
        }
        let body = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(error = ?err, "napcat hot reload: serialize payload failed");
                return "config_saved_pending_reload";
            }
        };
        match self
            .webui_client
            .set_ob11_config(port, &credential, &body)
            .await
        {
            Ok(_) => "config_hot_reloaded",
            Err(crate::napcat::webui_client::NapCatWebUiError::NotLogin) => {
                "config_saved_pending_login"
            }
            Err(err) => {
                tracing::warn!(port, error = ?err, "napcat hot reload: set_ob11_config failed");
                "config_saved_pending_reload"
            }
        }
    }

    /// 移除并取消指定 Bot 的 `NapCatLoginPoller`。多次调用幂等。
    /// 由 `run_napcat_login_listener` 在 `BotProcessExited` 事件到达时调用
    /// 也由 `delete_bot_internal` / `shutdown_all` 在生命周期收尾时调用。
    /// 同步清理 `napcat_endpoints` 中对应记录，避免后续保存配置查到陈旧端口。
    pub async fn dispose_poller(&self, bot_id: &BotId) {
        let mut pollers = self.login_pollers.write().await;
        if let Some(poller) = pollers.remove(bot_id) {
            poller.dispose();
        }
        drop(pollers);
        // endpoint 表与 poller 生命周期严格对齐：bot 进程一旦退出，原来的
        // (port, token) 立即作废（NapCat 重启时 token 会换、端口也可能换），
        // 必须立刻清掉，避免后续 upsert 查到陈旧值打到一个已经死亡的端口。
        self.napcat_endpoints.remove(bot_id).await;
    }

    /// 监听 `NapCatWebuiAvailable` 与 `BotProcessExited` 两路事件，分别驱动
    /// Poller 的创建与回收。
    /// - `Arc<Self>` 作为 receiver：`handle_webui_available` 需要把
    /// `Arc<BotManager<R, S>>` 转成 `Arc<dyn RestartHandle>` 注入 `PollerDeps`。
    /// - `tokio::select!` 同时消费两路 subscription；任一路关闭都会让 `else =>`
    /// 分支退出循环，避免半挂死。
    /// - 调用方（Tauri `setup` 或测试）通过 `tauri::async_runtime::spawn` /
    /// `tokio::spawn` 启动；与 `run_runtime_event_listener` 风格一致。
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

/// 把 `BackendType` 映射到 `BotFlavor`。
fn map_backend_flavor(backend: BackendType) -> BotFlavor {
    match backend {
        BackendType::NapCat => BotFlavor::NapCat,
        BackendType::SnowLuma => BotFlavor::SnowLuma,
    }
}

/// 按 dot-path（如 `network.httpServers`）在 JSON Value 树里设值。
/// 路径不存在的中间节点自动创建为 object。用于应用前端 ConfigDriftDialog
/// 的 `AcceptExternal` 决议到渲染输出。
/// 把 value 写到 root 的 dot-path 位置;value 为 null 表示删除该位置(DropAdded)。
///
/// 支持 object key 与 array index(纯数字段)混合,如 `network.httpClients.0.token`:
/// ConfigDrift 对连接数组里的字段就是这种路径。中间 object 缺失自动建;遇到数组时
/// 按下标定位现有元素,越界 / 段非数字 / 落到非容器值上一律返回错误,而不是像旧实现
/// 那样静默 return——否则用户在 ConfigDriftDialog 里对 token/url 的 AcceptExternal /
/// DropAdded 决议会"看起来点了却没生效"。
fn set_value_at_dot_path(
    root: &mut serde_json::Value,
    dot_path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let segments: Vec<&str> = dot_path.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return Err(format!("非法 dot-path: '{dot_path}'"));
    }

    let mut cursor = root;
    for seg in &segments[..segments.len() - 1] {
        cursor = match cursor {
            serde_json::Value::Object(map) => map
                .entry((*seg).to_string())
                .or_insert_with(|| serde_json::json!({})),
            serde_json::Value::Array(arr) => {
                let idx: usize = seg
                    .parse()
                    .map_err(|_| format!("数组路径段 '{seg}' 不是合法下标"))?;
                let len = arr.len();
                arr.get_mut(idx)
                    .ok_or_else(|| format!("数组下标 {idx} 越界(长度 {len})"))?
            }
            _ => return Err(format!("路径段 '{seg}' 落在非容器值上,无法继续")),
        };
    }

    let last = segments[segments.len() - 1];
    match cursor {
        serde_json::Value::Object(map) => {
            if value.is_null() {
                map.remove(last);
            } else {
                map.insert(last.to_string(), value);
            }
            Ok(())
        }
        serde_json::Value::Array(arr) => {
            let idx: usize = last
                .parse()
                .map_err(|_| format!("数组路径段 '{last}' 不是合法下标"))?;
            if value.is_null() {
                // DropAdded 整个数组元素:越界视作已不存在(已达成),不报错。
                if idx < arr.len() {
                    arr.remove(idx);
                }
                Ok(())
            } else {
                let len = arr.len();
                let slot = arr
                    .get_mut(idx)
                    .ok_or_else(|| format!("数组下标 {idx} 越界(长度 {len})"))?;
                *slot = value;
                Ok(())
            }
        }
        _ => Err(format!("路径 '{dot_path}' 的父级不是 object / array")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SecretStoreImpl;

    fn temp_secret_store() -> (tempfile::TempDir, Arc<dyn SecretStore + Send + Sync>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SecretStoreImpl::new_with_force_fallback(dir.path(), true));
        (dir, store)
    }

    #[test]
    fn docker_webui_token_is_stable_and_not_predictable() {
        let (_dir, store) = temp_secret_store();

        let first = BotManager::<
            crate::LocalBotConfigRepo<crate::LocalConfigStore>,
            crate::LocalConfigStore,
        >::get_or_create_docker_webui_token_from_store(Some(&store), 10001)
        .unwrap();
        let second = BotManager::<
            crate::LocalBotConfigRepo<crate::LocalConfigStore>,
            crate::LocalConfigStore,
        >::get_or_create_docker_webui_token_from_store(Some(&store), 10001)
        .unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, "10001");
        assert_ne!(first, "ncbot-10001");
        assert_ne!(first, "ncbot10001");
        assert_ne!(first, "test-webui-token");
    }

    #[test]
    fn docker_webui_token_replaces_blank_secret() {
        let (_dir, store) = temp_secret_store();
        let key = BotManager::<
            crate::LocalBotConfigRepo<crate::LocalConfigStore>,
            crate::LocalConfigStore,
        >::docker_webui_secret_key(10002);
        store.put(&key, "   ").unwrap();

        let token = BotManager::<
            crate::LocalBotConfigRepo<crate::LocalConfigStore>,
            crate::LocalConfigStore,
        >::get_or_create_docker_webui_token_from_store(Some(&store), 10002)
        .unwrap();

        assert_eq!(store.get(&key).unwrap().as_deref(), Some(token.as_str()));
        assert_eq!(token.len(), 64);
    }

    #[test]
    fn dot_path_sets_object_field() {
        let mut root = serde_json::json!({ "a": { "b": 1 } });
        set_value_at_dot_path(&mut root, "a.c", serde_json::json!("x")).unwrap();
        assert_eq!(root["a"]["c"], serde_json::json!("x"));
    }

    #[test]
    fn dot_path_sets_field_inside_array_element() {
        // ConfigDrift 的连接数组路径:network.httpClients.0.token。
        let mut root = serde_json::json!({
            "network": { "httpClients": [ { "token": "old" }, { "token": "keep" } ] }
        });
        set_value_at_dot_path(&mut root, "network.httpClients.0.token", serde_json::json!("new"))
            .unwrap();
        assert_eq!(root["network"]["httpClients"][0]["token"], "new");
        assert_eq!(root["network"]["httpClients"][1]["token"], "keep");
    }

    #[test]
    fn dot_path_null_removes_object_key_in_array_element() {
        let mut root = serde_json::json!({
            "network": { "httpClients": [ { "token": "drop", "url": "u" } ] }
        });
        set_value_at_dot_path(
            &mut root,
            "network.httpClients.0.token",
            serde_json::Value::Null,
        )
        .unwrap();
        assert!(root["network"]["httpClients"][0].get("token").is_none());
        assert_eq!(root["network"]["httpClients"][0]["url"], "u");
    }

    #[test]
    fn dot_path_array_index_out_of_bounds_errors() {
        let mut root = serde_json::json!({ "list": [ { "x": 1 } ] });
        let err = set_value_at_dot_path(&mut root, "list.3.x", serde_json::json!(2)).unwrap_err();
        assert!(err.contains("越界"));
    }

    #[test]
    fn dot_path_non_numeric_array_segment_errors() {
        let mut root = serde_json::json!({ "list": [ { "x": 1 } ] });
        let err = set_value_at_dot_path(&mut root, "list.foo.x", serde_json::json!(2)).unwrap_err();
        assert!(err.contains("不是合法下标"));
    }
}
