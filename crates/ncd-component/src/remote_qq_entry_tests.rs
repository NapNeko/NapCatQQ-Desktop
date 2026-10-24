use super::*;
use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostProcess, HostShell, Locality, Os,
    shell::BashShell,
};
use std::path::Path;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Notify, Semaphore};

struct EntryHost {
    package: Mutex<Vec<u8>>,
    reads: AtomicUsize,
    writes: AtomicUsize,
    commands: Mutex<Vec<HostCommand>>,
    reject_sudo: bool,
    native_entry: Option<&'static str>,
    read_gate: Option<Arc<Semaphore>>,
    read_entered: Notify,
    write_gate: Option<Arc<Semaphore>>,
    write_entered: Notify,
}

impl EntryHost {
    fn new(main: &str) -> Self {
        Self {
            package: Mutex::new(
                serde_json::to_vec(
                    &serde_json::json!({"main": main, "name":"qq", "keep":{"value":42}}),
                )
                .unwrap(),
            ),
            reads: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            commands: Mutex::new(Vec::new()),
            reject_sudo: false,
            native_entry: Some(QQ_MAIN_NATIVE),
            read_gate: None,
            read_entered: Notify::new(),
            write_gate: None,
            write_entered: Notify::new(),
        }
    }
}

#[async_trait]
impl Host for EntryHost {
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
        "entry-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.read_gate {
            self.read_entered.notify_one();
            gate.acquire().await.unwrap().forget();
        }
        Ok(self.package.lock().unwrap().clone().into())
    }
    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        assert_ne!(path.as_posix(), "/opt/QQ/resources/app/package.json");
        self.writes.fetch_add(1, Ordering::SeqCst);
        *self.package.lock().unwrap() = bytes.into();
        Ok(())
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        assert_eq!(command.program, "sh");
        if !command.elevated {
            assert_eq!(command.args[0], "-c");
            assert_eq!(command.args[1], RESOLVE_NATIVE_QQ_ENTRY);
            assert_eq!(command.args[2], "ncd-qq-native-entry");
            assert!(command.args[3].ends_with("/opt/QQ/resources/app"));
            assert!(command.stdin.is_none());
            self.commands.lock().unwrap().push(command);
            return Ok(CommandOutput {
                exit_code: Some(if self.native_entry.is_some() { 0 } else { 1 }),
                stdout: self.native_entry.unwrap_or_default().to_string(),
                stderr: String::new(),
            });
        }
        let input = command.stdin.as_ref().unwrap();
        let package = self.package.lock().unwrap().clone();
        assert_eq!(
            command.args,
            vec![
                "-c".to_string(),
                REPLACE_SYSTEM_QQ_ENTRY.to_string(),
                "ncd-qq-entry".to_string(),
                "/opt/QQ/resources/app/package.json".to_string(),
                input.len().to_string(),
                hex::encode(Sha256::digest(input)),
                hex::encode(Sha256::digest(package)),
            ]
        );
        if let Some(gate) = &self.write_gate {
            self.write_entered.notify_one();
            gate.acquire().await.unwrap().forget();
        }
        if !self.reject_sudo {
            *self.package.lock().unwrap() = command.stdin.clone().unwrap();
        }
        self.commands.lock().unwrap().push(command);
        Ok(CommandOutput {
            exit_code: Some(if self.reject_sudo { 1 } else { 0 }),
            stdout: String::new(),
            stderr: String::new(),
        })
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        panic!("idempotent read needs no exists RPC")
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("must not change installation directories")
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

#[tokio::test]
async fn already_native_system_package_only_reads_even_without_write_permission() {
    for main in [
        QQ_MAIN_NATIVE,
        "app_launcher/index.js",
        QQ_MAIN_NATIVE_ASAR,
        "application.asar/app_launcher/index.js",
    ] {
        let mut host = EntryHost::new(main);
        host.reject_sudo = true;
        set_remote_qq_package_main(&host, &HostPath::from_posix("/"), QQ_MAIN_NATIVE)
            .await
            .unwrap();
        assert_eq!(host.reads.load(Ordering::SeqCst), 1);
        assert_eq!(host.writes.load(Ordering::SeqCst), 0);
        assert!(host.commands.lock().unwrap().is_empty());
        let package: serde_json::Value =
            serde_json::from_slice(&host.package.lock().unwrap()).unwrap();
        assert_eq!(package["main"], main);
    }
}

#[tokio::test]
async fn system_transition_uses_authorized_elevation_and_preserves_other_fields() {
    let host = EntryHost::new(QQ_MAIN_NAPCAT_INJECT);
    set_remote_qq_package_main(&host, &HostPath::from_posix("/"), QQ_MAIN_NATIVE)
        .await
        .unwrap();
    assert_eq!(host.commands.lock().unwrap().len(), 2);
    assert_eq!(host.writes.load(Ordering::SeqCst), 0);
    let package: serde_json::Value = serde_json::from_slice(&host.package.lock().unwrap()).unwrap();
    assert_eq!(package["main"], QQ_MAIN_NATIVE);
    assert_eq!(package["keep"]["value"], 42);
    assert_eq!(host.reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn rootless_transition_stays_unprivileged_and_sudo_failure_is_not_success() {
    let host = EntryHost::new(QQ_MAIN_NAPCAT_INJECT);
    set_remote_qq_package_main(
        &host,
        &HostPath::from_posix("/home/a 'quoted/Napcat"),
        QQ_MAIN_NATIVE,
    )
    .await
    .unwrap();
    assert_eq!(host.writes.load(Ordering::SeqCst), 1);
    assert_eq!(host.commands.lock().unwrap().len(), 1);
    let mut denied = EntryHost::new(QQ_MAIN_NAPCAT_INJECT);
    denied.reject_sudo = true;
    assert!(
        set_remote_qq_package_main(&denied, &HostPath::from_posix("/"), QQ_MAIN_NATIVE)
            .await
            .is_err()
    );
    let package: serde_json::Value =
        serde_json::from_slice(&denied.package.lock().unwrap()).unwrap();
    assert_eq!(package["main"], QQ_MAIN_NAPCAT_INJECT);
}

#[tokio::test]
async fn native_transition_uses_detected_asar_and_refuses_unknown_layout() {
    let mut asar = EntryHost::new(QQ_MAIN_NAPCAT_INJECT);
    asar.native_entry = Some(QQ_MAIN_NATIVE_ASAR);
    set_remote_qq_package_main(&asar, &HostPath::from_posix("/"), QQ_MAIN_NATIVE)
        .await
        .unwrap();
    let package: serde_json::Value = serde_json::from_slice(&asar.package.lock().unwrap()).unwrap();
    assert_eq!(package["main"], QQ_MAIN_NATIVE_ASAR);
    let mut missing = EntryHost::new(QQ_MAIN_NAPCAT_INJECT);
    missing.native_entry = None;
    assert!(
        set_remote_qq_package_main(&missing, &HostPath::from_posix("/"), QQ_MAIN_NATIVE)
            .await
            .is_err()
    );
    assert_eq!(missing.commands.lock().unwrap().len(), 1);
    assert_eq!(missing.writes.load(Ordering::SeqCst), 0);
    let package: serde_json::Value =
        serde_json::from_slice(&missing.package.lock().unwrap()).unwrap();
    assert_eq!(package["main"], QQ_MAIN_NAPCAT_INJECT);
}

#[cfg(unix)]
#[test]
fn native_layout_probe_prefers_regular_asar_and_rejects_virtual_paths() {
    use std::process::Command;

    let directory = tempfile::tempdir().unwrap();
    let launcher = directory.path().join("app_launcher");
    std::fs::create_dir(&launcher).unwrap();
    let probe = || {
        Command::new("sh")
            .args(["-c", RESOLVE_NATIVE_QQ_ENTRY, "ncd-qq-native-entry"])
            .arg(directory.path())
            .output()
            .unwrap()
    };
    assert!(!probe().status.success());
    std::fs::write(launcher.join("index.js"), b"legacy-wrapper").unwrap();
    assert_eq!(probe().stdout, QQ_MAIN_NATIVE.as_bytes());
    let archive = directory.path().join("application.asar");
    std::fs::write(&archive, b"archive-file-fixture").unwrap();
    assert_eq!(probe().stdout, QQ_MAIN_NATIVE_ASAR.as_bytes());
    std::fs::remove_file(&archive).unwrap();
    std::fs::create_dir_all(archive.join("app_launcher")).unwrap();
    std::fs::write(archive.join("app_launcher/index.js"), b"not-an-archive").unwrap();
    assert_eq!(probe().stdout, QQ_MAIN_NATIVE.as_bytes());
}

#[tokio::test]
async fn entry_read_cancellation_never_writes_but_submitted_atomic_write_is_completed() {
    let mut host = EntryHost::new(QQ_MAIN_NAPCAT_INJECT);
    host.read_gate = Some(Arc::new(Semaphore::new(0)));
    let cancel = CancellationToken::new();
    let base = HostPath::from_posix("/");
    let change = set_remote_qq_package_main_cancellable(&host, &base, QQ_MAIN_NATIVE, &cancel);
    tokio::pin!(change);
    tokio::select! {
        result = &mut change => panic!("read should wait: {result:?}"),
        _ = host.read_entered.notified() => {}
    }
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), change)
            .await
            .unwrap()
            .is_err()
    );
    assert!(host.commands.lock().unwrap().is_empty());
    assert_eq!(host.writes.load(Ordering::SeqCst), 0);

    let mut host = EntryHost::new(QQ_MAIN_NAPCAT_INJECT);
    let release = Arc::new(Semaphore::new(0));
    host.write_gate = Some(release.clone());
    let cancel = CancellationToken::new();
    let change = set_remote_qq_package_main_cancellable(&host, &base, QQ_MAIN_NATIVE, &cancel);
    tokio::pin!(change);
    tokio::select! {
        result = &mut change => panic!("atomic write should wait: {result:?}"),
        _ = host.write_entered.notified() => {}
    }
    cancel.cancel();
    tokio::select! {
        biased;
        result = &mut change => panic!("submitted write must finish: {result:?}"),
        _ = std::future::ready(()) => {}
    }
    release.add_permits(1);
    assert!(change.await.is_err());
    let package: serde_json::Value = serde_json::from_slice(&host.package.lock().unwrap()).unwrap();
    assert_eq!(package["main"], QQ_MAIN_NATIVE);
}

#[cfg(unix)]
#[test]
fn atomic_system_entry_preserves_original_on_truncated_or_stale_input() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("package.json");
    let original = br#"{"main":"./loadNapCat.js"}"#;
    let desired = br#"{"main":"./app_launcher/index.js"}"#;
    std::fs::write(&target, original).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    let run = |input: &[u8], original_hash: String| {
        let mut child = Command::new("sh")
            .args(["-c", REPLACE_SYSTEM_QQ_ENTRY, "ncd-qq-entry"])
            .arg(&target)
            .arg(desired.len().to_string())
            .arg(hex::encode(Sha256::digest(desired)))
            .arg(original_hash)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        // A stale original may reject before consuming stdin.
        let _ = child.stdin.take().unwrap().write_all(input);
        child.wait().unwrap().success()
    };
    let original_hash = hex::encode(Sha256::digest(original));
    assert!(!run(&desired[..desired.len() - 1], original_hash.clone()));
    assert_eq!(std::fs::read(&target).unwrap(), original);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    assert!(!run(desired, hex::encode(Sha256::digest(b"stale"))));
    assert_eq!(std::fs::read(&target).unwrap(), original);
    let mut prefixed = b"unused-sudo-input\n".to_vec();
    prefixed.extend_from_slice(desired);
    assert!(run(&prefixed, original_hash));
    assert_eq!(std::fs::read(&target).unwrap(), desired);
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
