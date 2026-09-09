//! 框架适配器契约（Layer3）：把 Component / Integration / 应用端文件写入捏在一起。
//!
//! `ncd_traits::AppIntegration` 是纯计划；碰 Host 的部分（读 token / 写配置 / 回滚）
//! 放这里，因为 ncd-traits 不能依赖 ncd-host。

use std::sync::Arc;

use async_trait::async_trait;
use ncd_component::{Component, LaunchArgs};
use ncd_domain::{
    AppConfigDocument, AppConfigText, AppFrameworkManifest, AppInstance, AppStoreResource,
    OneBotLinkPlan,
};
use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::{AppFrameworkError, AppIntegration};

use crate::config_doc::{
    AppInstanceConfig, AppInstanceConfigEnvelope, read_document, write_document_text,
};
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};

/// 装更卸命令行输出；编排层转成任务 `ProgressKind::Log`。
pub type PluginLogSink = Arc<dyn Fn(String) + Send + Sync>;

/// 为某个应用实例构造 Component 所需的输入（编排层填）
#[derive(Debug, Clone)]
pub struct AppComponentSpec {
    /// 实例目录（HostPath）
    pub install_dir: HostPath,
    /// 实例监听端口（安装时写进应用端配置）
    pub port: u16,
    /// 桌面端管理的 Node 二进制（Node 系框架用）；None 则只看实例标记 / PATH
    pub node_bin: Option<HostPath>,
    /// 桌面端管理的 uv 二进制（Python 系框架用）；None 则只看实例标记 / PATH
    pub uv_bin: Option<HostPath>,
    /// npm registry 镜像；None 用默认源
    pub npm_registry: Option<String>,
    /// Karin：provision 时一并 `pnpm add @karinjs/plugin-puppeteer`。NoneBot2 忽略。
    pub install_renderer: bool,
}

#[async_trait]
pub trait AppFrameworkAdapter: Send + Sync {
    fn manifest(&self) -> &AppFrameworkManifest;

    /// 纯计划侧
    fn integration(&self) -> &dyn AppIntegration;

    /// 安装 / 探测 / 启动命令（R12：走既有 Component × Host × Action）
    fn component(&self, spec: &AppComponentSpec) -> Arc<dyn Component>;

    /// 启动命令（可做 IO 解析工具链，比 `Component::launch_command` 的同步版准确）
    async fn launch_command(
        &self,
        host: &dyn Host,
        spec: &AppComponentSpec,
        args: &LaunchArgs,
    ) -> Result<HostCommand, AppFrameworkError>;

    /// 应用端已配置的 OneBot 反向 WS token；没有或为空返回 None
    async fn read_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<String>, AppFrameworkError>;

    /// 把计划写进应用端：先备份 `<file>.ncd.bak`，任一步失败自动还原
    async fn apply_link(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        plan: &OneBotLinkPlan,
    ) -> Result<(), AppFrameworkError>;

    /// 解绑：应用端侧默认不动监听口（Bot 侧删连接即可）；有需要的框架覆写
    async fn unlink(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    /// `apply_link` 成功但 Bot 侧写失败时，把应用端文件从 `.ncd.bak` 还原
    async fn rollback_link(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    /// 实例运行日志文件（远端 tail 用）；None 表示只有进程 stdout
    fn log_file(&self, instance: &AppInstance) -> Option<HostPath>;

    /// 该实例可在「原始文件」Tab 编辑的配置文档；空表示没有可编辑配置
    fn config_documents(&self, _instance: &AppInstance) -> Vec<AppConfigDocument> {
        Vec::new()
    }

    /// 类型化配置读取；没有类型化模型的框架保持默认（`ConfigUnsupported`）
    async fn read_config(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        Err(AppFrameworkError::ConfigUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    /// 类型化配置写入（校验 → 备份写 → 回读）。返回写后信封，供编排层比对版本号
    async fn write_config(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _config: &AppInstanceConfig,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        Err(AppFrameworkError::ConfigUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    /// 读一份文档原文（缺文件返回空文本 + `"missing"` 版本号）
    async fn read_config_text(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let doc = self.find_document(instance, doc_id)?;
        let snap = read_document(host, &HostPath::from_posix(&instance.install_dir), &doc).await?;
        Ok(AppConfigText {
            doc_id: doc.id,
            text: snap.text.unwrap_or_default(),
            revision: snap.revision,
        })
    }

    /// 写一份文档原文（格式预检 + 版本号比对 + 备份写）
    async fn write_config_text(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        doc_id: &str,
        text: &str,
        base_revision: Option<&str>,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let doc = self.find_document(instance, doc_id)?;
        write_document_text(
            host,
            &HostPath::from_posix(&instance.install_dir),
            &doc,
            text,
            base_revision,
        )
        .await
    }

    fn find_document(
        &self,
        instance: &AppInstance,
        doc_id: &str,
    ) -> Result<AppConfigDocument, AppFrameworkError> {
        self.config_documents(instance)
            .into_iter()
            .find(|d| d.id == doc_id)
            .ok_or_else(|| AppFrameworkError::Validation(format!("未知的配置文档: {doc_id}")))
    }

    async fn list_installed(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _resource: AppStoreResource,
    ) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn install_store_item(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _entry: &AppStoreMarketEntry,
        _log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn update_store_item(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _entry: &AppStoreMarketEntry,
        _log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn uninstall_store_item(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _id: &str,
        _flavor: AppStoreFlavor,
        _resource: AppStoreResource,
        _log: Option<&PluginLogSink>,
    ) -> Result<(), AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn set_store_enabled(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _id: &str,
        _resource: AppStoreResource,
        _enabled: bool,
        _overwrite: bool,
    ) -> Result<(), AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    async fn list_plugin_config_docs(
        &self,
        _host: &dyn Host,
        instance: &AppInstance,
        _plugin_name: &str,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            instance.framework_id.as_str().to_string(),
        ))
    }

    /// 商店启停是否改类型化配置（再走 `write_config` 同步对接）。
    /// false：自己写盘（`set_store_enabled`）。
    fn store_enable_via_config(&self) -> bool {
        false
    }

    /// 在类型化配置上改商店启停；`None` 表示走 `set_store_enabled`。
    fn apply_store_enabled(
        &self,
        _config: &AppInstanceConfig,
        _name: &str,
        _resource: AppStoreResource,
        _enabled: bool,
    ) -> Result<Option<AppInstanceConfig>, AppFrameworkError> {
        Ok(None)
    }

    /// 装/更之后核对落盘（Karin 插件目录）；默认不做事。
    async fn confirm_store_item(
        &self,
        _host: &dyn Host,
        _instance: &AppInstance,
        _entry: &AppStoreMarketEntry,
    ) -> Result<(), AppFrameworkError> {
        Ok(())
    }

    fn store_market_urls(&self, _resource: AppStoreResource) -> Vec<String> {
        Vec::new()
    }

    fn store_market_cache_key(&self, _resource: AppStoreResource) -> Option<&'static str> {
        None
    }

    fn parse_store_market(
        &self,
        _resource: AppStoreResource,
        _text: &str,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        Err(AppFrameworkError::PluginUnsupported(
            self.manifest().id.as_str().to_string(),
        ))
    }

    /// App 口味文件落盘路径；None 表示此框架不收 app 文件。
    fn store_app_file_dest(&self, _instance: &AppInstance, _basename: &str) -> Option<HostPath> {
        None
    }
}

/// 「备份 → 写 → 失败还原」骨架：`write` 返回 Err 时把 `files` 全部还原到备份内容。
/// 备份文件名 `<file>.ncd.bak`，成功后保留最近一份供人工比对。
pub async fn apply_with_backup<F, Fut>(
    host: &dyn Host,
    files: &[HostPath],
    write: F,
) -> Result<(), AppFrameworkError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), AppFrameworkError>>,
{
    let mut originals: Vec<(HostPath, Option<Vec<u8>>)> = Vec::with_capacity(files.len());
    for file in files {
        let existed = host
            .exists(file)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        let content = if existed {
            let bytes = host
                .read_file(file)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
            host.write_file(&backup_path(file), &bytes)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
            Some(bytes.to_vec())
        } else {
            None
        };
        originals.push((file.clone(), content));
    }

    match write().await {
        Ok(()) => Ok(()),
        Err(err) => {
            for (file, original) in originals {
                let restore = match original {
                    Some(bytes) => host.write_file(&file, &bytes).await,
                    None => match host.exists(&file).await {
                        Ok(true) => host.remove_file(&file).await,
                        _ => Ok(()),
                    },
                };
                if let Err(e) = restore {
                    tracing::error!(
                        file = %file.as_posix(),
                        error = %e,
                        "restore app-side config after failed link"
                    );
                }
            }
            Err(err)
        }
    }
}

pub fn backup_path(file: &HostPath) -> HostPath {
    HostPath::from_posix(format!("{}.ncd.bak", file.as_posix()))
}

/// 把 `files` 各自从 `<file>.ncd.bak` 还原（没有备份的跳过）
pub async fn restore_from_backup(
    host: &dyn Host,
    files: &[HostPath],
) -> Result<(), AppFrameworkError> {
    for file in files {
        let backup = backup_path(file);
        let has_backup = host
            .exists(&backup)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        if !has_backup {
            continue;
        }
        let bytes = host
            .read_file(&backup)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        host.write_file(file, &bytes)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ncd_domain::AppStoreResource;

    use crate::config_doc::AppInstanceConfig;
    use crate::karin::config::KarinInstanceConfig;
    use crate::karin::KarinAdapter;
    use crate::nonebot2::NoneBot2Adapter;
    use crate::AppFrameworkAdapter;

    #[test]
    fn karin_store_enable_goes_through_typed_config() {
        let adapter = KarinAdapter::new();
        assert!(adapter.store_enable_via_config());
        let cfg = AppInstanceConfig::Karin(KarinInstanceConfig::upstream_default());
        let next = adapter
            .apply_store_enabled(&cfg, "@karinjs/plugin-basic", AppStoreResource::Plugin, false)
            .unwrap()
            .expect("Karin 启停应返回改过的配置");
        let AppInstanceConfig::Karin(k) = next else {
            panic!("expected Karin config");
        };
        assert!(
            k.groups
                .iter()
                .any(|r| r.key == "default" && r.disable.contains(&"@karinjs/plugin-basic".into()))
        );
    }

    #[test]
    fn nonebot2_store_enable_does_not_mutate_typed_config() {
        let adapter = NoneBot2Adapter::new();
        assert!(!adapter.store_enable_via_config());
        let cfg = AppInstanceConfig::NoneBot2(crate::nonebot2::NoneBot2InstanceConfig {
            env_prod: crate::nonebot2::config::NoneBot2EnvProd::default(),
        });
        assert!(adapter
            .apply_store_enabled(&cfg, "nonebot_plugin_foo", AppStoreResource::Plugin, false)
            .unwrap()
            .is_none());
    }

    #[test]
    fn karin_store_market_urls_only_for_plugins() {
        let adapter = KarinAdapter::new();
        assert_eq!(
            adapter.store_market_urls(AppStoreResource::Plugin),
            vec![crate::karin::plugin::KARIN_PLUGINS_LIST_URL.to_string()]
        );
        assert!(adapter.store_market_urls(AppStoreResource::Adapter).is_empty());
        assert!(adapter.store_market_cache_key(AppStoreResource::Plugin).is_none());
    }

    #[test]
    fn nonebot2_store_market_urls_follow_nb_cli() {
        let adapter = NoneBot2Adapter::new();
        let plugins = adapter.store_market_urls(AppStoreResource::Plugin);
        let adapters = adapter.store_market_urls(AppStoreResource::Adapter);
        assert_eq!(plugins[0], "https://registry.nonebot.dev/plugins.json");
        assert_eq!(adapters[0], "https://registry.nonebot.dev/adapters.json");
        assert!(plugins.iter().any(|u| u.contains("jsdelivr.net/gh/nonebot/registry@results")));
        assert_eq!(
            adapter.store_market_cache_key(AppStoreResource::Plugin),
            Some("plugins")
        );
        assert_eq!(
            adapter.store_market_cache_key(AppStoreResource::Adapter),
            Some("adapters")
        );
    }

    #[test]
    fn store_app_file_dest_is_per_adapter() {
        use ncd_domain::{
            AppFrameworkId, AppInstance, AppInstanceId, AppInstanceState, AppPlacement,
        };

        let instance = AppInstance {
            id: AppInstanceId::new("k1"),
            framework_id: AppFrameworkId::new("karin"),
            display_name: "K".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/apps/karin/k1".into(),
            port: 7777,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 1,
            install_renderer: true,
        };
        let dest = KarinAdapter::new()
            .store_app_file_dest(&instance, "index.js")
            .expect("Karin 收 app 文件");
        assert_eq!(dest.as_posix(), "/apps/karin/k1/plugins/karin-plugin-example/index.js");
        assert!(
            NoneBot2Adapter::new()
                .store_app_file_dest(&instance, "index.js")
                .is_none(),
            "NoneBot2 没有 Karin 式 app 文件落点"
        );
    }

    #[test]
    fn parse_store_market_dispatches_by_adapter() {
        let karin = KarinAdapter::new();
        let list = karin
            .parse_store_market(
                AppStoreResource::Plugin,
                r#"{"plugins":[{"name":"x","type":"npm","description":"d","time":"2025-01-01 00:00:00","home":"h","author":[],"repo":[]}]}"#,
            )
            .unwrap();
        assert_eq!(list[0].id, "x");

        let nb = NoneBot2Adapter::new();
        let adapters = nb
            .parse_store_market(
                AppStoreResource::Adapter,
                r#"[{"module_name":"nonebot.adapters.console","project_link":"nonebot-adapter-console","name":"Console"}]"#,
            )
            .unwrap();
        assert_eq!(adapters[0].id, "nonebot.adapters.console");
        assert_eq!(adapters[0].resource, AppStoreResource::Adapter);
    }
}
