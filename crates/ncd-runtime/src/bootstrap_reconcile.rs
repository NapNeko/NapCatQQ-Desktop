use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::bot_actor::BotActorHandle;
use crate::bot_manager::BotManagerError;
use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::host_resolver::HostResolver;
use crate::remote_runtime_sessions::RemoteRuntimeSessions;
use crate::runtime_router::RuntimeBackendRouter;
use ncd_backend_napcat::remote_native_launch::remote_napcat_running_pid;
use ncd_backend_snowluma::remote_snowluma::{RemoteSnowLumaBackend, remote_qq_running_pid};
use ncd_backend_snowluma::remote_snowluma_tunnel::RemoteSnowLumaTunnelRegistry;
use ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator;
use ncd_deploy::{Deployment, DeploymentState, DockerDeployment};
use ncd_domain::bot_status::BotStatus;
use ncd_domain::{BackendType, BotConfig, BotId, RuntimeScenario, RuntimeTarget};
use ncd_traits::BotConfigRepo;

pub(crate) struct BootstrapReconciler<R: BotConfigRepo + 'static> {
    actors: Arc<RwLock<HashMap<BotId, BotActorHandle>>>,
    host_resolver: Option<Arc<dyn HostResolver>>,
    event_bus: Arc<BroadcastEventBus>,
    runtime_router: RuntimeBackendRouter,
    remote_sessions: RemoteRuntimeSessions<R>,
    remote_snowluma_tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    remote_qq_entry_coordinator: Arc<RemoteQqEntryCoordinator>,
}

impl<R: BotConfigRepo + 'static> BootstrapReconciler<R> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        actors: Arc<RwLock<HashMap<BotId, BotActorHandle>>>,
        host_resolver: Option<Arc<dyn HostResolver>>,
        event_bus: Arc<BroadcastEventBus>,
        runtime_router: RuntimeBackendRouter,
        remote_sessions: RemoteRuntimeSessions<R>,
        remote_snowluma_tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        remote_qq_entry_coordinator: Arc<RemoteQqEntryCoordinator>,
    ) -> Self {
        Self {
            actors,
            host_resolver,
            event_bus,
            runtime_router,
            remote_sessions,
            remote_snowluma_tunnels,
            remote_qq_entry_coordinator,
        }
    }

    pub async fn reconcile_bootstrap_bots(
        &self,
        configs: &[BotConfig],
        skipped: &[BotId],
    ) -> HashSet<BotId> {
        let mut reconciled = HashSet::new();
        let resolver = match &self.host_resolver {
            Some(resolver) => resolver,
            None => return reconciled,
        };

        for config in configs {
            let bot_id = BotId::new(config.bot.qq_id.to_string());
            if skipped.contains(&bot_id) {
                continue;
            }
            let handle = match self.get_actor(&bot_id).await {
                Some(handle) => handle,
                None => continue,
            };
            let scenario = match RuntimeScenario::from_config(config) {
                Ok(scenario) => scenario,
                Err(err) => {
                    warn!(
                        target: "ncd_runtime::bootstrap_reconcile",
                        bot_id = %bot_id,
                        err = %err,
                        "bootstrap reconcile: runtime matrix 无效，跳过"
                    );
                    continue;
                }
            };

            match scenario {
                RuntimeScenario::RemoteDocker { server_id, .. } => {
                    if self
                        .reconcile_remote_docker(
                            &bot_id,
                            &handle,
                            config,
                            resolver.as_ref(),
                            &server_id,
                        )
                        .await
                    {
                        reconciled.insert(bot_id);
                    }
                }
                RuntimeScenario::RemoteNative {
                    server_id,
                    backend: BackendType::NapCat,
                } => {
                    if self
                        .reconcile_remote_native_napcat(
                            &bot_id,
                            &handle,
                            config,
                            resolver.as_ref(),
                            &server_id,
                        )
                        .await
                    {
                        reconciled.insert(bot_id);
                    }
                }
                RuntimeScenario::RemoteNative {
                    server_id,
                    backend: BackendType::SnowLuma,
                } => {
                    if self
                        .reconcile_remote_native_snowluma(
                            &bot_id,
                            &handle,
                            config,
                            resolver.as_ref(),
                            &server_id,
                        )
                        .await
                    {
                        reconciled.insert(bot_id);
                    }
                }
                RuntimeScenario::LocalNative { .. } => {}
            }
        }
        reconciled
    }

    async fn reconcile_remote_docker(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        config: &BotConfig,
        resolver: &dyn HostResolver,
        server_id: &str,
    ) -> bool {
        let target = RuntimeTarget::server(server_id.to_string());
        let host = match resolver.resolve(&target).await {
            Ok(host) => host,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bootstrap_reconcile",
                    bot_id = %bot_id,
                    err = %err,
                    "bootstrap reconcile: 远端主机未连接，跳过"
                );
                return false;
            }
        };

        let deployment = DockerDeployment::new();
        let state = match deployment.observe(host.as_ref(), bot_id).await {
            Ok(state) => state,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bootstrap_reconcile",
                    bot_id = %bot_id,
                    err = %err,
                    "bootstrap reconcile: observe 失败"
                );
                return false;
            }
        };
        if state != DeploymentState::Running {
            return false;
        }

        if let Err(err) = self.mark_reconciled_running(bot_id, handle, 0).await {
            warn!(
                target: "ncd_runtime::bootstrap_reconcile",
                bot_id = %bot_id,
                err = %err,
                "bootstrap reconcile: Docker actor 状态失败"
            );
            return false;
        }
        self.remote_sessions
            .attach_remote_docker(bot_id, config, host)
            .await;
        info!(
            target: "ncd_runtime::bootstrap_reconcile",
            bot_id = %bot_id,
            "bootstrap reconcile: 已恢复远端 Docker 运行态"
        );
        true
    }

    async fn reconcile_remote_native_napcat(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        config: &BotConfig,
        resolver: &dyn HostResolver,
        server_id: &str,
    ) -> bool {
        let target = RuntimeTarget::server(server_id.to_string());
        let host = match resolver.resolve(&target).await {
            Ok(host) => host,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bootstrap_reconcile",
                    bot_id = %bot_id,
                    err = %err,
                    "bootstrap reconcile: 远端 NapCat Native 主机未连接，跳过"
                );
                return false;
            }
        };

        let pid = match remote_napcat_running_pid(host.as_ref(), config.bot.qq_id).await {
            Ok(Some(pid)) => pid,
            Ok(None) => return false,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bootstrap_reconcile",
                    bot_id = %bot_id,
                    err = %err,
                    "bootstrap reconcile: 远端 NapCat pgrep 失败"
                );
                return false;
            }
        };

        if let Err(err) = self.mark_reconciled_running(bot_id, handle, pid).await {
            warn!(
                target: "ncd_runtime::bootstrap_reconcile",
                bot_id = %bot_id,
                err = %err,
                "bootstrap reconcile: 远端 NapCat Native actor 状态失败"
            );
            return false;
        }
        self.remote_sessions
            .attach_remote_native_napcat(bot_id, config, host)
            .await;
        info!(
            target: "ncd_runtime::bootstrap_reconcile",
            bot_id = %bot_id,
            pid,
            "bootstrap reconcile: 已恢复远端 NapCat Native 运行态"
        );
        true
    }

    async fn reconcile_remote_native_snowluma(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        config: &BotConfig,
        resolver: &dyn HostResolver,
        server_id: &str,
    ) -> bool {
        let target = RuntimeTarget::server(server_id.to_string());
        let host = match resolver.resolve(&target).await {
            Ok(host) => host,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bootstrap_reconcile",
                    bot_id = %bot_id,
                    err = %err,
                    "bootstrap reconcile: 远端 SnowLuma 主机未连接，跳过"
                );
                return false;
            }
        };

        let pid = match remote_qq_running_pid(host.as_ref(), config.bot.qq_id).await {
            Ok(Some(pid)) => pid,
            Ok(None) => return false,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bootstrap_reconcile",
                    bot_id = %bot_id,
                    err = %err,
                    "bootstrap reconcile: 远端 QQ pgrep 失败"
                );
                return false;
            }
        };

        let daemon = match self
            .runtime_router
            .remote_snowluma_daemon_for_server(server_id, Arc::clone(&host))
            .await
        {
            Ok(daemon) => daemon,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bootstrap_reconcile",
                    bot_id = %bot_id,
                    err = %err,
                    "bootstrap reconcile: 远端 SnowLuma daemon 初始化失败"
                );
                return false;
            }
        };

        let backend_id = BotId::new(format!("remote-sl-{}", config.bot.qq_id));
        let sl_paths = daemon.paths().clone();
        let sl_backend = RemoteSnowLumaBackend::new(
            backend_id,
            daemon,
            Arc::clone(&self.event_bus),
            Arc::clone(&self.remote_snowluma_tunnels),
            Arc::clone(&self.remote_qq_entry_coordinator),
        );
        if let Err(err) = sl_backend
            .attach_reconciled_running(bot_id.clone(), pid, config)
            .await
        {
            warn!(
                target: "ncd_runtime::bootstrap_reconcile",
                bot_id = %bot_id,
                err = %err,
                "bootstrap reconcile: 远端 SnowLuma attach 失败"
            );
            return false;
        }

        if let Err(err) = self.mark_reconciled_running(bot_id, handle, pid).await {
            warn!(
                target: "ncd_runtime::bootstrap_reconcile",
                bot_id = %bot_id,
                err = %err,
                "bootstrap reconcile: 远端 SnowLuma actor 状态失败"
            );
            return false;
        }
        self.remote_sessions
            .attach_remote_native_snowluma_log_follow(bot_id, config, host, &sl_paths)
            .await;
        info!(
            target: "ncd_runtime::bootstrap_reconcile",
            bot_id = %bot_id,
            pid,
            "bootstrap reconcile: 已恢复远端 SnowLuma Native 运行态"
        );
        true
    }

    async fn mark_reconciled_running(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        pid: u32,
    ) -> Result<(), BotManagerError> {
        let (starting, advanced) = handle.request_start_transition().await?;
        if advanced {
            self.publish_state_change(&starting, "bootstrap_reconcile");
        }
        let running = handle.confirm_running().await?;
        self.publish_state_change(&running, "bootstrap_reconcile");
        self.event_bus.publish(DomainEvent::bot_status_changed(
            BotStatus::running(bot_id.clone(), pid, 0),
            "bootstrap_reconcile",
        ));
        Ok(())
    }

    async fn get_actor(&self, bot_id: &BotId) -> Option<BotActorHandle> {
        self.actors.read().await.get(bot_id).cloned()
    }

    fn publish_state_change(&self, snapshot: &crate::bot_actor::BotActorSnapshot, reason: &str) {
        self.event_bus
            .publish(DomainEvent::bot_state_changed(snapshot.clone(), reason));
    }
}
