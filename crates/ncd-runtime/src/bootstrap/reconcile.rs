use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures_util::stream::{self, StreamExt};
use tokio::sync::{Mutex, RwLock};
use tracing::warn;

use crate::bot_actor::{BotActorHandle, BotActorState};
use crate::bot_manager::BotManagerError;
use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::host_resolver::HostResolver;
use crate::remote_runtime_sessions::RemoteRuntimeSessions;
use crate::runtime_router::RuntimeBackendRouter;
use ncd_backend_napcat::remote_native_launch::remote_napcat_running_pid;
use ncd_backend_snowluma::remote_snowluma::remote_qq_running_pid_with_hint;
use ncd_deploy::{Deployment, DeploymentState, DockerDeployment};
use ncd_domain::bot_status::BotStatus;
use ncd_domain::{BackendType, BotConfig, BotId, RuntimeScenario};
use ncd_traits::BotConfigRepo;

/// 单 Bot 主机才允许「整机唯一 QQ 主进程」归因兜底；多 Bot 主机证据不足时不猜。
pub(crate) fn count_native_snowluma_bots(configs: &[BotConfig]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for config in configs {
        if let Ok(RuntimeScenario::RemoteNative {
            server_id,
            backend: BackendType::SnowLuma,
        }) = RuntimeScenario::from_config(config)
        {
            *counts.entry(server_id).or_insert(0) += 1;
        }
    }
    counts
}

pub(crate) type ReconcileLocks = Arc<Mutex<HashMap<BotId, Arc<Mutex<()>>>>>;

pub(crate) struct BootstrapReconciler<R: BotConfigRepo + 'static> {
    actors: Arc<RwLock<HashMap<BotId, BotActorHandle>>>,
    host_resolver: Option<Arc<dyn HostResolver>>,
    event_bus: Arc<BroadcastEventBus>,
    runtime_router: RuntimeBackendRouter,
    remote_sessions: RemoteRuntimeSessions<R>,
    locks: ReconcileLocks,
}

impl<R: BotConfigRepo + 'static> BootstrapReconciler<R> {
    pub fn new(
        actors: Arc<RwLock<HashMap<BotId, BotActorHandle>>>,
        host_resolver: Option<Arc<dyn HostResolver>>,
        event_bus: Arc<BroadcastEventBus>,
        runtime_router: RuntimeBackendRouter,
        remote_sessions: RemoteRuntimeSessions<R>,
        locks: ReconcileLocks,
    ) -> Self {
        Self {
            actors,
            host_resolver,
            event_bus,
            runtime_router,
            remote_sessions,
            locks,
        }
    }

    pub async fn reconcile_bootstrap_bots(
        &self,
        configs: &[BotConfig],
        skipped: &[BotId],
        snowluma_counts: &HashMap<String, usize>,
    ) -> HashSet<BotId> {
        let mut tasks = stream::iter(configs.to_vec())
            .map(|config| async move {
                let bot_id = BotId::new(config.bot.qq_id.to_string());
                if skipped.contains(&bot_id) || config.bot.runtime_target.is_local() {
                    return None;
                }
                match self.reconcile_bot(&config, snowluma_counts, false).await {
                    Ok(true) => Some(bot_id),
                    Ok(false) => None,
                    Err(err) => {
                        warn!(target: "ncd_runtime::bootstrap_reconcile", %bot_id, %err,
                            "远端运行态恢复失败");
                        None
                    }
                }
            })
            .buffer_unordered(4);
        let mut reconciled = HashSet::new();
        while let Some(bot_id) = tasks.next().await {
            if let Some(bot_id) = bot_id {
                reconciled.insert(bot_id);
            }
        }
        reconciled
    }

    pub(crate) async fn reconcile_bot(
        &self,
        config: &BotConfig,
        snowluma_counts: &HashMap<String, usize>,
        refresh_stopped: bool,
    ) -> Result<bool, BotManagerError> {
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        let gate = {
            let mut locks = self.locks.lock().await;
            Arc::clone(
                locks
                    .entry(bot_id.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(()))),
            )
        };
        let _guard = gate.lock().await;
        let handle = self
            .actors
            .read()
            .await
            .get(&bot_id)
            .cloned()
            .ok_or_else(|| BotManagerError::BotNotFound(bot_id.clone()))?;
        let before = handle.snapshot();
        if !matches!(
            before.state,
            BotActorState::Stopped | BotActorState::Running | BotActorState::Crashed
        ) {
            return Err(BotManagerError::RemoteRead(
                "Bot 正在启停或修复，请稍后刷新".into(),
            ));
        }
        let resolver = self
            .host_resolver
            .as_ref()
            .ok_or_else(|| BotManagerError::RemoteRead("远端连接服务未就绪".into()))?;
        let host = resolver
            .resolve(&config.bot.runtime_target)
            .await
            .map_err(|err| BotManagerError::RemoteRead(err.to_string()))?;
        let scenario = RuntimeScenario::from_config(config)?;
        let mut sl_backend = None;
        let pid = match &scenario {
            RuntimeScenario::RemoteDocker { .. } => {
                let state = DockerDeployment::new()
                    .observe(host.as_ref(), &bot_id)
                    .await
                    .map_err(|err| BotManagerError::RemoteRead(err.to_string()))?;
                match state {
                    DeploymentState::Running => Some(0),
                    DeploymentState::Stopped => None,
                    DeploymentState::Failed { reason } => {
                        return Err(BotManagerError::RemoteRead(reason));
                    }
                    _ => {
                        return Err(BotManagerError::RemoteRead(
                            "容器状态正在变化，请稍后刷新".into(),
                        ));
                    }
                }
            }
            RuntimeScenario::RemoteNative {
                backend: BackendType::NapCat,
                ..
            } => remote_napcat_running_pid(host.as_ref(), config.bot.qq_id)
                .await
                .map_err(|err| BotManagerError::RemoteRead(err.to_string()))?,
            RuntimeScenario::RemoteNative {
                server_id,
                backend: BackendType::SnowLuma,
            } => {
                let backend = self
                    .runtime_router
                    .remote_snowluma_backend_for_server(server_id, Arc::clone(&host))
                    .await?;
                let hint = backend
                    .daemon_paths()
                    .pid_bot_path(&config.bot.qq_id.to_string());
                let pid = remote_qq_running_pid_with_hint(
                    host.as_ref(),
                    config.bot.qq_id,
                    Some(hint.as_str()),
                    Some(backend.qq_bin()),
                    snowluma_counts.get(server_id).copied().unwrap_or(0) == 1,
                )
                .await
                .map_err(|err| BotManagerError::RemoteRead(err.to_string()))?;
                sl_backend = Some(backend);
                pid
            }
            RuntimeScenario::LocalNative { .. } => return Ok(false),
        };
        if pid.is_none() && !refresh_stopped {
            return Ok(false);
        }
        let snapshot = handle
            .reconcile_observed(before.revision, pid.is_some())
            .await?
            .ok_or_else(|| {
                BotManagerError::RemoteRead("探测期间 Bot 状态已变化，请稍后刷新".into())
            })?;
        self.event_bus.publish(DomainEvent::bot_state_changed(
            snapshot,
            "runtime_refreshed",
        ));
        let Some(pid) = pid else {
            if let Some(backend) = sl_backend {
                backend.detach_status_poller(&bot_id).await;
            }
            self.remote_sessions.shutdown_bot(&bot_id).await;
            self.event_bus.publish(DomainEvent::bot_status_changed(
                BotStatus::stopped(bot_id),
                "runtime_refreshed",
            ));
            return Ok(false);
        };
        self.event_bus.publish(DomainEvent::bot_status_changed(
            BotStatus::running(bot_id.clone(), pid, 0),
            "runtime_refreshed",
        ));
        match scenario {
            RuntimeScenario::RemoteDocker { .. } => {
                self.remote_sessions
                    .attach_remote_docker(&bot_id, config, host)
                    .await;
            }
            RuntimeScenario::RemoteNative {
                backend: BackendType::NapCat,
                ..
            } => {
                self.remote_sessions
                    .attach_remote_native_napcat(&bot_id, config, host)
                    .await;
            }
            RuntimeScenario::RemoteNative {
                backend: BackendType::SnowLuma,
                ..
            } => {
                if let Some(backend) = sl_backend {
                    let result = backend
                        .attach_reconciled_running(bot_id.clone(), pid, config)
                        .await;
                    self.remote_sessions
                        .attach_remote_native_snowluma_log_follow(
                            &bot_id,
                            config,
                            host,
                            backend.daemon_paths(),
                        )
                        .await;
                    // 进程状态已确认；UI 接管失败仍须反馈给手动刷新调用方。
                    if refresh_stopped {
                        result?;
                    } else if let Err(err) = result {
                        warn!(target: "ncd_runtime::bootstrap_reconcile", %bot_id, %err,
                            "远端 SnowLuma WebUI 接管失败，QQ 进程仍在运行");
                    }
                }
            }
            RuntimeScenario::LocalNative { .. } => {}
        }
        Ok(true)
    }
}
