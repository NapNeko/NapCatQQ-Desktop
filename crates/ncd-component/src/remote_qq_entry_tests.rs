use super::*;
use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostProcess, HostShell, Locality, Os,
    shell::BashShell,
};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct EntryHost {
    package: Mutex<Vec<u8>>,
    reads: AtomicUsize,
    writes: AtomicUsize,
    commands: Mutex<Vec<HostCommand>>,
    reject_sudo: bool,
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
        Ok(self.package.lock().unwrap().clone().into())
    }
    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        assert_ne!(path.as_posix(), "/opt/QQ/resources/app/package.json");
        self.writes.fetch_add(1, Ordering::SeqCst);
        *self.package.lock().unwrap() = bytes.into();
        Ok(())
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        assert!(command.elevated);
        assert_eq!(command.program, "sh");
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
    for main in [QQ_MAIN_NATIVE, "app_launcher/index.js"] {
        let mut host = EntryHost::new(main);
        host.reject_sudo = true;
        set_remote_qq_package_main(&host, &HostPath::from_posix("/"), QQ_MAIN_NATIVE)
            .await
            .unwrap();
        assert_eq!(host.reads.load(Ordering::SeqCst), 1);
        assert_eq!(host.writes.load(Ordering::SeqCst), 0);
        assert!(host.commands.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn system_transition_uses_authorized_elevation_and_preserves_other_fields() {
    let host = EntryHost::new(QQ_MAIN_NAPCAT_INJECT);
    set_remote_qq_package_main(&host, &HostPath::from_posix("/"), QQ_MAIN_NATIVE)
        .await
        .unwrap();
    assert_eq!(host.commands.lock().unwrap().len(), 1);
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
    assert!(host.commands.lock().unwrap().is_empty());
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
