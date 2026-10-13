// 主机连接监听的订阅就绪、断线清理与连续重连顺序回归。

use super::*;
use crate::{HostResolveError, HostResolver};
use ncd_domain::BackendKind;
use ncd_traits::runtime_backend::{BotStatus, LogSnapshot, TailOpts};
use tokio::sync::{mpsc, oneshot};

struct FixedBotRepo(BotConfig);

#[async_trait]
impl BotConfigRepo for FixedBotRepo {
    async fn list(&self) -> Result<Vec<BotConfig>, BotConfigError> {
        Ok(vec![self.0.clone()])
    }

    async fn get(&self, qq_id: u64) -> Result<Option<BotConfig>, BotConfigError> {
        Ok((self.0.bot.qq_id == qq_id).then(|| self.0.clone()))
    }

    async fn upsert(&self, _config: BotConfig) -> Result<(), BotConfigError> {
        unreachable!("connection recovery must not rewrite bot configuration")
    }

    async fn delete(&self, _qq_id: u64) -> Result<bool, BotConfigError> {
        unreachable!("connection recovery must not delete bots")
    }

    async fn count(&self) -> Result<usize, BotConfigError> {
        Ok(1)
    }
}

struct UnusedBackend(BotId);

#[async_trait]
impl BotBackend for UnusedBackend {
    fn id(&self) -> &BotId {
        &self.0
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }

    fn flavor(&self) -> BotFlavor {
        BotFlavor::SnowLuma
    }

    async fn start(&self, _ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        unreachable!("reconcile must not launch a new QQ process")
    }

    async fn stop(&self, _bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        unreachable!("connection loss must not stop remote QQ")
    }

    async fn status(&self, _bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        unreachable!("this test stops at the host resolution boundary")
    }

    async fn read_config(&self, _bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        unreachable!("connection recovery must not read backend configuration")
    }

    async fn write_config(
        &self,
        _bot_id: BotId,
        _config: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        unreachable!("connection recovery must not write backend configuration")
    }

    async fn tail_log(
        &self,
        _bot_id: BotId,
        _opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        unreachable!("this test stops at the host resolution boundary")
    }
}

struct ObservedResolver {
    calls: mpsc::UnboundedSender<RuntimeTarget>,
    first_gate: Mutex<Option<oneshot::Receiver<()>>>,
}

#[async_trait]
impl HostResolver for ObservedResolver {
    async fn resolve(
        &self,
        target: &RuntimeTarget,
    ) -> Result<Arc<dyn ncd_host::Host>, HostResolveError> {
        self.calls.send(target.clone()).unwrap();
        let gate = self.first_gate.lock().await.take();
        if let Some(gate) = gate {
            gate.await.unwrap();
        }
        Err(HostResolveError::message("test host is unavailable"))
    }
}

#[tokio::test(start_paused = true)]
async fn remote_snowluma_listener_is_ready_and_cancels_old_recovery_before_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let server_id = "server-snowluma";
    let target = RuntimeTarget::server(server_id);
    let config = ncd_test_support::BotConfigBuilder::new()
        .backend_type(BackendType::SnowLuma)
        .runtime_target(target.clone())
        .build();
    let bot_id = BotId::new(config.bot.qq_id.to_string());
    let bus = Arc::new(BroadcastEventBus::default());
    let store = Arc::new(crate::LocalConfigStore::new(root.path()));
    let (calls_tx, mut calls_rx) = mpsc::unbounded_channel();
    let (mut first_release, first_gate) = oneshot::channel();
    let resolver = Arc::new(ObservedResolver {
        calls: calls_tx,
        first_gate: Mutex::new(Some(first_gate)),
    });
    let manager = BotManager::new(
        Arc::new(FixedBotRepo(config)),
        store,
        Arc::new(crate::DispatchRenderer::new(root.path(), root.path())),
        Arc::new(UnusedBackend(BotId::new("unused"))),
        Arc::new(crate::FileSystemRuntimeLaunchPlanner::new(root.path())),
        Arc::clone(&bus),
        Arc::new(crate::ReqwestNapCatWebUiClient::new().unwrap()),
        Arc::new(crate::NoopOfflineNotifier),
        Arc::new(RwLock::new(WebUiPollerSettings::default())),
        Arc::new(RwLock::new(DesktopNotifySettings::default())),
    )
    .with_host_resolver(resolver);
    manager
        .actors
        .write()
        .await
        .insert(bot_id.clone(), BotActorHandle::spawn(bot_id));

    let (ready_tx, ready_rx) = oneshot::channel();
    let listener = tokio::spawn(manager.clone().run_host_connection_listener(Some(ready_tx)));
    tokio::time::timeout(Duration::from_secs(1), ready_rx)
        .await
        .expect("listener must report subscription readiness")
        .unwrap();

    let started_at = tokio::time::Instant::now();
    bus.publish(DomainEvent::HostConnectionRecovered {
        server_id: server_id.into(),
        latency_ms: 1,
    });
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), calls_rx.recv())
            .await
            .expect("the event sent immediately after ready must reach reconcile"),
        Some(target.clone())
    );

    // 清理必须取得 backend 表锁；锁住它可直接观察第二次接管是否越过 Lost。
    let blocked_cleanup = manager.remote_snowluma_backends.lock().await;
    bus.publish(DomainEvent::HostConnectionLost {
        server_id: server_id.into(),
        reason: Some("brief disconnect".into()),
        consecutive_failures: 1,
    });
    bus.publish(DomainEvent::HostConnectionRecovered {
        server_id: server_id.into(),
        latency_ms: 2,
    });
    tokio::time::timeout(Duration::from_secs(1), first_release.closed())
        .await
        .expect("connection loss must cancel the old recovery without waiting for SSH timeout");
    assert!(
        tokio::time::timeout(Duration::from_millis(10), calls_rx.recv())
            .await
            .is_err(),
        "second recovery must wait until the queued loss cleanup completes"
    );

    drop(blocked_cleanup);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), calls_rx.recv())
            .await
            .expect("a second recovery within 30 seconds must not be debounced"),
        Some(target)
    );
    assert!(started_at.elapsed() < Duration::from_secs(30));
    listener.abort();
    assert!(listener.await.unwrap_err().is_cancelled());
}
