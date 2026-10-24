use super::*;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use ncd_backend_snowluma::remote_snowluma::RemoteSlMetricsInjector;
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell, Locality,
};
use ncd_test_support::BotConfigBuilder;

use crate::metrics::RuntimeRemoteMetricsInjector;

#[derive(Default)]
struct AssetHost {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    commands: Mutex<Vec<HostCommand>>,
    checksum_exit: AtomicI32,
    reads: AtomicUsize,
    writes: AtomicUsize,
    mkdirs: AtomicUsize,
}

impl AssetHost {
    fn reset_counts(&self) {
        self.commands.lock().unwrap().clear();
        self.reads.store(0, Ordering::SeqCst);
        self.writes.store(0, Ordering::SeqCst);
        self.mkdirs.store(0, Ordering::SeqCst);
    }
}

#[async_trait]
impl Host for AssetHost {
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
        "metrics-host"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }

    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.files
            .lock()
            .unwrap()
            .get(path.as_posix())
            .cloned()
            .map(Bytes::from)
            .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
    }
    async fn write_file(&self, path: &HostPath, body: &[u8]) -> Result<(), HostError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.files
            .lock()
            .unwrap()
            .insert(path.as_posix().into(), body.into());
        Ok(())
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        self.mkdirs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn rename(&self, source: &HostPath, destination: &HostPath) -> Result<(), HostError> {
        let mut files = self.files.lock().unwrap();
        let body = files.remove(source.as_posix()).unwrap();
        files.insert(destination.as_posix().into(), body);
        Ok(())
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        assert_eq!(command.program, "python3");
        assert_eq!(command.args, ["-I", "-B", "-c", SYNC_ASSETS]);
        let request: Request = serde_json::from_slice(command.stdin.as_ref().unwrap()).unwrap();
        let mut files = self.files.lock().unwrap();
        let exit = self.checksum_exit.load(Ordering::SeqCst);
        let mut changed = 0;
        if exit == 0 {
            for asset in request.assets {
                let path = format!(
                    "{}/ncd-watch/metrics/{}",
                    request.home.trim_end_matches('/'),
                    asset.path
                );
                let body = asset.body.into_bytes();
                assert_eq!(hex::encode(Sha256::digest(&body)), asset.sha256);
                if files.get(&path) != Some(&body) {
                    files.insert(path, body);
                    self.writes.fetch_add(1, Ordering::SeqCst);
                    changed += 1;
                }
            }
        }
        self.commands.lock().unwrap().push(command);
        Ok(CommandOutput {
            exit_code: Some(exit),
            stdout: format!(r#"{{"v":1,"checked":3,"changed":{changed}}}"#),
            stderr: String::new(),
        })
    }
    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
        self.files.lock().unwrap().remove(path.as_posix());
        Ok(())
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        panic!("unexpected exists")
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        panic!("unexpected listing")
    }
    async fn which(&self, _: &str) -> Result<Option<String>, HostError> {
        panic!("unexpected which")
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("unexpected spawn")
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("unexpected remove")
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        panic!("unexpected upload")
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        panic!("unexpected download")
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        panic!("unexpected extraction")
    }
}

const HOME: &str = "/home/test user/'quoted;$(id)";

#[tokio::test]
async fn unchanged_snowluma_prepare_uses_one_ssh_without_sftp_or_writes() {
    let root = tempfile::tempdir().unwrap();
    let injector = RuntimeRemoteMetricsInjector::new(root.path());
    let host = AssetHost::default();
    let config = BotConfigBuilder::new().qq_id(10001).build();
    let first = injector
        .prepare(&host, HOME, "10001", &config)
        .await
        .unwrap();
    host.reset_counts();

    let second = injector
        .prepare(&host, HOME, "10001", &config)
        .await
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(host.commands.lock().unwrap().len(), 1);
    assert_eq!(host.reads.load(Ordering::SeqCst), 0);
    assert_eq!(host.writes.load(Ordering::SeqCst), 0);
    assert_eq!(host.mkdirs.load(Ordering::SeqCst), 0);
    let commands = host.commands.lock().unwrap();
    assert!(!commands[0].args[3].contains(HOME));
    let request: Request = serde_json::from_slice(commands[0].stdin.as_ref().unwrap()).unwrap();
    assert_eq!(request.home, HOME);
    assert_eq!(
        request
            .assets
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>(),
        [
            REMOTE_PROBE_NAME,
            "10001/nodes.json",
            "snowluma-shared/10001.json"
        ]
    );
}

#[tokio::test]
async fn changed_preferences_or_account_do_not_reuse_old_registration() {
    let root = tempfile::tempdir().unwrap();
    let injector = RuntimeRemoteMetricsInjector::new(root.path());
    let host = AssetHost::default();
    let config = BotConfigBuilder::new().qq_id(10001).build();
    injector
        .prepare(&host, HOME, "10001", &config)
        .await
        .unwrap();
    let mut prefs = injector.prefs();
    prefs.enabled = !prefs.enabled;
    prefs.interval_ms += 1000;
    host.reset_counts();
    assert!(
        current_snowluma_assets(&host, HOME, "10001", &config, &prefs, root.path())
            .await
            .is_some()
    );
    assert_eq!(host.writes.load(Ordering::SeqCst), 1);
    let registration_path = format!("{HOME}/ncd-watch/metrics/snowluma-shared/10001.json");
    let registration: serde_json::Value =
        serde_json::from_slice(host.files.lock().unwrap().get(&registration_path).unwrap())
            .unwrap();
    assert_eq!(registration["enabled"], prefs.enabled);
    assert_eq!(registration["intervalMs"], prefs.interval_ms);
    let other = BotConfigBuilder::new().qq_id(20002).build();
    assert!(
        current_snowluma_assets(&host, HOME, "10001", &other, &injector.prefs(), root.path())
            .await
            .is_some()
    );
    let registration: serde_json::Value =
        serde_json::from_slice(host.files.lock().unwrap().get(&registration_path).unwrap())
            .unwrap();
    assert_eq!(registration["accountId"], "20002");
}

#[tokio::test]
async fn missing_or_modified_remote_assets_are_repaired_before_reuse() {
    let root = tempfile::tempdir().unwrap();
    let injector = RuntimeRemoteMetricsInjector::new(root.path());
    let host = AssetHost::default();
    let config = BotConfigBuilder::new().qq_id(10001).build();
    injector
        .prepare(&host, HOME, "10001", &config)
        .await
        .unwrap();
    let original = host.files.lock().unwrap().clone();
    for path in original.keys() {
        for missing in [true, false] {
            {
                let mut files = host.files.lock().unwrap();
                if missing {
                    files.remove(path);
                } else {
                    files.insert(path.clone(), b"outdated".to_vec());
                }
            }
            host.reset_counts();
            injector
                .prepare(&host, HOME, "10001", &config)
                .await
                .unwrap();
            assert_eq!(host.writes.load(Ordering::SeqCst), 1);
            assert_eq!(*host.files.lock().unwrap(), original);
        }
    }
}

#[tokio::test]
async fn permission_or_failed_batch_cannot_report_ready_or_fall_back_to_sftp() {
    let root = tempfile::tempdir().unwrap();
    let injector = RuntimeRemoteMetricsInjector::new(root.path());
    let host = AssetHost::default();
    let config = BotConfigBuilder::new().qq_id(10001).build();
    injector
        .prepare(&host, HOME, "10001", &config)
        .await
        .unwrap();
    for exit in [3, 127] {
        // 即使 stdout 形状正确，失败退出码也不能发布准备成功或退回逐项 SFTP。
        host.checksum_exit.store(exit, Ordering::SeqCst);
        assert!(
            current_snowluma_assets(
                &host,
                HOME,
                "10001",
                &config,
                &injector.prefs(),
                root.path()
            )
            .await
            .is_none()
        );
        host.reset_counts();
        assert!(
            injector
                .prepare(&host, HOME, "10001", &config)
                .await
                .is_none()
        );
        assert_eq!(host.reads.load(Ordering::SeqCst), 0);
        assert_eq!(host.writes.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn replacement_host_with_the_same_id_must_prepare_its_own_assets() {
    let root = tempfile::tempdir().unwrap();
    let injector = RuntimeRemoteMetricsInjector::new(root.path());
    let previous = AssetHost::default();
    let config = BotConfigBuilder::new().qq_id(10001).build();
    let before = injector
        .prepare(&previous, HOME, "10001", &config)
        .await
        .unwrap();
    let replacement = AssetHost::default();
    let after = injector
        .prepare(&replacement, HOME, "10001", &config)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert!(replacement.writes.load(Ordering::SeqCst) > 0);
    assert_eq!(
        *previous.files.lock().unwrap(),
        *replacement.files.lock().unwrap()
    );
}
