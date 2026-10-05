//! TRSS-Yunzai 适配器：manifest + Component + Integration + YAML 配置 + 插件索引商店。

pub mod component;
pub mod config;
pub mod git;
mod integration;
pub mod manifest;
mod probe;
pub mod store;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppFrameworkManifest, AppInstance, AppProjectProbe, AppStoreResource,
    OneBotLinkPlan, TerminalSnippet,
};
use ncd_host::{Host, HostCommand, HostPath, Locality, shell_single_quote};
use ncd_traits::{AppFrameworkError, AppIntegration};

pub use component::YunzaiComponent;
pub use config::{
    YunzaiBotConfig, YunzaiGroupConfig, YunzaiGroupDefaults, YunzaiGroupOverride,
    YunzaiInstanceConfig, YunzaiOtherConfig, YunzaiRedisConfig, YunzaiRendererConfig,
    YunzaiServerConfig, yunzai_config_documents,
};
pub use integration::{YunzaiIntegration, link_server_yaml};
pub use manifest::{YUNZAI_FRAMEWORK_ID, yunzai_manifest};

use crate::adapter::{
    AppComponentSpec, AppFrameworkAdapter, PluginLogSink, apply_with_backup_ex, restore_from_backup,
};
use crate::adopt::{self, write_project_sidecar};
use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, DocumentSnapshot, combined_revision_of,
};
use crate::store::{
    AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry, StoreMarketPart, StoreMarketText,
};
use manifest::{YUNZAI_JS_PLUGIN_DIR, YUNZAI_STDOUT_LOG};

fn envelope(config: YunzaiInstanceConfig, snaps: &[DocumentSnapshot]) -> AppInstanceConfigEnvelope {
    AppInstanceConfigEnvelope {
        config: AppInstanceConfig::Yunzai(config),
        revision: combined_revision_of(snaps),
        documents: snaps.iter().map(DocumentSnapshot::revision_entry).collect(),
    }
}

fn host_err(e: ncd_host::HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

pub struct YunzaiAdapter {
    integration: YunzaiIntegration,
}

impl Default for YunzaiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl YunzaiAdapter {
    pub fn new() -> Self {
        Self {
            integration: YunzaiIntegration::new(),
        }
    }

    fn root(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir)
    }

    fn server_yaml(instance: &AppInstance) -> HostPath {
        Self::root(instance).join(config::config_rel("server"))
    }

    fn component_of(spec: &AppComponentSpec) -> YunzaiComponent {
        YunzaiComponent::new(spec.install_dir.clone(), spec.port)
            .with_node_bin(spec.node_bin.clone())
            .with_git_bin(spec.git_bin.clone())
            .with_redis_bin(spec.redis_bin.clone())
            .with_npm_registry(spec.npm_registry.clone())
            .with_download_chrome(spec.install_renderer)
            .with_adopt_existing(spec.adopt_existing)
    }

    /// server.yaml 现在生效的端口和 auth（请求头名 → 值），`/exit` 要带着它们
    async fn server_auth(
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(u16, Vec<(String, String)>), AppFrameworkError> {
        let root = Self::root(instance);
        let snaps =
            crate::config_doc::read_documents(host, &root, &yunzai_config_documents()).await?;
        let sources = config::read_sources(host, &root, &snaps).await?;
        let cfg = config::config_from_sources(&sources);
        let mut headers = Vec::new();
        if let Some(src) = sources.iter().find(|s| s.name == "server") {
            if let Some(serde_yaml::Value::Mapping(auth)) = src
                .effective
                .iter()
                .find(|(k, _)| k.as_str() == Some("auth"))
                .map(|(_, v)| v)
            {
                for (k, v) in auth {
                    let key = k
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| k.as_u64().map(|n| n.to_string()));
                    let val = v
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| v.as_u64().map(|n| n.to_string()));
                    if let (Some(k), Some(v)) = (key, val) {
                        headers.push((k, v));
                    }
                }
            }
        }
        Ok((cfg.server.port, headers))
    }

    async fn local_exit(port: u16, headers: &[(String, String)]) -> bool {
        // 上游只认 ::1 / ::ffff:127.0.0.1 来的请求：先走 IPv6 回环，连不上（没开 IPv6）再走 IPv4
        for url in [
            format!("http://[::1]:{port}/exit"),
            format!("http://127.0.0.1:{port}/exit"),
        ] {
            let mut req = ncd_network::shared_client()
                .get(&url)
                .timeout(Duration::from_secs(3));
            for (k, v) in headers {
                req = req.header(k.as_str(), v.as_str());
            }
            match req.send().await {
                Ok(resp) => return resp.status() != reqwest::StatusCode::UNAUTHORIZED,
                // 上游收到后不回响应直接退出：超时或连接被断都说明请求已经进去了
                Err(e) if e.is_connect() => continue,
                Err(_) => return true,
            }
        }
        false
    }

    async fn remote_exit(host: &dyn Host, port: u16, headers: &[(String, String)]) -> bool {
        let header_args: String = headers
            .iter()
            .map(|(k, v)| format!(" -H {}", shell_single_quote(&format!("{k}: {v}"))))
            .collect();
        let script = format!(
            "command -v curl >/dev/null 2>&1 || {{ echo rc=127; exit 0; }}\n\
             code=$(curl -s -o /dev/null -w '%{{http_code}}' -m 3{header_args} 'http://[::1]:{port}/exit'); rc=$?\n\
             if [ $rc -eq 7 ]; then code=$(curl -s -o /dev/null -w '%{{http_code}}' -m 3{header_args} 'http://127.0.0.1:{port}/exit'); rc=$?; fi\n\
             echo rc=$rc code=$code"
        );
        let out = host
            .run_to_string(
                HostCommand::new("sh")
                    .arg("-c")
                    .arg(script)
                    .timeout(Duration::from_secs(15)),
            )
            .await;
        match out {
            Ok(out) => remote_exit_sent(&out.stdout),
            Err(_) => false,
        }
    }
}

/// `rc=<curl 退出码> code=<HTTP 码>`：连不上（7）、没有 curl（127）、鉴权没过（401）算没送到
fn remote_exit_sent(stdout: &str) -> bool {
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with("rc="))
        .unwrap_or_default();
    let mut rc = None;
    let mut code = "";
    for part in line.split_whitespace() {
        if let Some(v) = part.strip_prefix("rc=") {
            rc = v.parse::<i32>().ok();
        } else if let Some(v) = part.strip_prefix("code=") {
            code = v;
        }
    }
    match rc {
        None | Some(7) | Some(127) => false,
        _ => code != "401",
    }
}

#[async_trait]
impl AppFrameworkAdapter for YunzaiAdapter {
    fn manifest(&self) -> &AppFrameworkManifest {
        self.integration.manifest()
    }

    fn integration(&self) -> &dyn AppIntegration {
        &self.integration
    }

    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component> {
        Arc::new(Self::component_of(spec))
    }

    async fn probe_project(
        &self,
        host: &dyn Host,
        path: &HostPath,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        probe::probe_yunzai(host, path).await
    }

    async fn adopt_watch_rels(
        &self,
        host: &dyn Host,
        root: &HostPath,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let dotenv = adopt::list_dotenv_rels(host, root).await?;
        let owned: Vec<String> = yunzai_config_documents()
            .into_iter()
            .map(|d| d.rel_path)
            .collect();
        Ok(adopt::merge_rels(
            dotenv,
            &owned.iter().map(String::as_str).collect::<Vec<_>>(),
        ))
    }

    async fn request_graceful_stop(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        // Yunzai 走自己的 HTTP 退出入口，凭据来自实例配置里的 server_auth，不用面板密码
        _panel_password: Option<&str>,
    ) -> Result<bool, AppFrameworkError> {
        let (port, headers) = Self::server_auth(host, instance).await?;
        Ok(match host.locality() {
            Locality::Local => Self::local_exit(port, &headers).await,
            Locality::Remote => Self::remote_exit(host, port, &headers).await,
        })
    }

    /// 上游退出前等插件清理最多 10 秒，再给 Redis 存盘最多 5 秒
    fn graceful_stop_timeout(&self) -> Duration {
        Duration::from_secs(20)
    }

    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError> {
        Self::component_of(spec)
            .resolve_launch_command(host, args)
            .await
            .map_err(|e| AppFrameworkError::Runtime(e.to_string()))
    }

    async fn terminal_profile(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
    ) -> crate::terminal::AppTerminalProfile {
        let mut profile = crate::terminal::node_profile(
            host,
            spec,
            vec![
                TerminalSnippet::new("装依赖", "pnpm install --no-frozen-lockfile"),
                TerminalSnippet::new("看插件", "ls plugins"),
                TerminalSnippet::new("更新本体", "git pull"),
                TerminalSnippet::new("工具版本", "node -v && git --version"),
            ],
        )
        .await;
        if let Ok(git) = git::GitTool::resolve(host, spec.git_bin.as_ref()).await {
            if let Some(dir) = git.dir {
                profile.path_prefix.push(dir);
            }
        }
        profile.env.extend(
            git::git_env()
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        profile.hint = Some(
            "node、pnpm（实例私有的那份）和 git 都能直接用；云崽在跑时别在这里再起一份".into(),
        );
        profile
    }

    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError> {
        let (cfg, _) = config::read_yunzai_config(host, &Self::root(instance)).await?;
        Ok(Some(cfg.server.access_token).filter(|t| !t.trim().is_empty()))
    }

    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError> {
        let root = Self::root(instance);
        let path = Self::server_yaml(instance);
        let current = if host.exists(&path).await.map_err(host_err)? {
            Some(host.read_file(&path).await.map_err(host_err)?)
        } else {
            let default = root.join(config::default_config_rel("server"));
            if !host.exists(&default).await.map_err(host_err)? {
                return Err(AppFrameworkError::Integration(format!(
                    "云崽实例缺少 {}，请先完成安装",
                    config::config_rel("server")
                )));
            }
            Some(host.read_file(&default).await.map_err(host_err)?)
        };
        let text = String::from_utf8_lossy(&current.unwrap_or_default()).into_owned();
        let out = link_server_yaml(&text, instance.port, &plan.access_token)?;
        apply_with_backup_ex(
            host,
            std::slice::from_ref(&path),
            write_project_sidecar(instance),
            || async {
                crate::config_doc::ensure_parent_dir(host, &path).await?;
                host.write_file(&path, out.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))
            },
        )
        .await
    }

    async fn rollback_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        restore_from_backup(host, &[Self::server_yaml(instance)]).await
    }

    fn log_file(&self, instance: &AppInstance) -> Option<HostPath> {
        Some(Self::root(instance).join(YUNZAI_STDOUT_LOG))
    }

    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        yunzai_config_documents()
    }

    async fn read_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let (config, snaps) = config::read_yunzai_config(host, &Self::root(instance)).await?;
        Ok(envelope(config, &snaps))
    }

    async fn write_config(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let AppInstanceConfig::Yunzai(next) = config else {
            return Err(AppFrameworkError::Validation("写入的不是云崽配置".into()));
        };
        let root = Self::root(instance);
        let (_, current) = config::read_yunzai_config(host, &root).await?;
        let (config, snaps) = config::write_yunzai_config(
            host,
            &root,
            next,
            &current,
            write_project_sidecar(instance),
        )
        .await?;
        Ok(envelope(config, &snaps))
    }

    fn find_document(
        &self,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigDocument, AppFrameworkError> {
        if let Some((plugin, rel)) = store::parse_plugin_doc_id(doc_id) {
            return store::plugin_config_document(plugin, rel);
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
        if resource != AppStoreResource::Plugin {
            return Ok(Vec::new());
        }
        store::list_installed(host, instance).await
    }

    async fn install_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        match entry.flavor {
            AppStoreFlavor::Git => store::install_git_plugin(host, instance, entry, log).await,
            // 单 JS 由编排层按 files 下载；走到这里是条目本身就没有能下的文件
            _ => Err(AppFrameworkError::Validation(format!(
                "{} 在索引里没有能直接下载的 .js 地址，去它的主页手动装",
                entry.name
            ))),
        }
    }

    async fn update_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        match entry.flavor {
            AppStoreFlavor::Git => store::update_git_plugin(host, instance, &entry.id, log).await,
            _ => self.install_store_item(host, instance, entry, log).await,
        }
    }

    async fn uninstall_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        id: &str,
        flavor: AppStoreFlavor,
        _resource: AppStoreResource,
        log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        match flavor {
            AppStoreFlavor::App => store::remove_js_plugin(host, instance, id, log).await,
            _ => store::remove_git_plugin(host, instance, id, log).await,
        }
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
        if !id.ends_with(".js") {
            return Err(AppFrameworkError::Validation(
                "目录插件不能单独停用（云崽按目录整个加载），不想要就卸载".into(),
            ));
        }
        store::set_js_enabled(host, instance, id, enabled).await
    }

    async fn list_plugin_config_docs(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plugin_name: &str,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        if plugin_name.ends_with(".js") {
            return Ok(Vec::new());
        }
        store::list_plugin_config_docs(host, instance, plugin_name).await
    }

    async fn confirm_store_item(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        entry: &AppStoreMarketEntry,
    ) -> Result<(), AppFrameworkError> {
        store::confirm_on_disk(host, instance, entry).await
    }

    fn store_market_cache_key(&self, resource: AppStoreResource) -> Option<&'static str> {
        (resource == AppStoreResource::Plugin).then_some(store::YUNZAI_MARKET_CACHE_KEY)
    }

    fn store_market_urls(&self, _resource: AppStoreResource) -> Vec<String> {
        Vec::new()
    }

    fn store_market_parts(&self, resource: AppStoreResource) -> Vec<StoreMarketPart> {
        match resource {
            AppStoreResource::Plugin => store::yunzai_market_parts(),
            AppStoreResource::Adapter => Vec::new(),
        }
    }

    fn parse_store_market_parts(
        &self,
        resource: AppStoreResource,
        parts: &[StoreMarketText],
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        match resource {
            AppStoreResource::Plugin => Ok(store::parse_index(parts)),
            AppStoreResource::Adapter => Ok(Vec::new()),
        }
    }

    fn store_app_file_dest(&self, instance: &AppInstance, basename: &str) -> Option<HostPath> {
        store::reject_unsafe_name(basename).ok()?;
        Some(
            Self::root(instance)
                .join(YUNZAI_JS_PLUGIN_DIR)
                .join(basename),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_exit_result_reading() {
        assert!(
            remote_exit_sent("rc=28 code=000\n"),
            "超时说明请求进去了，上游在收尾"
        );
        assert!(remote_exit_sent("rc=52 code=000"));
        assert!(!remote_exit_sent("rc=7 code=000"));
        assert!(!remote_exit_sent("rc=127"));
        assert!(!remote_exit_sent("rc=0 code=401"));
        assert!(!remote_exit_sent(""));
    }

    #[test]
    fn store_is_split_into_index_parts() {
        let adapter = YunzaiAdapter::new();
        assert_eq!(
            adapter.store_market_parts(AppStoreResource::Plugin).len(),
            5
        );
        assert!(
            adapter
                .store_market_urls(AppStoreResource::Plugin)
                .is_empty()
        );
        assert!(
            adapter
                .store_market_parts(AppStoreResource::Adapter)
                .is_empty()
        );
        assert_eq!(
            adapter.store_market_cache_key(AppStoreResource::Plugin),
            Some("yunzai-plugins")
        );
    }
}
