// 组件终态只有在进度排空、库存失效与本机包类型落盘后才对外可见。
use super::*;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use ncd_component::{ActionError, DetectedVersion, LaunchArgs, VerifyReport};
use ncd_host::shell::BashShell;
use ncd_host::{Arch, ArchiveKind, DirEntry, HostError, HostPath, HostProcess, HostShell};
use tokio::sync::Mutex;

use crate::events::{DomainEventKind, EventFilter, EventSubscription};

const TASK_ID: &str = "completion-order";
const DEADLINE: Duration = Duration::from_secs(3);
const LOG_COUNT: usize = 80;

struct NoIoHost(Locality);

#[async_trait]
impl Host for NoIoHost {
    fn os(&self) -> Os {
        Os::Linux
    }

    fn arch(&self) -> Arch {
        Arch::X86_64
    }

    fn locality(&self) -> Locality {
        self.0
    }

    fn id(&self) -> &str {
        "completion-host"
    }

    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }

    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        unreachable!("test component performs no host I/O")
    }

    async fn run_to_string(&self, _: HostCommand) -> Result<ncd_host::CommandOutput, HostError> {
        unreachable!("test component performs no host I/O")
    }
}

#[derive(Clone, Copy)]
enum ActionResult {
    Success,
    Failure,
    Cancelled,
}

struct ReportingComponent {
    id: ComponentId,
    result: ActionResult,
    retained: Arc<Mutex<Vec<ActionCtx>>>,
}

#[async_trait]
impl Component for ReportingComponent {
    fn id(&self) -> ComponentId {
        self.id
    }

    fn supported_targets(&self) -> &'static [(Os, Locality)] {
        &[(Os::Linux, Locality::Local), (Os::Linux, Locality::Remote)]
    }

    async fn detect(&self, _: &dyn Host) -> Result<Option<DetectedVersion>, ActionError> {
        Ok(None)
    }

    async fn install(&self, _: &dyn Host, ctx: &mut ActionCtx) -> Result<(), ActionError> {
        self.retained.lock().await.push(ctx.clone());
        for index in 0..LOG_COUNT {
            ctx.info(format!("buffered-{index}")).await;
        }
        match self.result {
            ActionResult::Success => Ok(()),
            ActionResult::Failure => Err(ActionError::other("component failed")),
            ActionResult::Cancelled => Err(ActionError::Cancelled),
        }
    }

    async fn verify(&self, _: &dyn Host) -> Result<VerifyReport, ActionError> {
        Ok(VerifyReport::ok())
    }

    fn launch_command(&self, _: &dyn Host, _: &LaunchArgs) -> Result<HostCommand, ActionError> {
        unreachable!("test component is never launched")
    }
}

struct Harness {
    tasks: DeploymentTaskManager,
    bus: BroadcastEventBus,
    settings: Arc<RwLock<AppSettings>>,
    inventory: Arc<RemoteInventoryService>,
    server_manager: Arc<ServerManager>,
    cache: crate::data::cache::DownloadCacheService,
    retained: Arc<Mutex<Vec<ActionCtx>>>,
    root: PathBuf,
}

impl Harness {
    fn new(root: &Path) -> Self {
        let bus = BroadcastEventBus::default();
        let server_manager = Arc::new(ServerManager::new(
            root,
            Arc::new(ncd_server::InMemoryCredentialStore::default()),
        ));
        Self {
            tasks: DeploymentTaskManager::new(bus.clone()),
            bus,
            settings: Arc::new(RwLock::new(AppSettings {
                snowluma_package: Some(SnowLumaLinuxPackage::Full),
                ..AppSettings::default()
            })),
            inventory: Arc::new(RemoteInventoryService::new(Arc::clone(&server_manager))),
            server_manager,
            cache: crate::data::cache::DownloadCacheService::new(root),
            retained: Arc::new(Mutex::new(Vec::new())),
            root: root.to_path_buf(),
        }
    }

    fn runner(
        &self,
        component_id: ComponentId,
        locality: Locality,
        result: ActionResult,
    ) -> ComponentTaskRunner {
        let component: Arc<dyn Component> = Arc::new(ReportingComponent {
            id: component_id,
            result,
            retained: Arc::clone(&self.retained),
        });
        ComponentTaskRunner {
            component_id,
            kind: StepKind::ForceInstall,
            plan: DeployPlan::builder()
                .force_install("single", component)
                .build(),
            host: Arc::new(NoIoHost(locality)),
            server_id: (locality == Locality::Remote).then(|| "s1".to_string()),
            remote_long_install: false,
            sudo: None,
            snowluma_linux_package: Some(SnowLumaLinuxPackage::Lite),
            inventory: Arc::clone(&self.inventory),
            event_bus: self.bus.clone(),
            active_tasks: ActiveTasks::default(),
            download_cache: self.cache.clone(),
            app_settings: Arc::clone(&self.settings),
            data_root: self.root.clone(),
            server_manager: Arc::clone(&self.server_manager),
        }
    }

    fn subscribe(&self) -> EventSubscription {
        self.bus
            .subscribe(EventFilter::kind(DomainEventKind::ComponentActionProgress))
    }

    async fn submit(
        &self,
        runner: ComponentTaskRunner,
    ) -> oneshot::Receiver<DeploymentTaskRunResult> {
        let (started_tx, started_rx) = oneshot::channel();
        let (result_tx, result_rx) = oneshot::channel();
        self.tasks
            .submit(DeploymentTaskRequest {
                task_id: TASK_ID.to_string(),
                kind: DeploymentTaskKind::ComponentAction {
                    component_id: runner.component_id.as_str().to_string(),
                    action: runner.kind.as_str().to_string(),
                },
                host_id: runner
                    .server_id
                    .as_ref()
                    .map_or_else(|| "local".to_string(), |id| format!("remote:{id}")),
                title: "completion test".to_string(),
                resources: Vec::new(),
                depends_on: Vec::new(),
                dedupe_key: None,
                cancellable: true,
                runner: Box::new(move |ctx| {
                    Box::pin(async move {
                        let _ = started_tx.send(());
                        let result = runner.run(ctx).await;
                        let _ = result_tx.send(result.clone());
                        result
                    })
                }),
            })
            .await;
        tokio::time::timeout(DEADLINE, started_rx)
            .await
            .unwrap()
            .unwrap();
        result_rx
    }

    async fn assert_single_terminal(&self, ok: bool) {
        let list = self.tasks.list().await;
        let events = &list
            .tasks
            .iter()
            .find(|task| task.task_id == TASK_ID)
            .unwrap()
            .progress_events;
        let terminal: Vec<_> = events
            .iter()
            .filter_map(|event| match event.kind {
                ProgressKind::Finished { ok } => Some(ok),
                _ => None,
            })
            .collect();
        assert_eq!(terminal, vec![ok]);
        assert!(
            matches!(events.last().map(|event| &event.kind), Some(ProgressKind::Finished { ok: actual }) if *actual == ok)
        );
    }
}

async fn next_progress(subscription: &mut EventSubscription) -> ProgressKind {
    match tokio::time::timeout(DEADLINE, subscription.next())
        .await
        .unwrap()
        .unwrap()
    {
        DomainEvent::ComponentActionProgress { task_id, event } => {
            assert_eq!(task_id, TASK_ID);
            event.kind
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

async fn result(receiver: oneshot::Receiver<DeploymentTaskRunResult>) -> DeploymentTaskRunResult {
    tokio::time::timeout(DEADLINE, receiver)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn completion_marks_remote_inventory_dirty_and_drains_progress_before_finished() {
    let temp = tempfile::tempdir().unwrap();
    let harness = Harness::new(temp.path());
    let mut subscription = harness.subscribe();
    let done = harness
        .submit(harness.runner(ComponentId::NapCat, Locality::Remote, ActionResult::Success))
        .await;
    let mut buffered = Vec::new();
    loop {
        match next_progress(&mut subscription).await {
            ProgressKind::Finished { ok } => {
                assert!(ok);
                assert!(harness.inventory.is_dirty_for_test("s1"));
                assert!(!harness.inventory.is_dirty_for_test("another-host"));
                break;
            }
            ProgressKind::Log { message, .. } if message.starts_with("buffered-") => {
                buffered.push(message);
            }
            _ => {}
        }
    }
    assert!(result(done).await.ok);
    assert_eq!(
        buffered,
        (0..LOG_COUNT)
            .map(|index| format!("buffered-{index}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        harness.retained.lock().await.len(),
        1,
        "retained senders must not block relay shutdown"
    );
    assert_eq!(
        harness.settings.read().await.snowluma_package,
        Some(SnowLumaLinuxPackage::Full)
    );
    harness.assert_single_terminal(true).await;
}

#[tokio::test]
async fn completion_hides_plan_finished_until_local_package_is_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let harness = Harness::new(temp.path());
    let settings_guard = harness.settings.write().await;
    let mut subscription = harness.subscribe();
    let done = harness
        .submit(harness.runner(
            ComponentId::SnowLuma,
            Locality::Local,
            ActionResult::Success,
        ))
        .await;
    loop {
        match next_progress(&mut subscription).await {
            ProgressKind::StepEnd { ok: true, .. } => break,
            ProgressKind::Finished { .. } => {
                panic!("completion escaped before package persistence")
            }
            _ => {}
        }
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(40), subscription.next())
            .await
            .is_err(),
        "plan's internal Finished must stay private while settings are locked"
    );
    drop(settings_guard);

    let outcome = result(done).await;
    assert!(outcome.ok, "local persistence failed: {outcome:?}");
    assert_eq!(
        next_progress(&mut subscription).await,
        ProgressKind::Finished { ok: true }
    );
    assert_eq!(
        harness.settings.read().await.snowluma_package,
        Some(SnowLumaLinuxPackage::Lite)
    );
    assert_eq!(
        crate::desktop::read_app_settings_file(temp.path())
            .unwrap()
            .snowluma_package,
        Some(SnowLumaLinuxPackage::Lite)
    );
    assert!(
        !harness.inventory.is_dirty_for_test("s1"),
        "local completion must not invalidate a remote host"
    );
    harness.assert_single_terminal(true).await;
}

#[tokio::test]
async fn completion_late_cancel_keeps_local_package_persistence_before_failed_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let harness = Harness::new(temp.path());
    let settings_guard = harness.settings.write().await;
    let mut subscription = harness.subscribe();
    let done = harness
        .submit(harness.runner(
            ComponentId::SnowLuma,
            Locality::Local,
            ActionResult::Success,
        ))
        .await;
    loop {
        match next_progress(&mut subscription).await {
            ProgressKind::StepEnd { ok: true, .. } => break,
            ProgressKind::Finished { .. } => panic!("completion escaped before persistence"),
            _ => {}
        }
    }
    harness.tasks.cancel(TASK_ID).await.unwrap();
    drop(settings_guard);

    let outcome = result(done).await;
    assert!(!outcome.ok, "late cancellation was lost: {outcome:?}");
    assert!(
        outcome
            .error
            .as_deref()
            .is_some_and(|error| error.contains("取消")),
        "late cancellation failed before persistence completed: {outcome:?}"
    );
    assert_eq!(
        next_progress(&mut subscription).await,
        ProgressKind::Finished { ok: false }
    );
    assert_eq!(
        crate::desktop::read_app_settings_file(temp.path())
            .unwrap()
            .snowluma_package,
        Some(SnowLumaLinuxPackage::Lite)
    );
    harness.assert_single_terminal(false).await;
}

#[tokio::test]
async fn completion_reports_failure_when_local_package_persistence_fails() {
    let temp = tempfile::tempdir().unwrap();
    let harness = Harness::new(temp.path());
    std::fs::create_dir_all(temp.path().join("config").join("app-settings.json")).unwrap();
    let mut subscription = harness.subscribe();
    let done = harness
        .submit(harness.runner(
            ComponentId::SnowLuma,
            Locality::Local,
            ActionResult::Success,
        ))
        .await;
    loop {
        if let ProgressKind::Finished { ok } = next_progress(&mut subscription).await {
            assert!(
                !ok,
                "successful plan cannot announce success before persistence"
            );
            break;
        }
    }
    let outcome = result(done).await;
    assert!(!outcome.ok);
    assert!(outcome.error.unwrap().contains("保存 SnowLuma 包类型失败"));
    assert_eq!(
        harness.settings.read().await.snowluma_package,
        Some(SnowLumaLinuxPackage::Full)
    );
    harness.assert_single_terminal(false).await;
}

#[tokio::test]
async fn completion_keeps_one_failed_terminal_for_failed_and_cancelled_steps() {
    for action_result in [ActionResult::Failure, ActionResult::Cancelled] {
        let temp = tempfile::tempdir().unwrap();
        let harness = Harness::new(temp.path());
        let mut subscription = harness.subscribe();
        let done = harness
            .submit(harness.runner(ComponentId::NapCat, Locality::Remote, action_result))
            .await;
        loop {
            if let ProgressKind::Finished { ok } = next_progress(&mut subscription).await {
                assert!(!ok);
                assert!(harness.inventory.is_dirty_for_test("s1"));
                break;
            }
        }
        assert!(!result(done).await.ok);
        harness.assert_single_terminal(false).await;
    }
}

#[tokio::test]
async fn completion_emits_failed_terminal_when_plan_returns_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let harness = Harness::new(temp.path());
    let mut runner = harness.runner(ComponentId::NapCat, Locality::Remote, ActionResult::Success);
    runner.plan = DeployPlan::new();
    let mut subscription = harness.subscribe();
    let done = harness.submit(runner).await;
    assert!(matches!(next_progress(&mut subscription).await,
        ProgressKind::Log { level: ProgressLogLevel::Error, message } if message.starts_with("plan failed:")));
    assert_eq!(
        next_progress(&mut subscription).await,
        ProgressKind::Finished { ok: false }
    );
    assert!(harness.inventory.is_dirty_for_test("s1"));
    assert!(!result(done).await.ok);
    harness.assert_single_terminal(false).await;
}

#[tokio::test]
async fn completion_emits_failed_terminal_when_cancelled_while_waiting_for_cache_lease() {
    let temp = tempfile::tempdir().unwrap();
    let harness = Harness::new(temp.path());
    let _maintenance = harness.cache.maintenance().unwrap();
    let mut subscription = harness.subscribe();
    let done = harness
        .submit(harness.runner(ComponentId::NapCat, Locality::Remote, ActionResult::Success))
        .await;
    harness.tasks.cancel(TASK_ID).await.unwrap();
    assert_eq!(
        next_progress(&mut subscription).await,
        ProgressKind::Finished { ok: false }
    );
    assert!(harness.inventory.is_dirty_for_test("s1"));
    let outcome = result(done).await;
    assert!(!outcome.ok);
    assert!(outcome.error.unwrap().contains("取消"));
    assert!(
        harness.retained.lock().await.is_empty(),
        "component must not start without its lease"
    );
    harness.assert_single_terminal(false).await;
}

#[tokio::test]
async fn completion_emits_failed_terminal_when_cache_lease_fails() {
    let temp = tempfile::tempdir().unwrap();
    let harness = Harness::new(temp.path());
    std::fs::write(
        temp.path()
            .join(ncd_domain::DataRootRetiredMarker::FILE_NAME),
        b"retired",
    )
    .unwrap();
    let mut subscription = harness.subscribe();
    let done = harness
        .submit(harness.runner(ComponentId::NapCat, Locality::Remote, ActionResult::Success))
        .await;
    assert_eq!(
        next_progress(&mut subscription).await,
        ProgressKind::Finished { ok: false }
    );
    assert!(harness.inventory.is_dirty_for_test("s1"));
    assert!(!result(done).await.ok);
    harness.assert_single_terminal(false).await;
}
