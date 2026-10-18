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
use ncd_traits::runtime_backend::BotBackend;

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

pub(crate) async fn lock_bot_reconcile(
    locks: &ReconcileLocks,
    bot_id: &BotId,
) -> tokio::sync::OwnedMutexGuard<()> {
    let gate = {
        let mut locks = locks.lock().await;
        Arc::clone(
            locks
                .entry(bot_id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    };
    gate.lock_owned().await
}

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
        let _guard = lock_bot_reconcile(&self.locks, &bot_id).await;
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
        if host.os() == ncd_host::Os::Windows
            && matches!(scenario, RuntimeScenario::RemoteNative { .. })
        {
            let backend = self.runtime_router.backend_for_config(config).await?;
            let running = reconcile_windows_backend(
                backend.as_ref(),
                &handle,
                &before,
                self.event_bus.as_ref(),
                refresh_stopped,
            )
            .await?;
            if running {
                self.remote_sessions
                    .attach_after_runtime_start(&bot_id, config)
                    .await;
            } else {
                self.remote_sessions.shutdown_bot(&bot_id).await;
            }
            return Ok(running);
        }
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
            if handle.snapshot().revision != before.revision {
                return Err(BotManagerError::RemoteRead(
                    "探测期间 Bot 状态已变化，请稍后刷新".into(),
                ));
            }
            if let Some(backend) = sl_backend {
                backend.detach_status_poller(&bot_id).await;
            }
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

async fn reconcile_windows_backend(
    backend: &dyn BotBackend,
    handle: &BotActorHandle,
    before: &ncd_domain::BotActorSnapshot,
    bus: &dyn EventBus,
    refresh_stopped: bool,
) -> Result<bool, BotManagerError> {
    let status = backend.status(before.bot_id.clone()).await?;
    if status.bot_id != before.bot_id {
        return Err(BotManagerError::RemoteRead(
            "Windows Agent 返回了其它 Bot 的状态".into(),
        ));
    }
    if let Some(error) = &status.transport_error {
        return Err(BotManagerError::RemoteRead(error.clone()));
    }
    let running = match status.state {
        BotActorState::Running if status.pid.is_some_and(|pid| pid > 0) => true,
        BotActorState::Stopped | BotActorState::Crashed => false,
        _ => {
            return Err(BotManagerError::RemoteRead(
                "Windows Bot 状态正在变化或进程身份不明，请稍后刷新".into(),
            ));
        }
    };
    if !running && !refresh_stopped && before.state == BotActorState::Stopped {
        return Ok(false);
    }
    let snapshot = handle
        .reconcile_observed(before.revision, running)
        .await?
        .ok_or_else(|| BotManagerError::RemoteRead("探测期间 Bot 状态已变化，请稍后刷新".into()))?;
    bus.publish(DomainEvent::bot_state_changed(
        snapshot,
        "runtime_refreshed",
    ));
    bus.publish(DomainEvent::bot_status_changed(status, "runtime_refreshed"));
    Ok(running)
}

#[cfg(test)]
mod windows_tests {
    use super::*;
    use async_trait::async_trait;
    use ncd_domain::{BackendKind, BotFlavor, StopMode};
    use ncd_traits::runtime_backend::{
        BotBackendError, BotStartCtx, LogSnapshot, TailOpts,
    };

    struct StatusOnlyBackend {
        status: BotStatus,
        flavor: BotFlavor,
    }

    #[async_trait]
    impl BotBackend for StatusOnlyBackend {
        fn id(&self) -> &BotId {
            &self.status.bot_id
        }
        fn kind(&self) -> BackendKind {
            BackendKind::RemoteSsh
        }
        fn flavor(&self) -> BotFlavor {
            self.flavor
        }
        async fn status(&self, _: BotId) -> Result<BotStatus, BotBackendError> {
            Ok(self.status.clone())
        }
        async fn start(&self, _: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
            panic!("recovery must not spawn another QQ")
        }
        async fn stop(&self, _: BotId, _: StopMode) -> Result<(), BotBackendError> {
            panic!("recovery must not stop the remote bot")
        }
        async fn tail_log(&self, _: BotId, _: TailOpts) -> Result<LogSnapshot, BotBackendError> {
            panic!("status recovery must not start a Linux log follower")
        }
    }

    #[tokio::test]
    async fn windows_native_recovery_attaches_agent_status_for_both_frameworks_without_starting() {
        let bus = BroadcastEventBus::default();
        for flavor in [BotFlavor::NapCat, BotFlavor::SnowLuma] {
            let actor = BotActorHandle::spawn("10001");
            let backend = StatusOnlyBackend {
                status: BotStatus::running("10001", 8123, 123),
                flavor,
            };
            let before = actor.snapshot();
            assert!(
                reconcile_windows_backend(&backend, &actor, &before, &bus, false)
                    .await
                    .unwrap()
            );
            assert_eq!(actor.snapshot().state, BotActorState::Running);
            actor.shutdown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn reconnect_refreshes_a_previously_running_actor_when_agent_reports_stopped() {
        let actor = BotActorHandle::spawn("10001");
        actor.reconcile_observed(0, true).await.unwrap().unwrap();
        let backend = StatusOnlyBackend {
            status: BotStatus::stopped("10001"),
            flavor: BotFlavor::NapCat,
        };
        assert!(
            !reconcile_windows_backend(
                &backend,
                &actor,
                &actor.snapshot(),
                &BroadcastEventBus::default(),
                false
            )
            .await
            .unwrap()
        );
        assert_eq!(actor.snapshot().state, BotActorState::Stopped);
        actor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn uncertain_or_wrong_identity_status_does_not_overwrite_the_actor() {
        let actor = BotActorHandle::spawn("10001");
        actor.reconcile_observed(0, true).await.unwrap().unwrap();
        let before = actor.snapshot();
        let mut uncertain = BotStatus::running("10001", 8123, 0);
        uncertain.transport_error = Some("SSH unavailable".into());
        let statuses = [
            uncertain,
            BotStatus::running("other-bot", 8123, 0),
            BotStatus::running("10001", 0, 0),
        ];
        for status in statuses {
            let backend = StatusOnlyBackend {
                status,
                flavor: BotFlavor::SnowLuma,
            };
            assert!(
                reconcile_windows_backend(
                    &backend,
                    &actor,
                    &before,
                    &BroadcastEventBus::default(),
                    true
                )
                .await
                .is_err()
            );
            assert_eq!(actor.snapshot(), before);
        }
        actor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn recovery_cannot_overwrite_a_start_requested_after_observation() {
        let actor = BotActorHandle::spawn("10001");
        let before = actor.snapshot();
        actor.request_start().await.unwrap();
        let backend = StatusOnlyBackend {
            status: BotStatus::running("10001", 8123, 0),
            flavor: BotFlavor::NapCat,
        };
        assert!(
            reconcile_windows_backend(
                &backend,
                &actor,
                &before,
                &BroadcastEventBus::default(),
                true
            )
            .await
            .is_err()
        );
        assert_eq!(actor.snapshot().state, BotActorState::Starting);
        actor.shutdown().await.unwrap();
    }
}
