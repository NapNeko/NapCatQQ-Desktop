//! 应用端商店：Karin 插件、NoneBot 适配器和插件、AstrBot / 麦麦插件的列表、安装、启停

use super::*;

impl AppManager {
    pub async fn list_karin_plugin_market(
        &self,
    ) -> Result<Vec<KarinPluginMarketEntry>, AppFrameworkError> {
        super::plugin_market::fetch_karin_plugin_market(&self.registry, &self.market_cache).await
    }

    pub async fn list_store(
        &self,
        framework_id: &AppFrameworkId,
        resource: AppStoreResource,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        self.fetch_market(framework_id.as_str(), resource).await
    }

    async fn fetch_market(
        &self,
        framework_id: &str,
        resource: AppStoreResource,
    ) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
        super::plugin_market::fetch_store(&self.registry, &self.market_cache, framework_id, resource)
            .await
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
                let market = self
                    .fetch_market(instance.framework_id.as_str(), resource)
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
        let market = self.fetch_market(instance.framework_id.as_str(), resource).await;
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
            locked: false,
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
