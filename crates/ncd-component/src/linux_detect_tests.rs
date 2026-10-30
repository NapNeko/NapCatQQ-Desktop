use super::QQComponent;
use crate::{ActionError, Component, SnowLumaComponent};
use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostCommand, HostError, HostPath,
    HostProcess, HostShell, Locality, Os, shell::BashShell,
};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

enum PackageReply {
    Json(&'static [u8]),
    Missing,
    Denied,
    Disconnected,
}

struct DetectHost {
    package: PackageReply,
    entry_exists: bool,
    reads: AtomicUsize,
    exists: AtomicUsize,
}

impl DetectHost {
    fn new(package: PackageReply) -> Self {
        Self {
            package,
            entry_exists: true,
            reads: AtomicUsize::new(0),
            exists: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Host for DetectHost {
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
        "linux-detect-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        assert!(path.as_posix().ends_with("/package.json"));
        self.reads.fetch_add(1, Ordering::SeqCst);
        match self.package {
            PackageReply::Json(value) => Ok(Bytes::from_static(value)),
            PackageReply::Missing => Err(HostError::PathNotFound { path: path.clone() }),
            PackageReply::Denied => Err(HostError::PermissionDenied {
                path: path.clone(),
                operation: "read",
            }),
            PackageReply::Disconnected => Err(HostError::RemoteDisconnected {
                reason: "fixture transport unavailable".into(),
            }),
        }
    }
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        assert!(
            path.as_posix().ends_with("/index.mjs")
                || path.as_posix().ends_with("/opt/QQ/qq")
                || path.as_posix().ends_with("/client")
                || path.as_posix().ends_with("/native"),
            "unexpected executable identity check"
        );
        self.exists.fetch_add(1, Ordering::SeqCst);
        Ok(self.entry_exists)
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        unreachable!()
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!()
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
    async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
        unreachable!()
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        unreachable!()
    }
}

fn qq() -> QQComponent {
    QQComponent::default_v3_2_25(HostPath::from_posix("/home/test/Napcat"))
}

fn snowluma() -> SnowLumaComponent {
    SnowLumaComponent::new(
        HostPath::from_posix("/home/test/sl-workspace"),
        "https://example.test/SnowLuma.tar.gz",
    )
}

#[tokio::test]
async fn qq_linux_detect_reads_package_once_and_only_missing_is_absent() {
    let host = DetectHost::new(PackageReply::Json(br#"{"version":"3.2.25"}"#));
    assert_eq!(qq().detect(&host).await.unwrap().unwrap().version, "3.2.25");
    assert_eq!(host.reads.load(Ordering::SeqCst), 1);
    assert_eq!(host.exists.load(Ordering::SeqCst), 1);
    let missing = DetectHost::new(PackageReply::Missing);
    assert!(qq().detect(&missing).await.unwrap().is_none());
    let mut without_binary = DetectHost::new(PackageReply::Json(br#"{"version":"3.2.25"}"#));
    without_binary.entry_exists = false;
    assert!(qq().detect(&without_binary).await.unwrap().is_none());
    assert_eq!(without_binary.reads.load(Ordering::SeqCst), 0);
    for reply in [PackageReply::Denied, PackageReply::Disconnected] {
        let host = DetectHost::new(reply);
        assert!(matches!(
            qq().detect(&host).await,
            Err(ActionError::Host(_))
        ));
        assert_eq!(host.exists.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn snowluma_linux_detect_checks_entry_and_reads_package_once() {
    let host = DetectHost::new(PackageReply::Json(br#"{"version":"1.2.3"}"#));
    assert_eq!(
        snowluma().detect(&host).await.unwrap().unwrap().version,
        "1.2.3"
    );
    assert_eq!(host.reads.load(Ordering::SeqCst), 1);
    assert_eq!(host.exists.load(Ordering::SeqCst), 3);
    let mut absent = DetectHost::new(PackageReply::Denied);
    absent.entry_exists = false;
    assert!(snowluma().detect(&absent).await.unwrap().is_none());
    assert_eq!(absent.reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn snowluma_linux_detect_keeps_legacy_package_fallback_but_propagates_host_errors() {
    for reply in [
        PackageReply::Missing,
        PackageReply::Json(b"{}"),
        PackageReply::Json(b"old-package-format"),
    ] {
        let host = DetectHost::new(reply);
        assert_eq!(
            snowluma().detect(&host).await.unwrap().unwrap().version,
            "installed"
        );
        assert_eq!(host.reads.load(Ordering::SeqCst), 1);
        assert_eq!(host.exists.load(Ordering::SeqCst), 3);
    }
    for reply in [PackageReply::Denied, PackageReply::Disconnected] {
        let host = DetectHost::new(reply);
        assert!(matches!(
            snowluma().detect(&host).await,
            Err(ActionError::Host(_))
        ));
        assert_eq!(host.reads.load(Ordering::SeqCst), 1);
        assert_eq!(host.exists.load(Ordering::SeqCst), 3);
    }
}
