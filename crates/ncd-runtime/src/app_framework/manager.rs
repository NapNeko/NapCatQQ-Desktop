//! AppManager：应用实例表 + 生命周期 + 「协议 Bot 一键对接应用端」编排
//!
//! 对接 = 往 Bot 的 `connect.websocket_clients` 按名 upsert 一条反向 WS 连接，再调
//! `BotManager::upsert_bot_config`（持久化 + 渲染 + 热推）。不新写推送链路。
//! 首发只允许同机：Bot 的 `runtime_target` 与实例 `host_id` 必须落在同一台机器。
//!
//! 安装本身走既有 ComponentExecutor（R12），这里只给 hint、置 Installing、盯任务结束后
//! 用 detect 对账；框架差异全部封在 `ncd_appframework::AppFrameworkAdapter` 后面。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ncd_appframework::karin::config::link_inputs_changed;
use ncd_appframework::{
    AppComponentSpec, AppConfigWriteResult, AppFrameworkAdapter, AppFrameworkRegistry,
    AppInstanceConfig, AppInstanceConfigEnvelope,
};
use ncd_component::{DetectOutcome, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigText, AppFrameworkId, AppFrameworkManifest, AppInstance,
    AppInstanceId, AppInstanceState, AppLinkRecord, AppPlacement, BotConfig, BotId,
    CreateAppInstanceRequest, DomainEventKind, LOCAL_HOST_ID, OneBotLinkMode, OneBotLinkPlan,
    RuntimeTarget, app_link_connection_name, is_app_link_connection_name,
    runtime_target_matches_host, server_id_of_host,
};
use ncd_host::{Host, HostCommand, HostPath, Locality};
use ncd_server::HostResolver;
use ncd_traits::{AppFrameworkError, EventBus, EventFilter};
use rand::Rng;
use rand::distributions::Alphanumeric;

use super::instances::AppInstanceStore;
use super::native_runtime::{AppLaunchSpec, NativeAppRuntime};
use crate::bot_manager::BotManager;
use crate::components::{AppComponentHint, data_root_to_host_path};
use crate::events::{BroadcastEventBus, DomainEvent};
use crate::metrics::now_ms;

/// 本机实例目录：`data_root/apps/<framework>/<instance>`
const LOCAL_APPS_DIR: &str = "apps";
/// 远端实例目录：`$HOME/ncd/apps/<framework>/<instance>`
const REMOTE_APPS_REL: &str = "ncd/apps";

/// AppManager 对协议 Bot 侧唯一的依赖：读配置 + 走 `upsert_bot_config` 热推。
/// 抽成 trait 是为了不把 BotManager 的两个泛型参数带进来，也方便测试。
#[async_trait::async_trait]
pub trait BotConfigPort: Send + Sync {
    async fn bot_config(&self, bot_id: &BotId) -> Result<Option<BotConfig>, String>;
    async fn upsert_bot_config(&self, config: BotConfig) -> Result<(), String>;
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
}

pub struct AppManager {
    registry: Arc<AppFrameworkRegistry>,
    store: Arc<AppInstanceStore>,
    runtime: Arc<NativeAppRuntime>,
    host_resolver: Arc<dyn HostResolver>,
    bot_manager: Arc<dyn BotConfigPort>,
    event_bus: Arc<BroadcastEventBus>,
    data_root: PathBuf,
    npm_registry: Option<String>,
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
            npm_registry: None,
        }
    }

    pub fn with_npm_registry(mut self, registry: Option<String>) -> Self {
        self.npm_registry = registry.filter(|s| !s.trim().is_empty());
        self
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
        }
    }

    pub fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String> {
        let adapter = self.registry.get(&instance.framework_id).ok()?;
        adapter.integration().webui_url(instance, public_host)
    }

    /// 读盘拿到的 WebUI 登录密钥；配置读失败或没有密钥时返回空串。
    pub async fn webui_auth_key(&self, id: &AppInstanceId) -> String {
        match self.read_config(id).await {
            Ok(envelope) => envelope.config.webui_auth_key().to_string(),
            Err(_) => String::new(),
        }
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
        let port = req.port.unwrap_or(manifest.default_port);
        if port == 0 {
            return Err(AppFrameworkError::Validation("端口不能为 0".to_string()));
        }
        let taken = self
            .store
            .list()
            .await
            .into_iter()
            .any(|i| i.host_id == req.host_id && i.port == port);
        if taken {
            return Err(AppFrameworkError::Validation(format!(
                "该主机上已有应用实例占用端口 {port}"
            )));
        }

        let host = self.resolve_host(&req.host_id).await?;
        let id = AppInstanceId::new(short_id());
        let install_dir = self
            .install_dir_for(host.as_ref(), &req.framework_id, &id)
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
        };
        let saved = self.store.upsert(instance).await?;
        self.publish(&saved, "created");
        Ok(saved)
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
            while let Some(event) = sub.next().await {
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
        });
        Ok(updated)
    }

    async fn refresh_after_install(
        &self,
        id: &AppInstanceId,
        failure: Option<String>,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let host = self.resolve_host(&instance.host_id).await?;
        let detected = self.detect(host.as_ref(), &instance).await;
        let updated = self
            .store
            .update(id, |i| match &detected {
                Ok(DetectOutcome::Installed(v)) => {
                    i.state = AppInstanceState::Installed;
                    i.installed_version = Some(v.version.clone());
                    i.last_error = failure.clone();
                }
                Ok(DetectOutcome::Unusable(u)) => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some(failure.clone().unwrap_or_else(|| u.reason.clone()));
                }
                Ok(DetectOutcome::NotInstalled) => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = failure.clone();
                }
                Err(e) => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some(failure.clone().unwrap_or_else(|| e.to_string()));
                }
            })
            .await?;
        self.publish(
            &updated,
            if failure.is_some() {
                "install_failed"
            } else {
                "installed"
            },
        );
        Ok(updated)
    }

    /// 刷新单个实例：安装探测 + 进程对账（冷启动 / 页面手动刷新）
    pub async fn refresh_instance(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if instance.state == AppInstanceState::Installing {
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
        if running.is_some() && instance.state != AppInstanceState::Running {
            let adapter = self.registry.get(&instance.framework_id)?;
            let log = adapter
                .log_file(&instance)
                .unwrap_or_else(|| HostPath::from_posix(&instance.install_dir).join(".ncd-app.log"));
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
        Ok(updated)
    }

    /// 冷启动：逐实例对账；连不上的远端主机跳过（脱管语义）
    pub async fn reconcile_all(&self) {
        for instance in self.store.list().await {
            if let Err(e) = self.refresh_instance(&instance.id).await {
                tracing::info!(instance = instance.id.as_str(), error = %e, "app reconcile skipped");
            }
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

    /// 删除实例：停进程 → 解绑 Bot 侧连接 → （可选）删目录 → 删记录
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
        if remove_files {
            let host = host?;
            let dir = HostPath::from_posix(&instance.install_dir);
            if host.exists(&dir).await.map_err(host_err)? {
                host.remove_dir_all(&dir).await.map_err(host_err)?;
            }
        }
        if let Some(removed) = self.store.remove(id).await? {
            self.publish(&removed, "deleted");
        }
        Ok(())
    }

    // ---- 对接 ----

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
        let token = self.pick_access_token(host.as_ref(), &instance, &bot, adapter.as_ref()).await?;
        let plan = adapter.integration().plan_link(&instance, &bot, &token)?;

        adapter.apply_link(host.as_ref(), &instance, &plan).await?;

        upsert_ws_client(&mut bot, plan.connection.clone());
        if let Err(e) = self.bot_manager.upsert_bot_config(bot).await {
            if let Err(rb) = adapter.rollback_link(host.as_ref(), &instance).await {
                tracing::error!(instance = instance_id.as_str(), error = %rb, "rollback app-side link");
            }
            return Err(AppFrameworkError::Integration(format!(
                "写入协议 Bot 连接失败，应用端配置已还原：{e}"
            )));
        }

        // 同一实例换绑到别的 Bot：把旧 Bot 上的连接摘掉，避免两个 Bot 同时连
        if let Some(old) = instance.link.as_ref().filter(|l| &l.bot_id != bot_id) {
            if let Err(e) = self.remove_ws_client_from_bot(&old.bot_id, &old.connection_name).await {
                tracing::warn!(instance = instance_id.as_str(), error = %e, "detach previous bot");
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
        if !runtime_target_matches_host(&bot.bot.runtime_target, &instance.host_id) {
            return Err(AppFrameworkError::Validation(format!(
                "首发只支持同机对接：Bot 在 {}，应用实例在 {}",
                describe_target(&bot.bot.runtime_target),
                describe_host(&instance.host_id)
            )));
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
        let (instance, adapter, host) = self.config_context(id).await?;
        let before = adapter.read_config(host.as_ref(), &instance).await?;
        if let Some(base) = base_revision.as_deref()
            && base != before.revision
        {
            return Err(AppFrameworkError::ConfigConflict("config".to_string()));
        }

        let new_port = config_port(&config);
        if new_port != instance.port {
            self.ensure_port_free(&instance, new_port).await?;
        }

        let after = adapter
            .write_config(host.as_ref(), &instance, &config)
            .await?;
        let sync = self
            .sync_after_config_write(&instance, &before, &after)
            .await?;

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

    pub async fn list_config_documents(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        Ok(adapter.config_documents(&instance))
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
            let new_port = config_port(&after.config);
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
        let new_port = config_port(&after.config);
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
            && link_inputs_differ(&before.config, &after.config)
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

    async fn install_dir_for(
        &self,
        host: &dyn Host,
        framework: &AppFrameworkId,
        id: &AppInstanceId,
    ) -> Result<HostPath, AppFrameworkError> {
        match host.locality() {
            Locality::Local => Ok(data_root_to_host_path(&self.data_root, host.os())
                .join(LOCAL_APPS_DIR)
                .join(framework.as_str())
                .join(id.as_str())),
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
                    .join(framework.as_str())
                    .join(id.as_str()))
            }
        }
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

/// 应用端监听口在类型化配置里的位置（每接一个框架加一臂）
fn config_port(config: &AppInstanceConfig) -> u16 {
    match config {
        AppInstanceConfig::Karin(k) => k.env.http_port,
    }
}

/// 对接依赖的输入（端口 / 反向 WS 秘钥）是否变了
fn link_inputs_differ(before: &AppInstanceConfig, after: &AppInstanceConfig) -> bool {
    match (before, after) {
        (AppInstanceConfig::Karin(b), AppInstanceConfig::Karin(a)) => {
            link_inputs_changed(&b.env, &a.env)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::{
        AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, ConnectConfig,
        DeploymentType, MessagePostFormat, NetworkBaseFields, WebsocketClientConfig, WsRole,
    };

    fn bot() -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "b".into(),
                qq_id: 10001,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::NapCat,
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
        }

        struct Fixture {
            _tmp: tempfile::TempDir,
            inst_dir: std::path::PathBuf,
            manager: Arc<AppManager>,
            bots: Arc<MemoryBots>,
            id: AppInstanceId,
        }

        async fn fixture(linked: bool) -> Fixture {
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
                bots: AsyncMutex::new(vec![bot()]),
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
                    placement: AppPlacement::LocalNative,
                    host_id: LOCAL_HOST_ID.to_string(),
                    install_dir: install_dir.as_posix().to_string(),
                    port: 7777,
                    state: AppInstanceState::Stopped,
                    link: linked.then(|| AppLinkRecord {
                        bot_id: BotId::new("10001"),
                        mode: OneBotLinkMode::ReverseWs,
                        connection_name: app_link_connection_name(&id),
                        linked_at_ms: 1,
                    }),
                    installed_version: Some("1.0.0".into()),
                    last_error: None,
                    created_at_ms: 1,
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
            let AppInstanceConfig::Karin(k) = &envelope.config;
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
    }
}
