use std::sync::Arc;

use crate::docker_bot_session::{DockerBotSessionRegistry, SnowLumaDockerEndpoints};
use crate::events::BroadcastEventBus;
use crate::host_resolver::HostResolver;
use crate::remote_bot_log_follow::RemoteBotLogFollowRegistry;
use crate::runtime_router::DockerSecretProvider;
use crate::runtime_router::RuntimeBackendRouter;
use ncd_backend_napcat::remote_native_napcat_session::RemoteNativeNapcatSessionRegistry;
use ncd_backend_snowluma::remote_snowluma_layout::SnowLumaRemotePaths;
use ncd_backend_snowluma::remote_snowluma_log::RemoteSnowLumaLogRegistry;
use ncd_domain::{BackendType, BotConfig, BotId, RuntimeScenario, RuntimeTarget};
use ncd_host::Host;
use ncd_traits::BotConfigRepo;
use tracing::warn;

pub(crate) struct RemoteRuntimeSessions<R: BotConfigRepo + 'static> {
    repo: Arc<R>,
    event_bus: Arc<BroadcastEventBus>,
    host_resolver: Option<Arc<dyn HostResolver>>,
    runtime_router: RuntimeBackendRouter,
    docker_secrets: DockerSecretProvider,
    docker_sessions: Arc<DockerBotSessionRegistry>,
    remote_native_napcat_sessions: Arc<RemoteNativeNapcatSessionRegistry>,
    remote_bot_log_follow: Arc<RemoteBotLogFollowRegistry>,
    remote_sl_daemon_log: Arc<RemoteSnowLumaLogRegistry>,
}

impl<R: BotConfigRepo + 'static> RemoteRuntimeSessions<R> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<R>,
        event_bus: Arc<BroadcastEventBus>,
        host_resolver: Option<Arc<dyn HostResolver>>,
        runtime_router: RuntimeBackendRouter,
        docker_secrets: DockerSecretProvider,
        docker_sessions: Arc<DockerBotSessionRegistry>,
        remote_native_napcat_sessions: Arc<RemoteNativeNapcatSessionRegistry>,
        remote_bot_log_follow: Arc<RemoteBotLogFollowRegistry>,
        remote_sl_daemon_log: Arc<RemoteSnowLumaLogRegistry>,
    ) -> Self {
        Self {
            repo,
            event_bus,
            host_resolver,
            runtime_router,
            docker_secrets,
            docker_sessions,
            remote_native_napcat_sessions,
            remote_bot_log_follow,
            remote_sl_daemon_log,
        }
    }

    pub async fn attach_after_runtime_start(&self, bot_id: &BotId, config: &BotConfig) {
        let scenario = match RuntimeScenario::from_config(config) {
            Ok(scenario) => scenario,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::remote_runtime_sessions",
                    bot_id = %bot_id,
                    err = %err,
                    "启动后 attach: runtime matrix 无效"
                );
                return;
            }
        };
        match scenario {
            RuntimeScenario::RemoteDocker { server_id, .. } => {
                let Some(host) = self.resolve_remote_host(bot_id, &server_id).await else {
                    return;
                };
                self.attach_remote_docker(bot_id, config, host).await;
            }
            RuntimeScenario::RemoteNative {
                server_id,
                backend: BackendType::NapCat,
            } => {
                let Some(host) = self.resolve_remote_host(bot_id, &server_id).await else {
                    return;
                };
                self.attach_remote_native_napcat(bot_id, config, host).await;
            }
            RuntimeScenario::RemoteNative {
                server_id,
                backend: BackendType::SnowLuma,
            } => {
                let Some(host) = self.resolve_remote_host(bot_id, &server_id).await else {
                    return;
                };
                let daemon = match self
                    .runtime_router
                    .remote_snowluma_daemon_for_server(&server_id, Arc::clone(&host))
                    .await
                {
                    Ok(daemon) => daemon,
                    Err(err) => {
                        warn!(
                            target: "ncd_runtime::remote_runtime_sessions",
                            bot_id = %bot_id,
                            err = %err,
                            "启动后 attach: 远端 SnowLuma daemon 初始化失败"
                        );
                        return;
                    }
                };
                self.attach_remote_native_snowluma_log_follow(bot_id, config, host, daemon.paths())
                    .await;
            }
            RuntimeScenario::LocalNative { .. } => {}
        }
    }

    pub async fn prepare_before_runtime_start(&self, config: &BotConfig) {
        let Ok(RuntimeScenario::RemoteNative {
            server_id,
            backend: BackendType::SnowLuma,
        }) = RuntimeScenario::from_config(config)
        else {
            return;
        };
        self.stop_remote_native_napcat_sessions_on_server(&server_id, Some(config.bot.qq_id))
            .await;
    }

    pub async fn attach_remote_docker(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        host: Arc<dyn Host>,
    ) {
        let Ok(RuntimeScenario::RemoteDocker { backend, .. }) =
            RuntimeScenario::from_config(config)
        else {
            return;
        };

        let napcat_token = (backend == BackendType::NapCat)
            .then(|| self.docker_secrets.napcat_webui_token(config.bot.qq_id))
            .transpose()
            .ok()
            .flatten();
        let vnc_pass = (backend == BackendType::SnowLuma)
            .then(|| self.docker_secrets.snowluma_vnc_passwd(config.bot.qq_id))
            .transpose()
            .ok()
            .flatten();
        let sl_webui = (backend == BackendType::SnowLuma)
            .then(|| {
                self.docker_secrets
                    .snowluma_webui_bootstrap(config.bot.qq_id)
            })
            .transpose()
            .ok()
            .flatten();
        self.docker_sessions
            .start_session(
                bot_id.clone(),
                config.clone(),
                host,
                Arc::clone(&self.event_bus),
                napcat_token,
                vnc_pass,
                sl_webui,
            )
            .await;
    }

    pub async fn attach_remote_native_napcat(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        host: Arc<dyn Host>,
    ) {
        let Ok(RuntimeScenario::RemoteNative {
            server_id,
            backend: BackendType::NapCat,
        }) = RuntimeScenario::from_config(config)
        else {
            return;
        };

        self.stop_other_remote_native_napcat_sessions_on_server(&server_id, Some(config.bot.qq_id))
            .await;
        self.remote_native_napcat_sessions
            .start_session(
                bot_id.clone(),
                config.clone(),
                host,
                Arc::clone(&self.event_bus),
            )
            .await;
    }

    pub async fn attach_remote_native_snowluma_log_follow(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        host: Arc<dyn Host>,
        paths: &SnowLumaRemotePaths,
    ) {
        let Ok(RuntimeScenario::RemoteNative {
            server_id,
            backend: BackendType::SnowLuma,
        }) = RuntimeScenario::from_config(config)
        else {
            return;
        };

        self.remote_native_napcat_sessions
            .shutdown_bot(bot_id)
            .await;
        self.stop_other_remote_native_napcat_sessions_on_server(&server_id, Some(config.bot.qq_id))
            .await;

        let log_path = paths.log_bot_path(bot_id.as_str());
        self.remote_bot_log_follow
            .start_bot_log(
                bot_id.clone(),
                Arc::clone(&host),
                log_path,
                Arc::clone(&self.event_bus),
            )
            .await;
        self.remote_sl_daemon_log
            .start_daemon_follow_for_server(
                &server_id,
                host,
                paths.log_daemon.clone(),
                Arc::clone(&self.event_bus),
            )
            .await;
    }

    pub async fn mark_remote_docker_stop_expected(&self, bot_id: &BotId) {
        self.docker_sessions.stop_expected(bot_id).await;
    }

    pub async fn shutdown_bot(&self, bot_id: &BotId) {
        self.docker_sessions.shutdown_bot(bot_id).await;
        self.remote_native_napcat_sessions
            .shutdown_bot(bot_id)
            .await;
        self.remote_bot_log_follow.stop_bot(bot_id).await;
    }

    pub async fn shutdown_all(&self) {
        self.docker_sessions.shutdown_all().await;
        self.remote_native_napcat_sessions.shutdown_all().await;
        self.remote_bot_log_follow.shutdown_all().await;
        self.remote_sl_daemon_log.shutdown_all().await;
    }

    pub async fn snowluma_docker_endpoints(
        &self,
        bot_id: &BotId,
    ) -> Option<SnowLumaDockerEndpoints> {
        self.docker_sessions.snowluma_endpoints(bot_id).await
    }

    async fn resolve_remote_host(&self, bot_id: &BotId, server_id: &str) -> Option<Arc<dyn Host>> {
        let resolver = match self.host_resolver.as_ref() {
            Some(resolver) => resolver,
            None => {
                warn!(
                    target: "ncd_runtime::remote_runtime_sessions",
                    bot_id = %bot_id,
                    "启动后 attach: HostResolver 未初始化"
                );
                return None;
            }
        };
        let target = RuntimeTarget::server(server_id.to_string());
        match resolver.resolve(&target).await {
            Ok(host) => Some(host),
            Err(err) => {
                warn!(
                    target: "ncd_runtime::remote_runtime_sessions",
                    bot_id = %bot_id,
                    err = %err,
                    "启动后 attach: 远端主机未连接"
                );
                None
            }
        }
    }

    pub async fn stop_remote_native_napcat_sessions_on_server(
        &self,
        server_id: &str,
        except_qq_id: Option<u64>,
    ) {
        let configs = match self.repo.list().await {
            Ok(configs) => configs,
            Err(_) => return,
        };
        for cfg in configs {
            let Ok(RuntimeScenario::RemoteNative {
                server_id: cfg_server_id,
                backend: BackendType::NapCat,
            }) = RuntimeScenario::from_config(&cfg)
            else {
                continue;
            };
            if cfg.bot.qq_id == except_qq_id.unwrap_or(u64::MAX) {
                continue;
            }
            if cfg_server_id != server_id {
                continue;
            }
            let bid = BotId::new(cfg.bot.qq_id.to_string());
            self.remote_native_napcat_sessions.shutdown_bot(&bid).await;
        }
    }

    async fn stop_other_remote_native_napcat_sessions_on_server(
        &self,
        server_id: &str,
        except_qq_id: Option<u64>,
    ) {
        self.stop_remote_native_napcat_sessions_on_server(server_id, except_qq_id)
            .await;
    }
}
