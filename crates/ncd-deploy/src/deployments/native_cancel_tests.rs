use super::*;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use tokio::sync::Notify;

struct GateTranslator {
    calls: AtomicUsize,
    entered: Notify,
    release: Notify,
}

impl GateTranslator {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            entered: Notify::new(),
            release: Notify::new(),
        }
    }
}

#[async_trait]
impl NativeLaunchTranslator for GateTranslator {
    async fn translate(&self, _: &BotConfig) -> Result<NativeLaunchCommand, DeploymentError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        self.release.notified().await;
        Ok(command())
    }
}

fn command() -> NativeLaunchCommand {
    NativeLaunchCommand {
        program: "fixture-bot".into(),
        args: Vec::new(),
        working_dir: None,
        environment: BTreeMap::new(),
    }
}

#[tokio::test]
async fn cancelled_launch_does_not_translate_or_spawn() {
    let translator = Arc::new(GateTranslator::new());
    let deployment =
        NativeDeployment::new(translator.clone(), Arc::new(NullRuntimeEventSink), None);
    let host = FakeHost::new(Os::Windows);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        deployment
            .launch_cancellable(&host, &make_bot_config(25001), &cancel)
            .await,
        Err(DeploymentError::Host(HostError::Cancelled))
    ));
    assert_eq!(translator.calls.load(Ordering::SeqCst), 0);
    assert!(host.spawn_calls().is_empty());
}

#[tokio::test]
async fn cancellation_waits_for_existing_translator_then_prevents_spawn() {
    let translator = Arc::new(GateTranslator::new());
    let deployment =
        NativeDeployment::new(translator.clone(), Arc::new(NullRuntimeEventSink), None);
    let host = FakeHost::new(Os::Windows);
    let config = make_bot_config(25002);
    let cancel = CancellationToken::new();
    let launch = deployment.launch_cancellable(&host, &config, &cancel);
    tokio::pin!(launch);
    tokio::select! {
        result = &mut launch => panic!("translator was not held: {result:?}"),
        _ = translator.entered.notified() => {}
    }
    cancel.cancel();
    tokio::select! {
        biased;
        result = &mut launch => panic!("mutating translator was dropped: {result:?}"),
        _ = std::future::ready(()) => {}
    }
    translator.release.notify_one();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), launch)
            .await
            .unwrap(),
        Err(DeploymentError::Host(HostError::Cancelled))
    ));
    assert!(host.spawn_calls().is_empty());
}

#[tokio::test]
async fn cancellation_after_spawn_submission_preserves_handle_and_managed_process() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut host = FakeHost::new(Os::Windows);
    host.spawn_started = Some(started.clone());
    host.spawn_release = Some(release.clone());
    let (process, wait_tx) = ControlledProcess::with_wait_sender(25003);
    host.push_process(process);
    let deployment = NativeDeployment::new(
        Arc::new(FakeTranslator { plan: command() }),
        Arc::new(NullRuntimeEventSink),
        None,
    );
    let config = make_bot_config(25003);
    let cancel = CancellationToken::new();
    let launch = deployment.launch_cancellable(&host, &config, &cancel);
    tokio::pin!(launch);
    tokio::select! {
        result = &mut launch => panic!("spawn was not held: {result:?}"),
        _ = started.notified() => {}
    }
    cancel.cancel();
    tokio::select! {
        biased;
        result = &mut launch => panic!("submitted spawn was abandoned: {result:?}"),
        _ = std::future::ready(()) => {}
    }
    release.notify_one();
    let handle = tokio::time::timeout(Duration::from_secs(1), launch)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        handle,
        DeploymentHandle::Native { pid: 25003, .. }
    ));
    let bot_id = BotId::new("25003");
    assert_eq!(
        deployment.observe(&host, &bot_id).await.unwrap(),
        DeploymentState::Running
    );
    deployment
        .stop(&host, &bot_id, StopMode::Force)
        .await
        .unwrap();
    assert_eq!(host.kill_calls().len(), 1);
    assert_eq!(host.kill_calls()[0].args.last().unwrap(), "25003");
    let _ = wait_tx.send(Ok(CommandOutput {
        exit_code: Some(0),
        stdout: String::new(),
        stderr: String::new(),
    }));
}
