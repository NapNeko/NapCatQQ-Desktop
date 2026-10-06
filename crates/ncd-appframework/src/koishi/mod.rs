//! Koishi 适配器：官方整包 + 插件树配置 + 反向 WS 对接 + 插件市场。
//!
//! koishi.yml 的所有改动（对接条目、商店装卸启停、配置页保存）都走同一个口子 `mutate`：
//! 停着直接改文件；跑着算出差量交给控制台（见 live.rs），不和 Koishi 内存里那份打架。

mod bundle;
pub mod component;
pub mod console;
pub mod integration;
pub mod live;
pub mod manifest;
pub mod probe;
pub mod release;
pub mod runtime;
pub mod store;
pub mod yarn;
pub mod yml;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigFormat, AppConfigText, AppFrameworkManifest, AppInstance,
    AppInstanceState, AppStoreResource, OneBotLinkPlan, TerminalSnippet,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};
use serde_json::{Map, Value, json};

pub use component::KoishiComponent;
pub use integration::KoishiIntegration;
pub use manifest::{KOISHI_FRAMEWORK_ID, koishi_manifest};
pub use probe::{KoishiPackageInfo, KoishiPluginSchema};
pub use runtime::{
    KoishiBotState, KoishiBotStatus, KoishiCommandRow, KoishiDatabaseTable, KoishiFileContent,
    KoishiFileEntry, KoishiRuntimeApi, KoishiRuntimeGate, KoishiRuntimeStatus,
    KoishiSandboxMessage,
};
pub use yml::{KoishiInstanceConfig, KoishiPluginNode};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, PluginLogSink, apply_with_backup_ex, restore_from_backup,
};
use crate::adopt::write_project_sidecar;
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, read_document,
    write_document_text,
};
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};
use console::KoishiConsole;
use live::{LiveOp, plan_live_ops};
use manifest::{
    ADAPTER_GROUP_IDENT, KOISHI_STDOUT_LOG, KOISHI_YML, LINK_IDENT, ONEBOT_ADAPTER_NAME,
    PACKAGE_JSON,
};
use yml::GROUP_NAME;

pub const DOC_KOISHI_YML: &str = "koishi";
pub const DOC_ENV: &str = "env";
pub const DOC_PACKAGE_JSON: &str = "package";

pub fn koishi_config_documents() -> Vec<AppConfigDocument> {
    vec![
        AppConfigDocument {
            id: DOC_KOISHI_YML.into(),
            label: KOISHI_YML.into(),
            rel_path: KOISHI_YML.into(),
            format: AppConfigFormat::Yaml,
            // 跑着的时候 Koishi 不读文件，还会整份写回；类型化保存走控制台，原文只能停着改
            hot_reload: false,
        },
        AppConfigDocument {
            id: DOC_ENV.into(),
            label: ".env".into(),
            rel_path: ".env".into(),
            format: AppConfigFormat::DotEnv,
            hot_reload: false,
        },
        AppConfigDocument {
            id: DOC_PACKAGE_JSON.into(),
            label: PACKAGE_JSON.into(),
            rel_path: PACKAGE_JSON.into(),
            format: AppConfigFormat::Json,
            hot_reload: false,
        },
    ]
}

fn yml_doc() -> AppConfigDocument {
    koishi_config_documents().remove(0)
}

fn host_err(e: impl std::fmt::Display) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

fn yml_path(instance: &AppInstance) -> HostPath {
    HostPath::from_posix(&instance.install_dir).join(KOISHI_YML)
}

fn envelope(config: KoishiInstanceConfig, snap: &DocumentSnapshot) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::Koishi(config),
        revision: crate::config_doc::combined_revision_of(std::slice::from_ref(snap)),
        documents: vec![snap.revision_entry()],
    }
}

fn issues_error(issues: Vec<ncd_domain::AppConfigIssue>) -> AppFrameworkError {
    AppFrameworkError::ConfigInvalid(issues)
}

pub struct KoishiAdapter {
    integration: KoishiIntegration,
    console: KoishiConsole,
    /// 跑着的实例在桌面端这边的控制台回环口（远端经 SSH -L），由编排层告知
    live_ports: std::sync::Mutex<HashMap<String, u16>>,
}

impl Default for KoishiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl KoishiAdapter {
    pub fn new() -> Self {
        Self {
            integration: KoishiIntegration::new(),
            console: KoishiConsole::new(),
            live_ports: std::sync::Mutex::new(HashMap::new()),
        }
    }

    async fn read_yml(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(KoishiInstanceConfig, DocumentSnapshot), AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let snap = read_document(host, &root, &yml_doc()).await?;
        let Some(text) = snap.text.as_deref() else {
            return Err(AppFrameworkError::Integration(format!(
                "Koishi 实例缺少 koishi.yml（{}），请先完成安装",
                yml_path(instance).as_posix()
            )));
        };
        let cfg = KoishiInstanceConfig::parse(text).map_err(AppFrameworkError::Validation)?;
        Ok((cfg, snap))
    }

    async fn write_yml(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        cfg: &KoishiInstanceConfig,
    ) -> Result<(), AppFrameworkError> {
        let text = cfg.render().map_err(AppFrameworkError::Validation)?;
        let path = yml_path(instance);
        apply_with_backup_ex(
            host,
            std::slice::from_ref(&path),
            write_project_sidecar(instance),
            || async {
                host.write_file(&path, text.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))
            },
        )
        .await
    }

    /// 跑着就给出控制台口；本机没登记过就用实例口（控制台和 server 同一个口）
    fn live_port(&self, instance: &AppInstance) -> Result<Option<u16>, AppFrameworkError> {
        if instance.state != AppInstanceState::Running {
            return Ok(None);
        }
        if let Some(p) = self
            .live_ports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(instance.id.as_str())
        {
            return Ok(Some(*p));
        }
        if matches!(instance.placement, ncd_domain::AppPlacement::LocalNative) {
            return Ok(Some(instance.port));
        }
        Err(AppFrameworkError::Runtime(
            "远端 Koishi 的控制台隧道还没开，稍后重试".into(),
        ))
    }

    async fn run_ops(
        &self,
        instance: &AppInstance,
        port: u16,
        ops: &[LiveOp],
    ) -> Result<(), AppFrameworkError> {
        let id = instance.id.as_str();
        for op in ops {
            match op {
                LiveOp::AppReload { .. } => {
                    self.console
                        .request_until_restart(id, port, op.kind(), op.args())
                        .await?;
                }
                _ => {
                    self.console.request(id, port, op.kind(), op.args()).await?;
                }
            }
        }
        Ok(())
    }

    /// koishi.yml 的唯一写入口：停着改文件，跑着交给控制台
    async fn mutate<F>(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        edit: F,
    ) -> Result<KoishiInstanceConfig, AppFrameworkError>
    where
        F: FnOnce(&mut KoishiInstanceConfig) -> Result<(), AppFrameworkError>,
    {
        let (before, _) = self.read_yml(host, instance).await?;
        let mut after = before.clone();
        edit(&mut after)?;
        let issues = after.validate();
        if !issues.is_empty() {
            return Err(issues_error(issues));
        }
        if after == before {
            return Ok(after);
        }
        match self.live_port(instance)? {
            Some(port) => {
                self.run_ops(instance, port, &plan_live_ops(&before, &after))
                    .await?
            }
            None => self.write_yml(host, instance, &after).await?,
        }
        Ok(after)
    }

    async fn run_yarn(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        args: &[&str],
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        let dir = HostPath::from_posix(&instance.install_dir);
        let tc = yarn::resolve_node(host, &dir, None)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
        let yarn_rel = yarn::resolve_yarn_rel(host, &dir)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
        let cmd = yarn::yarn_command(&tc, &dir, &yarn_rel, host.os(), host.locality(), None, args)
            .timeout(std::time::Duration::from_secs(20 * 60));
        let sink = log.cloned();
        let out = host
            .run_streaming(
                cmd,
                Box::new(move |_src, line| {
                    let trimmed = line.trim_end();
                    if let Some(s) = &sink
                        && !trimmed.is_empty()
                    {
                        s(trimmed.to_string());
                    }
                }),
            )
            .await
            .map_err(host_err)?;
        if out.success() {
            return Ok(());
        }
        let tail = out
            .stdout
            .lines()
            .chain(out.stderr.lines())
            .rfind(|l| l.contains("YN0001") || l.contains("YN0035") || l.contains("rror"))
            .unwrap_or_default()
            .to_string();
        Err(AppFrameworkError::Runtime(format!(
            "yarn {} 失败（exit={:?}）：{tail}",
            args.first().copied().unwrap_or_default(),
            out.exit_code
        )))
    }

    /// 对接条目放哪：模板自带、开着的 `group:adapter`，不然根上
    fn link_parent(cfg: &KoishiInstanceConfig) -> String {
        let usable = cfg
            .effective()
            .iter()
            .any(|n| n.name == GROUP_NAME && n.ident == ADAPTER_GROUP_IDENT);
        if usable {
            ADAPTER_GROUP_IDENT.to_string()
        } else {
            String::new()
        }
    }

    /// 装 / 启用时插件树里还没有这个插件就补一条
    fn ensure_node(cfg: &mut KoishiInstanceConfig, name: &str, enabled: bool) {
        if cfg.walk().iter().any(|n| n.name == name) {
            return;
        }
        let ident = cfg.fresh_ident();
        cfg.plugins
            .push(KoishiPluginNode::plugin(name, &ident, enabled, Map::new()));
    }
}

#[async_trait]
impl AppFrameworkAdapter for KoishiAdapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(
            KoishiComponent::new(spec.install_dir.clone(), spec.port)
                .with_node_bin(spec.node_bin.clone())
                .with_npm_registry(spec.npm_registry.clone()),
        )
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        KoishiComponent::new(spec.install_dir.clone(), spec.port)
            .with_node_bin(spec.node_bin.clone())
            .with_npm_registry(spec.npm_registry.clone())
            .resolve_launch_command(host, args)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))
    }

    async fn terminal_profile(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
    ) -> crate::terminal::AppTerminalProfile {
        let mut path_prefix = vec![spec.install_dir.join("node_modules/.bin")];
        let node = yarn::resolve_node(host, &spec.install_dir, spec.node_bin.as_ref()).await;
        if let Some(dir) = node.as_ref().ok().and_then(|tc| tc.node_dir()) {
            path_prefix.push(dir);
        }
        let yarn_rel = yarn::resolve_yarn_rel(host, &spec.install_dir)
            .await
            .unwrap_or_else(|_| ".yarn/releases/yarn-4.12.0.cjs".into());
        let yarn = format!("node {yarn_rel}");
        let hint = if node.is_ok() {
            format!("node 已接好；yarn 是整包自带的那份，用 {yarn} 调")
        } else {
            "没找到实例用的 Node.js，yarn 可能用不了".to_string()
        };
        crate::terminal::AppTerminalProfile {
            path_prefix,
            env: Vec::new(),
            hint: Some(hint),
            snippets: vec![
                TerminalSnippet::new("装插件", format!("{yarn} add koishi-plugin-")),
                TerminalSnippet::new("看已装的插件", format!("{yarn} info --name-only")),
                TerminalSnippet::new("同步依赖", format!("{yarn} install")),
                TerminalSnippet::new("Node 版本", "node -v"),
            ],
        }
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        let (cfg, _) = self.read_yml(host, instance).await?;
        Ok(cfg
            .find(ONEBOT_ADAPTER_NAME, LINK_IDENT)
            .and_then(|n| n.config.get("token"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string))
    }

    async fn read_outbound_ws_urls(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let (cfg, _) = self.read_yml(host, instance).await?;
        Ok(cfg
            .effective()
            .into_iter()
            .filter(|n| n.name == ONEBOT_ADAPTER_NAME)
            .filter(|n| n.config.get("protocol").and_then(Value::as_str) == Some("ws"))
            .filter_map(|n| n.config.get("endpoint").and_then(Value::as_str))
            .map(str::to_string)
            .collect())
    }

    fn wants_live_port(&self) -> bool {
        true
    }

    fn note_live_port(&self, instance_id: &str, port: u16) {
        self.live_ports
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(instance_id.to_string(), port);
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let entry = integration::link_entry_config(plan.bot_id.as_str(), &plan.access_token);
        self.mutate(host, instance, |cfg| {
            if let Some(node) = cfg.find_mut(ONEBOT_ADAPTER_NAME, LINK_IDENT) {
                node.enabled = true;
                for (k, v) in entry {
                    node.config.insert(k, v);
                }
                return Ok(());
            }
            let parent = Self::link_parent(cfg);
            cfg.group_children_mut(&parent)
                .push(KoishiPluginNode::plugin(
                    ONEBOT_ADAPTER_NAME,
                    LINK_IDENT,
                    true,
                    entry,
                ));
            Ok(())
        })
        .await
        .map(|_| ())
    }

    async fn unlink(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        self.mutate(host, instance, |cfg| {
            if let Some(node) = cfg.find_mut(ONEBOT_ADAPTER_NAME, LINK_IDENT) {
                node.enabled = false;
            }
            Ok(())
        })
        .await
        .map(|_| ())
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        if self.live_port(instance)?.is_some() {
            return self.unlink(host, instance).await;
        }
        restore_from_backup(host, &[yml_path(instance)]).await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(HostPath::from_posix(&instance.install_dir).join(KOISHI_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        koishi_config_documents()
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let (cfg, snap) = self.read_yml(host, instance).await?;
        Ok(envelope(cfg, &snap))
    }

    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::Koishi(cfg) = config else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 Koishi 配置".to_string(),
            ));
        };
        let issues = cfg.validate();
        if !issues.is_empty() {
            return Err(issues_error(issues));
        }
        self.write_yml(host, instance, cfg).await?;
        self.read_config(host, instance).await
    }

    fn supports_live_config(&self) -> bool {
        true
    }

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
        let AppInstanceConfig::Koishi(after) = config else {
            return Err(AppFrameworkError::Validation(
                "写入的不是 Koishi 配置".to_string(),
            ));
        };
        let issues = after.validate();
        if !issues.is_empty() {
            return Err(issues_error(issues));
        }
        self.note_live_port(instance.id.as_str(), loopback_port);
        let (before, _) = self.read_yml(host, instance).await?;
        self.run_ops(instance, loopback_port, &plan_live_ops(&before, after))
            .await?;
        self.read_config(host, instance).await
    }

    async fn write_config_text(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        doc_id: &str,
        text: &str,
        base_revision: Option<&str>,
    ) -> Result<AppConfigText, AppFrameworkError> {
        if doc_id == DOC_KOISHI_YML && instance.state == AppInstanceState::Running {
            return Err(AppFrameworkError::Validation(
                "Koishi 跑着的时候自己管着 koishi.yml，直接改文件会被它覆盖。先停止实例再改原文，或者用配置页保存".into(),
            ));
        }
        if doc_id == DOC_KOISHI_YML {
            KoishiInstanceConfig::parse(text).map_err(AppFrameworkError::Validation)?;
        }
        let doc = self.resolve_document(host, instance, doc_id).await?;
        write_document_text(
            host,
            &HostPath::from_posix(&instance.install_dir),
            &doc,
            text,
            base_revision,
            write_project_sidecar(instance),
        )
        .await
    }

    fn koishi_runtime(&self) -> Option<&dyn KoishiRuntimeApi> {
        Some(self)
    }

    async fn list_installed(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        resource: AppStoreResource,
    ) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
        if resource != AppStoreResource::Plugin {
            return Ok(Vec::new());
        }
        let packages = probe::installed_packages(host, instance).await?;
        let (cfg, _) = self.read_yml(host, instance).await?;
        Ok(store::installed_rows(&packages, &cfg))
    }

    async fn install_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        let package = if entry.package.is_empty() {
            entry.id.clone()
        } else {
            entry.package.clone()
        };
        let spec = store::yarn_spec(&package, &entry.version);
        self.run_yarn(host, instance, &["add", &spec], log).await?;
        let short = probe::short_name(&package);
        // 新装的先停着：多数插件要先填配置，开着直接载入会报一堆缺字段
        self.mutate(host, instance, |cfg| {
            Self::ensure_node(cfg, &short, false);
            Ok(())
        })
        .await
        .map(|_| ())
    }

    async fn update_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        let package = if entry.package.is_empty() {
            entry.id.clone()
        } else {
            entry.package.clone()
        };
        let spec = store::yarn_spec(&package, &entry.version);
        self.run_yarn(host, instance, &["add", &spec], log).await?;
        // 已经载入的模块不会自己换，和上游 market 一样让 worker 重拉
        if let Some(port) = self.live_port(instance)? {
            self.restart(host, instance, port).await?;
        }
        Ok(())
    }

    async fn uninstall_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        _flavor: AppStoreFlavor,
        _resource: AppStoreResource,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        let packages = probe::installed_packages(host, instance)
            .await
            .unwrap_or_default();
        let (package, short) = store::resolve_ids(id, &packages);
        if store::is_locked(&package) {
            return Err(AppFrameworkError::Validation(format!(
                "{package} 是 Koishi 和桌面端要用的，不能卸"
            )));
        }
        self.mutate(host, instance, |cfg| {
            while let Some((name, ident)) = cfg
                .walk()
                .iter()
                .find(|n| n.name == short)
                .map(|n| (n.name.clone(), n.ident.clone()))
            {
                cfg.remove(&name, &ident);
            }
            Ok(())
        })
        .await?;
        self.run_yarn(host, instance, &["remove", &package], log)
            .await
    }

    async fn set_store_enabled(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        _resource: AppStoreResource,
        enabled: bool,
        _overwrite: bool,
    ) -> Result<(), AppFrameworkError> {
        let packages = probe::installed_packages(host, instance)
            .await
            .unwrap_or_default();
        let (package, short) = store::resolve_ids(id, &packages);
        if !enabled && store::is_locked(&package) {
            return Err(AppFrameworkError::Validation(format!(
                "{package} 是 Koishi 和桌面端要用的，不能停"
            )));
        }
        self.mutate(host, instance, |cfg| {
            Self::ensure_node(cfg, &short, enabled);
            let keys: Vec<(String, String)> = cfg
                .walk()
                .iter()
                .filter(|n| n.name == short)
                .map(|n| (n.name.clone(), n.ident.clone()))
                .collect();
            for (name, ident) in keys {
                if let Some(n) = cfg.find_mut(&name, &ident) {
                    n.enabled = enabled;
                }
            }
            Ok(())
        })
        .await
        .map(|_| ())
    }

    async fn list_plugin_config_docs(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
        _plugin_name: &str,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        // 插件配置在插件页按 schema 改，都在 koishi.yml 里，没有单独的文件
        Ok(Vec::new())
    }

    fn store_market_urls(&self, resource: AppStoreResource) -> Vec<String> {
        match resource {
            AppStoreResource::Plugin => store::KOISHI_MARKET_URLS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            AppStoreResource::Adapter => Vec::new(),
        }
    }

    fn store_market_cache_key(&self, resource: AppStoreResource) -> Option<&'static str> {
        match resource {
            AppStoreResource::Plugin => Some("koishi-plugins"),
            AppStoreResource::Adapter => None,
        }
    }

    fn parse_store_market(
        &self,
        resource: AppStoreResource,
        text: &str,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        match resource {
            AppStoreResource::Plugin => {
                store::parse_koishi_market(text).map_err(AppFrameworkError::Validation)
            }
            AppStoreResource::Adapter => Ok(Vec::new()),
        }
    }
}

#[async_trait]
impl KoishiRuntimeApi for KoishiAdapter {
    async fn status(&self, instance: &AppInstance, port: u16) -> KoishiRuntimeStatus {
        match self
            .console
            .snapshot(instance.id.as_str(), port, "status", runtime::STATUS_WAIT)
            .await
        {
            Ok(Some(v)) if !v.is_null() => runtime::parse_status(&v),
            Ok(Some(_)) => KoishiRuntimeStatus::gate(
                KoishiRuntimeGate::Auth,
                "控制台开了登录（auth 插件），桌面端看不到运行状态",
            ),
            Ok(None) => KoishiRuntimeStatus::gate(
                KoishiRuntimeGate::Unreachable,
                "控制台没推运行状态：status 插件可能被停用了",
            ),
            Err(AppFrameworkError::DashboardAuth(m)) => {
                KoishiRuntimeStatus::gate(KoishiRuntimeGate::Auth, m)
            }
            Err(e) => KoishiRuntimeStatus::gate(KoishiRuntimeGate::Unreachable, e.to_string()),
        }
    }

    async fn plugin_schemas(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        names: &[String],
    ) -> Result<Vec<KoishiPluginSchema>, AppFrameworkError> {
        probe::plugin_schemas(host, instance, names).await
    }

    async fn installed_packages(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<KoishiPackageInfo>, AppFrameworkError> {
        probe::installed_packages(host, instance).await
    }

    async fn restart(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        port: u16,
    ) -> Result<(), AppFrameworkError> {
        let (cfg, _) = self.read_yml(host, instance).await?;
        let mut global = cfg.global.clone();
        global.remove("plugins");
        self.console
            .request_until_restart(
                instance.id.as_str(),
                port,
                "manager/app-reload",
                vec![Value::Object(global)],
            )
            .await
            .map(|_| ())
    }

    async fn sandbox_send(
        &self,
        instance: &AppInstance,
        port: u16,
        platform: &str,
        user: &str,
        channel: &str,
        content: &str,
    ) -> Result<(), AppFrameworkError> {
        self.console
            .sandbox_send(instance.id.as_str(), port, platform, user, channel, content)
            .await
    }

    async fn sandbox_messages(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<Vec<KoishiSandboxMessage>, AppFrameworkError> {
        let raw = self
            .console
            .sandbox_messages(instance.id.as_str(), port)
            .await?;
        Ok(raw
            .iter()
            .filter_map(KoishiSandboxMessage::from_value)
            .collect())
    }

    async fn explorer_tree(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<Vec<KoishiFileEntry>, AppFrameworkError> {
        let v = self
            .snapshot_required(
                instance,
                port,
                "explorer",
                "explorer 插件（文件管理）被停用了",
            )
            .await?;
        serde_json::from_value(v)
            .map_err(|e| AppFrameworkError::Runtime(format!("文件树解析失败：{e}")))
    }

    async fn explorer_read(
        &self,
        instance: &AppInstance,
        port: u16,
        path: &str,
    ) -> Result<KoishiFileContent, AppFrameworkError> {
        let v = self
            .console
            .request(
                instance.id.as_str(),
                port,
                "explorer/read",
                vec![json!(path)],
            )
            .await?;
        Ok(KoishiFileContent {
            base64: v
                .get("base64")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            mime: v.get("mime").and_then(Value::as_str).map(str::to_string),
            encoding: v
                .get("encoding")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    async fn explorer_write(
        &self,
        instance: &AppInstance,
        port: u16,
        path: &str,
        content: &str,
        binary: bool,
    ) -> Result<(), AppFrameworkError> {
        self.console
            .request(
                instance.id.as_str(),
                port,
                "explorer/write",
                vec![json!(path), json!(content), json!(binary)],
            )
            .await?;
        Ok(())
    }

    async fn explorer_mkdir(
        &self,
        instance: &AppInstance,
        port: u16,
        path: &str,
    ) -> Result<(), AppFrameworkError> {
        self.console
            .request(
                instance.id.as_str(),
                port,
                "explorer/mkdir",
                vec![json!(path)],
            )
            .await?;
        Ok(())
    }

    async fn explorer_remove(
        &self,
        instance: &AppInstance,
        port: u16,
        path: &str,
    ) -> Result<(), AppFrameworkError> {
        self.console
            .request(
                instance.id.as_str(),
                port,
                "explorer/remove",
                vec![json!(path)],
            )
            .await?;
        Ok(())
    }

    async fn explorer_rename(
        &self,
        instance: &AppInstance,
        port: u16,
        from: &str,
        to: &str,
    ) -> Result<(), AppFrameworkError> {
        self.console
            .request(
                instance.id.as_str(),
                port,
                "explorer/rename",
                vec![json!(from), json!(to)],
            )
            .await?;
        Ok(())
    }

    async fn database_tables(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<Vec<KoishiDatabaseTable>, AppFrameworkError> {
        let v = self
            .snapshot_required(
                instance,
                port,
                "database",
                "dataview 插件（数据库页）被停用了",
            )
            .await?;
        let mut out = Vec::new();
        if let Some(tables) = v.get("tables").and_then(Value::as_object) {
            for (name, t) in tables {
                out.push(KoishiDatabaseTable {
                    name: name.clone(),
                    primary: t
                        .get("primary")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    fields: t.get("fields").cloned().unwrap_or(Value::Null),
                    count: t.get("count").and_then(Value::as_u64),
                });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    async fn database_rows(
        &self,
        instance: &AppInstance,
        port: u16,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<Value>, AppFrameworkError> {
        // dataview 的监听器参数全是它自己序列化后的串（字符串前缀 s，日期前缀 d）
        let v = self
            .console
            .request(
                instance.id.as_str(),
                port,
                "database/get",
                vec![
                    json!(runtime::dataview_serialize(&json!(table))),
                    json!(runtime::dataview_serialize(&json!({}))),
                    json!(runtime::dataview_serialize(
                        &json!({ "limit": limit, "offset": offset })
                    )),
                ],
            )
            .await?;
        let text = v.as_str().ok_or_else(|| {
            AppFrameworkError::Runtime("dataview 返回的不是串：database/get".into())
        })?;
        let parsed = runtime::dataview_deserialize(text)?;
        parsed.as_array().cloned().ok_or_else(|| {
            AppFrameworkError::Runtime("dataview 返回的不是数组：database/get".into())
        })
    }

    async fn commands(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<Vec<KoishiCommandRow>, AppFrameworkError> {
        let entry = self
            .snapshot_required(instance, port, "entry", "控制台入口数据没推过来")
            .await?;
        // 指令管理器的那份入口数据：值是 Dict<CommandData>（每条带 paths / initial）
        let mut rows = Vec::new();
        if let Value::Object(entries) = &entry {
            for one in entries.values() {
                let Some(data) = one.get("data").and_then(Value::as_object) else {
                    continue;
                };
                let looks_like_commands = data
                    .values()
                    .next()
                    .map(|c| c.get("paths").is_some() && c.get("initial").is_some())
                    .unwrap_or(false);
                if !looks_like_commands {
                    continue;
                }
                for (name, c) in data {
                    rows.push(command_row(name, c));
                }
            }
        }
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(rows)
    }

    async fn command_update(
        &self,
        instance: &AppInstance,
        port: u16,
        name: &str,
        config: Value,
    ) -> Result<(), AppFrameworkError> {
        self.console
            .request(
                instance.id.as_str(),
                port,
                "command/update",
                vec![json!(name), json!({ "config": config })],
            )
            .await?;
        Ok(())
    }

    async fn command_aliases(
        &self,
        instance: &AppInstance,
        port: u16,
        name: &str,
        aliases: Vec<String>,
    ) -> Result<(), AppFrameworkError> {
        // 上游是 Dict<Alias>，新别名给空对象
        let map: serde_json::Map<String, Value> =
            aliases.into_iter().map(|a| (a, json!({}))).collect();
        self.console
            .request(
                instance.id.as_str(),
                port,
                "command/aliases",
                vec![json!(name), Value::Object(map)],
            )
            .await?;
        Ok(())
    }
}

impl KoishiAdapter {
    /// 取一份推送，拿不到就说哪个插件可能没开
    async fn snapshot_required(
        &self,
        instance: &AppInstance,
        port: u16,
        key: &str,
        hint: &str,
    ) -> Result<Value, AppFrameworkError> {
        match self
            .console
            .snapshot(instance.id.as_str(), port, key, runtime::STATUS_WAIT)
            .await?
        {
            Some(v) if !v.is_null() => Ok(v),
            _ => Err(AppFrameworkError::Runtime(format!(
                "控制台没推 {key}：{hint}"
            ))),
        }
    }
}

/// 控制台 entry 里的一条 CommandData → 前端要的一行；initial 上叠 override 成生效值
fn command_row(name: &str, c: &Value) -> KoishiCommandRow {
    let pick = |k: &str| c.get(k).cloned().unwrap_or(Value::Null);
    let initial = pick("initial");
    let over = pick("override");
    let mut config = initial.get("config").cloned().unwrap_or(json!({}));
    if let (Some(base), Some(over)) = (
        config.as_object_mut(),
        over.get("config").and_then(Value::as_object),
    ) {
        for (k, v) in over {
            base.insert(k.clone(), v.clone());
        }
    }
    let aliases = over
        .get("aliases")
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    KoishiCommandRow {
        name: name.to_string(),
        children: c
            .get("children")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        created: c.get("create").and_then(Value::as_bool).unwrap_or(false),
        paths: c
            .get("paths")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        aliases,
        config,
    }
}

#[cfg(test)]
mod smoke;
#[cfg(test)]
mod tests;
