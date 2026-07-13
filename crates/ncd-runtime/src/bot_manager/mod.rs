use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};

use crate::backend_config_renderer::output_paths_for_backend;
use crate::bootstrap_reconcile::BootstrapReconciler;
use crate::bot_actor::{BotActorError, BotActorHandle, BotActorSnapshot, BotActorState};
use crate::docker_bot_session::DockerBotSessionRegistry;
// EventBus trait 必须 in scope 才能 publish；DomainEventKind/EventFilter 供 listeners 经 super::* 使用。
use crate::events::{BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter};
use crate::napcat::endpoint_table::{NapCatEndpoint, NapCatEndpointTable};
// PollerConfig/PollerDeps 仅 listeners 建 poller 用；RestartHandle 在本文件 impl。
use crate::napcat::login_poller::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};
use crate::napcat::offline_notifier::OfflineNotifier;
use crate::napcat::webui_client::NapCatWebUiClient;
use crate::remote_bot_log_follow::RemoteBotLogFollowRegistry;
use crate::remote_runtime_sessions::RemoteRuntimeSessions;
use crate::runtime_launch_plan::{RuntimeLaunchPlanError, RuntimeLaunchPlanner};
use crate::runtime_router::{DockerSecretProvider, RuntimeBackendRouter, RuntimeRouterError};
// SnowLumaWebUiClient: wait_ready/login/get_agreements 的 trait 方法解析需要 in scope。
use crate::snowluma::{AgreementsPayload, ReqwestSnowLumaWebUiClient, SnowLumaWebUiClient};
use crate::snowluma_agreements::SnowLumaAgreementService;
use ncd_backend_napcat::remote_native_napcat_session::RemoteNativeNapcatSessionRegistry;
use ncd_backend_snowluma::remote_snowluma::RemoteSnowLumaDaemon;
use ncd_backend_snowluma::remote_snowluma_log::RemoteSnowLumaLogRegistry;
use ncd_backend_snowluma::remote_snowluma_tunnel::RemoteSnowLumaTunnelRegistry;
use ncd_domain::app_config::WebUiPollerSettings;
use ncd_domain::bot_config::{BackendType, BotConfig, BotConfigError};
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{BotFlavor, RuntimeTarget, StopMode};
use ncd_domain::{DesktopNotifySettings, RuntimeScenario};
use ncd_traits::backend_config_renderer::BackendConfigRenderer;
use ncd_traits::runtime_backend::{BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx};
use ncd_traits::{BotConfigRepo, ConfigStore, JsonTransaction, SecretStore};

mod helpers;
mod listeners;
use helpers::{is_remote_transport_error, set_value_at_dot_path};

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

    #[error("task join failed: {0}")]
    TaskJoinFailed(String),

    #[error("start cancelled (bot stopped before completion)")]
    Cancelled,
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
    remote_native_napcat_sessions: Arc<RemoteNativeNapcatSessionRegistry>,
    remote_bot_log_follow: Arc<RemoteBotLogFollowRegistry>,
    remote_sl_daemon_log: Arc<RemoteSnowLumaLogRegistry>,
    /// 远端 SnowLuma:按 server_id 共享 daemon(多 Bot 同一 SSH 主机)
    remote_snowluma_daemons: Arc<Mutex<HashMap<String, Arc<RemoteSnowLumaDaemon>>>>,
    /// 远端 SL backend 单例缓存:持有 status poller,绝不能每次 start 新建
    remote_snowluma_backends: Arc<
        Mutex<HashMap<String, Arc<ncd_backend_snowluma::remote_snowluma::RemoteSnowLumaBackend>>>,
    >,
    remote_snowluma_tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    /// Per-remote-host coordination for flipping the shared ~/Napcat/opt/QQ tree's
    /// package.json main between NapCat-injected and vanilla native modes.
    /// See RemoteQqEntryCoordinator for rationale and batch-start safety.
    remote_qq_entry_coordinator: Arc<RemoteQqEntryCoordinator>,
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
            remote_native_napcat_sessions: Arc::clone(&self.remote_native_napcat_sessions),
            remote_bot_log_follow: Arc::clone(&self.remote_bot_log_follow),
            remote_sl_daemon_log: Arc::clone(&self.remote_sl_daemon_log),
            remote_snowluma_daemons: Arc::clone(&self.remote_snowluma_daemons),
            remote_snowluma_backends: Arc::clone(&self.remote_snowluma_backends),
            remote_snowluma_tunnels: Arc::clone(&self.remote_snowluma_tunnels),
            remote_qq_entry_coordinator: Arc::clone(&self.remote_qq_entry_coordinator),
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
            remote_native_napcat_sessions: Arc::new(RemoteNativeNapcatSessionRegistry::new()),
            remote_bot_log_follow: Arc::new(RemoteBotLogFollowRegistry::new()),
            remote_sl_daemon_log: Arc::new(RemoteSnowLumaLogRegistry::new()),
            remote_snowluma_daemons: Arc::new(Mutex::new(HashMap::new())),
            remote_snowluma_backends: Arc::new(Mutex::new(HashMap::new())),
            remote_snowluma_tunnels: Arc::new(RemoteSnowLumaTunnelRegistry::new()),
            remote_qq_entry_coordinator: Arc::new(RemoteQqEntryCoordinator::default()),
        }
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

    /// 从 data_root 读 AppSettings 派生指标偏好（缺文件/解析失败 → 默认关）
    fn load_metrics_prefs(data_root: &std::path::Path) -> crate::metrics::BotRuntimeMetricsPrefs {
        use crate::metrics::BotRuntimeMetricsPrefs;
        let settings_path = data_root.join("config").join("app-settings.json");
        let mut prefs = BotRuntimeMetricsPrefs::default();
        if !settings_path.is_file() {
            return prefs;
        }
        let Ok(text) = std::fs::read_to_string(&settings_path) else {
            return prefs;
        };
        let Ok(app) = serde_json::from_str::<ncd_domain::AppSettings>(&text) else {
            return prefs;
        };
        prefs = BotRuntimeMetricsPrefs::from_app(&app);
        prefs.normalize();
        prefs
    }

    /// 启动前按 AppSettings 注入指标探针（本机）；失败只记日志，不阻断启动
    async fn apply_runtime_metrics_inject(&self, bot_id: &BotId, config: &BotConfig) {
        use crate::metrics::{
            apply_metrics_to_environment, build_napcat_load_script, prepare_inject,
        };
        use ncd_domain::bot_config::BackendType;
        use ncd_domain::kinds::RuntimeTarget;

        // 仅本机启动注入；远端由部署/watch 路径处理
        if !matches!(config.bot.runtime_target, RuntimeTarget::Local) {
            return;
        }

        let Some(data_root) = self.store.config_dir().parent().map(|p| p.to_path_buf()) else {
            return;
        };
        let prefs = Self::load_metrics_prefs(&data_root);

        let plan = match prepare_inject(&data_root, bot_id.as_str(), config, &prefs) {
            Ok(p) => p,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    %err,
                    "metrics inject prepare failed"
                );
                return;
            }
        };

        match config.bot.backend_type {
            BackendType::SnowLuma => {
                if let Some(daemon) = &self.snowluma_daemon {
                    if let Some(plan) = plan.as_ref() {
                        let mut env = std::collections::BTreeMap::new();
                        apply_metrics_to_environment(&mut env, plan);
                        env.insert(
                            "NCD_METRICS_PROBE_PATH".into(),
                            plan.probe_script.to_string_lossy().into_owned(),
                        );
                        daemon.set_metrics_child_env(Some(env));
                    } else {
                        daemon.set_metrics_child_env(None);
                    }
                }
            }
            BackendType::NapCat => {
                // 改写 loadNapCat.js（require 探针）。NCD_* env 在 build_plan 之后
                // 合并进 BotRuntimeConfig.environment，由 native 启动透传，不碰进程全局 env。
                let napcat_dir = data_root.join("components").join("NapCatQQ");
                let load_path = napcat_dir.join("loadNapCat.js");
                let napcat_mjs = napcat_dir.join("napcat.mjs");
                if napcat_mjs.is_file() {
                    let uri = {
                        let normalized = napcat_mjs.to_string_lossy().replace('\\', "/");
                        if normalized.contains(":/") {
                            format!("file:///{normalized}")
                        } else {
                            format!("file://{normalized}")
                        }
                    };
                    let script = build_napcat_load_script(
                        &uri,
                        plan.as_ref().map(|p| p.probe_script.as_path()),
                    );
                    if let Err(err) = std::fs::write(&load_path, script) {
                        warn!(
                            target: "ncd_runtime::bot_manager",
                            bot_id = %bot_id,
                            %err,
                            "rewrite loadNapCat.js for metrics failed"
                        );
                    }
                }
            }
        }
    }

    /// 把指标 env 合并进本机启动用的 BotRuntimeConfig.environment
    fn merge_metrics_env_into_runtime_config(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        runtime_config: &mut ncd_traits::runtime_backend::BotRuntimeConfig,
    ) {
        use crate::metrics::{apply_metrics_to_environment, prepare_inject};
        use ncd_domain::kinds::RuntimeTarget;

        if !matches!(config.bot.runtime_target, RuntimeTarget::Local) {
            return;
        }
        let Some(data_root) = self.store.config_dir().parent().map(|p| p.to_path_buf()) else {
            return;
        };
        let prefs = Self::load_metrics_prefs(&data_root);
        // 此处再 prepare 一次：会覆盖探针脚本与 nodes 映射，与 apply 阶段一致且幂等
        if let Ok(Some(plan)) = prepare_inject(&data_root, bot_id.as_str(), config, &prefs) {
            apply_metrics_to_environment(&mut runtime_config.environment, &plan);
            runtime_config.environment.insert(
                "NCD_METRICS_PROBE_PATH".into(),
                plan.probe_script.to_string_lossy().into_owned(),
            );
        }
    }

    /// 按 flavor 选择 backend:SnowLuma 时优先用注入的 SL backend,否则
    /// 回落到默认 backend(向后兼容:未注入 SL 时与历史行为一致)
    fn backend_for(&self, flavor: BotFlavor) -> Arc<dyn BotBackend> {
        match flavor {
            BotFlavor::SnowLuma => self
                .snowluma_backend
                .clone()
                .unwrap_or_else(|| Arc::clone(&self.backend)),
            _ => Arc::clone(&self.backend),
        }
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
        RuntimeBackendRouter::new(
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
    }

    fn snowluma_agreements(&self) -> SnowLumaAgreementService {
        SnowLumaAgreementService::new(
            self.snowluma_daemon.clone(),
            self.host_resolver.clone(),
            self.runtime_router(),
            Arc::clone(&self.remote_snowluma_daemons),
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

    fn bootstrap_reconciler(&self) -> BootstrapReconciler<R> {
        BootstrapReconciler::new(
            Arc::clone(&self.actors),
            self.host_resolver.clone(),
            Arc::clone(&self.event_bus),
            self.runtime_router(),
            self.remote_runtime_sessions(),
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

    /// 启动时从持久化配置恢复所有 Bot Actor,并自动启动标记了 auto_start 的 Bot
    /// 返回 BootstrapResult,其中 skipped 包含超出 4 开上限而未注册的 Bot ID
    pub async fn bootstrap(&self) -> Result<BootstrapResult, BotManagerError> {
        self.bootstrap_with_auto_start(true).await
    }

    /// 与 [`Self::bootstrap`] 相同，但可关闭 auto_start（例如 Desktop 协议未同意时）。
    pub async fn bootstrap_with_auto_start(
        &self,
        enable_auto_start: bool,
    ) -> Result<BootstrapResult, BotManagerError> {
        let configs = self.repo.list().await?;
        info!(
            target: "ncd_runtime::bot_manager",
            bot_configs = configs.len(),
            enable_auto_start,
            "启动恢复：正在加载 Bot 配置并注册 Actor"
        );

        let mut skipped: Vec<BotId> = Vec::new();

        // 恢复 Actor(不超过 MAX_BOTS),超出的记入 skipped
        {
            let mut actors = self.actors.write().await;
            for config in &configs {
                let bot_id = BotId::new(config.bot.qq_id.to_string());
                if actors.len() >= MAX_BOTS {
                    skipped.push(bot_id);
                    continue;
                }
                actors
                    .entry(bot_id.clone())
                    .or_insert_with(|| BotActorHandle::spawn(bot_id.clone()));
            }
        }

        // 远端 Docker:桌面退出后容器可能仍在跑,先 reconcile attach,避免 auto_start 误 remove
        let reconciled = self
            .bootstrap_reconciler()
            .reconcile_bootstrap_bots(&configs, &skipped)
            .await;

        // 自动启动(只针对已注册的 actor,skipped / 已 reconcile 的不会被启动)
        let auto_start_ids: Vec<BotId> = if enable_auto_start {
            configs
                .iter()
                .filter(|c| c.advanced.auto_start)
                .map(|c| BotId::new(c.bot.qq_id.to_string()))
                .filter(|id| !skipped.contains(id) && !reconciled.contains(id))
                .collect()
        } else {
            Vec::new()
        };

        let started = if auto_start_ids.is_empty() {
            BatchResult {
                succeeded: Vec::new(),
                failed: Vec::new(),
            }
        } else {
            self.batch_start(&auto_start_ids).await?
        };

        if !skipped.is_empty() {
            warn!(
                target: "ncd_runtime::bot_manager",
                skipped = skipped.len(),
                "部分 Bot 超过单机 4 开上限，未注册 Actor"
            );
        }
        info!(
            target: "ncd_runtime::bot_manager",
            auto_started_ok = started.succeeded.len(),
            auto_started_fail = started.failed.len(),
            skipped = skipped.len(),
            reconciled = reconciled.len(),
            "Bot 启动恢复完成"
        );

        Ok(BootstrapResult { started, skipped })
    }

    /// 启动前检测派生配置文件 drift
    ///
    /// 返回 None:派生文件不存在或跟 BotConfig 完全一致,可以直接启动
    /// 返回 Some(drift):有差异,前端应弹 ConfigDriftDialog 让用户抉择
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

    /// 带用户决议启动 Bot
    ///
    /// decisions 来自前端 ConfigDriftDialog,包含:
    /// - AcceptExternal { file, path, value }:渲染输出时把对应 path 覆盖为外部值
    /// - DropAdded { file, path }:不保留新增字段(覆盖时这些字段不出现在 existing 里即可)
    /// - KeepAdded / UseInternal:无需特别处理(默认行为)
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

        // DropAdded 的决议:从 existing 文件里删掉对应 key 再渲染这里不做
        // 额外处理——因为 render_with_existing 只合并 existing 里在 known_keys
        // outside known_keys 的顶层字段如果要 drop 某个顶层扩展字段,需要从 existing map
        // 里主动 remove但当前 render_backend_config 是从磁盘现读的 existing,
        // 要 drop 的字段仍然在磁盘文件里
        //
        // 处理方式:把 DropAdded 转成"用 null 覆盖"→ set_value_at_dot_path 遇到
        // null 会 remove 该 key,效果等价于"用户选择丢弃"
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
        self.remote_runtime_sessions()
            .prepare_before_runtime_start(&config)
            .await;
        self.start_runtime_from_starting(bot_id, &handle, &config)
            .await
    }

    /// 启动指定 Bot(无 drift 决议版本,等价于全部 UseInternal)
    /// 前置条件:Actor 已存在且处于可启动状态(Stopped / Crashed)
    pub async fn start_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        info!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, "收到启动 Bot 请求");
        let handle = self.get_actor(bot_id).await?;
        let config = self.get_required_bot_config(bot_id).await?;
        self.render_backend_config(bot_id, &config, &std::collections::HashMap::new())
            .await?;

        let (starting, advanced) = handle.request_start_transition().await?;
        if !advanced {
            return Ok(starting);
        }
        self.publish_state_change(&starting, "start_requested");
        self.remote_runtime_sessions()
            .prepare_before_runtime_start(&config)
            .await;
        self.start_runtime_from_starting(bot_id, &handle, &config)
            .await
    }

    /// 停止指定 Bot
    pub async fn stop_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        info!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, "收到停止 Bot 请求");
        self.remote_runtime_sessions()
            .mark_remote_docker_stop_expected(bot_id)
            .await;
        let handle = self.get_actor(bot_id).await?;
        let stopping = handle.request_stop().await?;
        self.publish_state_change(&stopping, "stop_requested");

        // 按 deployment_type + runtime_target 选 backend;路由失败必须向上报错
        let cfg = self.get_required_bot_config(bot_id).await?;
        let backend = self.backend_for_lifecycle(&cfg).await?;

        let status = backend.status(bot_id.clone()).await?;
        if status.state == BotActorState::Stopped {
            let stopped = match stopping.state {
                BotActorState::Stopping => handle.confirm_stopped().await?,
                _ => stopping,
            };
            self.dispose_poller(bot_id).await;
            self.remote_runtime_sessions().shutdown_bot(bot_id).await;
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
                self.dispose_poller(bot_id).await;
                self.remote_runtime_sessions().shutdown_bot(bot_id).await;
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
                error!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    err = %message,
                    "停止 Bot 失败（后端 stop 报错）"
                );

                // 传输类错误(远端 SSH 断连,host 刷新失败等):只发信息性 bot_error,
                // 不 mark_crashed,actor 状态保持原样(Running/Stopping)
                // 这样前端看到的是“远端主机临时不可达”,而不是“bot 崩溃”
                if is_remote_transport_error(&err) {
                    self.event_bus.publish(DomainEvent::bot_error(
                        bot_id.clone(),
                        message,
                        Some("远端主机连接中断，停止操作未完成；连接恢复后可重试".to_string()),
                    ));
                    // 不改变 actor 状态,直接返回错误
                    return Err(err.into());
                }

                // 非传输类错误:按原有逻辑标记崩溃
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

    /// 重启指定 Bot
    /// 6 状态分支语义:
    /// - Running | Starting:actor.request_restart()(标记 pending_restart 并转 Stopping)
    ///   → backend.stop(Force) → 等 actor 经 confirm_stopped 转入 Starting → start_bot
    /// - Stopped | Crashed:直接 start_bot
    /// - Stopping:actor.request_restart() 标 pending_restart → 等 actor 转入 Starting → start_bot
    /// - Repairing:返回 BotManagerError::InvalidState
    ///
    /// 设计:复用 BotActor 现有的 pending_restart 机制,不新增状态机分支
    /// 错误返回给调用方;RestartHandle::restart_bot impl 会把
    /// 错误转为 DomainEvent::bot_error 发布给前端
    pub async fn restart_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        info!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, "收到重启 Bot 请求");
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
                // 远端 Native stop 不保证发 BotProcessExited,这里主动清 poller/会话
                self.dispose_poller(bot_id).await;
                self.remote_runtime_sessions().shutdown_bot(bot_id).await;
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

    /// 监听 actor 的 watch::Receiver 直到进入指定 target 状态或超时
    /// 用 watch::Receiver::borrow_and_update 先消化已有快照,再 changed
    /// 等下次更新;超时返回 BotManagerError::Render,邮箱关闭则返回
    /// BotManagerError::Actor(MailboxClosed)
    ///
    /// 当前 restart 路径全部走 fast-path(confirm_stopped 直接推进),不再
    /// 用这个 helper;保留是为了将来真正需要等异步状态转移时可以复用
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

    // ─── 配置管理 ─────────────────────────────────────────────────────────

    /// 新增或更新 Bot 配置
    /// 策略:先持久化 bot.json(source of truth),再写派生文件
    /// - 如果 bot.json 写入失败,派生文件不会被写入,状态完全未变
    /// - 如果派生文件写入失败,bot.json 已是最新,派生文件可在下次启动时重新生成
    ///   不会造成不可恢复的不一致
    /// - 新增时:检查 4 开上限,持久化,写派生文件,创建 Actor
    /// - 更新时:持久化,写派生文件,热推送(通过 restart 通知 Actor 重新加载)
    /// - 如果 backend_type 发生切换(NapCat <-> SnowLuma),必须用旧 backend
    ///   停掉运行中的进程,再用新 backend 启动,避免老进程留尸
    pub async fn upsert_bot_config(
        &self,
        config: BotConfig,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        self.upsert_bot_config_with_overrides(config, &std::collections::HashMap::new())
            .await
    }

    /// 带 drift overrides 的 upsert前端保存时如果检测到 drift 并确认了决议,
    /// 把 overrides 带进来;无 drift 时传空 map
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
        info!(
            target: "ncd_runtime::bot_manager",
            bot_id = %bot_id,
            qq_id = config.bot.qq_id,
            is_new,
            deployment = ?config.bot.deployment_type,
            "保存 Bot 配置"
        );

        if is_new {
            let current_count = {
                let actors = self.actors.read().await;
                actors.len()
            };
            if current_count >= MAX_BOTS {
                return Err(BotManagerError::BotLimitReached);
            }
        }

        // 0. 读旧 config 拿原 backend_type,用于检测 backend 切换走特殊路径
        //    新建 bot 没有旧 config,这步返回 None
        let previous_config: Option<BotConfig> = if is_new {
            None
        } else {
            self.repo.get(config.bot.qq_id).await.ok().flatten()
        };

        // 1. 先持久化 bot.json(source of truth)
        self.repo.upsert(config.clone()).await?;

        // 2. 渲染派生配置文件(走 render_backend_config:读 existing + merge unknown + apply overrides)
        self.render_backend_config(&bot_id, &config, overrides)
            .await?;
        // 清理不再需要的旧 backend 派生文件(例如 NapCat→SL 时删除 onebot11/napcat 文件)
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
            let mut txn = ncd_traits::JsonTransaction::new();
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
                if let Some(prev_config) = previous_config
                    .as_ref()
                    .filter(|prev| prev.bot.backend_type != target_backend)
                {
                    // backend 切换:必须停旧 + 起新,没法热推送(NapCat <-> SL 协议完全不同)
                    let snapshot = self
                        .restart_bot_with_backend_switch(&bot_id, prev_config.clone())
                        .await?;
                    self.publish_state_change(&snapshot, "config_hot_reload");
                    Ok(snapshot)
                } else {
                    // 同 backend 运行中:派生文件已写盘(step 2),尝试通过 WebUI 热推送
                    // NapCat: POST /api/OB11Config/SetConfig (需要 port + auth,当前实装延后)
                    // SnowLuma: POST /api/config/:uin (用 daemon 共享 client)
                    // 热推送失败不阻塞保存流程,只给前端一个 warning;下次重启生效
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
                        // NapCat 热推送:endpoint 表里有 (port, token) 才能继续
                        // (bot 已经把 WebUI 端点报到 stdout 上)没有就只写盘
                        // 等下次启动生效配置 payload 直接复用 renderer 写入
                        // onebot11_{bot}.json 的内容——NapCat WebUI
                        // /api/OB11Config/SetConfig 期望的 schema 与该文件一致
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
                            // 端点没拿到(bot 还没 ready)或者渲染异常 → 配置已落盘,
                            // 等下次重启再生效
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

    /// 切换 backend_type 时的专用 restart:用 previous_config 对应的完整路由
    /// 停掉老进程,再用 start_bot(自动按新 config 选 backend)启动新进程
    ///
    /// 与普通 restart_bot 的关键差异:
    /// - stop 阶段使用旧配置路由,避免远端 / Docker / backend 切换时老进程留尸
    /// - stop 返回后直接 confirm_stopped 推进 actor 到 Starting,不依赖
    ///   异步 BotProcessExited 事件链原因:
    ///   1. backend.stop 是同步 await 的,返回时进程树已被 force kill
    ///   2. 切换 backend 时 actor 上层不一定能立刻收到旧 backend 的 exit 事件
    ///      (例如旧 backend processes map 已被 stop 主动 remove,spawn_exit_watcher
    ///      持有的 child handle 还在等 wait 完成,wait_until_state 会 10s 超时)
    async fn restart_bot_with_backend_switch(
        &self,
        bot_id: &BotId,
        previous_config: BotConfig,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let stopping = handle.request_restart().await?;
        self.publish_state_change(&stopping, "restart_requested");
        // 用旧配置的完整 runtime scenario 停老进程
        self.backend_for_lifecycle(&previous_config)
            .await?
            .stop(bot_id.clone(), StopMode::Force)
            .await?;
        // confirm_stopped 可能跟 exit watcher listener 竞争(参见 restart_bot 注释)
        match handle.confirm_stopped().await {
            Ok(s) => self.publish_state_change(&s, "restart_stopped"),
            Err(crate::bot_actor::BotActorError::InvalidTransition { .. }) => {}
            Err(e) => return Err(e.into()),
        }
        // start_bot 内部按当前 config 重新选 backend,自动用新 flavor 启动
        self.start_bot(bot_id).await
    }

    /// 删除 Bot 配置及其 Actor如果 Bot 正在运行,先停止
    pub async fn delete_bot_config(&self, bot_id: &BotId) -> Result<(), BotManagerError> {
        self.delete_bot_internal(bot_id).await
    }

    // ─── 查询 ─────────────────────────────────────────────────────────────

    /// 获取所有 Bot 的当前快照
    pub async fn list_snapshots(&self) -> Vec<BotActorSnapshot> {
        let actors = self.actors.read().await;
        actors.values().map(|h| h.snapshot()).collect()
    }

    /// 获取指定 Bot 的当前快照
    pub async fn get_snapshot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        Ok(handle.snapshot())
    }

    /// 获取指定 Bot 的当前配置
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

    /// 列出全部 Bot 配置(持久化源)
    pub async fn list_bot_configs(&self) -> Result<Vec<BotConfig>, BotManagerError> {
        self.repo.list().await.map_err(BotManagerError::from)
    }

    /// 供 ncd-watch 同步:优先内存 endpoint(stdout 真实 port/token),
    /// Docker 再回退 secret store 中的 token + 可推导 host 端口。
    /// 不创建新 secret,避免 watch 同步副作用写盘。
    pub async fn napcat_webui_for_watch(&self, bot_id: &BotId) -> Option<(u16, String)> {
        if let Some(ep) = self.napcat_endpoints.snapshot(bot_id).await {
            let port = ep.watch_port();
            if port > 0 && !ep.token.trim().is_empty() {
                return Some((port, ep.token));
            }
        }
        let qq: u64 = bot_id.as_str().parse().ok()?;
        let token = self.peek_napcat_docker_webui_token(qq)?;
        let port = crate::ncd_watch_sync::napcat_docker_webui_host_port(qq);
        Some((port, token))
    }

    /// 列出当前内存中的 NapCat WebUI 端点(Desktop 本机可达 port + token)
    /// 冷启动 / 页面刷新后前端 hydrate 用;多实例时 port 可能是 6099/6100/...
    pub async fn list_napcat_webui_endpoints(&self) -> Vec<(BotId, u16, String)> {
        self.napcat_endpoints
            .list_all()
            .await
            .into_iter()
            .filter(|(_, ep)| ep.port > 0 && !ep.token.trim().is_empty())
            .map(|(id, ep)| (id, ep.port, ep.token))
            .collect()
    }

    /// SnowLuma UI 会话态快照(daemon + per-bot 登录/隧道),冷启动 hydrate 用
    pub async fn list_snowluma_ui_snapshot(&self) -> crate::snowluma_ui_state::SnowLumaUiSnapshot {
        self.snowluma_ui.snapshot().await
    }

    fn peek_napcat_docker_webui_token(&self, qq_id: u64) -> Option<String> {
        let store = self.docker_webui_secret_store.as_ref()?;
        let key = format!("bot:{qq_id}:napcat_docker_webui_token");
        store
            .get(&key)
            .ok()
            .flatten()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// 批量返回所有 Bot 的 backend_type,用于 UI 列表页一次性拿 flavor map
    /// 避免 BotListPage 对每个 bot 单独调 get_bot_config 造成 N+1
    /// key 为 BotId.to_string()(即 QQID 数字字符串)
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

    /// 当前托管的 Bot 数量
    pub async fn bot_count(&self) -> usize {
        self.actors.read().await.len()
    }

    /// 当前活跃(Starting / Running / Stopping)的 Bot 数量
    pub async fn active_count(&self) -> usize {
        let actors = self.actors.read().await;
        actors
            .values()
            .filter(|h| h.snapshot().state.is_active())
            .count()
    }

    /// 本机 runtime_target 且处于活跃态的 Bot 数量(退出拦截用)
    pub async fn count_local_active_bots(&self) -> Result<usize, BotManagerError> {
        self.count_active_bots_by_host(|t| t.is_local()).await
    }

    /// 远端 runtime_target 且处于活跃态的 Bot 数量(退出提示用,不拦截退出)
    pub async fn count_remote_active_bots(&self) -> Result<usize, BotManagerError> {
        self.count_active_bots_by_host(|t| !t.is_local()).await
    }

    /// 组件页 host_id(`local` / `remote:{server_id}`)上活跃 Bot 数
    /// Starting / Running / Stopping 均算活跃;与组件 update/uninstall 门禁一致
    pub async fn count_active_bots_on_component_host(
        &self,
        host_id: &str,
    ) -> Result<usize, BotManagerError> {
        let host_id = host_id.trim();
        if host_id.is_empty() {
            return Ok(0);
        }
        if host_id == "local" {
            return self.count_local_active_bots().await;
        }
        let Some(server_id) = host_id.strip_prefix("remote:") else {
            // 未知 host 形态不按本机兜底,避免误拦/误放
            return Ok(0);
        };
        if server_id.is_empty() {
            return Ok(0);
        }
        self.count_active_bots_by_host(|t| t.server_id() == Some(server_id))
            .await
    }

    async fn count_active_bots_by_host(
        &self,
        host_match: impl Fn(&RuntimeTarget) -> bool,
    ) -> Result<usize, BotManagerError> {
        let configs = self.repo.list().await?;
        // bot_id 字符串 → runtime_target,避免对每个 snapshot 线性扫 configs 并 to_string
        let target_by_bot_id: HashMap<String, &RuntimeTarget> = configs
            .iter()
            .map(|c| (c.bot.qq_id.to_string(), &c.bot.runtime_target))
            .collect();
        let snapshots = self.list_snapshots().await;
        Ok(snapshots
            .iter()
            .filter(|s| s.state.is_active())
            .filter(|s| {
                target_by_bot_id
                    .get(s.bot_id.as_str())
                    .is_some_and(|t| host_match(t))
            })
            .count())
    }

    /// 桌面进程退出:仅停止本机 Bot;远端(Docker / 直接运行)不停远端进程,只拆掉本机隧道/日志会话
    pub async fn exit_desktop(&self) -> BatchResult {
        info!(
            target: "ncd_runtime::bot_manager",
            "桌面退出：停止本机 Bot，远端保持运行"
        );
        let configs = self.repo.list().await.unwrap_or_default();
        let snapshots: Vec<BotActorSnapshot> = {
            let actors = self.actors.read().await;
            actors.values().map(|h| h.snapshot()).collect()
        };

        let local_active_ids: Vec<BotId> = snapshots
            .iter()
            .filter(|s| s.state.is_active() || matches!(s.state, BotActorState::Running))
            .filter(|s| {
                configs
                    .iter()
                    .find(|c| c.bot.qq_id.to_string() == s.bot_id.as_str())
                    .map(|c| c.bot.runtime_target.is_local())
                    .unwrap_or(false)
            })
            .map(|s| s.bot_id.clone())
            .collect();

        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };

        for bot_id in &local_active_ids {
            match self.stop_bot(bot_id).await {
                Ok(_) => result.succeeded.push(bot_id.clone()),
                Err(err) => result.failed.push((bot_id.clone(), err)),
            }
        }

        if let Some(daemon) = self.snowluma_daemon.as_ref() {
            daemon.shutdown().await;
        }

        self.remote_snowluma_tunnels.shutdown_all().await;
        {
            let daemons = self.remote_snowluma_daemons.lock().await;
            for daemon in daemons.values() {
                daemon.detach_local_sessions().await;
            }
        }

        self.remote_runtime_sessions().shutdown_all().await;

        result
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

    /// 拉取指定 Bot 的最近 lines 行日志快照
    /// 返回 [LogSnapshot],包含已截尾的日志行 + 总行数供 UI 在 BotLogPage
    /// 初次开页时一次性加载历史,再叠加 bot_log_appended / snowluma_daemon_log
    /// 实时事件对齐 legacy NapCatQQProcessLog.get_log_content 行为:本地是
    /// 内存 deque 快照(进程存活期间累计的全量),进程被 stop / 重启时缓冲清零
    ///
    /// 必须按 bot 当前配置的 backend 路由,不能写死走默认 NapCat backend:
    /// 否则 SnowLuma flavor 的 bot 会去 NapCat backend 拉历史,拿到的是磁盘
    /// 归档里 NC 旧日志,配置切换 NC → SL 后用户看到的依然是 NC 的内容
    pub async fn tail_log(
        &self,
        bot_id: &BotId,
        lines: usize,
    ) -> Result<ncd_traits::runtime_backend::LogSnapshot, BotManagerError> {
        // Actor 不存在直接返回空,UI 不需要为此报错
        if !self.actors.read().await.contains_key(bot_id) {
            return Ok(ncd_traits::runtime_backend::LogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            });
        }
        let opts = ncd_traits::runtime_backend::TailOpts { lines };
        match self.get_bot_config(bot_id).await? {
            Some(cfg) => {
                let backend = self.backend_for_lifecycle(&cfg).await?;
                backend
                    .tail_log(bot_id.clone(), opts)
                    .await
                    .map_err(BotManagerError::from)
            }
            None => Ok(ncd_traits::runtime_backend::LogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            }),
        }
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

    /// 渲染派生配置文件
    ///
    /// 调用 renderer.render_with_existing 而不是 render,让 NapCat / SnowLuma
    /// renderer 把磁盘上派生文件里用户加的扩展字段(如 imageDownloadProxy,
    /// autoTimeSync)合并进新输出,避免每次启动覆盖时丢掉用户的手改
    ///
    /// overrides 来自前端 ConfigDriftDialog 的 AcceptExternal 决议:先按
    /// 默认 BotConfig 渲染输出,再用 overrides 把对应 JSON path 的值换成外部值
    /// 没有决议时传空 map
    async fn render_backend_config(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        overrides: &std::collections::HashMap<String, Vec<(String, serde_json::Value)>>,
    ) -> Result<(), BotManagerError> {
        // 1. 把现有派生文件读进来(不存在的跳过)
        let mut existing: std::collections::HashMap<std::path::PathBuf, serde_json::Value> =
            std::collections::HashMap::new();
        for path in self.renderer.output_paths(bot_id) {
            match tokio::fs::read(&path).await {
                Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(value) => {
                        existing.insert(path, value);
                    }
                    Err(_) => {
                        // 派生文件被人改坏了,无法 parse;当作"不存在"处理,下面
                        // render_with_existing 会写一份干净的覆盖
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

        // 2. 渲染(合并 unknown 顶层字段)
        let mut txn = self
            .renderer
            .render_with_existing(bot_id, config, &existing)?;
        if txn.is_empty() {
            return Ok(());
        }

        // 3. 应用 overrides:按文件名找 write 项,按 dot-path 替换值
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
                        set_value_at_dot_path(&mut write.payload, path, value.clone()).map_err(
                            |e| {
                                BotManagerError::Render(format!(
                                    "应用 drift 决议到 {file_name} 的 {path} 失败: {e}"
                                ))
                            },
                        )?;
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

    fn snowluma_daemon_scope_for_config(config: &BotConfig) -> Option<String> {
        if config.bot.backend_type != BackendType::SnowLuma {
            return None;
        }
        match &config.bot.runtime_target {
            RuntimeTarget::Local => Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
            RuntimeTarget::Server(sid) => Some(sid.clone()),
        }
    }

    fn skip_post_start_status_recheck(config: &BotConfig) -> bool {
        config.bot.backend_type == BackendType::SnowLuma && config.bot.runtime_target.is_local()
    }

    async fn ensure_snowluma_agreements_ready(
        &self,
        config: &BotConfig,
    ) -> Result<(), BotManagerError> {
        if config.bot.backend_type != BackendType::SnowLuma {
            return Ok(());
        }
        if let Some(payload) = self.snowluma_agreements().prepare(config).await? {
            return Err(Self::snowluma_consent_required_error(&payload.version));
        }
        Ok(())
    }

    fn snowluma_consent_required_error(version: &str) -> BotManagerError {
        BotManagerError::Runtime(BotBackendError::Io(format!(
            "SNOWLUMA_CONSENT_REQUIRED: SnowLuma agreements version {version} requires consent"
        )))
    }

    async fn snowluma_docker_agreements_after_attach(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<Option<AgreementsPayload>, BotManagerError> {
        let RuntimeScenario::RemoteDocker {
            backend: BackendType::SnowLuma,
            ..
        } = RuntimeScenario::from_config(config)?
        else {
            return Ok(None);
        };
        let Some(endpoint) = self
            .remote_runtime_sessions()
            .snowluma_docker_endpoints(bot_id)
            .await
        else {
            return Ok(None);
        };
        let client = match ReqwestSnowLumaWebUiClient::new(
            endpoint.webui_local_port,
            endpoint.webui_password,
        ) {
            Ok(client) => client,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    err = %err,
                    "SnowLuma Docker 协议检查: WebUI client 初始化失败"
                );
                return Ok(None);
            }
        };
        if let Err(err) = client
            .wait_ready(Duration::from_secs(20), Box::new(|| false))
            .await
        {
            warn!(
                target: "ncd_runtime::bot_manager",
                bot_id = %bot_id,
                err = %err,
                "SnowLuma Docker 协议检查: WebUI 未就绪"
            );
            return Ok(None);
        }
        if let Err(err) = client.login().await {
            warn!(
                target: "ncd_runtime::bot_manager",
                bot_id = %bot_id,
                err = %err,
                "SnowLuma Docker 协议检查: WebUI 登录失败"
            );
            return Ok(None);
        }
        match client.get_agreements().await {
            Ok(payload) => Ok(Some(payload)),
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    err = %err,
                    "SnowLuma Docker 协议检查: 读取协议失败"
                );
                Ok(None)
            }
        }
    }

    async fn start_runtime_from_starting(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        config: &BotConfig,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        // 取消令牌:stop_bot 在 Starting 阶段会 cancel,检测到后静默退出,避免重复报错
        let cancel = handle.cancellation_token();

        // 实例运行时指标：启动时注入（开关关则清理）
        self.apply_runtime_metrics_inject(bot_id, config).await;

        let scenario = RuntimeScenario::from_config(config)?;
        let mut runtime_config = match &scenario {
            RuntimeScenario::LocalNative { .. } => {
                let base = self.build_runtime_config(bot_id, config);
                match self.launch_planner.build_plan(bot_id, config).await {
                    Ok(plan) => plan.into_runtime_config(base),
                    Err(err) => {
                        let message = err.to_string();
                        error!(
                            target: "ncd_runtime::bot_manager",
                            bot_id = %bot_id,
                            err = %message,
                            "构造 Bot 启动计划失败"
                        );
                        let hint = match &err {
                            RuntimeLaunchPlanError::SnowLumaNodeMissing(path) => Some(format!(
                                "未在 {} 找到 SnowLuma daemon 二进制。请安装 SnowLuma 运行时组件，或在 Bot 配置中把后端类型切换为 NapCat。",
                                path.display()
                            )),
                            RuntimeLaunchPlanError::SnowLumaInvalidStartMode(detail) => {
                                Some(format!(
                                    "SnowLuma 启动参数无效：{detail}。请在 Bot 配置中检查启动模式。"
                                ))
                            }
                            RuntimeLaunchPlanError::MissingFile { .. } => Some(
                                "NapCat 运行时组件缺失，请先在「设置」页安装运行时。".to_string(),
                            ),
                            _ => Some(
                                "启动计划构造失败：请检查后端类型与运行时安装状态。".to_string(),
                            ),
                        };
                        let crashed = handle.mark_crashed(message.clone()).await?;
                        self.publish_state_change(&crashed, "start_failed");
                        self.event_bus.publish(DomainEvent::bot_error(
                            bot_id.clone(),
                            message,
                            hint,
                        ));
                        return Err(BotManagerError::Render(err.to_string()));
                    }
                }
            }
            RuntimeScenario::RemoteNative { .. } | RuntimeScenario::RemoteDocker { .. } => {
                self.build_runtime_config(bot_id, config)
            }
        };
        self.merge_metrics_env_into_runtime_config(bot_id, config, &mut runtime_config);

        if let Err(err) = self.ensure_snowluma_agreements_ready(config).await {
            if cancel.is_cancelled() {
                return Err(BotManagerError::Cancelled);
            }
            let message = err.to_string();
            let crashed = handle.mark_crashed(message.clone()).await?;
            self.publish_state_change(&crashed, "start_failed");
            self.event_bus.publish(DomainEvent::bot_error(
                bot_id.clone(),
                message,
                Some("请先阅读并同意 SnowLuma 用户协议与隐私政策后重试启动。".to_string()),
            ));
            return Err(err);
        }

        let backend = match self.backend_for_config(config).await {
            Ok(b) => b,
            Err(err) => {
                if cancel.is_cancelled() {
                    return Err(BotManagerError::Cancelled);
                }
                let message = err.to_string();
                let crashed = handle.mark_crashed(message.clone()).await?;
                self.publish_state_change(&crashed, "start_failed");
                self.event_bus
                    .publish(DomainEvent::bot_error(bot_id.clone(), message, None));
                return Err(err);
            }
        };
        if cancel.is_cancelled() {
            return Err(BotManagerError::Cancelled);
        }
        match backend
            .start(&BotStartCtx {
                config: runtime_config,
                bot_config: Some(config.clone()),
            })
            .await
        {
            Ok(status) => {
                self.event_bus
                    .publish(DomainEvent::bot_status_changed(status, "runtime_start"));
                // 防快速退出竞态:backend.start Ok 只代表 spawn / compose up 成功,
                // 进程可能在 confirm_running 之前就崩了Starting 阶段的
                // BotProcessExited 被 handle_process_exited 有意忽略(无法区分本轮新
                // 进程退出与 restart fast-path 旧进程退出),所以这里启动后立即复查一次
                // backend.status:若已落到 Stopped / Crashed,直接按崩溃收口,避免 actor
                // 与 UI 停在假 Running复查本身报错时不阻断(查不到不代表没起来)
                // 远端 SL / 本机 SL:启动后立刻 status 复查易误判(远端 status 文件,本机 processes 写入竞态)
                if !Self::skip_post_start_status_recheck(config) {
                    let mut fail_detail = None;
                    for pass in 0..2u8 {
                        if pass == 1 {
                            tokio::time::sleep(Duration::from_millis(400)).await;
                        }
                        if let Ok(observed) = backend.status(bot_id.clone()).await {
                            if matches!(
                                observed.state,
                                BotActorState::Stopped | BotActorState::Crashed
                            ) {
                                fail_detail = Some(
                                    observed
                                        .extra
                                        .get("reason")
                                        .and_then(|v| v.as_str())
                                        .map(str::to_string)
                                        .unwrap_or_else(|| "进程启动后立即退出".to_string()),
                                );
                                break;
                            }
                        }
                    }
                    if let Some(detail) = fail_detail {
                        let crashed = handle.mark_crashed(detail.clone()).await?;
                        self.publish_state_change(&crashed, "start_failed");
                        self.event_bus.publish(DomainEvent::bot_error(
                            bot_id.clone(),
                            detail.clone(),
                            Some(
                                "Bot 启动后立即退出,请检查启动命令、运行时依赖与日志。".to_string(),
                            ),
                        ));
                        return Err(BotManagerError::Render(detail));
                    }
                }
                let mut attached_after_runtime_start = false;
                if matches!(
                    &scenario,
                    RuntimeScenario::RemoteDocker {
                        backend: BackendType::SnowLuma,
                        ..
                    }
                ) {
                    self.remote_runtime_sessions()
                        .attach_after_runtime_start(bot_id, config)
                        .await;
                    attached_after_runtime_start = true;
                    if let Some(payload) = self
                        .snowluma_docker_agreements_after_attach(bot_id, config)
                        .await?
                        && payload.consent_required
                    {
                        if cancel.is_cancelled() {
                            return Err(BotManagerError::Cancelled);
                        }
                        let err = Self::snowluma_consent_required_error(&payload.version);
                        let message = err.to_string();
                        let crashed = handle.mark_crashed(message.clone()).await?;
                        self.publish_state_change(&crashed, "start_failed");
                        self.event_bus.publish(DomainEvent::bot_error(
                            bot_id.clone(),
                            message,
                            Some(
                                "请先阅读并同意 SnowLuma 用户协议与隐私政策后重试启动。"
                                    .to_string(),
                            ),
                        ));
                        return Err(err);
                    }
                }
                let running = handle.confirm_running().await?;
                self.publish_state_change(&running, "start_completed");
                if !attached_after_runtime_start {
                    self.remote_runtime_sessions()
                        .attach_after_runtime_start(bot_id, config)
                        .await;
                }
                info!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    "Bot 已启动并就绪"
                );
                Ok(running)
            }
            Err(err) => {
                if cancel.is_cancelled() {
                    return Err(BotManagerError::Cancelled);
                }
                let message = err.to_string();
                error!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    err = %message,
                    "启动 Bot 失败（后端 start 报错）"
                );
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

    /// 应用退出收口:尝试停止所有运行中的 Bot 并 shutdown 它们的 Actor
    /// 用法:Tauri WindowEvent::CloseRequested 时调用,避免 QQ.exe 残留
    /// 行为:
    /// - 对所有处于 active 状态的 Bot 调用 stop_bot(内部走 kill_process_tree)
    /// - 不论 stop 是否成功,都会 shutdown 对应的 actor 释放邮箱
    /// - 任何错误只记录到返回值,不会阻塞其它 Bot 的清理
    pub async fn shutdown_all(&self) -> BatchResult {
        info!(target: "ncd_runtime::bot_manager", "应用退出：正在停止所有运行中的 Bot");
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

        // 关闭所有 actor 邮箱,释放后台任务
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

        // dispose 所有 NapCatLoginPoller,取消其后台轮询任务
        {
            let mut pollers = self.login_pollers.write().await;
            for (_, poller) in pollers.drain() {
                poller.dispose();
            }
        }

        // SnowLuma daemon 优雅关闭
        if let Some(daemon) = self.snowluma_daemon.as_ref() {
            daemon.shutdown().await;
        }

        self.remote_runtime_sessions().shutdown_all().await;

        result
    }

    /// 远端 SnowLuma Docker 隧道端口(供 Tauri open_snowluma_webui)
    pub async fn snowluma_docker_endpoints(
        &self,
        bot_id: &BotId,
    ) -> Option<crate::docker_bot_session::SnowLumaDockerEndpoints> {
        self.remote_runtime_sessions()
            .snowluma_docker_endpoints(bot_id)
            .await
    }

    /// 远端 SnowLuma Native:按 SSH server_id 查本机隧道端口(多 Bot 同机共享)
    pub async fn snowluma_native_endpoints_for_server(
        &self,
        server_id: &str,
    ) -> Option<ncd_backend_snowluma::remote_snowluma_tunnel::RemoteSnowLumaTunnelEndpoints> {
        self.remote_snowluma_tunnels
            .endpoints_for_server(server_id)
            .await
    }

    /// SnowLuma 协议预检：只确保 daemon/WebUI 可用，不启动 QQ，也不调用 load_process。
    ///
    /// 本地与远端 Native 都能在 Bot 启动前准备 WebUI；Docker 的 WebUI 跟容器生命周期
    /// 绑定，容器创建前没有端点，因此仍走启动后检测路径。
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

    /// 内部删除流程:停止旧 runtime → 持久化删除 → shutdown → 移除内存 Actor
    /// 停止运行中 Bot 必须发生在 repo.delete 前;否则旧 config/backend identity
    /// 丢失后只能按新默认路由停进程,远端或 Docker 场景会留下真实 runtime
    async fn delete_bot_internal(&self, bot_id: &BotId) -> Result<(), BotManagerError> {
        info!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, "删除 Bot 配置与 Actor");
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
        self.remote_runtime_sessions().shutdown_bot(bot_id).await;

        self.event_bus.publish(DomainEvent::BotStateChanged {
            snapshot: BotActorSnapshot::new(bot_id.clone()),
            reason: Some("bot_deleted".to_string()),
        });

        Ok(())
    }

    /// 把一份 OneBot 配置 payload 通过 NapCat WebUI 热推送给运行中的 bot
    ///
    /// 调用链:login(fetch_credential)→ check_login_status → set_ob11_config
    /// 任何一步失败都不阻塞保存——已经写盘了,最差就是等下次重启生效返回
    /// 一个 BotStateChanged 的 reason 字符串,让前端区分提示:
    /// - config_hot_reloaded:推送成功,配置已生效
    /// - config_saved_pending_login:QQ 还没扫码,待登录后下次启动生效
    /// - config_saved_pending_reload:网络 / 401 / 业务错误,等下次重启生效
    async fn push_napcat_hot_reload(
        &self,
        endpoint: NapCatEndpoint,
        payload: serde_json::Value,
    ) -> &'static str {
        let NapCatEndpoint {
            port,
            token,
            host_port: _,
        } = endpoint;
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
        // QQ 未登录时 set_ob11_config 会返回 NotLogin;提前查一把可以让
        // 前端拿到更准确的语义(避免把"等扫码"误显示成"推送失败")
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
