use super::*;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostProcess, HostShell,
    Locality, Os,
};
use tokio::sync::Barrier;

const EULA: &str = "# Fixture EULA\nPublic fixture terms.\n";
const PRIVACY: &str = "# Fixture privacy\nPublic fixture privacy policy.\n";

#[derive(Clone)]
enum Read {
    Data(Vec<u8>),
    Missing,
    Denied,
    Timeout,
}

struct AgreementHost {
    files: HashMap<String, Read>,
    reads: Mutex<Vec<String>>,
    gated: HashSet<String>,
    barrier: Option<Barrier>,
}

impl AgreementHost {
    fn new(paths: &SnowLumaRemotePaths) -> Self {
        Self {
            files: HashMap::from([
                (
                    format!("{}/EULA.md", paths.snowluma_dir),
                    Read::Data(EULA.as_bytes().to_vec()),
                ),
                (
                    format!("{}/PRIVACY.md", paths.snowluma_dir),
                    Read::Data(PRIVACY.as_bytes().to_vec()),
                ),
            ]),
            reads: Mutex::new(Vec::new()),
            gated: HashSet::new(),
            barrier: None,
        }
    }
}

#[async_trait]
impl Host for AgreementHost {
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
        "agreement-fixture"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        let path_text = path.as_posix();
        self.reads.lock().unwrap().push(path_text.to_owned());
        if self.gated.contains(path_text) {
            self.barrier.as_ref().unwrap().wait().await;
        }
        match self.files.get(path_text).cloned().unwrap_or(Read::Missing) {
            Read::Data(data) => Ok(Bytes::from(data)),
            Read::Missing => Err(HostError::PathNotFound { path: path.clone() }),
            Read::Denied => Err(HostError::PermissionDenied {
                path: path.clone(),
                operation: "read",
            }),
            Read::Timeout => Err(HostError::Timeout {
                operation: "agreement_read",
            }),
        }
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        panic!("preflight must only read")
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        panic!("preflight must read known files")
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("preflight must only read")
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("preflight must only read")
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("preflight must only read")
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        panic!("preflight must not add existence round trips")
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        panic!("preflight must only read")
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        panic!("preflight must only read")
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        panic!("preflight must only read")
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("preflight must not start anything")
    }
    async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
        panic!("agreement preflight must use file reads")
    }
}

fn paths() -> SnowLumaRemotePaths {
    SnowLumaRemotePaths::from_workspace_and_dir("/fixture/workspace", "/fixture/runtime")
}

#[tokio::test]
async fn remote_agreement_eula_privacy_and_consent_reads_start_concurrently() {
    let paths = paths();
    let mut host = AgreementHost::new(&paths);
    let version = payload_from_agreement_texts(EULA, PRIVACY, None)
        .unwrap()
        .version;
    let consent = format!("{}/consent.json", paths.config_dir);
    host.files.insert(
        consent.clone(),
        Read::Data(consent_record_json(&version).unwrap()),
    );
    host.gated = HashSet::from([
        format!("{}/EULA.md", paths.snowluma_dir),
        format!("{}/PRIVACY.md", paths.snowluma_dir),
        consent,
    ]);
    host.barrier = Some(Barrier::new(3));
    let payload = tokio::time::timeout(
        Duration::from_secs(2),
        load_payload_from_remote(&host, &paths),
    )
    .await
    .expect("all three independent reads must reach the barrier concurrently")
    .unwrap()
    .unwrap();
    assert!(!payload.consent_required);
    assert_eq!(payload.version, version);
    assert_eq!(host.reads.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn missing_primary_documents_use_dist_and_missing_consent_requires_acceptance() {
    let paths = paths();
    let mut host = AgreementHost::new(&paths);
    host.files.clear();
    host.files.insert(
        format!("{}/dist/EULA.md", paths.snowluma_dir),
        Read::Data(EULA.as_bytes().to_vec()),
    );
    host.files.insert(
        format!("{}/dist/PRIVACY.md", paths.snowluma_dir),
        Read::Data(PRIVACY.as_bytes().to_vec()),
    );
    let payload = load_payload_from_remote(&host, &paths)
        .await
        .unwrap()
        .unwrap();
    assert!(payload.consent_required);
    assert_eq!(
        payload.version,
        payload_from_agreement_texts(EULA, PRIVACY, None)
            .unwrap()
            .version
    );
    let reads = host.reads.lock().unwrap();
    assert!(reads.contains(&format!("{}/dist/EULA.md", paths.snowluma_dir)));
    assert!(reads.contains(&format!("{}/dist/PRIVACY.md", paths.snowluma_dir)));
    assert!(reads.contains(&format!("{}/consent.json", paths.config_dir)));
}

#[tokio::test]
async fn remote_agreement_permission_and_timeout_never_become_missing_or_optional_consent() {
    let paths = paths();
    for outcome in [Read::Denied, Read::Timeout] {
        for file in ["EULA.md", "PRIVACY.md", "consent.json"] {
            let mut host = AgreementHost::new(&paths);
            let directory = if file == "consent.json" {
                &paths.config_dir
            } else {
                &paths.snowluma_dir
            };
            host.files
                .insert(format!("{directory}/{file}"), outcome.clone());
            host.files.insert(
                format!("{}/dist/{file}", paths.snowluma_dir),
                Read::Data(b"fallback must not be used".to_vec()),
            );
            assert!(
                load_payload_from_remote(&host, &paths).await.is_err(),
                "must preserve failure for {file}"
            );
            assert!(
                !host
                    .reads
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|path| path.contains("/dist/"))
            );
        }
    }
}

#[tokio::test]
async fn invalid_utf8_in_an_agreement_cannot_fall_back_to_dist_or_skip_consent() {
    let paths = paths();
    for file in ["EULA.md", "PRIVACY.md", "consent.json"] {
        let mut host = AgreementHost::new(&paths);
        let directory = if file == "consent.json" {
            &paths.config_dir
        } else {
            &paths.snowluma_dir
        };
        host.files
            .insert(format!("{directory}/{file}"), Read::Data(vec![0xff, 0xfe]));
        assert!(load_payload_from_remote(&host, &paths).await.is_err());
        assert!(
            !host
                .reads
                .lock()
                .unwrap()
                .iter()
                .any(|path| path.contains("/dist/"))
        );
    }
}

#[tokio::test]
async fn truly_missing_remote_documents_remain_optional() {
    let paths = paths();
    let mut host = AgreementHost::new(&paths);
    host.files.clear();
    assert!(
        load_payload_from_remote(&host, &paths)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        read_remote_agreement(&host, &paths.snowluma_dir, "EULA.md")
            .await
            .unwrap(),
        ""
    );
}
