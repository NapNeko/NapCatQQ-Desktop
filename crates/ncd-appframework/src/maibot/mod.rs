//! MaiBot 适配器：manifest + Component + 正向对接 + 两份主配置全字段 + 上游条款核对。

pub mod api;
mod component;
pub mod config;
mod integration;
pub mod manifest;
pub mod release;
pub mod resources;
pub mod runtime;
pub mod schema;
pub mod store;
pub mod terms;
pub mod webui_client;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppFrameworkManifest, AppInstance, AppPendingTerms, AppProjectProbe,
    AppStoreResource, OneBotLinkPlan, TerminalSnippet,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::{
    MaiBotComponent, generate_webui_token, legacy_port_candidates, read_config_version,
    read_version_constant, render_webui_json,
};
pub use config::{
    MaiBotAdapterConfig, MaiBotChatFilter, MaiBotInstanceConfig, MaiBotListMode,
    maibot_config_documents,
};
pub use integration::MaiBotIntegration;
pub use manifest::{MAIBOT_FRAMEWORK_ID, maibot_manifest};
use webui_client::MaiBotWebUi;

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, PluginLogSink, apply_with_backup_ex, restore_from_backup,
};
use crate::adopt::write_project_sidecar;
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, combined_revision_of,
    read_documents,
};
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};
use manifest::{
    ADAPTER_CONFIG, ADAPTER_DIR, BOT_CONFIG, CONFIG_PY, MAIBOT_STDOUT_LOG, MODEL_CONFIG, WEBUI_JSON,
};

fn host_err(e: ncd_host::HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await.map_err(host_err)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn envelope(config: MaiBotInstanceConfig, snaps: &[DocumentSnapshot]) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::MaiBot(config),
        revision: combined_revision_of(snaps),
        documents: snaps.iter().map(DocumentSnapshot::revision_entry).collect(),
    }
}

fn invalid(doc: &str, e: String) -> AppFrameworkError {
    AppFrameworkError::ConfigInvalid(vec![ncd_domain::AppConfigIssue::new(doc, e)])
}

fn maibot_config_of(
    config: &AppInstanceConfig,
) -> Result<&MaiBotInstanceConfig, AppFrameworkError> {
    match config {
        AppInstanceConfig::MaiBot(cfg) => Ok(cfg),
        _ => Err(AppFrameworkError::Validation(
            "写入的不是 MaiBot 配置".to_string(),
        )),
    }
}

/// bot_config.toml 不在时新文件要带的 `[inner].version`；文件在就用不上
fn seed_version(config_py: &str, bot_text: Option<&str>) -> Result<String, AppFrameworkError> {
    if bot_text.is_some() {
        return Ok(String::new());
    }
    read_config_version(config_py).ok_or_else(|| {
        AppFrameworkError::Integration(format!("{CONFIG_PY} 里找不到 CONFIG_VERSION"))
    })
}

fn snapshot_text<'a>(snaps: &'a [DocumentSnapshot], id: &str) -> Option<&'a str> {
    snaps
        .iter()
        .find(|s| s.doc.id == id)
        .and_then(|s| s.text.as_deref())
}

pub struct MaiBotAdapter {
    integration: MaiBotIntegration,
}

impl Default for MaiBotAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MaiBotAdapter {
    pub fn new() -> Self {
        Self {
            integration: MaiBotIntegration::new(),
        }
    }

    fn component_for(spec: &AppComponentSpec) -> MaiBotComponent {
        MaiBotComponent::new(spec.install_dir.clone(), spec.port).with_uv_bin(spec.uv_bin.clone())
    }

    fn adapter_config_path(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir).join(ADAPTER_CONFIG)
    }

    /// 适配器目录不在（装坏了 / 被删了）就别往里写配置，写了也没人读
    async fn require_adapter_dir(
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        let dir = HostPath::from_posix(&instance.install_dir).join(ADAPTER_DIR);
        if host.exists(&dir).await.map_err(host_err)? {
            return Ok(());
        }
        Err(AppFrameworkError::Integration(
            "实例里没有 NapCat 适配器插件，重新安装实例可修复".to_string(),
        ))
    }

    /// 校验要写的配置，再读盘上现状当改前（编排层已经按版本号确认过没被别处改过）
    async fn before_write(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        cfg: &MaiBotInstanceConfig,
    ) -> Result<MaiBotInstanceConfig, AppFrameworkError> {
        let issues = config::validate(cfg);
        if !issues.is_empty() {
            return Err(AppFrameworkError::ConfigInvalid(issues));
        }
        let current = self.read_config(host, instance).await?;
        match current.config {
            AppInstanceConfig::MaiBot(before) => Ok(before),
            _ => Err(AppFrameworkError::Validation(
                "读回来的不是 MaiBot 配置".to_string(),
            )),
        }
    }

    /// 名单变了才出一份新文本；适配器目录不在就不写
    async fn adapter_chat_write(
        host: &dyn Host,
        root: &HostPath,
        before: &MaiBotInstanceConfig,
        after: &MaiBotInstanceConfig,
    ) -> Result<Option<(HostPath, String)>, AppFrameworkError> {
        let Some(adapter) = &after.adapter else {
            return Ok(None);
        };
        if before.adapter.as_ref().map(|a| &a.chat) == Some(&adapter.chat)
            || !host
                .exists(&root.join(ADAPTER_DIR))
                .await
                .map_err(host_err)?
        {
            return Ok(None);
        }
        let path = root.join(ADAPTER_CONFIG);
        let current = read_text(host, &path).await?;
        let next = config::write_adapter_chat(current.as_deref(), &adapter.chat)
            .map_err(|e| invalid(config::DOC_ADAPTER_CONFIG, e))?;
        Ok((current.as_deref() != Some(next.as_str())).then_some((path, next)))
    }

    /// 一次保存的几个文件一起备份，任一个写失败整批还原
    async fn write_files(
        host: &dyn Host,
        instance: &AppInstance,
        writes: Vec<(HostPath, String)>,
    ) -> Result<(), AppFrameworkError> {
        if writes.is_empty() {
            return Ok(());
        }
        let paths: Vec<HostPath> = writes.iter().map(|(p, _)| p.clone()).collect();
        apply_with_backup_ex(host, &paths, write_project_sidecar(instance), || async {
            for (path, text) in &writes {
                crate::config_doc::ensure_parent_dir(host, path).await?;
                host.write_file(path, text.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
            }
            Ok(())
        })
        .await
    }

    /// 模型能用了就把 WebUI 的首次配置向导标成完成（D0-4）。尽力而为：配置已经存好了，
    /// 这一步失败只是用户第一次开 WebUI 时多看一次向导，不该让保存报错
    async fn mark_setup_if_ready(
        host: &dyn Host,
        instance: &AppInstance,
        cfg: &MaiBotInstanceConfig,
    ) {
        if !config::models_ready(&cfg.models) {
            return;
        }
        let path = HostPath::from_posix(&instance.install_dir).join(WEBUI_JSON);
        let Ok(Some(current)) = read_text(host, &path).await else {
            return;
        };
        let now = chrono::Local::now()
            .format("%Y-%m-%dT%H:%M:%S%.6f")
            .to_string();
        if let Some(next) = config::mark_setup_completed(&current, &now)
            && let Err(e) = host.write_file(&path, next.as_bytes()).await
        {
            tracing::warn!(instance = %instance.id.as_str(), error = %e, "标记 MaiBot 首次配置完成失败");
        }
    }

    async fn write_adapter_config(
        host: &dyn Host,
        instance: &AppInstance,
        render: impl FnOnce(Option<&str>) -> Result<String, String> + Send,
    ) -> Result<(), AppFrameworkError> {
        Self::require_adapter_dir(host, instance).await?;
        let path = Self::adapter_config_path(instance);
        let current = read_text(host, &path).await?;
        let next = render(current.as_deref()).map_err(AppFrameworkError::Integration)?;
        apply_with_backup_ex(
            host,
            std::slice::from_ref(&path),
            write_project_sidecar(instance),
            || async {
                host.write_file(&path, next.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))
            },
        )
        .await
    }
}

#[async_trait]
impl AppFrameworkAdapter for MaiBotAdapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(Self::component_for(spec))
    }

    /// 首版不导入已有 MaiBot（0.x 升级确认、旧的独立适配器形态都还没处理），默认 probe 已经拒了
    fn suggested_import_port(&self, _probe: &AppProjectProbe) -> Option<u16> {
        None
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        // 不拦的话上游会卡在 input() 等「同意」，没有 stdin 直接退出，用户只看到一句异常退出
        let pending = terms::pending_terms(host, &spec.install_dir).await?;
        if !pending.is_empty() {
            let names: Vec<&str> = pending.iter().map(|t| t.title.as_str()).collect();
            return Err(AppFrameworkError::Validation(format!(
                "{}有更新或还没同意，到应用端页同意后再启动",
                names.join("、")
            )));
        }
        Self::component_for(spec)
            .resolve_launch_command(host, args)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))
    }

    async fn terminal_profile(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
    ) -> crate::terminal::AppTerminalProfile {
        // 麦麦的 pyproject 是上游的，uv add 会改动它的锁文件，更新时冲突；临时装包走 uv pip
        crate::terminal::uv_venv_profile(
            host,
            spec,
            vec![
                TerminalSnippet::new("装了哪些包", "uv pip list"),
                TerminalSnippet::new("装一个包", "uv pip install "),
                TerminalSnippet::new("Python 版本", "python -V"),
            ],
        )
        .await
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        let text = read_text(host, &Self::adapter_config_path(instance)).await?;
        Ok(config::read_adapter_token(text.as_deref()))
    }

    async fn read_outbound_ws_urls(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let text = read_text(host, &Self::adapter_config_path(instance)).await?;
        let a = config::read_adapter_config(text.as_deref());
        if !a.enabled {
            return Ok(Vec::new());
        }
        Ok(vec![format!("ws://{}:{}", a.napcat_host, a.napcat_port)])
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let Some(server) = plan.connection.as_ws_server() else {
            return Err(AppFrameworkError::Integration(
                "MaiBot 只能由适配器主动连协议 Bot（正向对接）".to_string(),
            ));
        };
        if server.port == 0 {
            return Err(AppFrameworkError::Integration(
                "协议 Bot 的 WS 服务还没分配端口".to_string(),
            ));
        }
        let listen_host = server.host.clone();
        let port = server.port;
        let token = plan.access_token.clone();
        Self::write_adapter_config(host, instance, move |text| {
            config::write_adapter_link(text, &listen_host, port, &token)
        })
        .await
    }

    async fn unlink(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        Self::write_adapter_config(host, instance, config::write_adapter_disabled).await
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        restore_from_backup(host, &[Self::adapter_config_path(instance)]).await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(MAIBOT_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        maibot_config_documents()
    }

    /// 插件的 `config.toml` 按目录名现拼，不进固定文档列表
    fn find_document(
        &self,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigDocument, AppFrameworkError> {
        if let Some(dir) = store::parse_plugin_doc_id(doc_id) {
            return Ok(store::plugin_config_document(dir));
        }
        self.config_documents(instance)
            .into_iter()
            .find(|d| d.id == doc_id)
            .ok_or_else(|| AppFrameworkError::Validation(format!("未知的配置文档: {doc_id}")))
    }

    async fn list_installed(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        resource: AppStoreResource,
    ) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
        store::list_installed(host, instance, resource).await
    }

    async fn install_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        store::install_item(host, instance, entry, log).await
    }

    async fn update_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        store::update_item(host, instance, entry, log).await
    }

    async fn uninstall_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        _flavor: AppStoreFlavor,
        resource: AppStoreResource,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        if resource != AppStoreResource::Plugin {
            return Err(AppFrameworkError::PluginUnsupported(
                "MaiBot 只有插件商店".into(),
            ));
        }
        store::uninstall_item(host, instance, id, log).await
    }

    async fn set_store_enabled(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        resource: AppStoreResource,
        enabled: bool,
        _overwrite: bool,
    ) -> Result<(), AppFrameworkError> {
        if resource != AppStoreResource::Plugin {
            return Err(AppFrameworkError::PluginUnsupported(
                "MaiBot 只有插件商店".into(),
            ));
        }
        store::set_enabled(host, instance, id, enabled).await
    }

    async fn list_plugin_config_docs(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plugin_name: &str,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        store::list_plugin_config_docs(host, instance, plugin_name).await
    }

    fn store_market_urls(&self, resource: AppStoreResource) -> Vec<String> {
        match resource {
            AppStoreResource::Plugin => store::maibot_plugin_market_urls(),
            AppStoreResource::Adapter => Vec::new(),
        }
    }

    fn store_market_cache_key(&self, resource: AppStoreResource) -> Option<&'static str> {
        match resource {
            AppStoreResource::Plugin => Some("maibot-plugins"),
            AppStoreResource::Adapter => None,
        }
    }

    fn parse_store_market(
        &self,
        resource: AppStoreResource,
        text: &str,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        match resource {
            AppStoreResource::Plugin => store::parse_maibot_plugins_json(text),
            AppStoreResource::Adapter => Err(AppFrameworkError::PluginUnsupported(
                "MaiBot 只有插件商店".into(),
            )),
        }
    }

    /// 上游距上次热加载不足 1s 的变更直接跳过（`_hot_reload_min_interval_s`），加上 600ms 防抖留足余量
    fn config_write_min_interval(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_millis(1500))
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let snaps = read_documents(host, &root, &maibot_config_documents()).await?;
        let bot = schema::read_bot_config_file(snapshot_text(&snaps, config::DOC_BOT_CONFIG))
            .map_err(AppFrameworkError::Integration)?;
        let models =
            schema::read_model_config_file(snapshot_text(&snaps, config::DOC_MODEL_CONFIG))
                .map_err(AppFrameworkError::Integration)?;
        let token_json = read_text(host, &root.join(WEBUI_JSON)).await?;
        let adapter = if host
            .exists(&root.join(ADAPTER_DIR))
            .await
            .map_err(host_err)?
        {
            Some(config::read_adapter_config(snapshot_text(
                &snaps,
                config::DOC_ADAPTER_CONFIG,
            )))
        } else {
            None
        };
        Ok(envelope(
            MaiBotInstanceConfig {
                bot: Box::new(bot),
                models: Box::new(models),
                webui_token: config::read_webui_token(token_json.as_deref()),
                adapter,
            },
            &snaps,
        ))
    }

    /// 停止时：两份主配置和适配器名单各自在盘上差量写，只写变了的文件
    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let cfg = maibot_config_of(config)?;
        let before = self.before_write(host, instance, cfg).await?;
        let root = HostPath::from_posix(&instance.install_dir);
        let config_py = read_text(host, &root.join(CONFIG_PY))
            .await?
            .unwrap_or_default();
        // 没变的文件不写，免得版本号跳了、白白提示重启
        let mut writes: Vec<(HostPath, String)> = Vec::new();

        let bot_path = root.join(BOT_CONFIG);
        let bot_text = read_text(host, &bot_path).await?;
        let bot_version = seed_version(&config_py, bot_text.as_deref())?;
        if let Some(next) = config::files::patch_bot_config(
            bot_text.as_deref(),
            &bot_version,
            &before.bot,
            &cfg.bot,
        )
        .map_err(|e| invalid(config::DOC_BOT_CONFIG, e))?
        {
            writes.push((bot_path, next));
        }

        let model_path = root.join(MODEL_CONFIG);
        let model_text = read_text(host, &model_path).await?;
        let model_version = read_version_constant(&config_py, "MODEL_CONFIG_VERSION");
        if let Some(next) = config::files::patch_model_config(
            model_text.as_deref(),
            model_version.as_deref(),
            &before.models,
            &cfg.models,
        )
        .map_err(|e| invalid(config::DOC_MODEL_CONFIG, e))?
        {
            writes.push((model_path, next));
        }

        writes.extend(Self::adapter_chat_write(host, &root, &before, cfg).await?);
        Self::write_files(host, instance, writes).await?;
        Self::mark_setup_if_ready(host, instance, cfg).await;
        self.read_config(host, instance).await
    }

    fn supports_live_config(&self) -> bool {
        true
    }

    fn maibot_runtime(&self) -> Option<&dyn api::MaiBotRuntimeApi> {
        Some(&api::WebUiRuntime)
    }

    /// 运行中：两份主配置交给麦麦自己的 WebUI 写（它校验、合并进当前文件、写盘、热加载，
    /// 和它自己的写入不打架）；适配器名单是插件自己的文件，照旧在盘上改，插件监听着
    async fn write_live_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        loopback_port: u16,
        _username: &str,
        _password: &str,
        _desktop_secret: Option<&str>,
        config: &AppInstanceConfig,
        _conf_id: &str,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let cfg = maibot_config_of(config)?;
        let before = self.before_write(host, instance, cfg).await?;
        let root = HostPath::from_posix(&instance.install_dir);
        let token_json = read_text(host, &root.join(WEBUI_JSON)).await?;
        let webui = MaiBotWebUi::connect(
            loopback_port,
            &config::read_webui_token(token_json.as_deref()),
        )?;

        let before_bot = schema::bot_config_to_toml(&before.bot)
            .map_err(|e| invalid(config::DOC_BOT_CONFIG, e))?;
        let after_bot =
            schema::bot_config_to_toml(&cfg.bot).map_err(|e| invalid(config::DOC_BOT_CONFIG, e))?;
        if before_bot != after_bot {
            if crate::toml_patch::removed_keys(&before_bot, &after_bot).is_empty() {
                let partial =
                    serde_json::to_value(crate::toml_patch::changes(&before_bot, &after_bot))
                        .map_err(|e| invalid(config::DOC_BOT_CONFIG, e.to_string()))?;
                webui.merge_bot_config(&partial).await?;
            } else {
                // 部分合并删不掉键（清空的可选项、删掉的映射项），只能整份：在盘上原文上差量改好再交给它
                let bot_text = read_text(host, &root.join(BOT_CONFIG)).await?;
                let config_py = read_text(host, &root.join(CONFIG_PY))
                    .await?
                    .unwrap_or_default();
                let version = seed_version(&config_py, bot_text.as_deref())?;
                if let Some(next) = config::files::patch_bot_config(
                    bot_text.as_deref(),
                    &version,
                    &before.bot,
                    &cfg.bot,
                )
                .map_err(|e| invalid(config::DOC_BOT_CONFIG, e))?
                {
                    webui.write_bot_config_raw(&next).await?;
                }
            }
        }
        if before.models != cfg.models {
            let full = schema::model_config_to_toml(&cfg.models)
                .map_err(|e| invalid(config::DOC_MODEL_CONFIG, e))?;
            let body = serde_json::to_value(full)
                .map_err(|e| invalid(config::DOC_MODEL_CONFIG, e.to_string()))?;
            webui.write_model_config(&body).await?;
        }

        let writes = Self::adapter_chat_write(host, &root, &before, cfg).await?;
        Self::write_files(host, instance, writes.into_iter().collect()).await?;
        Self::mark_setup_if_ready(host, instance, cfg).await;
        self.read_config(host, instance).await
    }

    async fn pending_terms(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<AppPendingTerms>, AppFrameworkError> {
        terms::pending_terms(host, &HostPath::from_posix(&instance.install_dir)).await
    }

    async fn accept_terms(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        terms::write_confirmations(host, &HostPath::from_posix(&instance.install_dir), false).await
    }
}
