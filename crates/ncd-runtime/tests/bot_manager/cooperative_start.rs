use super::*;
use tokio_util::sync::CancellationToken;

struct CooperativeBackend {
    inner: Arc<FakeBackend>,
    mutate_before_wait: bool,
    entered: tokio::sync::Notify,
    cancelled: tokio::sync::Notify,
    cleanup: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

#[async_trait]
impl BotBackend for CooperativeBackend {
    fn id(&self) -> &BotId {
        self.inner.id()
    }
    fn kind(&self) -> BackendKind {
        self.inner.kind()
    }
    fn flavor(&self) -> ncd_runtime::BotFlavor {
        self.inner.flavor()
    }
    async fn start(&self, context: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        self.inner.start(context).await
    }
    async fn start_cancellable(
        &self,
        context: &BotStartCtx,
        cancel: CancellationToken,
    ) -> Result<BotStatus, BotBackendError> {
        if self.mutate_before_wait {
            self.inner.start(context).await?;
        }
        self.entered.notify_one();
        cancel.cancelled().await;
        self.cancelled.notify_one();
        let cleanup = self.cleanup.lock().await.take();
        if let Some(cleanup) = cleanup {
            cleanup.await.unwrap();
        }
        if self.mutate_before_wait {
            self.inner
                .stop(context.config.bot_id.clone(), StopMode::Force)
                .await?;
        }
        Err(BotBackendError::Io(
            "cooperative start cancelled after cleanup".into(),
        ))
    }
    async fn stop(&self, id: BotId, mode: StopMode) -> Result<(), BotBackendError> {
        self.inner.stop(id, mode).await
    }
    async fn status(&self, id: BotId) -> Result<BotStatus, BotBackendError> {
        self.inner.status(id).await
    }
    async fn tail_log(
        &self,
        id: BotId,
        opts: TailOpts,
    ) -> Result<ncd_runtime::LogSnapshot, BotBackendError> {
        self.inner.tail_log(id, opts).await
    }
}

fn fixture(
    root: &std::path::Path,
    mutate_before_wait: bool,
    cleanup: Option<tokio::sync::oneshot::Receiver<()>>,
) -> (
    Arc<CooperativeBackend>,
    BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>,
) {
    let (store, repo, inner, _) = make_manager(root);
    let backend = Arc::new(CooperativeBackend {
        inner,
        mutate_before_wait,
        entered: tokio::sync::Notify::new(),
        cancelled: tokio::sync::Notify::new(),
        cleanup: Mutex::new(cleanup),
    });
    let manager = BotManager::new(
        repo,
        Arc::clone(&store),
        Arc::new(DispatchRenderer::new(
            store.config_dir(),
            store.config_dir(),
        )),
        backend.clone(),
        Arc::new(TestLaunchPlanner),
        Arc::new(BroadcastEventBus::default()),
        default_webui_client(),
        default_offline_notifier(),
        default_poller_settings(),
        default_desktop_notify(),
    );
    (backend, manager)
}

#[tokio::test]
async fn stop_cancels_a_cooperative_read_only_start_before_the_stop_deadline() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (backend, manager) = fixture(temp.path(), false, None);
    let bot = BotId::new("10420");
    manager
        .upsert_bot_config(bot_config(10420, "cooperative read"))
        .await
        .unwrap();
    let start_manager = manager.clone();
    let start_bot = bot.clone();
    let start = tokio::spawn(async move { start_manager.start_bot(&start_bot).await });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.entered.notified(),
    )
    .await
    .unwrap();
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(2), manager.stop_bot(&bot))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stopped.state, BotActorState::Stopped);
    assert!(stopped.last_error.is_none());
    assert!(matches!(
        start.await.unwrap(),
        Err(BotManagerError::Cancelled)
    ));
    assert_eq!(backend.inner.start_count(bot.clone()).await, 0);
    assert_eq!(backend.inner.stop_count(bot).await, 0);
}

#[tokio::test]
async fn cancelled_start_keeps_the_gate_until_mutation_cleanup_finishes_without_crashing_stopping()
{
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (release_cleanup, cleanup) = tokio::sync::oneshot::channel();
    let (backend, manager) = fixture(temp.path(), true, Some(cleanup));
    let bot = BotId::new("10421");
    manager
        .upsert_bot_config(bot_config(10421, "cooperative mutation"))
        .await
        .unwrap();
    let start_manager = manager.clone();
    let start_bot = bot.clone();
    let start = tokio::spawn(async move { start_manager.start_bot(&start_bot).await });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.entered.notified(),
    )
    .await
    .unwrap();
    let stop_manager = manager.clone();
    let stop_bot = bot.clone();
    let stop = tokio::spawn(async move { stop_manager.stop_bot(&stop_bot).await });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        backend.cancelled.notified(),
    )
    .await
    .unwrap();
    assert!(!stop.is_finished());
    assert!(!start.is_finished());
    assert_eq!(
        manager.get_snapshot(&bot).await.unwrap().state,
        BotActorState::Stopping
    );
    assert!(backend.inner.running.lock().await.contains(&bot));
    assert_eq!(backend.inner.stop_count(bot.clone()).await, 0);
    release_cleanup.send(()).unwrap();
    assert!(matches!(
        start.await.unwrap(),
        Err(BotManagerError::Cancelled)
    ));
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(2), stop)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(stopped.state, BotActorState::Stopped);
    assert!(stopped.last_error.is_none());
    assert!(!backend.inner.running.lock().await.contains(&bot));
    assert_eq!(backend.inner.stop_count(bot).await, 1);
}

struct StartOwnershipBackend {
    inner: Arc<FakeBackend>,
    cleanup_statuses: Mutex<Vec<BotStatus>>,
    cleanup_finished: tokio::sync::Notify,
    normal_stops: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl BotBackend for StartOwnershipBackend {
    fn id(&self) -> &BotId {
        self.inner.id()
    }
    fn kind(&self) -> BackendKind {
        self.inner.kind()
    }
    fn flavor(&self) -> ncd_runtime::BotFlavor {
        self.inner.flavor()
    }
    async fn start(&self, context: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        self.inner.start(context).await
    }
    async fn cleanup_unacknowledged_start(
        &self,
        started: &BotStatus,
    ) -> Result<(), BotBackendError> {
        self.cleanup_statuses.lock().await.push(started.clone());
        self.inner
            .stop(started.bot_id.clone(), StopMode::Force)
            .await?;
        self.cleanup_finished.notify_one();
        Ok(())
    }
    async fn stop(&self, _: BotId, _: StopMode) -> Result<(), BotBackendError> {
        self.normal_stops
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(BotBackendError::Io(
            "normal stop must not clean an unacknowledged start".into(),
        ))
    }
    async fn status(&self, id: BotId) -> Result<BotStatus, BotBackendError> {
        self.inner.status(id).await
    }
    async fn tail_log(
        &self,
        id: BotId,
        opts: TailOpts,
    ) -> Result<ncd_runtime::LogSnapshot, BotBackendError> {
        self.inner.tail_log(id, opts).await
    }
}

#[tokio::test]
async fn abandoned_start_uses_its_ownership_cleanup_hook_and_converges_actor() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (store, repo, inner, _) = make_manager(temp.path());
    let (release, entered) = inner.block_next_start().await;
    let backend = Arc::new(StartOwnershipBackend {
        inner,
        cleanup_statuses: Mutex::new(Vec::new()),
        cleanup_finished: tokio::sync::Notify::new(),
        normal_stops: std::sync::atomic::AtomicUsize::new(0),
    });
    let manager = BotManager::new(
        repo,
        Arc::clone(&store),
        Arc::new(DispatchRenderer::new(
            store.config_dir(),
            store.config_dir(),
        )),
        backend.clone(),
        Arc::new(TestLaunchPlanner),
        Arc::new(BroadcastEventBus::default()),
        default_webui_client(),
        default_offline_notifier(),
        default_poller_settings(),
        default_desktop_notify(),
    );
    let bot = BotId::new("10422");
    manager
        .upsert_bot_config(bot_config(10422, "owned start cleanup"))
        .await
        .unwrap();
    let starting_manager = manager.clone();
    let starting_bot = bot.clone();
    let start = tokio::spawn(async move { starting_manager.start_bot(&starting_bot).await });
    entered.await.unwrap();
    start.abort();
    assert!(start.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        backend.cleanup_finished.notified().await;
        while manager.get_snapshot(&bot).await.unwrap().state != BotActorState::Stopped {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        backend
            .normal_stops
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    let calls = backend.cleanup_statuses.lock().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].bot_id, bot);
    assert_eq!(calls[0].state, BotActorState::Running);
    assert!(calls[0].pid.is_some());
    assert!(!backend.inner.running.lock().await.contains(&bot));
}
