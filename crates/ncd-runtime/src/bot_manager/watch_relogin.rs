// 为 watch 提供远端主机本身可用的登录端点。
use super::*;
use crate::onebot_debug::DebugBotPort;
use ncd_watch::relogin_config::WatchLoginTarget;

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub async fn confirm_watch_relogin(
        &self,
        server_id: &str,
        config: Option<&ncd_watch::config::NotifyConfig>,
    ) {
        let ids = config
            .map(|c| {
                c.bots
                    .iter()
                    .filter(|b| ncd_watch::relogin_config::owns(c, &b.bot_id))
                    .map(|b| b.bot_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        self.watch_relogin_owners
            .lock()
            .await
            .insert(server_id.to_owned(), ids);
    }

    pub async fn watch_owns_relogin(&self, bot: &BotId) -> bool {
        self.watch_relogin_owners
            .lock()
            .await
            .values()
            .any(|ids| ids.iter().any(|id| id == bot.as_str()))
    }

    pub(super) async fn write_watch_intent(&self, config: &BotConfig, running: bool) {
        if config.bot.runtime_target.server_id().is_none() {
            return;
        }
        let Some(resolver) = &self.host_resolver else {
            return;
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let host = resolver
                .resolve(&config.bot.runtime_target)
                .await
                .map_err(|e| e.to_string())?;
            let home = if host.os() == ncd_host::Os::Windows {
                crate::ncd_watch_sync::target_watch_home(host.as_ref(), "")?
            } else {
                let output = host
                    .run_to_string(ncd_host::HostCommand::new("printenv").arg("HOME"))
                    .await
                    .map_err(|e| e.to_string())?;
                let home = output.stdout.trim();
                if !output.success() || !home.starts_with('/') || home.contains(['\n', '\r']) {
                    return Err("远端 HOME 不可用".to_string());
                }
                home.to_string()
            };
            let root = crate::ncd_watch_sync::remote_watch_root(&home);
            if !host
                .exists(&root.join("config").join("notify.json"))
                .await
                .map_err(|e| e.to_string())?
            {
                return Ok(());
            }
            let directory = root.join("state").join("intents");
            host.create_dir_all(&directory)
                .await
                .map_err(|e| e.to_string())?;
            let path = directory.join(&format!("{}.json", config.bot.qq_id));
            let temporary = directory.join(&format!(".{}.tmp", uuid::Uuid::new_v4()));
            let mut intent = if running && host.exists(&path).await.map_err(|e| e.to_string())? {
                let bytes = host.read_file(&path).await.map_err(|e| e.to_string())?;
                serde_json::from_slice::<ncd_watch::relogin_config::WatchRunIntent>(&bytes)
                    .map_err(|_| "watch 启停记录无效")?
            } else {
                ncd_watch::relogin_config::WatchRunIntent::default()
            };
            intent.running = running;
            // 停止后很快启动也保留取消代次，watch 不必恰好轮询到停止瞬间。
            if !running {
                intent.reset_generation = uuid::Uuid::new_v4().to_string();
            }
            let bytes = serde_json::to_vec(&intent).map_err(|e| e.to_string())?;
            host.write_file(&temporary, &bytes)
                .await
                .map_err(|e| e.to_string())?;
            host.rename(&temporary, &path)
                .await
                .map_err(|e| e.to_string())
        })
        .await;
        if !matches!(result, Ok(Ok(()))) {
            tracing::warn!(bot_id = %config.bot.qq_id, "watch 运行意图未同步");
        }
    }

    pub async fn watch_login_targets(
        &self,
        server_id: &str,
        bots: &[BotConfig],
    ) -> Vec<WatchLoginTarget> {
        let mut targets = Vec::new();
        for config in bots
            .iter()
            .filter(|c| c.bot.runtime_target.server_id() == Some(server_id))
        {
            let bot = BotId::new(config.bot.qq_id.to_string());
            let docker = config.bot.deployment_type == ncd_domain::DeploymentType::Docker;
            let mut target = WatchLoginTarget {
                bot_id: bot.to_string(),
                qr_enabled: true,
                ..Default::default()
            };
            if !docker {
                if let Some(resolver) = &self.host_resolver {
                    if let Ok(host) = resolver.resolve(&config.bot.runtime_target).await {
                        if host.os() == ncd_host::Os::Windows {
                            target.qr_enabled = config.bot.backend_type == BackendType::NapCat
                                || self.remote_snowluma_qr_login_enabled();
                            let _ = self.backend_for_config(config).await;
                            if let Some((_local, secret, remote)) = self
                                .windows_bots
                                .endpoint(server_id, config.bot.qq_id, config.bot.backend_type)
                                .await
                            {
                                target.webui_port = remote;
                                target.webui_secret = secret;
                            }
                            targets.push(target);
                            continue;
                        }
                    }
                }
            }
            if config.bot.backend_type == BackendType::NapCat {
                if let Some((port, secret)) = self.napcat_webui_for_watch(&bot).await {
                    target.webui_port = port;
                    target.webui_secret = secret;
                }
                if docker {
                    target.container_name = Some(ncd_deploy::bot_docker_container_name(
                        BackendType::NapCat,
                        config.bot.qq_id,
                    ));
                    target.container_webui_port = Some(6099);
                }
            } else {
                target.qr_enabled = self.remote_snowluma_qr_login_enabled();
                let Ok((_local_port, password)) = self.snowluma_webui(&bot).await else {
                    targets.push(target);
                    continue;
                };
                target.webui_secret = password.clone();
                if docker {
                    if let Some(endpoint) = self.snowluma_docker_endpoints(&bot).await {
                        target.webui_port = endpoint.webui_host_port;
                    }
                    target.container_name = Some(ncd_deploy::bot_docker_container_name(
                        BackendType::SnowLuma,
                        config.bot.qq_id,
                    ));
                    target.container_webui_port = Some(5099);
                } else {
                    let daemon = self
                        .remote_snowluma_daemons
                        .lock()
                        .await
                        .get(server_id)
                        .cloned();
                    let Some(daemon) = daemon else {
                        targets.push(target);
                        continue;
                    };
                    target.pid_file = Some(daemon.paths().pid_bot_path(bot.as_str()));
                    let Some(resolver) = &self.host_resolver else {
                        targets.push(target);
                        continue;
                    };
                    let Ok(host) = resolver.resolve(&config.bot.runtime_target).await else {
                        targets.push(target);
                        continue;
                    };
                    let Ok(port) =
                        ncd_backend_snowluma::remote_snowluma::resolve_remote_webui_port(
                            host.as_ref(),
                            daemon.layout(),
                        )
                        .await
                    else {
                        targets.push(target);
                        continue;
                    };
                    target.webui_port = port;
                }
            }
            targets.push(target);
        }
        targets
    }
}
