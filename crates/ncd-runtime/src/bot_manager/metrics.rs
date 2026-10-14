//! 共享 SnowLuma node 的注册生命周期，设置更新不重启其它 Bot。

use super::*;
use crate::metrics::{
    BotRuntimeMetricsPrefs, remote_snowluma_registry_posix, snowluma_registration_path,
    snowluma_registry_dir, write_local_snowluma_registration, write_remote_snowluma_registration,
};
use ncd_domain::DeploymentType;
use ncd_host::{Host, HostPath};

fn is_native_snowluma(config: &BotConfig) -> bool {
    config.bot.backend_type == BackendType::SnowLuma
        && config.bot.deployment_type == DeploymentType::Native
}

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    async fn registration_prefs(&self, config: &BotConfig) -> BotRuntimeMetricsPrefs {
        let mut prefs = self
            .store
            .config_dir()
            .parent()
            .map(Self::load_metrics_prefs)
            .unwrap_or_default();
        let id = BotId::new(config.bot.qq_id.to_string());
        let active = self
            .actors
            .read()
            .await
            .get(&id)
            .is_some_and(|actor| actor.snapshot().state.is_active());
        prefs.enabled &= active;
        prefs
    }

    /// 设置保存时刷新当前账号；远端心跳还会补同步暂时未连接的主机。
    pub async fn refresh_snowluma_metrics_registry(&self) {
        let configs = match self.repo.list().await {
            Ok(configs) => configs,
            Err(error) => {
                warn!(%error, "list bots for shared metrics failed");
                return;
            }
        };
        let config_dir = self.store.config_dir();
        let Some(data_root) = config_dir.parent() else {
            return;
        };
        let local_ids: HashSet<_> = configs
            .iter()
            .filter(|c| is_native_snowluma(c) && c.bot.runtime_target.is_local())
            .map(|c| c.bot.qq_id.to_string())
            .collect();
        if let Ok(entries) = std::fs::read_dir(snowluma_registry_dir(data_root)) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "json")
                    && path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .is_some_and(|id| !local_ids.contains(id))
                {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let mut remote_groups: HashMap<String, Vec<BotConfig>> = HashMap::new();
        for config in &configs {
            if !is_native_snowluma(config) {
                continue;
            }
            if let Some(server) = config.bot.runtime_target.server_id() {
                remote_groups
                    .entry(server.to_string())
                    .or_default()
                    .push(config.clone());
            } else {
                let prefs = self.registration_prefs(config).await;
                if let Err(error) = write_local_snowluma_registration(
                    data_root,
                    &config.bot.qq_id.to_string(),
                    config,
                    &prefs,
                ) {
                    warn!(%error, "refresh local shared metrics failed");
                }
            }
        }
        let Some(resolver) = &self.host_resolver else {
            return;
        };
        for (server, bots) in remote_groups {
            let outcome = tokio::time::timeout(Duration::from_secs(10), async {
                let host = resolver
                    .resolve(&RuntimeTarget::server(&server))
                    .await
                    .map_err(|e| e.to_string())?;
                let home = crate::metrics::probe_remote_home(host.as_ref()).await?;
                self.refresh_snowluma_metrics_for_server(&server, host.as_ref(), &home, &bots)
                    .await
            })
            .await;
            if !matches!(outcome, Ok(Ok(()))) {
                warn!(server_id = %server, "refresh remote shared metrics deferred until reconnect");
            }
        }
    }

    /// 只更新注册与节点文件，不重复上传探针，也不修改已运行 node 的环境。
    pub async fn refresh_snowluma_metrics_for_server(
        &self,
        server_id: &str,
        host: &dyn Host,
        home: &str,
        configs: &[BotConfig],
    ) -> Result<(), String> {
        let bots: Vec<_> = configs
            .iter()
            .filter(|config| {
                is_native_snowluma(config)
                    && config.bot.runtime_target.server_id() == Some(server_id)
            })
            .collect();
        let registry = HostPath::from_posix(remote_snowluma_registry_posix(home));
        let ids: HashSet<_> = bots.iter().map(|c| c.bot.qq_id.to_string()).collect();
        if let Ok(entries) = host.list_dir(&registry).await {
            for entry in entries {
                if !entry.is_dir
                    && entry
                        .name
                        .strip_suffix(".json")
                        .is_some_and(|id| !ids.contains(id))
                {
                    host.remove_file(&registry.join(entry.name))
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        for config in bots {
            let prefs = self.registration_prefs(config).await;
            write_remote_snowluma_registration(
                host,
                home,
                &config.bot.qq_id.to_string(),
                config,
                &prefs,
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn refresh_snowluma_metrics_registration(&self, config: &BotConfig) {
        if !is_native_snowluma(config) {
            return;
        }
        let config_dir = self.store.config_dir();
        let Some(data_root) = config_dir.parent() else {
            return;
        };
        let prefs = self.registration_prefs(config).await;
        let id = config.bot.qq_id.to_string();
        let result = if config.bot.runtime_target.is_local() {
            write_local_snowluma_registration(data_root, &id, config, &prefs)
        } else {
            let Some(resolver) = &self.host_resolver else {
                return;
            };
            match resolver.resolve(&config.bot.runtime_target).await {
                Ok(host) => match crate::metrics::probe_remote_home(host.as_ref()).await {
                    Ok(home) => {
                        write_remote_snowluma_registration(
                            host.as_ref(),
                            &home,
                            &id,
                            config,
                            &prefs,
                        )
                        .await
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error.to_string()),
            }
        };
        if let Err(error) = result {
            warn!(bot_id = %id, %error, "shared metrics registration refresh failed");
        }
    }

    pub(super) async fn remove_snowluma_metrics_registration(&self, config: &BotConfig) {
        if !is_native_snowluma(config) {
            return;
        }
        let id = config.bot.qq_id.to_string();
        if config.bot.runtime_target.is_local() {
            if let Some(root) = self.store.config_dir().parent() {
                let _ = std::fs::remove_file(snowluma_registration_path(root, &id));
            }
        } else if let Some(resolver) = &self.host_resolver {
            if let Ok(host) = resolver.resolve(&config.bot.runtime_target).await {
                if let Ok(home) = crate::metrics::probe_remote_home(host.as_ref()).await {
                    let path = HostPath::from_posix(remote_snowluma_registry_posix(&home))
                        .join(format!("{id}.json"));
                    let _ = host.remove_file(&path).await;
                }
            }
        }
    }
}
