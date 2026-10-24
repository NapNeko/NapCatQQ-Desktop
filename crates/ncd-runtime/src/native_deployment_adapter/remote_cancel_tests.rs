use super::*;
use bytes::Bytes;
use ncd_deploy::deployments::native::NullRuntimeEventSink;
use ncd_deploy::{NativeLaunchCommand, NativeLaunchTranslator};
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostProcess, HostShell, Locality, Os,
    ShellKind,
};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

struct ProbeHost {
    commands: AtomicUsize,
    original: bool,
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
impl Host for ProbeHost {
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
        if self.original {
            "old-host"
        } else {
            "new-host"
        }
    }
    fn shell(&self) -> &dyn HostShell {
        &FixtureShell
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        assert!(
            self.original,
            "old startup must never operate on the replacement host"
        );
        self.commands.fetch_add(1, Ordering::SeqCst);
        assert_eq!(command.program, "sh");
        Ok(CommandOutput {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        unreachable!()
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        unreachable!()
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        unreachable!()
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
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
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        unreachable!()
    }
}

struct SwitchedResolver {
    replacement: Arc<dyn Host>,
    calls: AtomicUsize,
}

#[async_trait]
impl crate::HostResolver for SwitchedResolver {
    async fn resolve(&self, _: &RuntimeTarget) -> Result<Arc<dyn Host>, crate::HostResolveError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::clone(&self.replacement))
    }
}

struct NeverTranslate;

#[async_trait]
impl NativeLaunchTranslator for NeverTranslate {
    async fn translate(
        &self,
        _: &ncd_domain::BotConfig,
    ) -> Result<NativeLaunchCommand, DeploymentError> {
        unreachable!("unacknowledged cleanup must not translate or launch")
    }
}

async fn fixture() -> (
    RemoteNativeDeploymentBackend,
    Arc<ProbeHost>,
    Arc<ProbeHost>,
    Arc<SwitchedResolver>,
    BotStatus,
) {
    let original = Arc::new(ProbeHost {
        commands: AtomicUsize::new(0),
        original: true,
    });
    let replacement = Arc::new(ProbeHost {
        commands: AtomicUsize::new(0),
        original: false,
    });
    let resolver = Arc::new(SwitchedResolver {
        replacement: replacement.clone(),
        calls: AtomicUsize::new(0),
    });
    let deployment = Arc::new(NativeDeployment::new(
        Arc::new(NeverTranslate),
        Arc::new(NullRuntimeEventSink),
        None,
    ));
    let backend = RemoteNativeDeploymentBackend::new(
        deployment,
        resolver.clone(),
        RuntimeTarget::server("same-profile-id"),
        BotId::new("10423"),
        BotFlavor::NapCat,
    );
    let started = BotStatus::running(BotId::new("10423"), 0, 12345);
    *backend.pending_start.lock().await = Some(PendingNativeStart {
        host: original.clone(),
        bot_id: started.bot_id.clone(),
        pid: 0,
        started_at: 12345,
    });
    (backend, original, replacement, resolver, started)
}

#[tokio::test]
async fn unacknowledged_start_cleanup_uses_its_original_host_after_profile_replacement() {
    let (backend, original, replacement, resolver, started) = fixture().await;
    backend
        .cleanup_unacknowledged_start(&started)
        .await
        .unwrap();
    assert_eq!(original.commands.load(Ordering::SeqCst), 2);
    assert_eq!(replacement.commands.load(Ordering::SeqCst), 0);
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert!(backend.pending_start.lock().await.is_none());
}

#[tokio::test]
async fn mismatched_unacknowledged_start_cannot_touch_either_host_or_consume_ownership() {
    for mismatch in ["bot", "pid", "start"] {
        let (backend, original, replacement, resolver, mut started) = fixture().await;
        match mismatch {
            "bot" => started.bot_id = BotId::new("10424"),
            "pid" => started.pid = Some(99),
            _ => started.started_at = Some(54321),
        }
        assert!(
            backend
                .cleanup_unacknowledged_start(&started)
                .await
                .is_err()
        );
        assert_eq!(original.commands.load(Ordering::SeqCst), 0);
        assert_eq!(replacement.commands.load(Ordering::SeqCst), 0);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
        assert!(backend.pending_start.lock().await.is_some());
    }
}
