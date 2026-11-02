//! Bot 生命周期编排入口与共享依赖装配。
//!
//! `start` / `stop` / `shutdown` 负责状态迁移与运行资源，`configuration` 负责持久化，
//! `bootstrap` / `refresh` / `host_recovery` 负责恢复与对账，`queries` 仅提供查询。
//! 子模块显式声明依赖，不借入口导入内部实现细节。`local_metrics` 是独立的
//! 文件准备服务，`metrics` 负责运行期注册与同步，二者不持有彼此的编排状态。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::sync::{Mutex, RwLock};

use crate::bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
use crate::docker_bot_session::DockerBotSessionRegistry;
use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::launch::router::DockerQrLoginLocks;
use crate::napcat::endpoint_table::NapCatEndpointTable;
use crate::napcat::login_poller::{NapCatLoginPoller, RestartHandle};
use crate::napcat::offline_notifier::OfflineNotifier;
use crate::napcat::webui_client::NapCatWebUiClient;
use crate::remote_bot_log_follow::RemoteBotLogFollowRegistry;
use crate::remote_runtime_sessions::RemoteRuntimeSessions;
use crate::runtime_launch_plan::{RuntimeLaunchPlanError, RuntimeLaunchPlanner};
use crate::runtime_router::{DockerSecretProvider, RuntimeBackendRouter, RuntimeRouterError};
use crate::snowluma_agreements::SnowLumaAgreementService;
use ncd_backend_napcat::remote_native_napcat_session::RemoteNativeNapcatSessionRegistry;
use ncd_backend_snowluma::remote_snowluma::RemoteSnowLumaDaemon;
use ncd_backend_snowluma::remote_snowluma_log::RemoteSnowLumaLogRegistry;
use ncd_backend_snowluma::remote_snowluma_tunnel::RemoteSnowLumaTunnelRegistry;
use ncd_domain::DesktopNotifySettings;
use ncd_domain::app_config::WebUiPollerSettings;
use ncd_domain::bot_config::{BotConfig, BotConfigError};
use ncd_domain::ids::BotId;
use ncd_traits::backend_config_renderer::BackendConfigRenderer;
use ncd_traits::runtime_backend::{BotBackend, BotBackendError};
use ncd_traits::{BotConfigRepo, ConfigStore, SecretStore};

pub mod auto_restart;
mod bootstrap;
mod configuration;
mod debug_port;
mod helpers;
pub(crate) mod host_recovery;
mod hot_reload;
mod listeners;
mod local_metrics;
mod metrics;
mod migration;
mod novnc;
mod qr_login;
mod queries;
mod read_only;
mod refresh;
mod remote_network;
pub mod runtime_gate;
mod shutdown;
mod start;
mod stop;
mod watch_relogin;
mod webui;
pub(crate) use migration::migration_protocol_scope;
pub use runtime_gate::{RuntimeReadinessGate, describe_not_ready, framework_component_for};

// ─── 常量 ──────────────────────────────────────────────────────────────────────

/// Desktop 单机上限:最多同时托管 4 个 Bot
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

    /// 框架或其运行依赖没装好;在进入 Starting 之前拦下,不标 Crashed
    #[error("RUNTIME_NOT_READY: {0}")]
    RuntimeNotReady(String),

    #[error("task join failed: {0}")]
    TaskJoinFailed(String),

    #[error("start cancelled (bot stopped before completion)")]
    Cancelled,

    /// 从远端回读框架配置失败;文案直接给用户看,不加前缀
    #[error("{0}")]
    RemoteRead(String),
}

impl From<ncd_traits::RenderError> for BotManagerError {
    fn from(err: ncd_traits::RenderError) -> Self {
        Self::Render(err.to_string())
    }
}

impl From<RuntimeLaunchPlanError> for BotManagerError {
    fn from(err: RuntimeLaunchPlanError) -> Self {
        Self::Render(err.to_string())
    }
}

impl From<RuntimeRouterError> for BotManagerError {
    fn from(err: RuntimeRouterError) -> Self {
        match err {
            RuntimeRouterError::Config(err) => Self::Config(err),
            RuntimeRouterError::Render(message) => Self::Render(message),
            err @ RuntimeRouterError::MissingLocalSnowLumaBackend => Self::Render(err.to_string()),
        }
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

/// Coordinator for the shared remote QQ installation tree's package.json "main" entry point.
///
/// On a single remote Linux host (identified by server_id), NapCat and SnowLuma flavors
/// share the same rootless QQ tree ($HOME/Napcat/opt/QQ). Starting a bot of either flavor
/// must atomically switch the main field (to ./loadNapCat.js or ./app_launcher/index.js)
/// and, for NapCat, verify that the injection artifacts actually exist.
///
use ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator;

// ─── BotManager ────────────────────────────────────────────────────────────────

/// 编排层:统一管理所有 Bot 的生命周期
/// - 每个 Bot 对应一个 BotActorHandle(状态机)
/// - BotConfigRepo 负责持久化配置
/// - BackendConfigRenderer 负责生成后端运行时配置文件
/// - BroadcastEventBus 负责事件广播给前端
///
/// BotManager 自身不持有可变业务状态,所有可变状态都封装在
/// actors map(由 RwLock 保护)和各 BotActorHandle 内部
pub struct BotManager<R: BotConfigRepo + 'static, S: ConfigStore + 'static> {
    repo: Arc<R>,
    store: Arc<S>,
    renderer: Arc<dyn BackendConfigRenderer>,
    backend: Arc<dyn BotBackend>,
    launch_planner: Arc<dyn RuntimeLaunchPlanner>,
    event_bus: Arc<BroadcastEventBus>,
    actors: Arc<RwLock<HashMap<BotId, BotActorHandle>>>,
    /// per-Bot WebUI 登录轮询组件,由 run_napcat_login_listener 在收到
    /// NapCatWebuiAvailable 事件时插入;BotProcessExited / delete_bot
    /// / shutdown_all 时移除并 dispose
    login_pollers: Arc<RwLock<HashMap<BotId, NapCatLoginPoller>>>,
    /// per-Bot NapCat WebUI 端点 (port + token) 的内存表
    /// NapCat 多 bot 启动时端口会自动 +1,webui.json 不能区分,必须从每个 bot
    /// 自己的 stdout 抓 WebUi User Panel Url本表的写/删时机与 login_pollers
    /// 完全对齐,供配置热推送在保存配置时反查 (port, token)
    napcat_endpoints: NapCatEndpointTable,
    /// SnowLuma UI 会话态镜像表:冷启动 hydrate 用(对齐 napcat_endpoints)
    snowluma_ui: crate::snowluma_ui_state::SnowLumaUiStateTable,
    /// NapCat WebUI HTTP 客户端依赖,可注入 mock 用于测试
    webui_client: Arc<dyn NapCatWebUiClient>,
    /// 离线通知通道依赖,可注入 mock;默认 wiring 走 NoopOfflineNotifier
    offline_notifier: Arc<dyn OfflineNotifier>,
    /// App 级 Poller 设置,热更新通过 poller_settings.write() 即可生效
    /// 下次 handle_webui_available 创建 Poller 时读取最新值
    poller_settings: Arc<RwLock<WebUiPollerSettings>>,
    /// 桌面 Toast 开关(与 app-settings.json 同步)
    desktop_notify: Arc<RwLock<DesktopNotifySettings>>,
    /// SnowLuma flavor backend(可选)None 时所有 bot 走 backend(NapCat 路径)
    /// 由 wiring 阶段构造并通过 with_snowluma_backend 注入
    snowluma_backend: Option<Arc<dyn BotBackend>>,
    /// SnowLuma 全局 daemon 句柄(可选),用于 shutdown_all 关闭 daemon,
    /// run_snowluma_listener 监听 daemon Crashed 级联级 actor
    snowluma_daemon: Option<Arc<crate::snowluma::SnowLumaDaemon>>,
    /// 把 BotConfig 的 runtime_target 解析成 host(本机 / 远端 SSH)
    /// None 时走旧路径(backend 自带的本机 host,行为同历史版本);生产侧由
    /// with_host_resolver 注入 TauriHostResolver 后,启动时按 target 取 host
    host_resolver: Option<Arc<dyn crate::host_resolver::HostResolver>>,
    /// Docker NapCat WebUI token 的凭据存储DockerDeployment 要求上层显式传入
    /// token,不能从 QQ 号或容器名派生;这里按 Bot 持久化,保证重启后 token 稳定
    docker_webui_secret_store: Option<Arc<dyn SecretStore + Send + Sync>>,
    docker_sessions: Arc<DockerBotSessionRegistry>,
    docker_qr_login_locks: DockerQrLoginLocks,
    qr_login_restarts: qr_login::QrRestartTable,
    remote_native_napcat_sessions: Arc<RemoteNativeNapcatSessionRegistry>,
    windows_bots: Arc<crate::launch::windows::WindowsBotRegistry>,
    remote_bot_log_follow: Arc<RemoteBotLogFollowRegistry>,
    remote_sl_daemon_log: Arc<RemoteSnowLumaLogRegistry>,
    /// 远端 SnowLuma:按 server_id 共享 daemon(多 Bot 同一 SSH 主机)
    remote_snowluma_daemons: Arc<Mutex<HashMap<String, Arc<RemoteSnowLumaDaemon>>>>,
    remote_snowluma_init_locks: crate::launch::router::RemoteSnowLumaInitLocks,
    watch_relogin_owners: Arc<Mutex<HashMap<String, Vec<String>>>>,
    /// 远端 SL backend 单例缓存:持有 status poller,绝不能每次 start 新建
    remote_snowluma_backends: Arc<
        Mutex<HashMap<String, Arc<ncd_backend_snowluma::remote_snowluma::RemoteSnowLumaBackend>>>,
    >,
    remote_snowluma_tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    /// Per-remote-host coordination for flipping the shared ~/Napcat/opt/QQ tree's
    /// package.json main between NapCat-injected and vanilla native modes.
    /// See RemoteQqEntryCoordinator for rationale and batch-start safety.
    remote_qq_entry_coordinator: Arc<RemoteQqEntryCoordinator>,
    server_manager: Option<Arc<crate::ServerManager>>,
    reconcile_locks: crate::bootstrap_reconcile::ReconcileLocks,
    refresh_lock: Arc<Mutex<()>>,
    config_write_lock: Arc<Mutex<()>>,
    migration_reserved: Arc<std::sync::Mutex<migration::MigrationProtocolGate>>,
    /// 和组件执行器共用的远端库存副本;启动路由按它取选中路径,装卸后的重探对这边也生效
    remote_inventory: Option<Arc<crate::remote::inventory::RemoteInventoryService>>,
    /// 启动前的框架 / 依赖预检;None 不检查(测试与纯本机 wiring)
    runtime_gate: Option<Arc<dyn RuntimeReadinessGate>>,
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
            snowluma_ui: self.snowluma_ui.clone(),
            webui_client: Arc::clone(&self.webui_client),
            offline_notifier: Arc::clone(&self.offline_notifier),
            poller_settings: Arc::clone(&self.poller_settings),
            desktop_notify: Arc::clone(&self.desktop_notify),
            snowluma_backend: self.snowluma_backend.clone(),
            snowluma_daemon: self.snowluma_daemon.clone(),
            host_resolver: self.host_resolver.clone(),
            docker_webui_secret_store: self.docker_webui_secret_store.clone(),
            docker_sessions: Arc::clone(&self.docker_sessions),
            docker_qr_login_locks: Arc::clone(&self.docker_qr_login_locks),
            qr_login_restarts: Arc::clone(&self.qr_login_restarts),
            remote_native_napcat_sessions: Arc::clone(&self.remote_native_napcat_sessions),
            windows_bots: Arc::clone(&self.windows_bots),
            remote_bot_log_follow: Arc::clone(&self.remote_bot_log_follow),
            remote_sl_daemon_log: Arc::clone(&self.remote_sl_daemon_log),
            remote_snowluma_daemons: Arc::clone(&self.remote_snowluma_daemons),
            remote_snowluma_init_locks: Arc::clone(&self.remote_snowluma_init_locks),
            watch_relogin_owners: Arc::clone(&self.watch_relogin_owners),
            remote_snowluma_backends: Arc::clone(&self.remote_snowluma_backends),
            remote_snowluma_tunnels: Arc::clone(&self.remote_snowluma_tunnels),
            remote_qq_entry_coordinator: Arc::clone(&self.remote_qq_entry_coordinator),
            server_manager: self.server_manager.clone(),
            reconcile_locks: Arc::clone(&self.reconcile_locks),
            refresh_lock: Arc::clone(&self.refresh_lock),
            config_write_lock: Arc::clone(&self.config_write_lock),
            migration_reserved: Arc::clone(&self.migration_reserved),
            remote_inventory: self.remote_inventory.clone(),
            runtime_gate: self.runtime_gate.clone(),
        }
    }
}

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    #[allow(clippy::too_many_arguments)]
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
        desktop_notify: Arc<RwLock<DesktopNotifySettings>>,
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
            snowluma_ui: crate::snowluma_ui_state::SnowLumaUiStateTable::new(),
            webui_client,
            offline_notifier,
            poller_settings,
            desktop_notify,
            snowluma_backend: None,
            snowluma_daemon: None,
            host_resolver: None,
            docker_webui_secret_store: None,
            docker_sessions: Arc::new(DockerBotSessionRegistry::new()),
            docker_qr_login_locks: Arc::new(Mutex::new(HashMap::new())),
            qr_login_restarts: Arc::new(Mutex::new(HashMap::new())),
            remote_native_napcat_sessions: Arc::new(RemoteNativeNapcatSessionRegistry::new()),
            windows_bots: Arc::new(crate::launch::windows::WindowsBotRegistry::default()),
            remote_bot_log_follow: Arc::new(RemoteBotLogFollowRegistry::new()),
            remote_sl_daemon_log: Arc::new(RemoteSnowLumaLogRegistry::new()),
            remote_snowluma_daemons: Arc::new(Mutex::new(HashMap::new())),
            remote_snowluma_init_locks: Arc::new(Mutex::new(HashMap::new())),
            watch_relogin_owners: Arc::new(Mutex::new(HashMap::new())),
            remote_snowluma_backends: Arc::new(Mutex::new(HashMap::new())),
            remote_snowluma_tunnels: Arc::new(RemoteSnowLumaTunnelRegistry::new()),
            remote_qq_entry_coordinator: Arc::new(RemoteQqEntryCoordinator::default()),
            server_manager: None,
            reconcile_locks: Arc::new(Mutex::new(HashMap::new())),
            refresh_lock: Arc::new(Mutex::new(())),
            config_write_lock: Arc::new(Mutex::new(())),
            migration_reserved: Arc::new(std::sync::Mutex::new(Default::default())),
            remote_inventory: None,
            runtime_gate: None,
        }
    }

    pub fn with_server_manager(mut self, mgr: Arc<crate::ServerManager>) -> Self {
        self.server_manager = Some(mgr);
        self
    }

    pub fn with_remote_inventory(
        mut self,
        inventory: Arc<crate::remote::inventory::RemoteInventoryService>,
    ) -> Self {
        self.remote_inventory = Some(inventory);
        self
    }

    pub fn with_runtime_gate(mut self, gate: Arc<dyn RuntimeReadinessGate>) -> Self {
        self.runtime_gate = Some(gate);
        self
    }

    /// 注入 HostResolver(wiring 阶段调用),让启动时按 runtime_target 选本机/远端 host
    /// 不注入时走旧路径(全本机)链式 builder 风格
    pub fn with_host_resolver(
        mut self,
        resolver: Arc<dyn crate::host_resolver::HostResolver>,
    ) -> Self {
        self.host_resolver = Some(resolver);
        self
    }

    /// 注入 Docker WebUI token 的 SecretStoreDockerDeployment 不生成生产默认 token,
    /// runtime 层负责按 Bot 持久化后显式传入 deploy 层
    pub fn with_docker_webui_secret_store(
        mut self,
        store: Arc<dyn SecretStore + Send + Sync>,
    ) -> Self {
        self.docker_webui_secret_store = Some(store);
        self
    }

    /// 注入 SnowLuma flavor 路由依赖( wiring 阶段调用)
    /// 链式 builder 风格,便于 setup 代码组装
    pub fn with_snowluma(
        mut self,
        backend: Arc<dyn BotBackend>,
        daemon: Arc<crate::snowluma::SnowLumaDaemon>,
    ) -> Self {
        self.snowluma_backend = Some(backend);
        self.snowluma_daemon = Some(daemon);
        self
    }

    /// 热更新 App 级轮询设置运行中的 Poller 在下次创建(启动 / 重启)时
    /// 从 poller_settings 读最新值;已在跑的 Poller 不强制重建,避免抖动
    /// 设置页 set_app_settings 写盘后调用此方法让内存值同步
    pub async fn update_poller_settings(&self, settings: WebUiPollerSettings) {
        *self.poller_settings.write().await = settings;
    }

    pub async fn update_desktop_notify_settings(&self, settings: DesktopNotifySettings) {
        *self.desktop_notify.write().await = settings;
    }

    /// stop / restart / delete 必须按完整 BotConfig 路由 backend;backend_for_config
    /// 失败时不得静默回落本机 baked backend,否则远端 Docker 会假停/假删
    async fn backend_for_lifecycle(
        &self,
        config: &BotConfig,
    ) -> Result<Arc<dyn BotBackend>, BotManagerError> {
        self.backend_for_config(config).await
    }

    fn docker_secrets(&self) -> DockerSecretProvider {
        DockerSecretProvider::new(self.docker_webui_secret_store.clone())
    }

    fn runtime_router(&self) -> RuntimeBackendRouter {
        let mut router = RuntimeBackendRouter::new(
            Arc::clone(&self.backend),
            self.snowluma_backend.clone(),
            self.host_resolver.clone(),
            self.docker_secrets(),
            Arc::clone(&self.event_bus),
            Arc::clone(&self.remote_snowluma_daemons),
            Arc::clone(&self.remote_snowluma_backends),
            Arc::clone(&self.remote_snowluma_tunnels),
            Arc::clone(&self.remote_qq_entry_coordinator),
        )
        .with_remote_snowluma_init_locks(Arc::clone(&self.remote_snowluma_init_locks))
        .with_docker_qr_login_locks(Arc::clone(&self.docker_qr_login_locks))
        .with_windows_bots(Arc::clone(&self.windows_bots));
        if let Some(mgr) = &self.server_manager {
            router = router.with_server_manager(Arc::clone(mgr));
        }
        if let Some(inventory) = &self.remote_inventory {
            router = router.with_remote_inventory(Arc::clone(inventory));
        }
        // 远端 NC / Docker 启动注入探针：开关开时挂上（失败不阻断启动）
        if let Some(data_root) = self.store.config_dir().parent() {
            router = router
                .with_snowluma_data_root(crate::DataPaths::new(data_root).snowluma_data_dir());
            let prefs = crate::metrics::BotRuntimeMetricsPrefs::from_data_root(data_root);
            // SL 即使统计关闭也预加载共享探针，后续设置通过注册表热更新。
            router = router.with_remote_metrics_injector(Arc::new(
                crate::metrics::RuntimeRemoteMetricsInjector::new(data_root.to_path_buf()),
            ));
            if prefs.enabled {
                router = router.with_docker_metrics(data_root.to_path_buf(), prefs);
            }
        }
        router
    }

    fn snowluma_agreements(&self) -> SnowLumaAgreementService {
        SnowLumaAgreementService::new(
            self.snowluma_daemon.clone(),
            self.host_resolver.clone(),
            self.runtime_router(),
        )
    }

    fn remote_runtime_sessions(&self) -> RemoteRuntimeSessions<R> {
        RemoteRuntimeSessions::new(
            Arc::clone(&self.repo),
            Arc::clone(&self.event_bus),
            self.host_resolver.clone(),
            self.runtime_router(),
            self.docker_secrets(),
            Arc::clone(&self.docker_sessions),
            Arc::clone(&self.remote_native_napcat_sessions),
            Arc::clone(&self.remote_bot_log_follow),
            Arc::clone(&self.remote_sl_daemon_log),
        )
    }

    /// 按完整 BotConfig 路由 backend。唯一矩阵入口在 RuntimeScenario/RuntimeBackendRouter。
    async fn backend_for_config(
        &self,
        config: &BotConfig,
    ) -> Result<Arc<dyn BotBackend>, BotManagerError> {
        self.runtime_router()
            .backend_for_config(config)
            .await
            .map_err(BotManagerError::from)
    }

    // ─── 批量操作 ─────────────────────────────────────────────────────────

    /// 批量启动并发调度所有目标 Bot,收集成功/失败
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

    /// 批量停止
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

    /// 批量删除:先停止运行中的 Bot,再 shutdown Actor,最后删除持久化配置
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

    /// 单测 / 集成测试:把 Actor 标为 Running,不经过 backend start
    #[doc(hidden)]
    pub async fn test_confirm_actor_running(
        &self,
        bot_id: &BotId,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let _ = handle
            .request_start()
            .await
            .map_err(BotManagerError::Actor)?;
        handle
            .confirm_running()
            .await
            .map_err(BotManagerError::Actor)
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
        self.get_bot_config(bot_id)
            .await?
            .ok_or_else(|| BotManagerError::BotNotFound(bot_id.clone()))
    }

    fn publish_state_change(&self, snapshot: &BotActorSnapshot, reason: &str) {
        self.event_bus
            .publish(DomainEvent::bot_state_changed(snapshot.clone(), reason));
    }

    /// SnowLuma 协议预检：读协议文件，不启动 QQ / node，也不 login。
    ///
    /// 本机与远端 Native 都只读 EULA/PRIVACY/consent.json（与本机 daemon.runtime_root 一致）；
    /// Docker 经 cp 读取容器文件，停止时仍可展示协议；首次创建后补查一次。
    pub async fn prepare_snowluma_agreements(
        &self,
        bot_id: &BotId,
    ) -> Result<Option<crate::snowluma::AgreementsPayload>, BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        self.snowluma_agreements().prepare(&config).await
    }

    pub async fn record_snowluma_agreement_consent(
        &self,
        bot_id: &BotId,
        version: &str,
    ) -> Result<bool, BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        self.snowluma_agreements()
            .record_consent(&config, version)
            .await
    }

    pub async fn release_snowluma_agreement_session(
        &self,
        bot_id: &BotId,
    ) -> Result<(), BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        self.snowluma_agreements().release(&config).await
    }
}
// ─── RestartHandle 实现 ────────────────────────────────────────────────────────

/// BotManager 实现 RestartHandle,让 NapCatLoginPoller 可以在踢线 +
/// offline_auto_restart=true 分支调用 restart_bot 而不直接持有
/// BotManager 引用(避免循环依赖)
/// 失败处理:把错误转成 DomainEvent::bot_error 发布到事件总线,附中文
/// 提示「自动重启失败,请手动启动 Bot」Poller 不感知失败
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
