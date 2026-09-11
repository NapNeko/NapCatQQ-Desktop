//! AppManager：应用实例表 + 生命周期 + 「协议 Bot 一键对接应用端」编排
//!
//! 对接 = 往 Bot 的 `connect.websocket_clients` 按名 upsert 一条反向 WS 连接，再调
//! `BotManager::upsert_bot_config`（持久化 + 渲染 + 热推）。不新写推送链路。
//! 拓扑按主机分、不看 BackendType：同机走实例口；本机 Bot→远端应用 SSH `-L`；远端 Bot→本机应用 SSH `-R`；
//! 两台远端走应用机常驻 `ssh -R`（Desktop 只编排）。
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
    KarinPluginInstalled,
    KarinPluginMarketEntry, PluginLogSink, app_file_basename, restore_adopted_files,
    remove_ncd_debris, AdoptRestoreScope,
};
use ncd_component::{DetectOutcome, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigText, AppFrameworkId, AppFrameworkManifest, AppInstance,
    AppInstanceId, AppInstanceState, AppLinkRecord, AppLinkTopology, AppPlacement, AppPluginAction,
    AppInstanceOrigin, AppPluginConfigSchema, AppProjectProbe, AppStoreResource, AppWebUiAccount,
    AppWebUiAuthKind, BotConfig, BotId,
    CreateAppInstanceRequest, DomainEventKind, ImportAppInstanceRequest, LOCAL_HOST_ID,
    REMOTE_HOST_ID_PREFIX,
    OneBotLinkMode, OneBotLinkPlan, RuntimeTarget, app_link_connection_name, classify_app_link,
    host_id_of_runtime_target, is_app_link_connection_name, rewrite_ws_loopback_port,
    server_id_of_host,
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
use super::listen_port::{allocate_listen_port, local_port_free};
use super::native_runtime::{AppLaunchSpec, NativeAppRuntime};
use super::resident_link::{self, ResidentLinkSpec};
use crate::bot_manager::BotManager;
use crate::components::{AppComponentHint, data_root_to_host_path};
use crate::events::{BroadcastEventBus, DomainEvent};
use crate::metrics::now_ms;

/// 本机实例目录：`data_root/apps/<framework>/<instance>`
const LOCAL_APPS_DIR: &str = "apps";
/// SecretStore 里 WebUI 账号的键后缀（`app:<instance_id>:<suffix>`）；明文只存这里，实例记录不带
const SECRET_WEBUI_USERNAME: &str = "webui_username";
const SECRET_WEBUI_PASSWORD: &str = "webui_password";

fn secret_key(instance_id: &str, suffix: &str) -> String {
    format!("app:{instance_id}:{suffix}")
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
/// 远端实例目录：`$HOME/ncd/apps/<framework>/<instance>`
const REMOTE_APPS_REL: &str = "ncd/apps";

pub fn parse_user_install_dir(raw: &str, os: Os) -> Result<HostPath, AppFrameworkError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(AppFrameworkError::Validation("安装目录为空".into()));
    }
    let path = match os {
        Os::Windows => {
            if t.starts_with('/') {
                HostPath::from_posix(t)
            } else {
                HostPath::from_windows(t)
            }
        }
        _ => HostPath::from_posix(t),
    };
    if !path.is_absolute() {
        return Err(AppFrameworkError::Validation(
            "安装目录必须是绝对路径".into(),
        ));
    }
    Ok(path)
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

struct AppInstanceTunnel {
    app_port: u16,
    topology: AppLinkTopology,
    handle: TunnelHandle,
}

fn astrbot_api(
    adapter: &dyn AppFrameworkAdapter,
) -> Result<&dyn AstrBotRuntimeApi, AppFrameworkError> {
    adapter.astrbot_runtime().ok_or_else(|| {
        AppFrameworkError::ConfigUnsupported(adapter.manifest().id.as_str().to_string())
    })
}

fn needs_desktop_ssh_tunnel(topology: AppLinkTopology) -> bool {
    matches!(
        topology,
        AppLinkTopology::LocalBotRemoteApp | AppLinkTopology::RemoteBotLocalApp
    )
}

fn webui_tunnel_key(instance: &AppInstance) -> String {
    format!("{}:webui", instance.id.as_str())
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

    pub fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String> {
        let adapter = self.registry.get(&instance.framework_id).ok()?;
        adapter.integration().webui_url(instance, public_host)
    }

    async fn webui_login_username(&self, instance: &AppInstance) -> String {
        if let Some(account) = self.webui_account(instance).await {
            let name = account.username.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
        self.remembered_secret(instance, SECRET_WEBUI_USERNAME)
            .unwrap_or_else(|| "astrbot".into())
    }

    /// 桌面端打开 WebUI 用的本机口：本机即实例口，远端是 SSH `-L` 分配口。
    pub async fn desktop_loopback_port(
        &self,
        instance: &AppInstance,
    ) -> Result<u16, AppFrameworkError> {
        self.ensure_local_to_remote_tunnel(instance).await
    }

    /// WebUI 在应用机上的 HTTP 口（AstrBot 是 dashboard.port，不是 OneBot 口）。
    pub async fn webui_listen_port(&self, instance: &AppInstance) -> u16 {
        match self.read_config(&instance.id).await {
            Ok(env) => env.config.webui_port().unwrap_or(instance.port),
            Err(_) => self
                .registry
                .get(&instance.framework_id)
                .map(|adapter| adapter.webui_fallback_port(instance))
                .unwrap_or(instance.port),
        }
    }

    /// 打开 WebUI：远端隧道打到 WebUI 口，再交给 integration 拼 URL。
    pub async fn desktop_webui_loopback_port(
        &self,
        instance: &AppInstance,
    ) -> Result<u16, AppFrameworkError> {
        let remote = self.webui_listen_port(instance).await;
        self.ensure_local_to_remote_port_tunnel(instance, remote, webui_tunnel_key(instance))
            .await
    }

    /// 读盘拿到的 WebUI 登录密钥；配置读失败或没有密钥时返回空串。
    pub async fn webui_auth_key(&self, id: &AppInstanceId) -> String {
        match self.read_config(id).await {
            Ok(envelope) => envelope.config.webui_auth_key().to_string(),
            Err(_) => String::new(),
        }
    }

    /// 用户名密码类 WebUI 的账号：用户名以落盘为准，密码只有桌面端自己设过才有。
    /// 落盘读不到时也给出默认用户名，让用户至少知道该填什么。
    pub async fn webui_account(&self, instance: &AppInstance) -> Option<AppWebUiAccount> {
        let adapter = self.registry.get(&instance.framework_id).ok()?;
        if adapter.manifest().webui_auth != AppWebUiAuthKind::UserPassword {
            return None;
        }
        let remembered = self.remembered_secret(instance, SECRET_WEBUI_PASSWORD);
        let probe = match self.resolve_host(&instance.host_id).await {
            Ok(host) => adapter
                .read_webui_account(host.as_ref(), instance, remembered.as_deref())
                .await
                .ok()
                .flatten(),
            Err(_) => None,
        };
        let can_reset = !matches!(instance.state, AppInstanceState::Running);
        Some(match probe {
            Some(p) => AppWebUiAccount {
                username: p.username,
                password: remembered,
                password_matches: p.password_matches,
                can_reset,
            },
            None => AppWebUiAccount {
                username: self
                    .remembered_secret(instance, SECRET_WEBUI_USERNAME)
                    .unwrap_or_default(),
                password: remembered,
                password_matches: None,
                can_reset,
            },
        })
    }

    /// 重置 WebUI 密码（None = 随机生成）：实例必须已停止，写完下次启动生效。
    pub async fn reset_webui_password(
        &self,
        id: &AppInstanceId,
        password: Option<String>,
    ) -> Result<AppWebUiAccount, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        if adapter.manifest().webui_auth != AppWebUiAuthKind::UserPassword {
            return Err(AppFrameworkError::ConfigUnsupported(
                instance.framework_id.as_str().to_string(),
            ));
        }
        if matches!(instance.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::Validation(
                "实例运行中，先停止再重置密码".to_string(),
            ));
        }
        let password = resolve_webui_password(adapter.as_ref(), password)?;
        let host = self.resolve_host(&instance.host_id).await?;
        adapter
            .write_webui_password(host.as_ref(), &instance, &password)
            .await?;
        self.remember_secret(id, SECRET_WEBUI_PASSWORD, &password);
        Ok(self
            .webui_account(&instance)
            .await
            .unwrap_or(AppWebUiAccount {
                username: String::new(),
                password: Some(password),
                password_matches: Some(true),
                can_reset: true,
            }))
    }

    // ---- 实例生命周期 ----

    pub async fn create_instance(
        &self,
        req: CreateAppInstanceRequest,
    ) -> Result<AppInstance, AppFrameworkError> {
        let adapter = self.registry.get(&req.framework_id)?;
        let manifest = adapter.manifest();
        let placement = AppPlacement::native_for_host(&req.host_id);
        if !manifest.supported_placements.contains(&placement) {
            return Err(AppFrameworkError::PlacementUnsupported(format!(
                "{} 不支持 {}",
                manifest.display_name,
                placement.as_str()
            )));
        }
        let siblings = self.store.list().await;
        let taken: Vec<u16> = siblings
            .iter()
            .filter(|i| i.host_id == req.host_id)
            .map(|i| i.port)
            .collect();
        let probe_local = req.host_id == LOCAL_HOST_ID;
        let port = allocate_listen_port(req.port, &taken, probe_local)
            .map_err(AppFrameworkError::Validation)?;
        // 账号密码类 WebUI：先把密码定下来，安装时种进配置，之后打开 WebUI 才有得显示
        let webui_account = if manifest.webui_auth == AppWebUiAuthKind::UserPassword {
            let password = resolve_webui_password(adapter.as_ref(), req.webui_password.clone())?;
            let username = req
                .webui_username
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned);
            Some((username, password))
        } else {
            None
        };

        let host = self.resolve_host(&req.host_id).await?;
        let id = AppInstanceId::new(short_id());
        if let Some((username, password)) = &webui_account {
            if let Some(name) = username {
                self.remember_secret(&id, SECRET_WEBUI_USERNAME, name);
            }
            self.remember_secret(&id, SECRET_WEBUI_PASSWORD, password);
        }
        let install_dir = self
            .resolve_install_dir(
                host.as_ref(),
                &req.host_id,
                &req.framework_id,
                &id,
                req.install_dir.as_deref(),
            )
            .await?;
        let display_name = if req.display_name.trim().is_empty() {
            format!("{} {}", manifest.display_name, id.as_str())
        } else {
            req.display_name.trim().to_string()
        };

        let instance = AppInstance {
            id,
            framework_id: req.framework_id.clone(),
            display_name,
            placement,
            host_id: req.host_id.clone(),
            install_dir: install_dir.as_posix().to_string(),
            port,
            state: AppInstanceState::NotInstalled,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: now_ms(),
            install_renderer: req.install_renderer.unwrap_or(true),
            origin: ncd_domain::AppInstanceOrigin::Created,
        };
        let saved = self.store.upsert(instance).await?;
        self.publish(&saved, "created");
        Ok(saved)
    }

    pub async fn probe_project(
        &self,
        host_id: &str,
        framework_id: &AppFrameworkId,
        raw_path: &str,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        let adapter = self.registry.get(framework_id)?;
        let host = self.resolve_host(host_id).await?;
        let path = parse_user_install_dir(raw_path, host.os())?;
        if !host
            .exists(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?
        {
            return Err(AppFrameworkError::Validation("目录不存在".into()));
        }
        let mut probe = adapter.probe_project(host.as_ref(), &path).await?;
        probe.path = path.as_posix().to_string();
        probe.supervisors =
            super::supervisor::list_supervisors(host.as_ref(), path.as_posix()).await?;
        if !probe.supervisors.is_empty() {
            probe.warnings.push(format!(
                "现在由 systemd 在跑（{}）。导入后改由这边开关，不要了可以还回去。",
                probe.supervisors.join("、")
            ));
        }
        let kind = super::supervisor::AppProcessKind::from_framework(framework_id.as_str());
        probe.running = match host.locality() {
            Locality::Remote => {
                let listing =
                    super::supervisor::list_cwd_processes(host.as_ref(), path.as_posix()).await?;
                super::supervisor::pick_app_pid(&listing, kind).is_some()
            }
            Locality::Local => {
                super::native_runtime::discover_local_pid(path.as_posix(), kind).is_some()
            }
        };
        let stub = probe_read_instance(
            framework_id,
            host_id,
            path.as_posix(),
            probe.port.unwrap_or(0),
        );
        if let Some(found) = self.discover_existing_link(&stub).await {
            probe.detected_bot_id = Some(found.bot_id);
        }
        Ok(probe)
    }

    pub async fn import_instance(
        &self,
        req: ImportAppInstanceRequest,
    ) -> Result<AppInstance, AppFrameworkError> {
        let adapter = self.registry.get(&req.framework_id)?;
        let manifest = adapter.manifest();
        let placement = AppPlacement::native_for_host(&req.host_id);
        if !manifest.supported_placements.contains(&placement) {
            return Err(AppFrameworkError::PlacementUnsupported(format!(
                "{} 不支持 {}",
                manifest.display_name,
                placement.as_str()
            )));
        }
        let probe = self
            .probe_project(&req.host_id, &req.framework_id, &req.path)
            .await?;
        let host = self.resolve_host(&req.host_id).await?;
        let id = AppInstanceId::new(short_id());
        let install_dir = self
            .bind_existing_dir(
                host.as_ref(),
                &req.host_id,
                &id,
                Some(probe.path.as_str()),
            )
            .await?;
        let siblings = self.store.list().await;
        let taken: Vec<u16> = siblings
            .iter()
            .filter(|i| i.host_id == req.host_id)
            .map(|i| i.port)
            .collect();
        // 接管后对接用 instance.port；必须跟项目实际监听口一致，不能悄悄换成随机高位。
        // 探测不到口时不要填框架默认口：多监听口会把默认口误认成认领。
        let port = match adapter.suggested_import_port(&probe).filter(|p| *p > 0) {
            Some(p) => allocate_listen_port(Some(p), &taken, false)
                .map_err(AppFrameworkError::Validation)?,
            None => 0,
        };
        let display_name = if req.display_name.trim().is_empty() {
            probe.display_name.clone()
        } else {
            req.display_name.trim().to_string()
        };

        let instance = AppInstance {
            id,
            framework_id: req.framework_id.clone(),
            display_name,
            placement,
            host_id: req.host_id.clone(),
            install_dir: install_dir.as_posix().to_string(),
            port,
            state: AppInstanceState::NotInstalled,
            link: None,
            installed_version: probe.version.clone(),
            last_error: None,
            created_at_ms: now_ms(),
            install_renderer: false,
            origin: AppInstanceOrigin::Imported,
        };
        let rels = adapter
            .adopt_watch_rels(host.as_ref(), &install_dir)
            .await?;
        let snapshot = adopt::capture_snapshot(
            host.as_ref(),
            &instance.id,
            &instance.host_id,
            &instance.install_dir,
            &rels,
            &probe.supervisors,
            instance.created_at_ms,
        )
        .await?;
        let saved = self.store.upsert(instance).await?;
        if let Err(e) = self.adopt_store.save(&snapshot) {
            let _ = self.store.remove(&saved.id).await;
            return Err(e);
        }
        self.publish(&saved, "imported");

        if !probe.supervisors.is_empty() {
            if let Err(e) =
                super::supervisor::disable_now(host.as_ref(), &probe.supervisors).await
            {
                let _ = self.adopt_store.remove(&saved.id);
                let _ = self.store.remove(&saved.id).await;
                return Err(e);
            }
        }

        let refreshed = self.refresh_instance(&saved.id).await?;
        // systemd 没停干净就再拉一份，会和 Restart=always 对打
        let current = if probe.ready && refreshed.state != AppInstanceState::Running {
            match self.start_instance(&refreshed.id).await {
                Ok(started) => started,
                Err(_) => self.store.require(&refreshed.id).await?,
            }
        } else {
            refreshed
        };
        self.adopt_existing_link(&current.id).await
    }

    /// 安装任务已提交：置 Installing 并盯任务结束
    pub async fn track_install(
        self: &Arc<Self>,
        id: &AppInstanceId,
        task_id: String,
    ) -> Result<AppInstance, AppFrameworkError> {
        let updated = self
            .store
            .update(id, |i| {
                i.state = AppInstanceState::Installing;
                i.last_error = None;
            })
            .await?;
        self.publish(&updated, "installing");

        let this = Arc::clone(self);
        let id = id.clone();
        let mut sub = self
            .event_bus
            .subscribe(EventFilter::kind(DomainEventKind::DeploymentTaskChanged));
        tokio::spawn(async move {
            let deadline = Instant::now() + Duration::from_mins(30);
            loop {
                tokio::select! {
                    event = sub.next() => {
                        let Some(event) = event else {
                            return;
                        };
                        let DomainEvent::DeploymentTaskChanged { task } = event else {
                            continue;
                        };
                        if task.task_id != task_id || !task.status.is_terminal() {
                            continue;
                        }
                        let failure = (!matches!(
                            task.status,
                            ncd_domain::DeploymentTaskStatus::Success
                        ))
                        .then(|| {
                            task.error
                                .clone()
                                .unwrap_or_else(|| "安装任务未成功结束".to_string())
                        });
                        if let Err(e) = this.refresh_after_install(&id, failure).await {
                            tracing::warn!(instance = id.as_str(), error = %e, "refresh after install");
                        }
                        return;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        if Instant::now() > deadline {
                            return;
                        }
                        match this.refresh_instance(&id).await {
                            Ok(inst) if inst.state != AppInstanceState::Installing => return,
                            Err(e) => tracing::warn!(
                                instance = id.as_str(),
                                error = %e,
                                "poll after install"
                            ),
                            _ => {}
                        }
                    }
                }
            }
        });
        Ok(updated)
    }

    async fn refresh_after_install(
        &self,
        id: &AppInstanceId,
        failure: Option<String>,
    ) -> Result<AppInstance, AppFrameworkError> {
        // 任务成功先改状态，避免 UI 卡在「安装中」等 detect 读刚写完的 uv.lock / .venv
        if failure.is_none() {
            let instant = self
                .store
                .update(id, |i| {
                    if matches!(
                        i.state,
                        AppInstanceState::Installing | AppInstanceState::NotInstalled
                    ) {
                        i.state = AppInstanceState::Installed;
                    }
                    i.last_error = None;
                })
                .await?;
            self.publish(&instant, "installed");
        }

        let instance = self.store.require(id).await?;
        let host = match self.resolve_host(&instance.host_id).await {
            Ok(host) => host,
            Err(e) if failure.is_some() => {
                let updated = self
                    .store
                    .update(id, |i| {
                        i.state = AppInstanceState::NotInstalled;
                        i.last_error = Some(failure.clone().unwrap_or_else(|| e.to_string()));
                    })
                    .await?;
                self.publish(&updated, "install_failed");
                return Ok(updated);
            }
            Err(_) => return Ok(instance),
        };
        let detected = self.detect(host.as_ref(), &instance).await;
        let updated = self
            .store
            .update(id, |i| match &detected {
                Ok(DetectOutcome::Installed(v)) => {
                    i.state = AppInstanceState::Installed;
                    i.installed_version = Some(v.version.clone());
                    i.last_error = failure.clone();
                }
                Ok(DetectOutcome::Unusable(u)) if failure.is_some() => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some(failure.clone().unwrap_or_else(|| u.reason.clone()));
                }
                Ok(DetectOutcome::NotInstalled) if failure.is_some() => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = failure.clone();
                }
                Err(e) if failure.is_some() => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some(failure.clone().unwrap_or_else(|| e.to_string()));
                }
                Ok(DetectOutcome::Unusable(u)) => {
                    i.last_error = Some(u.reason.clone());
                }
                _ => {}
            })
            .await?;
        if updated != instance {
            self.publish(
                &updated,
                if failure.is_some() {
                    "install_failed"
                } else {
                    "installed"
                },
            );
        }
        Ok(updated)
    }

    /// 刷新单个实例：安装探测 + 进程对账（冷启动 / 页面手动刷新）
    pub async fn refresh_instance(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if instance.state == AppInstanceState::Installing {
            let host = self.resolve_host(&instance.host_id).await?;
            let detected = self.detect(host.as_ref(), &instance).await?;
            if let DetectOutcome::Installed(v) = detected {
                let updated = self
                    .store
                    .update(id, |i| {
                        if i.state == AppInstanceState::Installing {
                            i.state = AppInstanceState::Installed;
                            i.installed_version = Some(v.version.clone());
                        }
                    })
                    .await?;
                if updated.state != instance.state {
                    self.publish(&updated, "installed");
                }
                return Ok(updated);
            }
            return Ok(instance);
        }
        let host = self.resolve_host(&instance.host_id).await?;
        let detected = self.detect(host.as_ref(), &instance).await?;
        let running = match &detected {
            DetectOutcome::Installed(_) => {
                self.runtime.reconcile_pid(host.as_ref(), &instance).await?
            }
            _ => None,
        };
        if running.is_some() && !self.runtime.is_following(&instance.id).await {
            let log = self.resolve_log_file(host.as_ref(), &instance).await;
            self.runtime.attach(Arc::clone(&host), &instance, log).await?;
        }
        let updated = self
            .store
            .update(id, |i| match &detected {
                DetectOutcome::Installed(v) => {
                    i.installed_version = Some(v.version.clone());
                    i.state = if running.is_some() {
                        AppInstanceState::Running
                    } else if i.state == AppInstanceState::Running {
                        AppInstanceState::Stopped
                    } else if i.state == AppInstanceState::NotInstalled {
                        AppInstanceState::Installed
                    } else {
                        i.state
                    };
                }
                DetectOutcome::Unusable(u) => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some(u.reason.clone());
                }
                DetectOutcome::NotInstalled => {
                    i.state = AppInstanceState::NotInstalled;
                    i.installed_version = None;
                }
            })
            .await?;
        if updated.state != instance.state || updated.installed_version != instance.installed_version {
            self.publish(&updated, "refreshed");
        }
        if updated.origin.is_imported() && updated.link.is_none() {
            return self.adopt_existing_link(&updated.id).await;
        }
        Ok(updated)
    }

    /// 开页拉历史:broadcast 无 backlog,启动对账推过的行会丢
    pub async fn tail_log(
        &self,
        id: &AppInstanceId,
        lines: usize,
    ) -> Result<LogSnapshot, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let host = self.resolve_host(&instance.host_id).await?;
        let path = self.resolve_log_file(host.as_ref(), &instance).await;
        let mut collected = super::log_tail::tail_file(host.as_ref(), &path, lines).await;
        if collected.is_empty() {
            let units = self
                .adopt_store
                .load(&instance.id)
                .ok()
                .flatten()
                .map(|s| s.supervisors)
                .unwrap_or_default();
            let pid = self
                .runtime
                .reconcile_pid(host.as_ref(), &instance)
                .await
                .ok()
                .flatten();
            collected = super::log_tail::tail_journal(host.as_ref(), &units, pid, lines).await;
        }
        Ok(LogSnapshot {
            total_lines: collected.len(),
            lines: collected,
        })
    }

    async fn resolve_log_file(&self, host: &dyn Host, instance: &AppInstance) -> HostPath {
        let primary = self
            .registry
            .get(&instance.framework_id)
            .ok()
            .and_then(|a| a.log_file(instance))
            .unwrap_or_else(|| HostPath::from_posix(&instance.install_dir).join(".ncd-app.log"));
        if super::log_tail::file_size(host, &primary)
            .await
            .unwrap_or(0)
            > 0
        {
            return primary;
        }
        if let Some(found) = super::log_tail::newest_project_log(host, &instance.install_dir).await {
            if super::log_tail::file_size(host, &found).await.unwrap_or(0) > 0 {
                return found;
            }
        }
        primary
    }

    /// 冷启动：逐实例对账；连不上的远端主机跳过（脱管语义）
    pub async fn reconcile_all(&self) {
        for instance in self.store.list().await {
            self.reconcile_one(&instance.id).await;
        }
    }

    /// 主机连上后补跑启动时因 SSH 未就绪跳过的远端对账 / 日志挂接
    pub async fn reconcile_for_server(&self, server_id: &str) {
        let host_id = format!("{REMOTE_HOST_ID_PREFIX}{server_id}");
        for instance in self.store.list().await {
            if instance.host_id != host_id {
                continue;
            }
            self.reconcile_one(&instance.id).await;
        }
    }

    async fn reconcile_one(&self, id: &AppInstanceId) {
        if let Err(e) = self.refresh_instance(id).await {
            tracing::info!(instance = id.as_str(), error = %e, "app reconcile skipped");
        }
        let current = match self.store.require(id).await {
            Ok(i) => i,
            Err(_) => return,
        };
        if current
            .link
            .as_ref()
            .and_then(|l| l.resident_forward_port)
            .is_some()
        {
            if let Err(e) = self.reconcile_resident_link(&current).await {
                tracing::info!(
                    instance = id.as_str(),
                    error = %e,
                    "app resident link reconcile skipped"
                );
            }
        } else if let Err(e) = self.reconcile_link_tunnel(&current).await {
            tracing::info!(
                instance = id.as_str(),
                error = %e,
                "app link tunnel reconcile skipped"
            );
        }
    }

    pub async fn run_host_connection_recovered_listener(self: Arc<Self>) {
        let mut subscription = self.event_bus.subscribe(EventFilter::all());
        let mut last_reconciled: HashMap<String, Instant> = HashMap::new();
        while let Some(event) = subscription.next().await {
            let DomainEvent::HostConnectionRecovered { server_id, .. } = event else {
                continue;
            };
            let now = Instant::now();
            if let Some(prev) = last_reconciled.get(&server_id) {
                if now.duration_since(*prev) < Duration::from_secs(30) {
                    continue;
                }
            }
            last_reconciled.insert(server_id.clone(), now);
            self.reconcile_for_server(&server_id).await;
        }
    }

    pub async fn start_instance(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation("实例尚未安装".to_string()));
        }
        if instance.state == AppInstanceState::Running {
            return Ok(instance);
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        let spec = self.component_spec(host.as_ref(), &instance);
        let command = adapter
            .launch_command(host.as_ref(), &spec, &LaunchArgs::default())
            .await?;
        let log_file = adapter
            .log_file(&instance)
            .unwrap_or_else(|| HostPath::from_posix(&instance.install_dir).join(".ncd-app.log"));
        let launch = AppLaunchSpec { command, log_file };

        match self.runtime.start(Arc::clone(&host), &instance, launch).await {
            Ok(_pid) => {
                let updated = self
                    .store
                    .update(id, |i| {
                        i.state = AppInstanceState::Running;
                        i.last_error = None;
                    })
                    .await?;
                self.publish(&updated, "started");
                Ok(updated)
            }
            Err(e) => {
                let updated = self
                    .store
                    .update(id, |i| {
                        i.state = AppInstanceState::Stopped;
                        i.last_error = Some(e.to_string());
                    })
                    .await?;
                self.publish(&updated, "start_failed");
                Err(e)
            }
        }
    }

    pub async fn stop_instance(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let host = self.resolve_host(&instance.host_id).await?;
        self.runtime.stop(host, &instance).await?;
        let updated = self
            .store
            .update(id, |i| {
                if i.state == AppInstanceState::Running {
                    i.state = AppInstanceState::Stopped;
                }
                i.last_error = None;
            })
            .await?;
        self.publish(&updated, "stopped");
        Ok(updated)
    }

    /// 删除实例：停进程 → 解绑 Bot 侧连接 → 导入项先还原快照 → （可选）删目录 → 删记录
    pub async fn delete_instance(
        &self,
        id: &AppInstanceId,
        remove_files: bool,
    ) -> Result<(), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let host = self.resolve_host(&instance.host_id).await;
        if let Ok(host) = &host {
            if let Err(e) = self.runtime.stop(Arc::clone(host), &instance).await {
                tracing::warn!(instance = id.as_str(), error = %e, "stop before delete");
            }
        }
        if instance.link.is_some() {
            if let Err(e) = self.unlink(id).await {
                tracing::warn!(instance = id.as_str(), error = %e, "unlink before delete");
            }
        }
        self.drop_instance_tunnel(id).await;
        if instance.origin.is_imported() && !remove_files {
            let host = host?;
            self.release_imported(&instance, host.as_ref()).await?;
        } else if remove_files {
            let host = host?;
            let dir = HostPath::from_posix(&instance.install_dir);
            if host.exists(&dir).await.map_err(host_err)? {
                host.remove_dir_all(&dir).await.map_err(host_err)?;
            }
            let _ = self.adopt_store.remove(id);
        } else {
            let _ = self.adopt_store.remove(id);
        }
        if let Some(removed) = self.store.remove(id).await? {
            self.forget_secrets(id);
            self.publish(&removed, "deleted");
        }
        Ok(())
    }

    /// 还原导入快照 + 清桌面端落盘 + 把 systemd 还给系统。失败则保留实例记录以便重试。
    async fn release_imported(
        &self,
        instance: &AppInstance,
        host: &dyn Host,
    ) -> Result<(), AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let snap = self.adopt_store.load(&instance.id)?;
        if let Some(snap) = &snap {
            restore_adopted_files(host, &root, &snap.files, AdoptRestoreScope::All).await?;
            if let Err(e) = remove_ncd_debris(host, &root, &snap.files).await {
                tracing::warn!(instance = instance.id.as_str(), error = %e, "clean ncd debris");
            }
            if !snap.supervisors.is_empty() {
                super::supervisor::enable_now(host, &snap.supervisors).await?;
            }
        } else {
            if let Err(e) = remove_ncd_debris(host, &root, &[]).await {
                tracing::warn!(instance = instance.id.as_str(), error = %e, "clean ncd debris");
            }
            let units = super::supervisor::list_supervisors(host, root.as_posix()).await?;
            if !units.is_empty() {
                super::supervisor::enable_now(host, &units).await?;
            }
        }
        self.adopt_store.remove(&instance.id)
    }

    // ---- 对接 ----

    async fn discover_existing_link(&self, instance: &AppInstance) -> Option<AppLinkRecord> {
        let bots = self.bot_manager.list_bot_configs_for_link().await.ok()?;
        if bots.is_empty() {
            return None;
        }
        let host = self.resolve_host(&instance.host_id).await.ok()?;
        let adapter = self.registry.get(&instance.framework_id).ok()?;
        let token = adapter
            .read_access_token(host.as_ref(), instance)
            .await
            .ok()
            .flatten();
        let outbound = adapter
            .read_outbound_ws_urls(host.as_ref(), instance)
            .await
            .unwrap_or_default();
        let claimed = self.claimed_link_names(&instance.id).await;
        super::existing_link::discover_existing_link(
            &instance.host_id,
            instance.port,
            token.as_deref(),
            &outbound,
            &claimed,
            &bots,
        )
    }

    async fn claimed_link_names(&self, except: &AppInstanceId) -> HashSet<(String, String)> {
        self.store
            .list()
            .await
            .into_iter()
            .filter(|i| &i.id != except)
            .filter_map(|i| {
                let link = i.link?;
                if link.connection_name.is_empty() {
                    return None;
                }
                Some((link.bot_id.as_str().to_string(), link.connection_name))
            })
            .collect()
    }

    /// 导入实例:已有反向 / 正向 WS 能唯一对上协议 Bot 时补上 link,不改双方配置
    async fn adopt_existing_link(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if instance.link.is_some() || !instance.origin.is_imported() {
            return Ok(instance);
        }
        let Some(found) = self.discover_existing_link(&instance).await else {
            return Ok(instance);
        };
        let updated = self
            .store
            .update(id, |i| {
                if i.link.is_none() {
                    i.link = Some(found);
                }
            })
            .await?;
        if updated.link.is_some() {
            self.publish(&updated, "linked");
        }
        Ok(updated)
    }

    /// 预览：不写任何东西，只算计划
    pub async fn preview_link(
        &self,
        instance_id: &AppInstanceId,
        bot_id: &BotId,
    ) -> Result<OneBotLinkPlan, AppFrameworkError> {
        let (instance, bot, adapter, host) = self.link_context(instance_id, bot_id).await?;
        let token = self.pick_access_token(host.as_ref(), &instance, &bot, adapter.as_ref()).await?;
        adapter.integration().plan_link(&instance, &bot, &token)
    }

    /// 应用：写应用端 → Bot 侧 upsert 连接 + 热推 → 记 link；Bot 侧失败回滚应用端
    pub async fn apply_link(
        &self,
        instance_id: &AppInstanceId,
        bot_id: &BotId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let (instance, mut bot, adapter, host) = self.link_context(instance_id, bot_id).await?;
        let topology = classify_app_link(&bot.bot.runtime_target, &instance.host_id).ok_or_else(
            || unsupported_link_topology(&bot.bot.runtime_target, &instance.host_id),
        )?;
        if let Some(old) = instance.link.as_ref() {
            if old.resident_forward_port.is_some() {
                self.teardown_resident_best_effort(&instance, &old.bot_id).await;
            }
        }
        if !needs_desktop_ssh_tunnel(topology) {
            self.drop_instance_tunnel(instance_id).await;
        }
        let token = self.pick_access_token(host.as_ref(), &instance, &bot, adapter.as_ref()).await?;
        let mut plan = adapter.integration().plan_link(&instance, &bot, &token)?;

        let mut resident_forward_port = None;
        if topology == AppLinkTopology::RemoteBotRemoteApp {
            let fwd = self.ensure_resident_link(&instance, &bot, bot_id).await?;
            plan.connection.url = rewrite_ws_loopback_port(&plan.connection.url, fwd)
                .map_err(AppFrameworkError::Validation)?;
            resident_forward_port = Some(fwd);
        } else if needs_desktop_ssh_tunnel(topology) {
            let loopback = self.ensure_link_tunnel(&instance, &bot).await?;
            plan.connection.url = rewrite_ws_loopback_port(&plan.connection.url, loopback)
                .map_err(AppFrameworkError::Validation)?;
        }

        if let Err(e) = adapter.apply_link(host.as_ref(), &instance, &plan).await {
            if resident_forward_port.is_some() {
                self.teardown_resident_best_effort(&instance, bot_id).await;
            }
            if needs_desktop_ssh_tunnel(topology) {
                self.drop_instance_tunnel(instance_id).await;
            }
            return Err(e);
        }

        upsert_ws_client(&mut bot, plan.connection.clone());
        if let Err(e) = self.bot_manager.upsert_bot_config(bot).await {
            if instance.origin.is_imported() {
                if let Ok(Some(snap)) = self.adopt_store.load(&instance.id) {
                    let root = HostPath::from_posix(&instance.install_dir);
                    if let Err(rb) = restore_adopted_files(
                        host.as_ref(),
                        &root,
                        &snap.files,
                        AdoptRestoreScope::Link,
                    )
                    .await
                    {
                        tracing::error!(
                            instance = instance_id.as_str(),
                            error = %rb,
                            "restore import snapshot after failed bot upsert"
                        );
                    }
                }
            }
            if let Err(rb) = adapter.rollback_link(host.as_ref(), &instance).await {
                tracing::error!(instance = instance_id.as_str(), error = %rb, "rollback app-side link");
            }
            if resident_forward_port.is_some() {
                self.teardown_resident_best_effort(&instance, bot_id).await;
            }
            if needs_desktop_ssh_tunnel(topology) {
                self.drop_instance_tunnel(instance_id).await;
            }
            return Err(AppFrameworkError::Integration(format!(
                "写入协议 Bot 连接失败，应用端配置已还原：{e}"
            )));
        }

        // 换 Bot,或导入认领的旧连接名和 ncd-app:<id> 不同:把旧条目摘掉,避免双连
        if let Some(old) = instance.link.as_ref() {
            let replacing = old.bot_id != *bot_id || old.connection_name != plan.connection.base.name;
            if replacing && !old.connection_name.is_empty() {
                if let Err(e) = self
                    .remove_ws_client_from_bot(&old.bot_id, &old.connection_name)
                    .await
                {
                    tracing::warn!(instance = instance_id.as_str(), error = %e, "detach previous bot");
                }
            }
        }

        let updated = self
            .store
            .update(instance_id, |i| {
                i.link = Some(AppLinkRecord {
                    bot_id: bot_id.clone(),
                    mode: OneBotLinkMode::ReverseWs,
                    connection_name: plan.connection.base.name.clone(),
                    linked_at_ms: now_ms(),
                    resident_forward_port,
                });
                i.last_error = None;
            })
            .await?;
        self.publish(&updated, "linked");
        Ok(updated)
    }

    /// 解绑：按名从 Bot 的 websocket_clients 删并热推；应用端监听口不动
    pub async fn unlink(&self, instance_id: &AppInstanceId) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(instance_id).await?;
        let Some(link) = instance.link.clone() else {
            return Ok(instance);
        };
        self.remove_ws_client_from_bot(&link.bot_id, &link.connection_name)
            .await?;
        if let Ok(adapter) = self.registry.get(&instance.framework_id)
            && let Ok(host) = self.resolve_host(&instance.host_id).await
            && let Err(e) = adapter.unlink(host.as_ref(), &instance).await
        {
            tracing::warn!(instance = instance_id.as_str(), error = %e, "app-side unlink");
        }
        if link.resident_forward_port.is_some() {
            self.teardown_resident_best_effort(&instance, &link.bot_id).await;
        }
        self.drop_instance_tunnel(instance_id).await;
        let updated = self
            .store
            .update(instance_id, |i| {
                i.link = None;
            })
            .await?;
        self.publish(&updated, "unlinked");
        Ok(updated)
    }

    async fn link_context(
        &self,
        instance_id: &AppInstanceId,
        bot_id: &BotId,
    ) -> Result<
        (
            AppInstance,
            BotConfig,
            Arc<dyn AppFrameworkAdapter>,
            Arc<dyn Host>,
        ),
        AppFrameworkError,
    > {
        let instance = self.store.require(instance_id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation(
                "应用实例尚未安装，先完成安装再对接".to_string(),
            ));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        if !adapter.manifest().link_modes.contains(&OneBotLinkMode::ReverseWs) {
            return Err(AppFrameworkError::LinkModeUnsupported(
                "该框架不支持反向 WS 对接".to_string(),
            ));
        }
        let bot = self
            .bot_manager
            .bot_config(bot_id)
            .await
            .map_err(AppFrameworkError::Integration)?
            .ok_or_else(|| {
                AppFrameworkError::Validation(format!("协议 Bot {} 不存在", bot_id.as_str()))
            })?;
        if classify_app_link(&bot.bot.runtime_target, &instance.host_id).is_none() {
            return Err(unsupported_link_topology(
                &bot.bot.runtime_target,
                &instance.host_id,
            ));
        }
        let host = self.resolve_host(&instance.host_id).await?;
        Ok((instance, bot, adapter, host))
    }

    /// token 优先级：应用端已有 → Bot 上同名连接已有 → 新生成
    async fn pick_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        bot: &BotConfig,
        adapter: &dyn AppFrameworkAdapter,
    ) -> Result<String, AppFrameworkError> {
        if let Some(t) = adapter.read_access_token(host, instance).await? {
            return Ok(t);
        }
        let name = app_link_connection_name(&instance.id);
        if let Some(existing) = bot
            .connect
            .websocket_clients
            .iter()
            .find(|c| c.base.name == name)
            .map(|c| c.base.token.trim().to_string())
            .filter(|t| !t.is_empty())
        {
            return Ok(existing);
        }
        Ok(generate_token())
    }

    async fn remove_ws_client_from_bot(
        &self,
        bot_id: &BotId,
        connection_name: &str,
    ) -> Result<(), AppFrameworkError> {
        let Some(mut bot) = self
            .bot_manager
            .bot_config(bot_id)
            .await
            .map_err(AppFrameworkError::Integration)?
        else {
            // Bot 已删：没有连接可摘
            return Ok(());
        };
        let before = bot.connect.websocket_clients.len();
        bot.connect
            .websocket_clients
            .retain(|c| c.base.name != connection_name);
        if bot.connect.websocket_clients.len() == before {
            return Ok(());
        }
        self.bot_manager
            .upsert_bot_config(bot)
            .await
            .map_err(AppFrameworkError::Integration)
    }

    // ---- 配置（类型化 + 原始文件）----

    /// 读类型化配置（含合并版本号）。未安装的实例没有配置可读
    pub async fn read_config(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let (instance, adapter, host) = self.config_context(id).await?;
        adapter.read_config(host.as_ref(), &instance).await
    }

    /// 写类型化配置：版本号比对 → 端口占用预检 → 适配器写盘 → 同步实例端口 / 重新对接 / 重启提示。
    /// `base_revision` 为 None 表示用户选择「覆盖」。
    pub async fn write_config(
        &self,
        id: &AppInstanceId,
        config: AppInstanceConfig,
        base_revision: Option<String>,
    ) -> Result<AppConfigWriteResult, AppFrameworkError> {
        self.write_config_profile(id, config, base_revision, None)
            .await
    }

    /// `conf_id` 只对跑着的 AstrBot 有意义（写到选中的 abconf）；停着始终只 patch `cmd_config.json`。
    pub async fn write_config_profile(
        &self,
        id: &AppInstanceId,
        config: AppInstanceConfig,
        base_revision: Option<String>,
        conf_id: Option<String>,
    ) -> Result<AppConfigWriteResult, AppFrameworkError> {
        let (instance, adapter, host) = self.config_context(id).await?;
        let before = adapter.read_config(host.as_ref(), &instance).await?;
        if let Some(base) = base_revision.as_deref()
            && base != before.revision
        {
            return Err(AppFrameworkError::ConfigConflict("config".to_string()));
        }

        let new_port = config.listen_port();
        if new_port != instance.port {
            self.ensure_port_free(&instance, new_port).await?;
        }

        let decided_running = matches!(instance.state, AppInstanceState::Running);
        let live = decided_running && adapter.supports_live_config();
        let after = if live {
            let port = self.desktop_webui_loopback_port(&instance).await?;
            let again = self.store.require(id).await?;
            if matches!(again.state, AppInstanceState::Running) != decided_running {
                return Err(AppFrameworkError::StateChanged(
                    "实例状态已变，请重试".into(),
                ));
            }
            let username = self.webui_login_username(&instance).await;
            let password = self
                .remembered_secret(&instance, SECRET_WEBUI_PASSWORD)
                .ok_or_else(|| {
                    AppFrameworkError::DashboardAuth(
                        "没有可用的 WebUI 密码。到连接页写下密码后再保存".into(),
                    )
                })?;
            let profile = conf_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("default");
            adapter
                .write_live_config(
                    host.as_ref(),
                    &instance,
                    port,
                    &username,
                    &password,
                    &config,
                    profile,
                )
                .await?
        } else {
            let again = self.store.require(id).await?;
            if matches!(again.state, AppInstanceState::Running) && adapter.supports_live_config() {
                return Err(AppFrameworkError::StateChanged(
                    "实例状态已变，请重试".into(),
                ));
            }
            adapter
                .write_config(host.as_ref(), &instance, &config)
                .await?
        };
        let mut sync = self
            .sync_after_config_write(&instance, &before, &after)
            .await?;
        if live {
            sync.restart_required = false;
        }

        // 重新对接会再改 .env（HTTP_PORT / WS_SERVER_AUTH_KEY 对齐），回读一次让前端拿到最终版本号
        let fin = if sync.relinked {
            adapter.read_config(host.as_ref(), &instance).await?
        } else {
            after
        };
        Ok(AppConfigWriteResult {
            config: fin.config,
            revision: fin.revision,
            documents: fin.documents,
            restart_required: sync.restart_required,
            relinked: sync.relinked,
            port_changed: sync.port_changed,
        })
    }

    pub async fn astrbot_dashboard_status(
        &self,
        id: &AppInstanceId,
    ) -> Result<AstrBotDashboardStatus, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        let runtime = adapter.astrbot_runtime().ok_or_else(|| {
            AppFrameworkError::ConfigUnsupported(instance.framework_id.as_str().to_string())
        })?;
        if !matches!(instance.state, AppInstanceState::Running) {
            return Ok(AstrBotDashboardStatus::not_running());
        }
        let password = self.remembered_secret(&instance, SECRET_WEBUI_PASSWORD);
        let port = match self.desktop_webui_loopback_port(&instance).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(AstrBotDashboardStatus::unreachable(
                    password.is_some(),
                    e.to_string(),
                ));
            }
        };
        let again = self.store.require(id).await?;
        if !matches!(again.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::StateChanged(
                "实例状态已变，请重试".into(),
            ));
        }
        let session = AstrBotSession {
            instance_id: instance.id.as_str().to_string(),
            port,
            username: self.webui_login_username(&instance).await,
            password,
        };
        Ok(runtime.dashboard_status(&session).await)
    }

    /// 运行期资源都要实例在跑；口和密码由这里备齐，具体调用交给适配器的能力对象。
    /// 返回 Arc 而不是 `&dyn AstrBotRuntimeApi`：借用挂在 Arc 上，调用方自己 `astrbot_api`。
    async fn astrbot_session(
        &self,
        id: &AppInstanceId,
    ) -> Result<(Arc<dyn AppFrameworkAdapter>, AstrBotSession), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        if adapter.astrbot_runtime().is_none() {
            return Err(AppFrameworkError::ConfigUnsupported(
                instance.framework_id.as_str().to_string(),
            ));
        }
        if !matches!(instance.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::NotRunning(
                "启动实例后才能改人格、知识库和会话规则".into(),
            ));
        }
        let port = self.desktop_webui_loopback_port(&instance).await?;
        let again = self.store.require(id).await?;
        if !matches!(again.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::StateChanged(
                "实例状态已变，请重试".into(),
            ));
        }
        let session = AstrBotSession {
            instance_id: instance.id.as_str().to_string(),
            port,
            username: self.webui_login_username(&instance).await,
            password: self.remembered_secret(&instance, SECRET_WEBUI_PASSWORD),
        };
        Ok((adapter, session))
    }

    pub async fn astrbot_list_personas(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.list_personas(&s).await
    }

    pub async fn astrbot_upsert_persona(
        &self,
        id: &AppInstanceId,
        persona: AstrBotPersona,
        creating: bool,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .upsert_persona(&s, &persona, creating)
            .await
    }

    pub async fn astrbot_delete_persona(
        &self,
        id: &AppInstanceId,
        persona_id: &str,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .delete_persona(&s, persona_id)
            .await
    }

    pub async fn astrbot_list_kbs(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.list_kbs(&s).await
    }

    pub async fn astrbot_create_kb(
        &self,
        id: &AppInstanceId,
        req: AstrBotKbCreate,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.create_kb(&s, &req).await
    }

    pub async fn astrbot_delete_kb(
        &self,
        id: &AppInstanceId,
        kb_id: &str,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.delete_kb(&s, kb_id).await
    }

    pub async fn astrbot_list_session_rules(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .list_session_rules(&s)
            .await
    }

    pub async fn astrbot_update_session_rule(
        &self,
        id: &AppInstanceId,
        rule: AstrBotSessionRule,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .update_session_rule(&s, &rule)
            .await
    }

    pub async fn astrbot_delete_session_rule(
        &self,
        id: &AppInstanceId,
        umo: &str,
        rule_key: &str,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .delete_session_rule(&s, umo, rule_key)
            .await
    }

    pub async fn astrbot_list_abconfs(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.list_abconfs(&s).await
    }

    pub async fn astrbot_create_abconf(
        &self,
        id: &AppInstanceId,
        name: &str,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .create_abconf(&s, name)
            .await
    }

    pub async fn astrbot_delete_abconf(
        &self,
        id: &AppInstanceId,
        abconf_id: &str,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .delete_abconf(&s, abconf_id)
            .await
    }

    pub async fn astrbot_list_source_models(
        &self,
        id: &AppInstanceId,
        source_id: &str,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .list_source_models(&s, source_id)
            .await
    }

    pub async fn astrbot_list_subagent_tools(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .list_subagent_tools(&s)
            .await
    }

    pub async fn list_config_documents(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        match self.resolve_host(&instance.host_id).await {
            Ok(host) => adapter.list_config_documents(host.as_ref(), &instance).await,
            Err(_) => Ok(adapter.config_documents(&instance)),
        }
    }

    pub async fn read_config_text(
        &self,
        id: &AppInstanceId,
        doc_id: &str,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let (instance, adapter, host) = self.config_context(id).await?;
        adapter
            .read_config_text(host.as_ref(), &instance, doc_id)
            .await
    }

    /// 原始文本写入；写完对有类型化模型的框架做一次端口 / 对接同步（手改 HTTP_PORT 也不失联）
    pub async fn write_config_text(
        &self,
        id: &AppInstanceId,
        doc_id: &str,
        text: &str,
        base_revision: Option<String>,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let (instance, adapter, host) = self.config_context(id).await?;
        let before = self
            .typed_snapshot(adapter.as_ref(), host.as_ref(), &instance)
            .await;
        let written = adapter
            .write_config_text(
                host.as_ref(),
                &instance,
                doc_id,
                text,
                base_revision.as_deref(),
            )
            .await?;
        if let Some(before) = before
            && let Some(after) = self
                .typed_snapshot(adapter.as_ref(), host.as_ref(), &instance)
                .await
        {
            let new_port = after.config.listen_port();
            if new_port != instance.port
                && let Err(e) = self.ensure_port_free(&instance, new_port).await
            {
                tracing::warn!(instance = id.as_str(), error = %e, "raw edit picked a taken port");
            } else if let Err(e) = self.sync_after_config_write(&instance, &before, &after).await {
                tracing::warn!(instance = id.as_str(), error = %e, "sync after raw config write");
            }
        }
        Ok(written)
    }

    // ---- 应用端商店（Karin 插件 / NoneBot 适配器+插件）----

    pub async fn list_karin_plugin_market(
        &self,
    ) -> Result<Vec<KarinPluginMarketEntry>, AppFrameworkError> {
        super::plugin_market::fetch_karin_plugin_market().await
    }

    pub async fn list_store(
        &self,
        framework_id: &AppFrameworkId,
        resource: AppStoreResource,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        super::plugin_market::fetch_store(framework_id.as_str(), resource).await
    }

    pub async fn list_plugins(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<KarinPluginInstalled>, AppFrameworkError> {
        Ok(self
            .list_store_installed(id, AppStoreResource::Plugin)
            .await?
            .into_iter()
            .filter_map(|item| item.to_karin())
            .collect())
    }

    pub async fn list_store_installed(
        &self,
        id: &AppInstanceId,
        resource: AppStoreResource,
    ) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        adapter
            .list_installed(host.as_ref(), &instance, resource)
            .await
    }

    pub async fn list_plugin_config_docs(
        &self,
        id: &AppInstanceId,
        plugin_name: &str,
    ) -> Result<Vec<ncd_domain::AppConfigDocument>, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation(
                "应用实例尚未安装".to_string(),
            ));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        adapter
            .list_plugin_config_docs(host.as_ref(), &instance, plugin_name)
            .await
    }

    pub async fn plugin_config_schema(
        &self,
        id: &AppInstanceId,
        plugin_name: &str,
    ) -> Result<Option<AppPluginConfigSchema>, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation(
                "应用实例尚未安装".to_string(),
            ));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        adapter
            .plugin_config_schema(host.as_ref(), &instance, plugin_name)
            .await
    }

    pub async fn run_plugin_op(
        &self,
        id: &AppInstanceId,
        name: &str,
        action: AppPluginAction,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        self.run_store_op(id, name, action, AppStoreResource::Plugin, log)
            .await
    }

    pub async fn run_store_op(
        &self,
        id: &AppInstanceId,
        name: &str,
        action: AppPluginAction,
        resource: AppStoreResource,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation(
                "应用实例尚未安装".to_string(),
            ));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        if let Some(sink) = log {
            sink("读取官方目录".into());
        }
        match action {
            AppPluginAction::Install | AppPluginAction::Update => {
                let entry = self
                    .resolve_store_entry(
                        host.as_ref(),
                        adapter.as_ref(),
                        &instance,
                        name,
                        action,
                        resource,
                        log,
                    )
                    .await?;
                self.dispatch_store_write(
                    host.as_ref(),
                    adapter.as_ref(),
                    &instance,
                    &entry,
                    action,
                    log,
                )
                .await?;
                adapter
                    .confirm_store_item(host.as_ref(), &instance, &entry)
                    .await?;
                Ok(())
            }
            AppPluginAction::Uninstall => {
                let installed = adapter
                    .list_installed(host.as_ref(), &instance, resource)
                    .await?;
                if let Some(found) = installed
                    .iter()
                    .find(|p| p.id == name || p.name == name)
                {
                    return adapter
                        .uninstall_store_item(
                            host.as_ref(),
                            &instance,
                            &found.id,
                            found.flavor,
                            resource,
                            log,
                        )
                        .await;
                }
                let market = super::plugin_market::fetch_store(
                    instance.framework_id.as_str(),
                    resource,
                )
                .await?;
                let entry = find_store_entry(&market, name).ok_or_else(|| {
                    AppFrameworkError::Validation(format!("未安装且目录中没有 {name}"))
                })?;
                self.uninstall_market_entry(host.as_ref(), adapter.as_ref(), &instance, entry, log)
                    .await
            }
        }
    }

    pub async fn set_plugin_enabled(
        &self,
        id: &AppInstanceId,
        name: &str,
        enabled: bool,
        overwrite: bool,
    ) -> Result<AppConfigWriteResult, AppFrameworkError> {
        self.set_store_enabled(id, name, AppStoreResource::Plugin, enabled, overwrite)
            .await
    }

    pub async fn set_store_enabled(
        &self,
        id: &AppInstanceId,
        name: &str,
        resource: AppStoreResource,
        enabled: bool,
        overwrite: bool,
    ) -> Result<AppConfigWriteResult, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        if adapter.store_enable_via_config() {
            let envelope = self.read_config(id).await?;
            let Some(cfg) = adapter.apply_store_enabled(
                &envelope.config,
                name,
                resource,
                enabled,
            )?
            else {
                return Err(AppFrameworkError::Validation(
                    "该应用端声称走配置启停，但没有返回新配置".into(),
                ));
            };
            let base = if overwrite {
                None
            } else {
                Some(envelope.revision)
            };
            return self.write_config(id, cfg, base).await;
        }

        let host = self.resolve_host(&instance.host_id).await?;
        adapter
            .set_store_enabled(host.as_ref(), &instance, name, resource, enabled, overwrite)
            .await?;
        let envelope = adapter.read_config(host.as_ref(), &instance).await?;
        Ok(AppConfigWriteResult {
            config: envelope.config,
            revision: envelope.revision,
            documents: envelope.documents,
            restart_required: instance.state == AppInstanceState::Running,
            relinked: false,
            port_changed: false,
        })
    }

    async fn resolve_store_entry(
        &self,
        host: &dyn Host,
        adapter: &dyn AppFrameworkAdapter,
        instance: &AppInstance,
        name: &str,
        action: AppPluginAction,
        resource: AppStoreResource,
        log: Option<&PluginLogSink>,
    ) -> Result<AppStoreMarketEntry, AppFrameworkError> {
        let market = super::plugin_market::fetch_store(instance.framework_id.as_str(), resource).await;
        let market_failed = market.is_err();
        let fallback_err = match market {
            Ok(list) => {
                if let Some(entry) = find_store_entry(&list, name) {
                    return Ok(entry.clone());
                }
                AppFrameworkError::Validation(format!("目录没有 {name}"))
            }
            Err(err) => err,
        };
        if action == AppPluginAction::Install {
            return Err(fallback_err);
        }
        // 目录挂了或条目下架时，NoneBot 还能用已装 PyPI 包名 `uv add pkg@latest`。
        // Karin git/app 缺 repo/url，不能合成空壳。
        let installed = adapter.list_installed(host, instance, resource).await?;
        let Some(found) = installed.iter().find(|p| {
            (p.id == name || p.name == name) && p.flavor == AppStoreFlavor::Pypi
        }) else {
            return Err(fallback_err);
        };
        if market_failed && let Some(sink) = log {
            sink("官方目录不可用，使用已装包名更新".into());
        }
        installed_to_market_entry(found, resource)
    }

    async fn dispatch_store_write(
        &self,
        host: &dyn Host,
        adapter: &dyn AppFrameworkAdapter,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        action: AppPluginAction,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        if entry.flavor == AppStoreFlavor::App {
            return self
                .install_app_files(host, adapter, instance, entry, log)
                .await;
        }
        match action {
            AppPluginAction::Update => adapter.update_store_item(host, instance, entry, log).await,
            _ => adapter.install_store_item(host, instance, entry, log).await,
        }
    }

    async fn install_app_files(
        &self,
        host: &dyn Host,
        adapter: &dyn AppFrameworkAdapter,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        for file in &entry.files {
            let basename = app_file_basename(&file.url)?;
            let dest = adapter.store_app_file_dest(instance, &basename).ok_or_else(|| {
                AppFrameworkError::PluginUnsupported(instance.framework_id.as_str().to_string())
            })?;
            if let Some(sink) = log {
                sink(format!("下载 {basename}"));
            }
            super::download::download_url_to_host(host, &file.url, &dest).await?;
            if let Some(sink) = log {
                sink(format!("已写入 {basename}"));
            }
        }
        Ok(())
    }

    async fn uninstall_market_entry(
        &self,
        host: &dyn Host,
        adapter: &dyn AppFrameworkAdapter,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        if entry.flavor == AppStoreFlavor::App {
            for file in &entry.files {
                let basename = app_file_basename(&file.url)?;
                adapter
                    .uninstall_store_item(
                        host,
                        instance,
                        &basename,
                        AppStoreFlavor::App,
                        entry.resource,
                        log,
                    )
                    .await?;
            }
            return Ok(());
        }
        adapter
            .uninstall_store_item(
                host,
                instance,
                &entry.id,
                entry.flavor,
                entry.resource,
                log,
            )
            .await
    }

    async fn config_context(
        &self,
        id: &AppInstanceId,
    ) -> Result<(AppInstance, Arc<dyn AppFrameworkAdapter>, Arc<dyn Host>), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation(
                "应用实例尚未安装，还没有可编辑的配置".to_string(),
            ));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        Ok((instance, adapter, host))
    }

    /// 类型化读取；没有类型化模型（ConfigUnsupported）或读失败都视作「无」
    async fn typed_snapshot(
        &self,
        adapter: &dyn AppFrameworkAdapter,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Option<AppInstanceConfigEnvelope> {
        match adapter.read_config(host, instance).await {
            Ok(env) => Some(env),
            Err(AppFrameworkError::ConfigUnsupported(_)) => None,
            Err(e) => {
                tracing::debug!(instance = instance.id.as_str(), error = %e, "typed config snapshot");
                None
            }
        }
    }

    async fn ensure_port_free(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<(), AppFrameworkError> {
        if port == 0 {
            return Err(AppFrameworkError::Validation("端口不能为 0".to_string()));
        }
        let taken = self
            .store
            .list()
            .await
            .into_iter()
            .any(|i| i.id != instance.id && i.host_id == instance.host_id && i.port == port);
        if taken {
            return Err(AppFrameworkError::Validation(format!(
                "该主机上已有应用实例占用端口 {port}"
            )));
        }
        if instance.host_id == LOCAL_HOST_ID && !local_port_free(port) {
            return Err(AppFrameworkError::Validation(format!(
                "本机端口 {port} 已被其它程序占用"
            )));
        }
        Ok(())
    }

    /// 写盘之后的联动：端口同步 → 已对接且对接输入变了就重新 apply_link → 非热加载文件变了提示重启
    async fn sync_after_config_write(
        &self,
        instance: &AppInstance,
        before: &AppInstanceConfigEnvelope,
        after: &AppInstanceConfigEnvelope,
    ) -> Result<ConfigSyncOutcome, AppFrameworkError> {
        let mut outcome = ConfigSyncOutcome::default();
        let new_port = after.config.listen_port();
        let running = instance.state == AppInstanceState::Running;

        if new_port != instance.port {
            let updated = self
                .store
                .update(&instance.id, |i| i.port = new_port)
                .await?;
            self.publish(&updated, "port_changed");
            outcome.port_changed = true;
            // 监听口是启动时绑定的，热加载救不了
            outcome.restart_required |= running;
        }

        if let Some(link) = instance.link.as_ref()
            && before.config.link_inputs_changed(&after.config)
        {
            self.apply_link(&instance.id, &link.bot_id)
                .await
                .map_err(|e| {
                    AppFrameworkError::Integration(format!(
                        "配置已保存，但重新对接协议 Bot 失败：{e}"
                    ))
                })?;
            outcome.relinked = true;
        }

        if running {
            let cold_changed = after.documents.iter().any(|d| {
                !d.hot_reload
                    && before
                        .documents
                        .iter()
                        .find(|b| b.doc_id == d.doc_id)
                        .is_none_or(|b| b.revision != d.revision)
            });
            outcome.restart_required |= cold_changed;
        }
        Ok(outcome)
    }

    // ---- 内部 ----

    async fn ensure_link_tunnel(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
    ) -> Result<u16, AppFrameworkError> {
        match classify_app_link(&bot.bot.runtime_target, &instance.host_id) {
            Some(AppLinkTopology::SameHost) => Ok(instance.port),
            Some(AppLinkTopology::LocalBotRemoteApp) => {
                self.ensure_local_to_remote_tunnel(instance).await
            }
            Some(AppLinkTopology::RemoteBotLocalApp) => {
                self.ensure_remote_to_local_tunnel(instance, bot).await
            }
            Some(AppLinkTopology::RemoteBotRemoteApp) => Err(AppFrameworkError::Validation(
                "两台远端对接不走桌面隧道".into(),
            )),
            None => Err(unsupported_link_topology(
                &bot.bot.runtime_target,
                &instance.host_id,
            )),
        }
    }

    async fn ensure_local_to_remote_tunnel(
        &self,
        instance: &AppInstance,
    ) -> Result<u16, AppFrameworkError> {
        self.ensure_local_to_remote_port_tunnel(
            instance,
            instance.port,
            instance.id.as_str().to_string(),
        )
        .await
    }

    async fn ensure_local_to_remote_port_tunnel(
        &self,
        instance: &AppInstance,
        remote_port: u16,
        key: String,
    ) -> Result<u16, AppFrameworkError> {
        if server_id_of_host(&instance.host_id).is_none() {
            return Ok(remote_port);
        }
        if remote_port == 0 {
            return Err(AppFrameworkError::Validation("应用实例端口无效".into()));
        }
        {
            let mut map = self.tunnels.lock().await;
            if let Some(existing) = map.get(&key) {
                if existing.app_port == remote_port
                    && existing.topology == AppLinkTopology::LocalBotRemoteApp
                {
                    return Ok(existing.handle.local_port());
                }
            }
            map.remove(&key);
        }
        let host = self.resolve_host(&instance.host_id).await?;
        let handle = host
            .open_tunnel(TunnelSpec::local_to_remote(0, remote_port))
            .await
            .map_err(host_err)?;
        let local_port = handle.local_port();
        if local_port == 0 {
            return Err(AppFrameworkError::Host("SSH 隧道未分配本地端口".into()));
        }
        let mut map = self.tunnels.lock().await;
        if let Some(existing) = map.get(&key) {
            if existing.app_port == remote_port
                && existing.topology == AppLinkTopology::LocalBotRemoteApp
            {
                return Ok(existing.handle.local_port());
            }
        }
        map.insert(
            key,
            AppInstanceTunnel {
                app_port: remote_port,
                topology: AppLinkTopology::LocalBotRemoteApp,
                handle,
            },
        );
        Ok(local_port)
    }

    async fn ensure_remote_to_local_tunnel(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
    ) -> Result<u16, AppFrameworkError> {
        if instance.port == 0 {
            return Err(AppFrameworkError::Validation("应用实例端口无效".into()));
        }
        let key = instance.id.as_str().to_string();
        {
            let mut map = self.tunnels.lock().await;
            if let Some(existing) = map.get(&key) {
                if existing.app_port == instance.port
                    && existing.topology == AppLinkTopology::RemoteBotLocalApp
                    && existing.handle.remote_listen_port() != 0
                {
                    return Ok(existing.handle.remote_listen_port());
                }
            }
            map.remove(&key);
        }
        let host = self
            .resolve_host(&host_id_of_runtime_target(&bot.bot.runtime_target))
            .await?;
        let handle = host
            .open_tunnel(TunnelSpec::remote_to_local(0, instance.port))
            .await
            .map_err(host_err)?;
        let bot_port = handle.remote_listen_port();
        if bot_port == 0 {
            return Err(AppFrameworkError::Host("SSH 隧道未分配远端端口".into()));
        }
        let mut map = self.tunnels.lock().await;
        if let Some(existing) = map.get(&key) {
            if existing.app_port == instance.port
                && existing.topology == AppLinkTopology::RemoteBotLocalApp
                && existing.handle.remote_listen_port() != 0
            {
                return Ok(existing.handle.remote_listen_port());
            }
        }
        map.insert(
            key,
            AppInstanceTunnel {
                app_port: instance.port,
                topology: AppLinkTopology::RemoteBotLocalApp,
                handle,
            },
        );
        Ok(bot_port)
    }

    async fn drop_instance_tunnel(&self, id: &AppInstanceId) {
        let id_str = id.as_str().to_string();
        let webui = format!("{id_str}:webui");
        let mut map = self.tunnels.lock().await;
        map.remove(&id_str);
        map.remove(&webui);
    }

    async fn ensure_resident_link(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
        bot_id: &BotId,
    ) -> Result<u16, AppFrameworkError> {
        let bot_host_id = host_id_of_runtime_target(&bot.bot.runtime_target);
        let bot_host = self.resolve_host(&bot_host_id).await?;
        let app_host = self.resolve_host(&instance.host_id).await?;
        let taken = self.taken_resident_ports(&instance.id, &bot_host_id).await;
        let reuse = instance
            .link
            .as_ref()
            .filter(|l| &l.bot_id == bot_id)
            .and_then(|l| l.resident_forward_port);
        resident_link::ensure_resident_link(
            app_host.as_ref(),
            bot_host.as_ref(),
            ResidentLinkSpec {
                instance,
                reuse_port: reuse,
                taken_ports: &taken,
            },
        )
        .await
    }

    async fn taken_resident_ports(&self, current: &AppInstanceId, bot_host_id: &str) -> Vec<u16> {
        let mut taken = Vec::new();
        for inst in self.store.list().await {
            if inst.host_id == bot_host_id {
                taken.push(inst.port);
            }
            let Some(link) = inst.link.as_ref() else {
                continue;
            };
            let Some(port) = link.resident_forward_port else {
                continue;
            };
            if inst.id == *current {
                continue;
            }
            let Ok(Some(other)) = self.bot_manager.bot_config(&link.bot_id).await else {
                continue;
            };
            if host_id_of_runtime_target(&other.bot.runtime_target) == bot_host_id {
                taken.push(port);
            }
        }
        taken
    }

    async fn teardown_resident_best_effort(&self, instance: &AppInstance, bot_id: &BotId) {
        let app_host = match self.resolve_host(&instance.host_id).await {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(
                    instance = instance.id.as_str(),
                    error = %e,
                    "resident link teardown skipped (app host)"
                );
                return;
            }
        };
        let bot_host = match self.bot_manager.bot_config(bot_id).await {
            Ok(Some(bot)) => self
                .resolve_host(&host_id_of_runtime_target(&bot.bot.runtime_target))
                .await
                .ok(),
            _ => None,
        };
        if let Err(e) = resident_link::teardown_resident_link(
            app_host.as_ref(),
            bot_host.as_ref().map(|h| h.as_ref()),
            &instance.id,
        )
        .await
        {
            tracing::warn!(
                instance = instance.id.as_str(),
                error = %e,
                "resident link teardown failed"
            );
        }
    }

    async fn reconcile_resident_link(&self, instance: &AppInstance) -> Result<(), AppFrameworkError> {
        let Some(link) = instance.link.as_ref() else {
            return Ok(());
        };
        let Some(fwd) = link.resident_forward_port else {
            return Ok(());
        };
        let Some(bot) = self
            .bot_manager
            .bot_config(&link.bot_id)
            .await
            .map_err(AppFrameworkError::Integration)?
        else {
            return Ok(());
        };
        let app_host = self.resolve_host(&instance.host_id).await?;
        let bot_host = self
            .resolve_host(&host_id_of_runtime_target(&bot.bot.runtime_target))
            .await?;
        resident_link::reconcile_resident_link(app_host.as_ref(), bot_host.as_ref(), instance, fwd)
            .await
    }

    /// 已对接的跨机实例：重开 Desktop 隧道；分配口变了就再热推 Bot URL。
    async fn reconcile_link_tunnel(
        &self,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        let Some(link) = instance.link.as_ref() else {
            return Ok(());
        };
        let Some(mut bot) = self
            .bot_manager
            .bot_config(&link.bot_id)
            .await
            .map_err(AppFrameworkError::Integration)?
        else {
            return Ok(());
        };
        let Some(topology) = classify_app_link(&bot.bot.runtime_target, &instance.host_id) else {
            return Ok(());
        };
        if !needs_desktop_ssh_tunnel(topology) {
            return Ok(());
        }
        let local_port = self.ensure_link_tunnel(instance, &bot).await?;
        let Some(conn) = bot
            .connect
            .websocket_clients
            .iter_mut()
            .find(|c| c.base.name == link.connection_name)
        else {
            return Ok(());
        };
        let next = rewrite_ws_loopback_port(&conn.url, local_port)
            .map_err(AppFrameworkError::Validation)?;
        if next == conn.url {
            return Ok(());
        }
        conn.url = next;
        self.bot_manager
            .upsert_bot_config(bot)
            .await
            .map_err(AppFrameworkError::Integration)?;
        Ok(())
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

    fn component_spec(&self, host: &dyn Host, instance: &AppInstance) -> AppComponentSpec {
        AppComponentSpec {
            install_dir: HostPath::from_posix(&instance.install_dir),
            port: instance.port,
            node_bin: self
                .managed_component_dir(host, "NodeJs")
                .map(|dir| ncd_component::NodeJsComponent::node_binary_path_for_os(&dir, host.os())),
            uv_bin: self
                .managed_component_dir(host, "Uv")
                .map(|dir| ncd_component::UvComponent::uv_binary_path_for_os(&dir, host.os())),
            npm_registry: self.npm_registry.clone(),
            install_renderer: instance.install_renderer,
            adopt_existing: instance.origin.is_imported(),
            instance_id: instance.id.as_str().to_string(),
            webui_username: None,
            webui_password: None,
        }
    }

    /// 桌面端管理的运行时依赖落点（与对应组件的安装目录一致）；远端安装时由 factory 按远端路径
    /// 推断并写进实例标记，起停 / 探测时框架组件读标记，这里只管本机
    fn managed_component_dir(&self, host: &dyn Host, name: &str) -> Option<HostPath> {
        if host.locality() != Locality::Local {
            return None;
        }
        Some(
            data_root_to_host_path(&self.data_root, host.os())
                .join("components")
                .join(name),
        )
    }

    async fn detect(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<DetectOutcome, AppFrameworkError> {
        let adapter = self.registry.get(&instance.framework_id)?;
        let component = adapter.component(&self.component_spec(host, instance));
        component
            .detect_outcome(host)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))
    }

    pub async fn preview_install_dir(
        &self,
        host_id: &str,
        framework_id: &AppFrameworkId,
    ) -> Result<HostPath, AppFrameworkError> {
        let host = self.resolve_host(host_id).await?;
        self.apps_root_for(host.as_ref(), framework_id).await
    }

    pub async fn resolve_install_dir(
        &self,
        host: &dyn Host,
        host_id: &str,
        framework: &AppFrameworkId,
        id: &AppInstanceId,
        override_dir: Option<&str>,
    ) -> Result<HostPath, AppFrameworkError> {
        let path = match override_dir.map(str::trim).filter(|s| !s.is_empty()) {
            None => self.install_dir_for(host, framework, id).await?,
            Some(raw) => parse_user_install_dir(raw, host.os())?,
        };
        let posix = path.as_posix();
        let taken = self.store.list().await.into_iter().any(|i| {
            i.host_id == host_id && i.id != *id && i.install_dir == posix
        });
        if taken {
            return Err(AppFrameworkError::Validation(
                "该目录已被其它实例占用".to_string(),
            ));
        }
        let exists = host
            .exists(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        if exists {
            match host.list_dir(&path).await {
                Ok(entries) if entries.is_empty() => {}
                Ok(_) => {
                    return Err(AppFrameworkError::Validation(
                        "目录非空，请选空文件夹或不存在的路径".to_string(),
                    ));
                }
                Err(_) => {
                    return Err(AppFrameworkError::Validation(
                        "安装路径必须是目录".to_string(),
                    ));
                }
            }
        }
        Ok(path)
    }

    async fn bind_existing_dir(
        &self,
        host: &dyn Host,
        host_id: &str,
        id: &AppInstanceId,
        override_dir: Option<&str>,
    ) -> Result<HostPath, AppFrameworkError> {
        let raw = override_dir.map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| {
            AppFrameworkError::Validation("导入必须指定已有项目目录".into())
        })?;
        let path = parse_user_install_dir(raw, host.os())?;
        let posix = path.as_posix();
        let taken = self.store.list().await.into_iter().any(|i| {
            i.host_id == host_id && i.id != *id && i.install_dir == posix
        });
        if taken {
            return Err(AppFrameworkError::Validation(
                "该目录已被其它实例占用".to_string(),
            ));
        }
        if !host
            .exists(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?
        {
            return Err(AppFrameworkError::Validation("目录不存在".into()));
        }
        if host.list_dir(&path).await.is_err() {
            return Err(AppFrameworkError::Validation("导入路径必须是目录".into()));
        }
        Ok(path)
    }

    async fn apps_root_for(
        &self,
        host: &dyn Host,
        framework: &AppFrameworkId,
    ) -> Result<HostPath, AppFrameworkError> {
        match host.locality() {
            Locality::Local => Ok(data_root_to_host_path(&self.data_root, host.os())
                .join(LOCAL_APPS_DIR)
                .join(framework.as_str())),
            Locality::Remote => {
                let out = host
                    .run_to_string(
                        HostCommand::new("sh")
                            .arg("-c")
                            .arg("printf '%s' \"$HOME\"")
                            .timeout(Duration::from_secs(15)),
                    )
                    .await
                    .map_err(host_err)?;
                let home = out.stdout.trim();
                if home.is_empty() || !home.starts_with('/') {
                    return Err(AppFrameworkError::Host(
                        "无法解析远端 $HOME，不能决定安装目录".to_string(),
                    ));
                }
                Ok(HostPath::from_posix(home)
                    .join(REMOTE_APPS_REL)
                    .join(framework.as_str()))
            }
        }
    }

    async fn install_dir_for(
        &self,
        host: &dyn Host,
        framework: &AppFrameworkId,
        id: &AppInstanceId,
    ) -> Result<HostPath, AppFrameworkError> {
        Ok(self.apps_root_for(host, framework).await?.join(id.as_str()))
    }

    #[cfg(test)]
    async fn resolve_install_dir_for_test(
        &self,
        host_id: &str,
        framework: &AppFrameworkId,
        id: &AppInstanceId,
        override_dir: Option<&str>,
    ) -> Result<HostPath, AppFrameworkError> {
        let host = self.resolve_host(host_id).await?;
        self.resolve_install_dir(host.as_ref(), host_id, framework, id, override_dir)
            .await
    }

    fn publish(&self, instance: &AppInstance, reason: &str) {
        self.event_bus
            .publish(DomainEvent::app_instance_changed(instance.clone(), reason));
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct ConfigSyncOutcome {
    port_changed: bool,
    relinked: bool,
    restart_required: bool,
}

fn find_store_entry<'a>(
    market: &'a [AppStoreMarketEntry],
    name: &str,
) -> Option<&'a AppStoreMarketEntry> {
    // 不用 package：OneBot V11/V12 共用 nonebot-adapter-onebot，按包名会装错
    market
        .iter()
        .find(|e| e.id == name || e.module_name == name)
        .or_else(|| {
            let hits: Vec<_> = market.iter().filter(|e| e.name == name).collect();
            (hits.len() == 1).then_some(hits[0])
        })
}

fn installed_to_market_entry(
    item: &AppStoreInstalled,
    resource: AppStoreResource,
) -> Result<AppStoreMarketEntry, AppFrameworkError> {
    if item.package.trim().is_empty() {
        return Err(AppFrameworkError::Validation(
            "官方目录不可用，且已装条目没有包名".into(),
        ));
    }
    Ok(AppStoreMarketEntry {
        resource,
        id: item.id.clone(),
        name: item.name.clone(),
        description: String::new(),
        version: item.version.clone().unwrap_or_default(),
        author: String::new(),
        homepage: String::new(),
        time: String::new(),
        package: item.package.clone(),
        module_name: item.id.clone(),
        flavor: item.flavor,
        is_official: false,
        valid: true,
        tags: Vec::new(),
        supported_adapters: Vec::new(),
        authors: Vec::new(),
        repos: Vec::new(),
        files: Vec::new(),
        allow_build: Vec::new(),
    })
}

#[cfg(test)]
mod installed_fallback_tests {
    use super::*;

    fn installed(package: &str, flavor: AppStoreFlavor) -> AppStoreInstalled {
        AppStoreInstalled {
            id: "nonebot_plugin_foo".into(),
            name: "foo".into(),
            resource: AppStoreResource::Plugin,
            flavor,
            version: Some("1.0.0".into()),
            enabled: true,
            package: package.into(),
        }
    }

    #[test]
    fn installed_fallback_needs_package() {
        assert!(
            installed_to_market_entry(&installed("", AppStoreFlavor::Pypi), AppStoreResource::Plugin)
                .is_err()
        );
        let entry = installed_to_market_entry(
            &installed("nonebot-plugin-foo", AppStoreFlavor::Pypi),
            AppStoreResource::Plugin,
        )
        .unwrap();
        assert_eq!(entry.package, "nonebot-plugin-foo");
        assert_eq!(entry.module_name, "nonebot_plugin_foo");
        assert_eq!(entry.id, "nonebot_plugin_foo");
        assert_eq!(entry.flavor, AppStoreFlavor::Pypi);
        assert_eq!(entry.resource, AppStoreResource::Plugin);
    }
}

/// 按连接名 upsert（同名替换，保证重复对接是替换不是追加）
pub fn upsert_ws_client(bot: &mut BotConfig, connection: ncd_domain::WebsocketClientConfig) {
    match bot
        .connect
        .websocket_clients
        .iter_mut()
        .find(|c| c.base.name == connection.base.name)
    {
        Some(slot) => *slot = connection,
        None => bot.connect.websocket_clients.push(connection),
    }
}

/// Bot 上有没有应用端对接连接（UI 徽章 / 迁移提示用）
pub fn app_link_connections(bot: &BotConfig) -> Vec<String> {
    bot.connect
        .websocket_clients
        .iter()
        .filter(|c| is_app_link_connection_name(&c.base.name))
        .map(|c| c.base.name.clone())
        .collect()
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::{
        AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, ConnectConfig,
        DeploymentType, MessagePostFormat, NetworkBaseFields, WebsocketClientConfig,
        WebsocketServerConfig, WsRole,
    };

    fn bot() -> BotConfig {
        bot_with(10001, BackendType::NapCat, RuntimeTarget::Local)
    }

    fn bot_with(qq_id: u64, backend: BackendType, target: RuntimeTarget) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "b".into(),
                qq_id,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: target,
                backend_type: backend,
                deployment_type: DeploymentType::default(),
                snowluma_start_mode: None,
                webui_password_takeover: false,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            status_command: None,
        }
    }

    fn ws(name: &str, url: &str) -> WebsocketClientConfig {
        WebsocketClientConfig {
            base: NetworkBaseFields {
                enable: true,
                name: name.into(),
                message_post_format: MessagePostFormat::Array,
                token: "t".into(),
                debug: false,
            },
            url: url.into(),
            report_self_message: false,
            heart_interval: 30000,
            reconnect_interval: 30000,
            role: WsRole::Universal,
        }
    }

    #[test]
    fn upsert_replaces_same_name_and_keeps_others() {
        let mut b = bot();
        b.connect.websocket_clients.push(ws("user", "ws://x"));
        upsert_ws_client(&mut b, ws("ncd-app:k1", "ws://127.0.0.1:7777/onebot/v11/ws"));
        upsert_ws_client(&mut b, ws("ncd-app:k1", "ws://127.0.0.1:7801/onebot/v11/ws"));
        assert_eq!(b.connect.websocket_clients.len(), 2);
        assert_eq!(
            b.connect.websocket_clients[1].url,
            "ws://127.0.0.1:7801/onebot/v11/ws"
        );
        assert_eq!(app_link_connections(&b), vec!["ncd-app:k1".to_string()]);
    }

    #[test]
    fn token_and_id_shapes() {
        let t = generate_token();
        assert_eq!(t.len(), 24);
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric()));
        let id = short_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// 类型化配置写入的编排分支：版本冲突 / 端口同步 / 已对接时重新 upsert Bot 连接。
    /// 用真实本机 Host + 临时目录当 Karin 实例目录（只碰文件，不起进程）。
    #[cfg(windows)]
    mod write_config {
        use super::*;
        use crate::events::BroadcastEventBus;
        use ncd_appframework::AppInstanceConfig;
        use ncd_appframework::karin::config::KarinInstanceConfig;
        use ncd_appframework::AppFrameworkRegistry;
        use ncd_domain::WebsocketClientConfig;
        use std::sync::Mutex;
        use tokio::sync::Mutex as AsyncMutex;

        struct MemoryBots {
            bots: AsyncMutex<Vec<BotConfig>>,
            upserts: Mutex<usize>,
        }

        #[async_trait::async_trait]
        impl BotConfigPort for MemoryBots {
            async fn bot_config(&self, bot_id: &BotId) -> Result<Option<BotConfig>, String> {
                Ok(self
                    .bots
                    .lock()
                    .await
                    .iter()
                    .find(|b| b.bot.qq_id.to_string() == bot_id.as_str())
                    .cloned())
            }

            async fn upsert_bot_config(&self, config: BotConfig) -> Result<(), String> {
                let mut bots = self.bots.lock().await;
                match bots.iter_mut().find(|b| b.bot.qq_id == config.bot.qq_id) {
                    Some(slot) => *slot = config,
                    None => bots.push(config),
                }
                *self.upserts.lock().unwrap() += 1;
                Ok(())
            }

            async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
                Ok(self.bots.lock().await.clone())
            }
        }

        struct Fixture {
            _tmp: tempfile::TempDir,
            inst_dir: std::path::PathBuf,
            manager: Arc<AppManager>,
            bots: Arc<MemoryBots>,
            id: AppInstanceId,
        }

        async fn fixture(linked: bool) -> Fixture {
            fixture_on_host(linked, LOCAL_HOST_ID, AppPlacement::LocalNative).await
        }

        async fn fixture_on_host(
            linked: bool,
            host_id: &str,
            placement: AppPlacement,
        ) -> Fixture {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("data");
            let inst_dir = tmp.path().join("karin");
            std::fs::create_dir_all(inst_dir.join("@karinjs/config")).unwrap();
            std::fs::write(
                inst_dir.join(".env"),
                "# HTTP监听端口\nHTTP_PORT=7777\n# ws_server鉴权秘钥\nWS_SERVER_AUTH_KEY=tok-1\nLOG_LEVEL=info\n",
            )
            .unwrap();
            std::fs::write(
                inst_dir.join("@karinjs/config/redis.json"),
                "{\"url\":\"redis://127.0.0.1:6379\",\"username\":\"\",\"password\":\"\",\"database\":0}\n",
            )
            .unwrap();

            let bus = Arc::new(BroadcastEventBus::default());
            let store = Arc::new(AppInstanceStore::empty(&root));
            let local: Arc<dyn Host> = Arc::new(ncd_host::local::LocalWindowsHost::new());
            let bots = Arc::new(MemoryBots {
                bots: AsyncMutex::new(vec![
                    bot(),
                    bot_with(20002, BackendType::SnowLuma, RuntimeTarget::Local),
                    bot_with(30003, BackendType::NapCat, RuntimeTarget::server("vps")),
                    bot_with(40004, BackendType::SnowLuma, RuntimeTarget::server("vps")),
                    bot_with(50005, BackendType::NapCat, RuntimeTarget::server("other")),
                ]),
                upserts: Mutex::new(0),
            });
            let manager = Arc::new(AppManager::new(
                Arc::new(AppFrameworkRegistry::with_builtin()),
                Arc::clone(&store),
                Arc::new(NativeAppRuntime::new(Arc::clone(&bus), Arc::clone(&store))),
                Arc::new(ncd_server::LocalOnlyHostResolver::new(local)),
                bots.clone(),
                bus,
                &root,
            ));
            let id = AppInstanceId::new("k1");
            let install_dir = HostPath::from_windows(inst_dir.to_string_lossy().as_ref());
            store
                .upsert(AppInstance {
                    id: id.clone(),
                    framework_id: AppFrameworkId::new("karin"),
                    display_name: "Karin".into(),
                    placement,
                    host_id: host_id.to_string(),
                    install_dir: install_dir.as_posix().to_string(),
                    port: 7777,
                    state: AppInstanceState::Stopped,
                    link: linked.then(|| AppLinkRecord {
                        bot_id: BotId::new("10001"),
                        mode: OneBotLinkMode::ReverseWs,
                        connection_name: app_link_connection_name(&id),
                        linked_at_ms: 1,
                        resident_forward_port: None,
                    }),
                    installed_version: Some("1.0.0".into()),
                    last_error: None,
                    created_at_ms: 1,
                    install_renderer: true,
                    origin: ncd_domain::AppInstanceOrigin::Created,
                })
                .await
                .unwrap();
            Fixture {
                _tmp: tmp,
                inst_dir,
                manager,
                bots,
                id,
            }
        }

        fn karin(envelope: &AppInstanceConfigEnvelope) -> KarinInstanceConfig {
            let AppInstanceConfig::Karin(k) = &envelope.config else {
                panic!("expected Karin config");
            };
            k.clone()
        }

        fn ws_client_urls(bot: &BotConfig) -> Vec<String> {
            bot.connect
                .websocket_clients
                .iter()
                .map(|c: &WebsocketClientConfig| c.url.clone())
                .collect()
        }

        #[tokio::test]
        async fn tail_log_reads_ncd_stdout_file() {
            let f = fixture(false).await;
            std::fs::write(
                f.inst_dir.join(".ncd-karin.log"),
                "boot\n[INFO] karin listening\n",
            )
            .unwrap();
            let snap = f.manager.tail_log(&f.id, 100).await.unwrap();
            assert!(
                snap.lines.iter().any(|l| l.contains("karin listening")),
                "{:?}",
                snap.lines
            );
        }

        #[tokio::test]
        async fn tail_log_falls_back_to_project_logs_dir() {
            let f = fixture(false).await;
            std::fs::create_dir_all(f.inst_dir.join("logs")).unwrap();
            std::fs::write(f.inst_dir.join("logs").join("nonebot.log"), "from logs dir\n").unwrap();
            let snap = f.manager.tail_log(&f.id, 100).await.unwrap();
            assert!(
                snap.lines.iter().any(|l| l.contains("from logs dir")),
                "{:?}",
                snap.lines
            );
        }

        #[tokio::test]
        async fn stale_revision_is_rejected_and_none_means_overwrite() {
            let f = fixture(false).await;
            let env = f.manager.read_config(&f.id).await.unwrap();
            let mut cfg = karin(&env);
            cfg.config.master.push("123".into());

            let err = f
                .manager
                .write_config(
                    &f.id,
                    AppInstanceConfig::Karin(cfg.clone()),
                    Some("deadbeef".into()),
                )
                .await
                .unwrap_err();
            assert!(matches!(err, AppFrameworkError::ConfigConflict(_)));

            let ok = f
                .manager
                .write_config(&f.id, AppInstanceConfig::Karin(cfg.clone()), None)
                .await
                .unwrap();
            assert!(!ok.restart_required);
            assert!(!ok.relinked);
            assert!(!ok.port_changed);
            assert_ne!(ok.revision, env.revision);

            // 正确的 base_revision 通过
            let mut cfg2 = cfg.clone();
            cfg2.env.log_level = "debug".into();
            f.manager
                .write_config(&f.id, AppInstanceConfig::Karin(cfg2), Some(ok.revision))
                .await
                .unwrap();
            let again = karin(&f.manager.read_config(&f.id).await.unwrap());
            assert_eq!(again.env.log_level, "debug");
            assert_eq!(again.config.master, vec!["console", "123"]);
        }

        #[tokio::test]
        async fn port_change_syncs_instance_and_relinks_bot() {
            let f = fixture(true).await;
            let env = f.manager.read_config(&f.id).await.unwrap();
            let mut cfg = karin(&env);
            cfg.env.http_port = 7801;

            let res = f
                .manager
                .write_config(&f.id, AppInstanceConfig::Karin(cfg), Some(env.revision))
                .await
                .unwrap();
            assert!(res.port_changed);
            assert!(res.relinked);
            assert!(!res.restart_required, "实例未运行，不提示重启");

            let inst = f.manager.get_instance(&f.id).await.unwrap();
            assert_eq!(inst.port, 7801);
            assert_eq!(*f.bots.upserts.lock().unwrap(), 1);
            let bot = f.bots.bot_config(&BotId::new("10001")).await.unwrap().unwrap();
            assert_eq!(
                ws_client_urls(&bot),
                vec!["ws://127.0.0.1:7801/onebot/v11/ws".to_string()]
            );
            assert_eq!(bot.connect.websocket_clients[0].base.token, "tok-1");
        }

        #[tokio::test]
        async fn ws_key_change_relinks_with_new_token_but_plain_edit_does_not() {
            let f = fixture(true).await;
            let env = f.manager.read_config(&f.id).await.unwrap();
            let mut cfg = karin(&env);
            cfg.config.admin.push("42".into());
            let res = f
                .manager
                .write_config(&f.id, AppInstanceConfig::Karin(cfg), Some(env.revision))
                .await
                .unwrap();
            assert!(!res.relinked);
            assert_eq!(*f.bots.upserts.lock().unwrap(), 0);

            let mut cfg = karin(&f.manager.read_config(&f.id).await.unwrap());
            cfg.env.ws_server_auth_key = "tok-2".into();
            let res = f
                .manager
                .write_config(&f.id, AppInstanceConfig::Karin(cfg), Some(res.revision))
                .await
                .unwrap();
            assert!(res.relinked);
            let bot = f.bots.bot_config(&BotId::new("10001")).await.unwrap().unwrap();
            assert_eq!(bot.connect.websocket_clients[0].base.token, "tok-2");
        }

        #[tokio::test]
        async fn raw_text_write_checks_revision_and_json_syntax() {
            let f = fixture(false).await;
            let docs = f.manager.list_config_documents(&f.id).await.unwrap();
            assert_eq!(docs.len(), 7);
            let redis = f.manager.read_config_text(&f.id, "redis").await.unwrap();
            assert!(redis.text.contains("6379"));

            let bad = f
                .manager
                .write_config_text(&f.id, "redis", "{nope", Some(redis.revision.clone()))
                .await
                .unwrap_err();
            assert!(matches!(bad, AppFrameworkError::ConfigInvalid(_)));

            let stale = f
                .manager
                .write_config_text(&f.id, "redis", "{}", Some("old".into()))
                .await
                .unwrap_err();
            assert!(matches!(stale, AppFrameworkError::ConfigConflict(_)));

            let ok = f
                .manager
                .write_config_text(
                    &f.id,
                    "redis",
                    "{\"url\":\"redis://10.0.0.1:6379\"}\n",
                    Some(redis.revision),
                )
                .await
                .unwrap();
            assert_ne!(ok.revision, "missing");
            let cfg = karin(&f.manager.read_config(&f.id).await.unwrap());
            assert_eq!(cfg.redis.url, "redis://10.0.0.1:6379");

            // 缺失文件读出来是空文本 + missing
            let groups = f.manager.read_config_text(&f.id, "groups").await.unwrap();
            assert_eq!(groups.revision, "missing");
            assert_eq!(groups.text, "");
        }

        #[tokio::test]
        async fn unlink_then_relink_rewrites_bot_connection_and_fills_ws_server() {
            let f = fixture(false).await;
            std::fs::write(
                f.inst_dir.join("@karinjs/config/adapter.json"),
                r#"{"console":{"isLocal":true},"onebot":{}}"#,
            )
            .unwrap();

            f.manager
                .apply_link(&f.id, &BotId::new("10001"))
                .await
                .unwrap();
            let bot = f.bots.bot_config(&BotId::new("10001")).await.unwrap().unwrap();
            assert_eq!(bot.connect.websocket_clients.len(), 1);
            assert_eq!(bot.connect.websocket_clients[0].base.name, "ncd-app:k1");
            assert_eq!(bot.connect.websocket_clients[0].base.token, "tok-1");

            f.manager.unlink(&f.id).await.unwrap();
            let bot = f.bots.bot_config(&BotId::new("10001")).await.unwrap().unwrap();
            assert!(bot.connect.websocket_clients.is_empty());
            assert!(f.manager.get_instance(&f.id).await.unwrap().link.is_none());

            f.manager
                .apply_link(&f.id, &BotId::new("10001"))
                .await
                .unwrap();
            let bot = f.bots.bot_config(&BotId::new("10001")).await.unwrap().unwrap();
            assert_eq!(bot.connect.websocket_clients.len(), 1);
            assert_eq!(bot.connect.websocket_clients[0].base.token, "tok-1");
            assert!(f.manager.get_instance(&f.id).await.unwrap().link.is_some());

            let adapter: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(f.inst_dir.join("@karinjs/config/adapter.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(adapter["onebot"]["ws_server"]["enable"], true);
        }

        #[tokio::test]
        async fn preview_link_allows_local_nc_and_sl_to_remote_app() {
            let f = fixture_on_host(false, "remote:vps", AppPlacement::RemoteNative).await;
            for bot_id in ["10001", "20002"] {
                let plan = f
                    .manager
                    .preview_link(&f.id, &BotId::new(bot_id))
                    .await
                    .unwrap();
                assert_eq!(plan.connection.url, "ws://127.0.0.1:7777/onebot/v11/ws");
            }
        }

        #[tokio::test]
        async fn apply_link_local_bot_remote_app_needs_ssh_tunnel() {
            let f = fixture_on_host(false, "remote:vps", AppPlacement::RemoteNative).await;
            for bot_id in ["10001", "20002"] {
                let err = f
                    .manager
                    .apply_link(&f.id, &BotId::new(bot_id))
                    .await
                    .unwrap_err();
                let msg = err.to_string();
                assert!(
                    msg.contains("open_tunnel"),
                    "NC/SL 都应走同一条隧道：{msg}"
                );
            }
        }

        #[tokio::test]
        async fn preview_link_allows_remote_nc_and_sl_to_local_app() {
            let f = fixture(false).await;
            for bot_id in ["30003", "40004"] {
                let plan = f
                    .manager
                    .preview_link(&f.id, &BotId::new(bot_id))
                    .await
                    .unwrap();
                assert_eq!(plan.connection.url, "ws://127.0.0.1:7777/onebot/v11/ws");
            }
        }

        #[tokio::test]
        async fn apply_link_remote_bot_local_app_needs_ssh_tunnel() {
            let f = fixture(false).await;
            for bot_id in ["30003", "40004"] {
                let err = f
                    .manager
                    .apply_link(&f.id, &BotId::new(bot_id))
                    .await
                    .unwrap_err();
                let msg = err.to_string();
                assert!(
                    msg.contains("open_tunnel"),
                    "远端 NC/SL 都应走同一条 -R：{msg}"
                );
            }
        }

        #[tokio::test]
        async fn preview_link_allows_two_remote_hosts() {
            let f = fixture_on_host(false, "remote:vps", AppPlacement::RemoteNative).await;
            let plan = f
                .manager
                .preview_link(&f.id, &BotId::new("50005"))
                .await
                .unwrap();
            assert_eq!(plan.connection.url, "ws://127.0.0.1:7777/onebot/v11/ws");
        }

        #[tokio::test]
        async fn apply_link_two_remotes_needs_bot_ssh_dial() {
            let f = fixture_on_host(false, "remote:vps", AppPlacement::RemoteNative).await;
            let err = f
                .manager
                .apply_link(&f.id, &BotId::new("50005"))
                .await
                .unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("SSH") || msg.contains("Linux"),
                "P2 应进入常驻隧道而不是桌面 open_tunnel: {msg}"
            );
            assert!(!msg.contains("尚未开放"), "{msg}");
            assert!(!msg.contains("open_tunnel"), "{msg}");
        }
    }

    #[test]
    fn parse_user_install_dir_windows_and_posix() {
        let p = parse_user_install_dir(r"D:\bots\karin-a", Os::Windows).unwrap();
        assert!(p.is_absolute());
        assert_eq!(p.as_posix(), "/d/bots/karin-a");
        assert!(parse_user_install_dir("relative/path", Os::Windows).is_err());
        let nix = parse_user_install_dir("/home/u/ncd/apps/karin/x", Os::Linux).unwrap();
        assert_eq!(nix.as_posix(), "/home/u/ncd/apps/karin/x");
    }

    #[cfg(windows)]
    mod custom_install_dir {
        use super::*;
        use crate::events::BroadcastEventBus;
        use ncd_appframework::AppFrameworkRegistry;
        use ncd_host::local::LocalWindowsHost;

        async fn dir_manager(root: &std::path::Path) -> Arc<AppManager> {
            dir_manager_with_bots(root, Arc::new(MemoryBotsStub)).await
        }

        async fn dir_manager_with_bots(
            root: &std::path::Path,
            bots: Arc<dyn BotConfigPort>,
        ) -> Arc<AppManager> {
            let bus = Arc::new(BroadcastEventBus::default());
            let store = Arc::new(AppInstanceStore::empty(root));
            let local: Arc<dyn Host> = Arc::new(LocalWindowsHost::new());
            Arc::new(AppManager::new(
                Arc::new(AppFrameworkRegistry::with_builtin()),
                Arc::clone(&store),
                Arc::new(NativeAppRuntime::new(Arc::clone(&bus), Arc::clone(&store))),
                Arc::new(ncd_server::LocalOnlyHostResolver::new(local)),
                bots,
                bus,
                root,
            ))
        }

        struct MemoryBotsStub;

        #[async_trait::async_trait]
        impl BotConfigPort for MemoryBotsStub {
            async fn bot_config(&self, _: &BotId) -> Result<Option<BotConfig>, String> {
                Ok(None)
            }
            async fn upsert_bot_config(&self, _: BotConfig) -> Result<(), String> {
                Ok(())
            }
        }

        #[tokio::test]
        async fn custom_install_dir_rejects_relative_and_nonempty() {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("data");
            std::fs::create_dir_all(&root).unwrap();
            let manager = dir_manager(&root).await;

            let rel = manager
                .resolve_install_dir_for_test(
                    "local",
                    &AppFrameworkId::new("karin"),
                    &AppInstanceId::new("x"),
                    Some("apps/foo"),
                )
                .await;
            assert!(matches!(rel, Err(AppFrameworkError::Validation(m)) if m.contains("绝对")));

            let occupied = tmp.path().join("taken");
            std::fs::create_dir_all(&occupied).unwrap();
            std::fs::write(occupied.join("keep.txt"), b"x").unwrap();
            let err = manager
                .resolve_install_dir_for_test(
                    "local",
                    &AppFrameworkId::new("karin"),
                    &AppInstanceId::new("x"),
                    Some(occupied.to_str().unwrap()),
                )
                .await;
            assert!(matches!(err, Err(AppFrameworkError::Validation(m)) if m.contains("非空")));
        }

        #[tokio::test]
        async fn custom_install_dir_rejects_collision() {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("data");
            std::fs::create_dir_all(&root).unwrap();
            let manager = dir_manager(&root).await;
            let created = manager
                .create_instance(CreateAppInstanceRequest {
                    framework_id: AppFrameworkId::new("karin"),
                    host_id: "local".into(),
                    display_name: "a".into(),
                    port: Some(7777),
                    install_dir: None,
                    install_renderer: None,
                    webui_username: None,
                    webui_password: None,
                })
                .await
                .unwrap();
            let err = manager
                .resolve_install_dir_for_test(
                    "local",
                    &AppFrameworkId::new("karin"),
                    &AppInstanceId::new("other"),
                    Some(&created.install_dir),
                )
                .await;
            assert!(matches!(err, Err(AppFrameworkError::Validation(m)) if m.contains("占用")));
        }

        #[tokio::test]
        async fn import_nonebot_adopts_nonempty_dir() {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("data");
            std::fs::create_dir_all(&root).unwrap();
            let project = tmp.path().join("bot-xiuxian");
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(
                project.join("pyproject.toml"),
                r#"
[project]
name = "bot-xiuxian"
dependencies = ["nonebot2[httpx,websockets]>=2.5.0"]

[tool.nonebot]
plugin_dirs = ["src/plugins"]
"#,
            )
            .unwrap();
            std::fs::write(
                project.join("bot.py"),
                "import nonebot\nfrom nonebot.adapters.onebot.v11 import Adapter\nnonebot.init()\n",
            )
            .unwrap();
            std::fs::write(
                project.join(".env"),
                "DRIVER=~httpx+~websockets\nPORT=13120\nONEBOT_WS_URLS=[\"ws://127.0.0.1:3001\"]\n",
            )
            .unwrap();
            let manager = dir_manager(&root).await;
            let imported = manager
                .import_instance(ImportAppInstanceRequest {
                    framework_id: AppFrameworkId::new("nonebot2"),
                    host_id: "local".into(),
                    path: project.to_string_lossy().into_owned(),
                    display_name: String::new(),
                })
                .await
                .unwrap();
            assert_eq!(imported.origin, AppInstanceOrigin::Imported);
            assert_eq!(imported.port, 13120);
            assert_eq!(imported.display_name, "bot-xiuxian");
            assert_eq!(imported.state, AppInstanceState::NotInstalled);

            let create_err = manager
                .create_instance(CreateAppInstanceRequest {
                    framework_id: AppFrameworkId::new("nonebot2"),
                    host_id: "local".into(),
                    display_name: "x".into(),
                    port: Some(20001),
                    install_dir: Some(project.to_string_lossy().into_owned()),
                    install_renderer: None,
                    webui_username: None,
                    webui_password: None,
                })
                .await;
            assert!(
                matches!(create_err, Err(AppFrameworkError::Validation(ref m)) if m.contains("占用") || m.contains("非空")),
                "{create_err:?}"
            );

            std::fs::write(project.join(".env"), "PORT=1\nONEBOT_ACCESS_TOKEN=stolen\n").unwrap();
            std::fs::write(project.join(".ncd-uv"), "marker\n").unwrap();
            manager
                .delete_instance(&imported.id, false)
                .await
                .unwrap();
            assert_eq!(
                std::fs::read_to_string(project.join(".env")).unwrap(),
                "DRIVER=~httpx+~websockets\nPORT=13120\nONEBOT_WS_URLS=[\"ws://127.0.0.1:3001\"]\n"
            );
            assert_eq!(
                std::fs::read_to_string(project.join("bot.py")).unwrap(),
                "import nonebot\nfrom nonebot.adapters.onebot.v11 import Adapter\nnonebot.init()\n"
            );
            assert!(!project.join(".ncd-uv").exists());
            assert!(!project.join(".env.ncd.bak").exists());
            assert!(manager.get_instance(&imported.id).await.is_err());
        }

        struct ForwardWsBots;

        #[async_trait::async_trait]
        impl BotConfigPort for ForwardWsBots {
            async fn bot_config(&self, bot_id: &BotId) -> Result<Option<BotConfig>, String> {
                Ok(self
                    .list_bot_configs_for_link()
                    .await?
                    .into_iter()
                    .find(|b| b.bot.qq_id.to_string() == bot_id.as_str()))
            }
            async fn upsert_bot_config(&self, _: BotConfig) -> Result<(), String> {
                Ok(())
            }
            async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
                let mut b = super::bot();
                b.connect.websocket_servers.push(WebsocketServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "ws".into(),
                        message_post_format: MessagePostFormat::Array,
                        token: String::new(),
                        debug: false,
                    },
                    host: "0.0.0.0".into(),
                    port: 3001,
                    report_self_message: false,
                    enable_force_push_event: false,
                    heart_interval: 30000,
                    path: "/".into(),
                    role: WsRole::Universal,
                });
                Ok(vec![b])
            }
        }

        #[tokio::test]
        async fn import_nonebot_adopts_existing_forward_ws() {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("data");
            std::fs::create_dir_all(&root).unwrap();
            let project = tmp.path().join("bot-xiuxian");
            std::fs::create_dir_all(&project).unwrap();
            std::fs::write(
                project.join("pyproject.toml"),
                r#"
[project]
name = "bot-xiuxian"
dependencies = ["nonebot2[httpx,websockets]>=2.5.0"]

[tool.nonebot]
plugin_dirs = ["src/plugins"]
"#,
            )
            .unwrap();
            std::fs::write(
                project.join("bot.py"),
                "import nonebot\nfrom nonebot.adapters.onebot.v11 import Adapter\nnonebot.init()\n",
            )
            .unwrap();
            std::fs::write(
                project.join(".env"),
                "DRIVER=~httpx+~websockets\nPORT=13120\nONEBOT_WS_URLS=[\"ws://127.0.0.1:3001\"]\n",
            )
            .unwrap();
            let manager = dir_manager_with_bots(&root, Arc::new(ForwardWsBots)).await;
            let probe = manager
                .probe_project("local", &AppFrameworkId::new("nonebot2"), project.to_str().unwrap())
                .await
                .unwrap();
            assert_eq!(
                probe.detected_bot_id.as_ref().map(|id| id.as_str()),
                Some("10001")
            );
            let imported = manager
                .import_instance(ImportAppInstanceRequest {
                    framework_id: AppFrameworkId::new("nonebot2"),
                    host_id: "local".into(),
                    path: project.to_string_lossy().into_owned(),
                    display_name: String::new(),
                })
                .await
                .unwrap();
            let link = imported.link.expect("should adopt existing bot");
            assert_eq!(link.bot_id.as_str(), "10001");
            assert_eq!(link.connection_name, ncd_domain::APP_LINK_ADOPTED_FORWARD);
        }
    }
}
