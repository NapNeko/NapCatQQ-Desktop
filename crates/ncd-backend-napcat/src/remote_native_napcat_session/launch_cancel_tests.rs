use super::*;
use bytes::Bytes;
use ncd_deploy::NativeDeployment;
use ncd_deploy::deployments::native::NullRuntimeEventSink;
use ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell, Locality, Os,
    ShellKind,
};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Notify;

#[derive(Default)]
struct CountingHost {
    reads: AtomicUsize,
    writes: AtomicUsize,
    spawns: AtomicUsize,
    directories: AtomicUsize,
    commands: AtomicUsize,
    read_started: Notify,
    pending_read: bool,
}

struct FixtureShell;

impl HostShell for FixtureShell {
    fn kind(&self) -> ShellKind {
        ShellKind::Bash
    }
    fn escape(&self, value: &str) -> String {
        value.to_owned()
    }
    fn line_separator(&self) -> &'static str {
        "\n"
    }
}

#[async_trait]
impl Host for CountingHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        Locality::Remote
    }
    fn id(&self) -> &str {
        "cancel-fixture"
    }
    fn shell(&self) -> &dyn HostShell {
        &FixtureShell
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.read_started.notify_one();
        if self.pending_read {
            return std::future::pending().await;
        }
        Err(HostError::PathNotFound { path: path.clone() })
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        self.directories.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        self.spawns.fetch_add(1, Ordering::SeqCst);
        Err(HostError::Unsupported {
            operation: "fixture spawn",
        })
    }
    async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
        self.commands.fetch_add(1, Ordering::SeqCst);
        Err(HostError::Unsupported {
            operation: "fixture command",
        })
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        unreachable!()
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!()
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!()
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        unreachable!()
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        unreachable!()
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        unreachable!()
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        unreachable!()
    }
}

fn deployment(host: Arc<dyn Host>, coordinator: Arc<RemoteQqEntryCoordinator>) -> NativeDeployment {
    let selected = RemoteSelectedPaths {
        home: "/home/fixture".into(),
        qq_install_base: Some("/home/fixture/Napcat".into()),
        qq_bin: Some("/home/fixture/Napcat/opt/QQ/qq".into()),
        ..RemoteSelectedPaths::default()
    };
    NativeDeployment::new(
        Arc::new(
            RemoteNativeLaunchTranslator::new(
                host,
                BotFlavor::NapCat,
                "s-server".into(),
                coordinator,
            )
            .with_selected(selected),
        ),
        Arc::new(NullRuntimeEventSink),
        None,
    )
}

fn assert_no_mutations(host: &CountingHost) {
    assert_eq!(host.writes.load(Ordering::SeqCst), 0);
    assert_eq!(host.spawns.load(Ordering::SeqCst), 0);
    assert_eq!(host.directories.load(Ordering::SeqCst), 0);
    assert_eq!(host.commands.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancellation_releases_entry_gate_wait_without_touching_host() {
    let coordinator = RemoteQqEntryCoordinator::default();
    let host = CountingHost::default();
    let entered = Notify::new();
    let release = Notify::new();
    let holder = coordinator.with_server("s-server", || async {
        entered.notify_one();
        release.notified().await;
    });
    tokio::pin!(holder);
    tokio::select! {
        _ = &mut holder => panic!("coordinator lock was not held"),
        _ = entered.notified() => {}
    }
    let cancel = CancellationToken::new();
    let install_base = HostPath::from_posix("/home/fixture/Napcat");
    let ensure =
        coordinator.ensure_for_napcat_cancellable(&host, "s-server", &install_base, &cancel);
    tokio::pin!(ensure);
    tokio::select! {
        biased;
        result = &mut ensure => panic!("entry gate was bypassed: {result:?}"),
        _ = std::future::ready(()) => {}
    }
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), ensure)
            .await
            .unwrap()
            .is_err()
    );
    assert_eq!(host.reads.load(Ordering::SeqCst), 0);
    assert_no_mutations(&host);
    release.notify_one();
    holder.await;
}

#[tokio::test]
async fn pre_cancelled_native_napcat_launch_never_reads_or_mutates_host() {
    let host = Arc::new(CountingHost::default());
    let deployment = deployment(host.clone(), Arc::new(RemoteQqEntryCoordinator::default()));
    let config = ncd_test_support::BotConfigBuilder::new()
        .qq_id(26001)
        .build();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        deployment
            .launch_cancellable(host.as_ref(), &config, &cancel)
            .await,
        Err(DeploymentError::Host(HostError::Cancelled))
    ));
    assert_eq!(host.reads.load(Ordering::SeqCst), 0);
    assert_no_mutations(host.as_ref());
}

#[tokio::test]
async fn cancelling_pending_native_napcat_config_read_does_not_write_or_spawn() {
    let host = Arc::new(CountingHost {
        pending_read: true,
        ..CountingHost::default()
    });
    let deployment = deployment(host.clone(), Arc::new(RemoteQqEntryCoordinator::default()));
    let config = ncd_test_support::BotConfigBuilder::new()
        .qq_id(26002)
        .build();
    let cancel = CancellationToken::new();
    let launch = deployment.launch_cancellable(host.as_ref(), &config, &cancel);
    tokio::pin!(launch);
    tokio::select! {
        result = &mut launch => panic!("config read was not held: {result:?}"),
        _ = host.read_started.notified() => {}
    }
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), launch)
            .await
            .unwrap(),
        Err(DeploymentError::Host(HostError::Cancelled))
    ));
    assert_eq!(host.reads.load(Ordering::SeqCst), 1);
    assert_no_mutations(host.as_ref());
}
