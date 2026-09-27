//! 会话管理层的测试：替身主机把终端后端交给测试，测试扮演终端那头

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use ncd_domain::{
    LocalShellOption, TerminalEvent, TerminalFeatures, TerminalHostOs, TerminalOpenRequest,
    TerminalStatus, TerminalTarget,
};
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostCommand, HostError, HostPath,
    HostProcess, HostShell, Locality, Os, PackageManager, PtyBackend, PtyExit, PtyInput,
    PtyProgram, PtyRequest, PtySession, PtySize, pty_channel_pair,
};
use tokio::sync::mpsc;

use super::{TerminalError, TerminalLaunchPlan, TerminalManager, TerminalPlanner, TerminalSink};

struct FakeHost {
    backends: mpsc::UnboundedSender<PtyBackend>,
}

#[async_trait]
impl Host for FakeHost {
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
        "remote:fake"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    fn pkg_manager(&self) -> Option<&dyn PackageManager> {
        None
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        Err(HostError::PathNotFound { path: path.clone() })
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        Ok(())
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Ok(Vec::new())
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        Ok(false)
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        Ok(())
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        Ok(())
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        Err(HostError::Unsupported { operation: "spawn" })
    }
    async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
        Err(HostError::Unsupported {
            operation: "run_to_string",
        })
    }
    async fn open_pty(&self, _req: PtyRequest) -> Result<PtySession, HostError> {
        let (session, backend) = pty_channel_pair();
        let _ = self.backends.send(backend);
        Ok(session)
    }
}

struct FakePlanner {
    host: Arc<FakeHost>,
    password: Option<String>,
    fail: AtomicBool,
}

#[async_trait]
impl TerminalPlanner for FakePlanner {
    async fn plan(&self, _request: &TerminalOpenRequest) -> Result<TerminalLaunchPlan, TerminalError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(TerminalError::Plan("连不上主机：测试".into()));
        }
        Ok(TerminalLaunchPlan {
            host: Arc::clone(&self.host) as Arc<dyn Host>,
            host_id: "remote:fake".into(),
            host_label: "fake".into(),
            host_os: TerminalHostOs::Linux,
            title: "fake".into(),
            program: PtyProgram::LoginShell,
            cwd: None,
            env: BTreeMap::new(),
            cwd_display: Some("/home/u".into()),
            shell: None,
            features: TerminalFeatures::default(),
            snippets: Vec::new(),
            banner: vec!["banner line".into()],
        })
    }

    fn sudo_password(&self, host_id: &str) -> Option<String> {
        if host_id == "remote:fake" { self.password.clone() } else { None }
    }

    fn local_shells(&self) -> Vec<LocalShellOption> {
        Vec::new()
    }
}

struct RecordingSink {
    bytes: Mutex<Vec<u8>>,
    events: Mutex<Vec<TerminalEvent>>,
}

impl RecordingSink {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            bytes: Mutex::new(Vec::new()),
            events: Mutex::new(Vec::new()),
        })
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes.lock().unwrap()).into_owned()
    }

    fn events(&self) -> Vec<TerminalEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl TerminalSink for RecordingSink {
    fn output(&self, bytes: &[u8]) -> bool {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        true
    }
    fn event(&self, event: &TerminalEvent) -> bool {
        self.events.lock().unwrap().push(event.clone());
        true
    }
}

fn request() -> TerminalOpenRequest {
    TerminalOpenRequest {
        target: TerminalTarget::Server {
            server_id: "fake".into(),
        },
        cols: 80,
        rows: 24,
        shell: None,
    }
}

fn fixture(password: Option<&str>) -> (TerminalManager, mpsc::UnboundedReceiver<PtyBackend>, Arc<FakePlanner>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let planner = Arc::new(FakePlanner {
        host: Arc::new(FakeHost { backends: tx }),
        password: password.map(str::to_string),
        fail: AtomicBool::new(false),
    });
    (TerminalManager::new(planner.clone()), rx, planner)
}

async fn eventually(what: &str, check: impl Fn() -> bool) {
    for _ in 0..300 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn attach_gets_banner_then_live_output() {
    let (manager, mut backends, _) = fixture(None);
    let info = manager.open(request()).await.unwrap();
    assert_eq!(info.status, TerminalStatus::Running);
    let backend = backends.recv().await.unwrap();
    let sink = RecordingSink::new();
    manager.attach(info.id.as_str(), sink.clone()).unwrap();
    assert!(sink.text().contains("banner line"));

    backend.output.send(Bytes::from_static(b"hello")).await.unwrap();
    eventually("live output", || sink.text().contains("hello")).await;
}

#[tokio::test]
async fn reattach_replays_history_and_takes_over() {
    let (manager, mut backends, _) = fixture(None);
    let info = manager.open(request()).await.unwrap();
    let backend = backends.recv().await.unwrap();
    let first = RecordingSink::new();
    manager.attach(info.id.as_str(), first.clone()).unwrap();
    backend.output.send(Bytes::from_static(b"before\r\n")).await.unwrap();
    eventually("first output", || first.text().contains("before")).await;

    let second = RecordingSink::new();
    manager.attach(info.id.as_str(), second.clone()).unwrap();
    assert!(second.text().contains("banner line"));
    assert!(second.text().contains("before"));

    backend.output.send(Bytes::from_static(b"after")).await.unwrap();
    eventually("output on the new sink", || second.text().contains("after")).await;
    assert!(!first.text().contains("after"));
}

#[tokio::test]
async fn input_and_resize_reach_the_backend() {
    let (manager, mut backends, _) = fixture(None);
    let info = manager.open(request()).await.unwrap();
    let mut backend = backends.recv().await.unwrap();
    manager.write(info.id.as_str(), "ls\r").unwrap();
    manager.resize(info.id.as_str(), 100, 30).unwrap();
    match backend.input.recv().await {
        Some(PtyInput::Write(data)) => assert_eq!(&data[..], b"ls\r"),
        other => panic!("unexpected {other:?}"),
    }
    match backend.input.recv().await {
        Some(PtyInput::Resize(size)) => assert_eq!(size, PtySize::new(100, 30)),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn exit_is_reported_and_restart_reopens_the_same_tab() {
    let (manager, mut backends, _) = fixture(None);
    let info = manager.open(request()).await.unwrap();
    let id = info.id.as_str().to_string();
    let backend = backends.recv().await.unwrap();
    let sink = RecordingSink::new();
    manager.attach(&id, sink.clone()).unwrap();

    let PtyBackend { output, exit, .. } = backend;
    drop(output);
    let _ = exit.send(PtyExit::Exited(Some(3)));
    eventually("exit line", || sink.text().contains("退出码 3")).await;
    assert!(sink.events().iter().any(|e| matches!(
        e,
        TerminalEvent::Status { status: TerminalStatus::Exited { code: Some(3) } }
    )));
    assert!(matches!(manager.write(&id, "x"), Err(TerminalError::NotRunning)));

    let reopened = manager.restart(&id).await.unwrap();
    assert_eq!(reopened.status, TerminalStatus::Running);
    assert_eq!(reopened.id, info.id);
    let fresh = backends.recv().await.unwrap();
    assert!(sink.text().contains("重新打开"));
    assert!(sink.events().iter().any(|e| matches!(e, TerminalEvent::Info { .. })));
    manager.write(&id, "pwd\r").unwrap();
    drop(fresh);
}

#[tokio::test]
async fn failed_restart_leaves_a_failed_tab() {
    let (manager, mut backends, planner) = fixture(None);
    let info = manager.open(request()).await.unwrap();
    let id = info.id.as_str().to_string();
    let backend = backends.recv().await.unwrap();
    let sink = RecordingSink::new();
    manager.attach(&id, sink.clone()).unwrap();
    let PtyBackend { output, exit, .. } = backend;
    drop(output);
    let _ = exit.send(PtyExit::Disconnected("网络断了".into()));
    eventually("disconnect line", || sink.text().contains("按回车重连")).await;

    planner.fail.store(true, Ordering::SeqCst);
    assert!(manager.restart(&id).await.is_err());
    assert!(sink.text().contains("没开起来"));
    let status = manager.list().first().map(|i| i.status.clone());
    assert!(matches!(status, Some(TerminalStatus::Failed { .. })));
}

#[tokio::test]
async fn close_tells_the_backend_and_forgets_the_session() {
    let (manager, mut backends, _) = fixture(None);
    let info = manager.open(request()).await.unwrap();
    let mut backend = backends.recv().await.unwrap();
    manager.close(info.id.as_str());
    assert!(matches!(backend.input.recv().await, Some(PtyInput::Close) | None));
    assert!(manager.list().is_empty());
    assert!(matches!(manager.write(info.id.as_str(), "x"), Err(TerminalError::NotFound)));
}

#[tokio::test]
async fn sudo_fill_types_the_saved_password() {
    let (manager, mut backends, _) = fixture(Some("s3cret"));
    let info = manager.open(request()).await.unwrap();
    let mut backend = backends.recv().await.unwrap();
    manager.fill_sudo(info.id.as_str()).unwrap();
    match backend.input.recv().await {
        Some(PtyInput::Write(data)) => assert_eq!(&data[..], b"s3cret\r"),
        other => panic!("unexpected {other:?}"),
    }

    let (no_password, mut others, _) = fixture(None);
    let other = no_password.open(request()).await.unwrap();
    let _backend = others.recv().await.unwrap();
    assert!(matches!(no_password.fill_sudo(other.id.as_str()), Err(TerminalError::Invalid(_))));
}

#[tokio::test]
async fn slow_client_holds_back_reading_until_it_acks() {
    let (manager, mut backends, _) = fixture(None);
    let info = manager.open(request()).await.unwrap();
    let id = info.id.as_str().to_string();
    let backend = backends.recv().await.unwrap();
    let sink = RecordingSink::new();
    manager.attach(&id, sink.clone()).unwrap();

    // 前端一直不确认：读满高水位后不再读，输出通道塞满，发送卡住
    let chunk = Bytes::from(vec![b'x'; 16 * 1024]);
    let mut sent = 0usize;
    let mut blocked = false;
    for _ in 0..1000 {
        match tokio::time::timeout(Duration::from_millis(100), backend.output.send(chunk.clone())).await {
            Ok(Ok(())) => sent += 1,
            Ok(Err(_)) => panic!("output channel closed"),
            Err(_) => {
                blocked = true;
                break;
            }
        }
    }
    assert!(blocked, "reading never paused after {sent} chunks");
    assert!(sent >= 128, "paused too early after {sent} chunks");

    manager.ack(&id, u64::MAX);
    tokio::time::timeout(Duration::from_secs(2), backend.output.send(chunk))
        .await
        .expect("reading resumes after the ack")
        .unwrap();
}
