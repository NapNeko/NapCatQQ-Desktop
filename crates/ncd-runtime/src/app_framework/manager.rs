//! AppManager：应用实例表 + 生命周期 + 「协议 Bot 一键对接应用端」编排
//!
//! 对接 = 往 Bot 里按名 upsert 一条连接，再调 `BotManager::upsert_bot_config`（持久化 + 渲染 +
//! 热推）。不新写推送链路。反向进 `connect.websocket_clients`（Bot 连应用端）；正向进
//! `connect.websocket_servers`（Bot 开服务端，应用端来连，听口由这里在 Bot 主机上分配）。
//! 反向拓扑按主机分、不看 BackendType：同机走实例口；本机 Bot→远端应用 SSH `-L`；远端 Bot→本机应用 SSH `-R`；
//! 两台远端走应用机常驻 `ssh -R`（Desktop 只编排）。正向的听口 P 在 Bot 侧，隧道方向全反：远端 Bot→本机应用
//! 桌面端 `-L`，本机 Bot→远端应用桌面端对应用机 `-R`，两台远端应用机常驻 `ssh -L`；Bot 写 P，应用端连隧道口 Q。
//! Docker 部署的 Bot 不给对接：容器里的 127.0.0.1 不是宿主机。
//!
//! 安装本身走既有 ComponentExecutor（R12），这里只给 hint、置 Installing、盯任务结束后
//! 用 detect 对账；框架差异全部封在 `ncd_appframework::AppFrameworkAdapter` 后面。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ncd_appframework::{
    AppComponentSpec, AppConfigWriteResult, AppFrameworkAdapter, AppFrameworkRegistry,
    AppInstanceConfig, AppInstanceConfigEnvelope, AppStoreFlavor, AppStoreInstalled,
    AppStoreMarketEntry, AstrBotAbconfInfo, AstrBotDashboardStatus, AstrBotKbCreate,
    AstrBotKnowledgeBase, AstrBotPersona, AstrBotRuntimeApi, AstrBotSession, AstrBotSessionRule,
    KarinPluginInstalled, MaiBotAPIProvider, MaiBotBehaviorDetail, MaiBotBehaviorOverview,
    MaiBotBehaviorPage, MaiBotBehaviorQuery, MaiBotChatSession, MaiBotChatTicket, MaiBotMCPServerItemConfig,
    MaiBotEmojiAction, MaiBotEmojiImage, MaiBotEmojiOverview, MaiBotEmojiPage, MaiBotEmojiQuery,
    MaiBotEmojiUpload, MaiBotEmojiUploadDone, MaiBotLocalImage,
    MaiBotExpressionAction, MaiBotExpressionOverview, MaiBotExpressionPage, MaiBotExpressionQuery,
    MaiBotJargonAction, MaiBotJargonOverview, MaiBotJargonPage, MaiBotJargonQuery, MaiBotResourceDone,
    MaiBotPersonAction, MaiBotPersonOverview, MaiBotPersonPage, MaiBotPersonQuery,
    MaiBotLocalTextFile, MaiBotMemoryDeleteAction, MaiBotMemoryDeleteOp, MaiBotMemoryDeleteResult,
    MaiBotMemoryGraph, MaiBotMemoryGraphHit, MaiBotMemoryImport, MaiBotMemoryImportSetup,
    MaiBotMemoryNodeDetail, MaiBotMemoryQuery, MaiBotMemoryRecordDetail, MaiBotMemoryRecordKind,
    MaiBotMemoryRecordPage, MaiBotMemorySource, MaiBotMemoryStatus, MaiBotMemoryTask,
    MaiBotMemoryTaskAction, MaiBotMemoryTaskDetail,
    MaiBotMcpStatus, MaiBotMcpTest, MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile,
    MaiBotPromptTarget, MaiBotProviderCheck, MaiBotProviderModel, MaiBotProviderSource,
    MaiBotRuntimeApi, MaiBotRuntimeGate, MaiBotRuntimeStatus, MaiBotSession, MaiBotStatsSummary,
    KarinPluginMarketEntry, PluginLogSink, app_file_basename, restore_adopted_files,
    remove_ncd_debris, AdoptRestoreScope,
};
use ncd_component::{DetectOutcome, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigText, AppFrameworkId, AppFrameworkManifest, AppInstance,
    AppInstanceId, AppInstanceState, AppLinkRecord, AppLinkTopology, AppPlacement, AppPluginAction,
    AppInstanceOrigin, AppPendingTerms, AppPluginConfigSchema, AppProjectProbe, AppStoreResource,
    AppWebUiAccount,
    AppWebUiAuthKind, BotConfig, BotId,
    CreateAppInstanceRequest, DeploymentType, DomainEventKind, ImportAppInstanceRequest, LOCAL_HOST_ID,
    REMOTE_HOST_ID_PREFIX,
    OneBotLinkEndpoint, OneBotLinkMode, OneBotLinkPlan, RuntimeTarget, app_link_connection_name,
    classify_app_link, host_id_of_runtime_target, is_app_link_connection_name, parse_ws_url,
    rewrite_ws_loopback_port, runtime_target_matches_host, server_id_of_host,
};
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use ncd_server::HostResolver;
use ncd_traits::{AppFrameworkError, EventBus, EventFilter, SecretStore};
use ncd_traits::runtime_backend::LogSnapshot;
use rand::Rng;
use rand::distributions::Alphanumeric;

use super::adopt::{self, AdoptStore};
use super::instances::AppInstanceStore;
use super::listen_port::{
    allocate_listen_port, allocate_stable_port, local_port_free, remote_listening_ports,
};
use super::native_runtime::{self, AppLaunchSpec, NativeAppRuntime};
use super::resident_link::{self, ResidentForward, ResidentLinkSpec};
// 子模块里的 `super::supervisor::…` 这类路径经由这几条落到 app_framework 下的同名模块
use super::{download, existing_link, log_tail, plugin_market, supervisor};
use crate::bot_manager::BotManager;
use crate::components::{AppComponentHint, data_root_to_host_path};
use crate::deploy::DeploymentTaskManager;
use crate::events::{BroadcastEventBus, DomainEvent};
use crate::metrics::now_ms;

mod astrbot;
mod config;
mod install_dir;
mod lifecycle;
mod link;
mod maibot;
mod store;
mod terminal;
mod tunnel;
mod webui;

pub use link::{app_link_connections, upsert_ws_client};
pub use terminal::AppTerminalContext;
use install_dir::parse_user_install_dir;
use tunnel::AppInstanceTunnel;
use webui::WebUiEndpoint;

/// SecretStore 里 WebUI 账号的键后缀（`app:<instance_id>:<suffix>`）；明文只存这里，实例记录不带
const SECRET_WEBUI_USERNAME: &str = "webui_username";
const SECRET_WEBUI_PASSWORD: &str = "webui_password";

fn secret_key(instance_id: &str, suffix: &str) -> String {
    format!("app:{instance_id}:{suffix}")
}

/// 盯安装时事件之外的兜底：隔这么久查一次任务队列
const INSTALL_POLL_INTERVAL: Duration = Duration::from_secs(2);

enum InstallEnd {
    Finished {
        status: ncd_domain::DeploymentTaskStatus,
        error: Option<String>,
    },
    /// 队列里已经查不到这个任务（终态后被清掉了），结果只能看目录
    Vanished,
}

/// 用户给的密码过框架口令策略；没给（或空）就按框架策略生成
fn resolve_webui_password(
    adapter: &dyn AppFrameworkAdapter,
    requested: Option<String>,
) -> Result<String, AppFrameworkError> {
    match requested.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
        Some(p) => {
            adapter
                .validate_webui_password(&p)
                .map_err(|m| AppFrameworkError::ConfigInvalid(vec![ncd_domain::AppConfigIssue::new("webui_password", m)]))?;
            Ok(p)
        }
        None => Ok(adapter.generate_webui_password()),
    }
}

/// AppManager 对协议 Bot 侧唯一的依赖：读配置 + 走 `upsert_bot_config` 热推。
/// 抽成 trait 是为了不把 BotManager 的两个泛型参数带进来，也方便测试。
#[async_trait::async_trait]
pub trait BotConfigPort: Send + Sync {
    async fn bot_config(&self, bot_id: &BotId) -> Result<Option<BotConfig>, String>;
    async fn upsert_bot_config(&self, config: BotConfig) -> Result<(), String>;
    async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
        Ok(Vec::new())
    }
}

#[async_trait::async_trait]
impl<R, S> BotConfigPort for BotManager<R, S>
where
    R: ncd_traits::BotConfigRepo + 'static,
    S: ncd_traits::ConfigStore + 'static,
{
    async fn bot_config(&self, bot_id: &BotId) -> Result<Option<BotConfig>, String> {
        BotManager::get_bot_config(self, bot_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn upsert_bot_config(&self, config: BotConfig) -> Result<(), String> {
        BotManager::upsert_bot_config(self, config)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
        self.list_bot_configs().await.map_err(|e| e.to_string())
    }
}

/// `date +%z` 的输出（`+0800` / `-0530`）→ 秒
fn parse_utc_offset(raw: &str) -> Option<i32> {
    let s = raw.trim();
    let (sign, digits) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits[2..].parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

async fn remote_utc_offset(host: &dyn Host) -> Option<i32> {
    let cmd = HostCommand::new("date").arg("+%z").timeout(Duration::from_secs(10));
    let out = host.run_to_string(cmd).await.ok()?;
    parse_utc_offset(&out.stdout)
}

/// 桌面端起的进程输出写到哪：框架指定了就用框架的，没指定落到实例目录的 `.ncd-app.log`
fn launch_log_file(adapter: &dyn AppFrameworkAdapter, instance: &AppInstance) -> HostPath {
    adapter
        .log_file(instance)
        .unwrap_or_else(|| HostPath::from_posix(&instance.install_dir).join(".ncd-app.log"))
}

/// 应用端现在去连的口（它自己配置里写的）；没对接、适配器关着就是 None
async fn adapter_link_port(
    adapter: &dyn AppFrameworkAdapter,
    host: &dyn Host,
    instance: &AppInstance,
) -> Option<u16> {
    let urls = adapter.read_outbound_ws_urls(host, instance).await.ok()?;
    let parts = parse_ws_url(urls.first()?)?;
    (parts.port != 0).then_some(parts.port)
}

fn needs_desktop_ssh_tunnel(topology: AppLinkTopology) -> bool {
    matches!(
        topology,
        AppLinkTopology::LocalBotRemoteApp | AppLinkTopology::RemoteBotLocalApp
    )
}

/// 正向对接的桌面端隧道单独一个键：反向那条（键是实例 id）的端口语义不同，别互相认错
fn forward_tunnel_key(id: &AppInstanceId) -> String {
    format!("{}:fwd", id.as_str())
}

fn unsupported_link_topology(bot_target: &RuntimeTarget, app_host_id: &str) -> AppFrameworkError {
    AppFrameworkError::Validation(format!(
        "不支持该对接拓扑：Bot 在 {}，应用实例在 {}",
        describe_target(bot_target),
        describe_host(app_host_id)
    ))
}

pub struct AppManager {
    registry: Arc<AppFrameworkRegistry>,
    store: Arc<AppInstanceStore>,
    runtime: Arc<NativeAppRuntime>,
    host_resolver: Arc<dyn HostResolver>,
    bot_manager: Arc<dyn BotConfigPort>,
    event_bus: Arc<BroadcastEventBus>,
    data_root: PathBuf,
    adopt_store: AdoptStore,
    npm_registry: Option<String>,
    /// 用户名密码类 WebUI 的明文密码落点；None（测试）时创建实例不种密码，交给框架首启自生成
    secrets: Option<Arc<dyn SecretStore + Send + Sync>>,
    /// Desktop 握着的跨机隧道。key = instance id；解绑 / 删实例 / 改端口时释放。
    tunnels: tokio::sync::Mutex<HashMap<String, AppInstanceTunnel>>,
    /// 正在盯的安装：instance id → task id。不在表里的「安装中」是上次装到一半桌面端退了
    install_watches: std::sync::Mutex<HashMap<AppInstanceId, String>>,
    /// 运行中写配置的下一个可用时刻（应用要求两次写之间留间隔时才记）
    config_write_slots: std::sync::Mutex<HashMap<AppInstanceId, Instant>>,
    webui_endpoints: std::sync::Mutex<HashMap<AppInstanceId, WebUiEndpoint>>,
    market_cache: plugin_market::MarketCache,
}

impl AppManager {
    pub fn new(
        registry: Arc<AppFrameworkRegistry>,
        store: Arc<AppInstanceStore>,
        runtime: Arc<NativeAppRuntime>,
        host_resolver: Arc<dyn HostResolver>,
        bot_manager: Arc<dyn BotConfigPort>,
        event_bus: Arc<BroadcastEventBus>,
        data_root: &Path,
    ) -> Self {
        Self {
            registry,
            store,
            runtime,
            host_resolver,
            bot_manager,
            event_bus,
            data_root: data_root.to_path_buf(),
            adopt_store: AdoptStore::new(data_root),
            npm_registry: None,
            secrets: None,
            tunnels: tokio::sync::Mutex::new(HashMap::new()),
            install_watches: std::sync::Mutex::new(HashMap::new()),
            config_write_slots: std::sync::Mutex::new(HashMap::new()),
            webui_endpoints: std::sync::Mutex::new(HashMap::new()),
            market_cache: plugin_market::MarketCache::default(),
        }
    }

    pub fn with_npm_registry(mut self, registry: Option<String>) -> Self {
        self.npm_registry = registry.filter(|s| !s.trim().is_empty());
        self
    }

    pub fn with_secret_store(mut self, secrets: Arc<dyn SecretStore + Send + Sync>) -> Self {
        self.secrets = Some(secrets);
        self
    }

    fn remembered_secret(&self, instance: &AppInstance, suffix: &str) -> Option<String> {
        let store = self.secrets.as_ref()?;
        match store.get(&secret_key(instance.id.as_str(), suffix)) {
            Ok(v) => v.filter(|s| !s.is_empty()),
            Err(e) => {
                tracing::warn!(instance = instance.id.as_str(), suffix, error = %e, "read app secret");
                None
            }
        }
    }

    fn remember_secret(&self, instance_id: &AppInstanceId, suffix: &str, value: &str) {
        let Some(store) = self.secrets.as_ref() else {
            return;
        };
        if let Err(e) = store.put(&secret_key(instance_id.as_str(), suffix), value) {
            tracing::warn!(instance = instance_id.as_str(), suffix, error = %e, "store app secret");
        }
    }

    fn forget_secrets(&self, instance_id: &AppInstanceId) {
        let Some(store) = self.secrets.as_ref() else {
            return;
        };
        for suffix in [SECRET_WEBUI_USERNAME, SECRET_WEBUI_PASSWORD] {
            if let Err(e) = store.delete(&secret_key(instance_id.as_str(), suffix)) {
                tracing::debug!(instance = instance_id.as_str(), suffix, error = %e, "drop app secret");
            }
        }
    }

    pub fn runtime(&self) -> &Arc<NativeAppRuntime> {
        &self.runtime
    }

    // ---- 查询 ----

    pub fn list_frameworks(&self) -> Vec<AppFrameworkManifest> {
        self.registry.manifests()
    }

    pub async fn list_instances(&self) -> Vec<AppInstance> {
        self.store.list().await
    }

    pub async fn get_instance(&self, id: &AppInstanceId) -> Result<AppInstance, AppFrameworkError> {
        self.store.require(id).await
    }

    /// 给 L4 喂 ComponentExecutor 的实例级输入
    pub fn component_hint(&self, instance: &AppInstance) -> AppComponentHint {
        AppComponentHint {
            instance_id: instance.id.as_str().to_string(),
            install_dir: HostPath::from_posix(&instance.install_dir),
            port: instance.port,
            npm_registry: self.npm_registry.clone(),
            install_renderer: instance.install_renderer,
            adopt_existing: instance.origin.is_imported(),
            webui_username: self.remembered_secret(instance, SECRET_WEBUI_USERNAME),
            webui_password: self.remembered_secret(instance, SECRET_WEBUI_PASSWORD),
        }
    }

    async fn resolve_host(&self, host_id: &str) -> Result<Arc<dyn Host>, AppFrameworkError> {
        let target = if host_id == LOCAL_HOST_ID {
            RuntimeTarget::Local
        } else if let Some(server_id) = server_id_of_host(host_id) {
            RuntimeTarget::Server(server_id.to_string())
        } else {
            return Err(AppFrameworkError::Validation(format!(
                "无法识别的主机: {host_id}"
            )));
        };
        self.host_resolver
            .resolve(&target)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))
    }

    fn publish(&self, instance: &AppInstance, reason: &str) {
        self.event_bus
            .publish(DomainEvent::app_instance_changed(instance.clone(), reason));
    }
}

fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect()
}

fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

fn describe_target(target: &RuntimeTarget) -> String {
    match target {
        RuntimeTarget::Local => "本机".to_string(),
        RuntimeTarget::Server(id) => format!("远端主机 {id}"),
    }
}

fn describe_host(host_id: &str) -> String {
    match server_id_of_host(host_id) {
        Some(id) => format!("远端主机 {id}"),
        None => "本机".to_string(),
    }
}

fn host_err(e: ncd_host::HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

fn probe_read_instance(
    framework_id: &AppFrameworkId,
    host_id: &str,
    path: &str,
    port: u16,
) -> AppInstance {
    AppInstance {
        id: AppInstanceId::new("probe"),
        framework_id: framework_id.clone(),
        display_name: String::new(),
        placement: AppPlacement::native_for_host(host_id),
        host_id: host_id.to_string(),
        install_dir: path.to_string(),
        port,
        state: AppInstanceState::Installed,
        link: None,
        installed_version: None,
        last_error: None,
        created_at_ms: 0,
        install_renderer: false,
        origin: AppInstanceOrigin::Imported,
        auto_start: true,
    }
}

#[cfg(test)]
mod tests;
