use super::*;

use std::path::Path;
use std::sync::Mutex as StdMutex;

use bytes::Bytes;
use ncd_host::remote::tunnel::{TunnelHandle, TunnelSpec};
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostProcess, HostShell,
    Locality, Os,
};
use tokio::sync::Semaphore;

use super::super::layout::RemoteSnowLumaLayout;
use crate::snowluma::error::SnowLumaWebUiError;
use crate::snowluma::webui_client::{
    AgreementsPayload, AuthState, HookProcessInfo, HookProcessStatus, OneBotInstanceInfo,
    QqPortLoginInfo,
};

#[path = "backend_qq_process_tests.rs"]
mod qq_process_tests;

#[derive(Default)]
struct MetricsHost {
    node_running: AtomicBool,
    commands: StdMutex<Vec<String>>,
    qq_start_time: StdMutex<Option<String>>,
    qq_replies: StdMutex<std::collections::VecDeque<String>>,
    qq_operations: StdMutex<Vec<(String, bool, u32, String)>>,
    qq_block: AtomicBool,
    birth_reads: AtomicU32,
}

#[async_trait]
impl Host for MetricsHost {
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
        "reconcile-metrics-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        if path.as_posix() == "/proc/42/stat" {
            let start_time = self
                .qq_start_time
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "12345".into());
            let mut fields = vec!["S"; 20];
            fields[19] = &start_time;
            Ok(Bytes::from(format!("42 (qq) {}", fields.join(" "))))
        } else if path.as_posix().ends_with("runtime.json") {
            Ok(Bytes::from_static(br#"{"webuiPort":13105}"#))
        } else if path.as_posix().ends_with(".secret") {
            Ok(Bytes::from_static(b"fixture-password"))
        } else {
            Err(HostError::PathNotFound { path: path.clone() })
        }
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        Ok(())
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Ok(vec![])
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
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("must not spawn QQ")
    }
    async fn which(&self, _: &str) -> Result<Option<String>, HostError> {
        Ok(Some("/bin/bash".into()))
    }
    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        Ok(TunnelHandle::detached(spec.local_port, 0))
    }
    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        if cmd.program == "head" {
            assert_eq!(&cmd.args[..3], ["-c", "4097", "--"]);
            assert_eq!(cmd.timeout, Some(Duration::from_secs(3)));
            self.birth_reads.fetch_add(1, Ordering::SeqCst);
            let bytes = self
                .read_file(&HostPath::from_posix(cmd.args.last().unwrap()))
                .await?;
            return Ok(CommandOutput {
                exit_code: Some(0),
                stdout: String::from_utf8_lossy(&bytes).into_owned(),
                stderr: String::new(),
            });
        }
        let script = cmd.args.last().cloned().unwrap_or_default();
        self.commands.lock().unwrap().push(script.clone());
        let mut output = CommandOutput {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        };
        if cmd
            .args
            .iter()
            .any(|arg| arg == ncd_component::LINUX_QQ_PROCESS_PROBE)
        {
            let request: serde_json::Value =
                serde_json::from_slice(cmd.stdin.as_ref().unwrap()).unwrap();
            let action = request["action"].as_str().unwrap();
            let pid = request["pid"].as_u64().unwrap() as u32;
            let birth = request["start_time"].as_str().unwrap();
            self.qq_operations.lock().unwrap().push((
                action.into(),
                cmd.elevated,
                pid,
                birth.into(),
            ));
            if self.qq_block.load(Ordering::SeqCst) {
                std::future::pending::<()>().await;
            }
            let state = self
                .qq_replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    if action == "stop" {
                        "stopped"
                    } else {
                        "verified"
                    }
                    .into()
                });
            output.stdout =
                serde_json::json!({"v":1,"pid":pid,"start_time":birth,"state":state,"uid":if cmd.elevated { 0 } else { 1001 }}).to_string();
        } else if cmd.program == "id" {
            output.stdout = "1000\n".into();
        } else if cmd
            .args
            .iter()
            .any(|arg| arg.starts_with("# Read only same-user Node identities;"))
        {
            let mode = cmd.args.get(4).map(String::as_str);
            output.stdout = if mode == Some("snapshot") {
                r#"{"state":"snapshot","snapshot":[{"pid":901,"start_time":"12345"}]}"#.into()
            } else if self.node_running.load(Ordering::SeqCst)
                && matches!(mode, Some("endpoint_scan" | "endpoint_inspect"))
            {
                r#"{"state":"present","node":{"pid":901,"uid":1000,"start_time":"12345","exe":"/opt/sl/node","cwd":"/opt/sl","entry":"/opt/sl/index.mjs","ns_pids":[901,19],"pid_namespace":"pid:[4026532569]"},"snapshot":[{"pid":901,"start_time":"12345"}],"host":"127.0.0.1","port":13105,"listeners":[{"host":"127.0.0.1","port":13105}]}"#.into()
            } else if self.node_running.load(Ordering::SeqCst) {
                r#"{"state":"present","node":{"pid":901,"uid":1000,"start_time":"12345","exe":"/opt/sl/node"},"snapshot":[{"pid":901,"start_time":"12345"}]}"#.into()
            } else {
                r#"{"state":"absent","snapshot":[]}"#.into()
            };
        } else if script.contains("node_pid=$!") {
            self.node_running.store(true, Ordering::SeqCst);
            output.stdout = "901\n".into();
        } else if script.starts_with("# ncd-snowluma-node-identity") {
            output.stdout = "901:12345\n".into();
        } else if script.contains("node_not_started") {
            output.stdout = "13105\n".into();
        } else if script.starts_with("QQ_BIN=") {
            output.stdout = "running\n".into();
        }
        Ok(output)
    }
}

struct Gates {
    entered: Semaphore,
    release: Semaphore,
    cancelled: Semaphore,
}

impl Gates {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
            cancelled: Semaphore::new(0),
        })
    }
}

struct PendingPrepare {
    gates: Arc<Gates>,
    completed: bool,
}

impl Drop for PendingPrepare {
    fn drop(&mut self) {
        if !self.completed {
            self.gates.cancelled.add_permits(1);
        }
    }
}

struct GatedInjector(Arc<Gates>);

#[async_trait]
impl RemoteSlMetricsInjector for GatedInjector {
    async fn prepare(
        &self,
        _: &dyn Host,
        _: &str,
        _: &str,
        _: &BotConfig,
    ) -> Option<BTreeMap<String, String>> {
        let mut pending = PendingPrepare {
            gates: Arc::clone(&self.0),
            completed: false,
        };
        self.0.entered.add_permits(1);
        self.0.release.acquire().await.unwrap().forget();
        pending.completed = true;
        Some(BTreeMap::from([(
            "NCD_RECONCILE_TEST".into(),
            "prepared-later".into(),
        )]))
    }
}

fn fixture() -> (Arc<RemoteSnowLumaBackend>, Arc<MetricsHost>, Arc<Gates>) {
    let host = Arc::new(MetricsHost::default());
    let gates = Gates::new();
    let bus = Arc::new(BroadcastEventBus::default());
    let tunnels = Arc::new(RemoteSnowLumaTunnelRegistry::new());
    let daemon = Arc::new(RemoteSnowLumaDaemon::from_layout(
        "server".into(),
        host.clone(),
        Arc::clone(&tunnels),
        Arc::clone(&bus),
        RemoteSnowLumaLayout {
            home: "/home/test".into(),
            paths: SnowLumaRemotePaths::from_workspace_and_dir("/opt/sl", "/opt/sl"),
            node_bin: "/opt/sl/node".into(),
            qq_bin: "/opt/QQ/qq".into(),
            qq_install_base: "/".into(),
        },
    ));
    let backend = Arc::new(RemoteSnowLumaBackend::new_with_metrics(
        "server",
        daemon,
        bus,
        tunnels,
        Arc::new(ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator::default()),
        Some(Arc::new(GatedInjector(Arc::clone(&gates)))),
    ));
    (backend, host, gates)
}

fn prepare(
    backend: &Arc<RemoteSnowLumaBackend>,
    host: &Arc<MetricsHost>,
    bot: &str,
) -> tokio::task::JoinHandle<()> {
    let backend = Arc::clone(backend);
    let host: Arc<dyn Host> = host.clone();
    let bot = BotId::new(bot);
    tokio::spawn(async move {
        let config = ncd_test_support::BotConfigBuilder::new().build();
        backend.prepare_reconcile_metrics(&bot, &config, host).await;
    })
}

async fn permit(semaphore: &Semaphore) {
    tokio::time::timeout(Duration::from_secs(1), semaphore.acquire())
        .await
        .expect("operation did not reach its expected lifecycle state")
        .unwrap()
        .forget();
}

#[tokio::test(start_paused = true)]
async fn slow_metrics_have_one_shared_budget_and_apply_env_after_the_wait_expires() {
    let (backend, host, gates) = fixture();
    let first = prepare(&backend, &host, "10001");
    let second = prepare(&backend, &host, "10002");
    permit(&gates.entered).await;
    permit(&gates.entered).await;
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::time::timeout(Duration::from_millis(1), async {
        first.await.unwrap();
        second.await.unwrap();
    })
    .await
    .expect("optional uploads exceeded the two-second reconcile budget");
    assert_eq!(
        gates.cancelled.available_permits(),
        0,
        "timeout must leave uploads running"
    );
    gates.release.add_permits(2);
    let mut tasks = backend.reconcile_metrics.lock().await;
    for task in tasks.values_mut() {
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }
    drop(tasks);
    backend
        .daemon
        .ensure_running_for_reconcile_bot(&BotId::new("10001"))
        .await
        .unwrap();
    assert!(
        host.commands.lock().unwrap().iter().any(|script| {
            script.contains("node_pid=$!")
                && script.contains("export NCD_RECONCILE_TEST='prepared-later'")
        }),
        "completed metrics must be retained for the next actual node start"
    );
}

#[tokio::test(start_paused = true)]
async fn disconnect_cancels_pending_metrics_without_waiting_for_the_upload_budget() {
    let (backend, host, gates) = fixture();
    let prepare = prepare(&backend, &host, "10001");
    permit(&gates.entered).await;
    backend.daemon.detach_local_sessions().await;
    tokio::time::timeout(Duration::from_millis(1), backend.detach_login_pollers())
        .await
        .expect("disconnect must not queue behind the optional upload timeout");
    permit(&gates.cancelled).await;
    prepare.await.unwrap();
    gates.release.add_permits(1);
    assert!(backend.reconcile_metrics.lock().await.is_empty());
    assert!(!host.node_running.load(Ordering::SeqCst));
}

#[tokio::test(start_paused = true)]
async fn stop_cancels_pending_metrics_without_waiting_for_the_upload_budget() {
    let (backend, host, gates) = fixture();
    let bot = BotId::new("10001");
    backend
        .start_modes
        .lock()
        .await
        .insert(bot.clone(), SnowLumaStartMode::HotStart);
    let prepare = prepare(&backend, &host, bot.as_str());
    permit(&gates.entered).await;
    tokio::time::timeout(
        Duration::from_millis(1),
        backend.stop(bot, StopMode::Graceful),
    )
    .await
    .expect("stop must not queue behind the optional upload timeout")
    .unwrap();
    permit(&gates.cancelled).await;
    prepare.await.unwrap();
    assert!(backend.reconcile_metrics.lock().await.is_empty());
    assert!(!host.node_running.load(Ordering::SeqCst));
}

struct ReuseClient {
    pid: AtomicU32,
    status: StdMutex<HookProcessStatus>,
    failed: AtomicBool,
    requests: AtomicU32,
    allow_unload: AtomicBool,
    unloads: AtomicU32,
}

#[async_trait]
impl SnowLumaWebUiClient for ReuseClient {
    async fn wait_ready(
        &self,
        _: Duration,
        _: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), SnowLumaWebUiError> {
        panic!("healthy reuse must not prepare a replacement client")
    }
    async fn login(&self) -> Result<(), SnowLumaWebUiError> {
        panic!("healthy reuse must not log in again")
    }
    async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
        panic!("healthy reuse must not log out")
    }
    async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        if self.failed.load(Ordering::SeqCst) {
            return Err(SnowLumaWebUiError::Timeout {
                endpoint: "/api/processes".into(),
            });
        }
        Ok(vec![HookProcessInfo {
            pid: self.pid.load(Ordering::SeqCst),
            name: "qq".into(),
            path: "/opt/QQ/qq".into(),
            uin: "10001".into(),
            status: *self.status.lock().unwrap(),
            logged_in: Some(true),
            connected: Some(true),
            injected: Some(true),
            error: String::new(),
        }])
    }
    async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
        Ok(Vec::new())
    }
    async fn probe_process_login_info(
        &self,
        _: u32,
    ) -> Result<Option<QqPortLoginInfo>, SnowLumaWebUiError> {
        Ok(None)
    }
    async fn load_process(&self, _: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        panic!("healthy reuse must not inject again")
    }
    async fn unload_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        assert!(
            self.allow_unload.load(Ordering::SeqCst),
            "healthy reuse must not unload"
        );
        self.unloads.fetch_add(1, Ordering::SeqCst);
        if self.failed.load(Ordering::SeqCst) {
            return Err(SnowLumaWebUiError::Timeout {
                endpoint: "/api/processes/unload".into(),
            });
        }
        Ok(HookProcessInfo {
            pid,
            name: "qq".into(),
            path: "/opt/QQ/qq".into(),
            uin: "10001".into(),
            status: HookProcessStatus::Available,
            logged_in: Some(true),
            connected: Some(false),
            injected: Some(false),
            error: String::new(),
        })
    }
    async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError> {
        panic!("healthy reuse must not restart auth setup")
    }
    async fn get_agreements(&self) -> Result<AgreementsPayload, SnowLumaWebUiError> {
        panic!("healthy reuse must not restart consent setup")
    }
    async fn record_agreement_consent(&self, _: &str) -> Result<(), SnowLumaWebUiError> {
        panic!("healthy reuse must not change consent")
    }
    async fn update_onebot_config(
        &self,
        _: &str,
        _: &serde_json::Value,
    ) -> Result<bool, SnowLumaWebUiError> {
        panic!("healthy reuse must not update OneBot again")
    }
}

async fn install_reusable_poller(backend: &RemoteSnowLumaBackend, bot: &BotId) -> Arc<ReuseClient> {
    let client = Arc::new(ReuseClient {
        pid: AtomicU32::new(42),
        status: StdMutex::new(HookProcessStatus::Online),
        failed: AtomicBool::new(false),
        requests: AtomicU32::new(0),
        allow_unload: AtomicBool::new(false),
        unloads: AtomicU32::new(0),
    });
    backend
        .replace_poller(
            bot.clone(),
            42,
            PollerDeps {
                event_bus: Arc::clone(&backend.event_bus),
                http: client.clone(),
                proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(
                    42,
                )),
                expected_uin: Some("10001".into()),
            },
        )
        .await;
    backend.login_pids.lock().await.insert(
        bot.clone(),
        TrackedQq {
            pid: 42,
            start_time: "12345".into(),
        },
    );
    client
}

#[tokio::test(start_paused = true)]
async fn healthy_reconcile_reuses_poller_client_and_qr_without_preparing_metrics() {
    let (backend, host, gates) = fixture();
    let bot = BotId::new("10001");
    let client = install_reusable_poller(&backend, &bot).await;
    let activity = Arc::new(QrActivity::new());
    backend
        .qr_activity
        .lock()
        .await
        .insert(bot.clone(), Arc::clone(&activity));
    let config = ncd_test_support::BotConfigBuilder::new().build();
    backend
        .attach_reconciled_running(bot.clone(), 42, &config)
        .await
        .unwrap();
    let expected: Arc<dyn SnowLumaWebUiClient> = client.clone();
    assert!(Arc::ptr_eq(
        backend.login_clients.lock().await.get(&bot).unwrap(),
        &expected
    ));
    assert!(backend.pollers.lock().await.contains_key(&bot));
    assert_eq!(backend.login_pids.lock().await.get(&bot).unwrap().pid, 42);
    assert!(
        !activity.cancel.is_cancelled(),
        "reuse must preserve QR observation"
    );
    assert!(Arc::ptr_eq(
        backend.qr_activity.lock().await.get(&bot).unwrap(),
        &activity
    ));
    assert_eq!(gates.entered.available_permits(), 0);
    assert!(backend.reconcile_metrics.lock().await.is_empty());
    assert!(
        host.commands.lock().unwrap().is_empty(),
        "healthy reuse must not bootstrap again"
    );
    backend.detach_login_pollers().await;
}

#[tokio::test(start_paused = true)]
async fn reconcile_reuse_rejects_wrong_birth_pid_missing_target_and_terminal_status() {
    for case in [
        "birth",
        "tracked-pid",
        "response-pid",
        "error",
        "disconnected",
    ] {
        let (backend, host, gates) = fixture();
        let bot = BotId::new("10001");
        let client = install_reusable_poller(&backend, &bot).await;
        match case {
            "birth" => *host.qq_start_time.lock().unwrap() = Some("67890".into()),
            "response-pid" => client.pid.store(43, Ordering::SeqCst),
            "error" => *client.status.lock().unwrap() = HookProcessStatus::Error,
            "disconnected" => *client.status.lock().unwrap() = HookProcessStatus::Disconnected,
            _ => {}
        }
        let pid = if case == "tracked-pid" { 43 } else { 42 };
        assert!(
            !backend.reuse_reconciled_poller(&bot, pid).await,
            "case={case}"
        );
        assert!(backend.pollers.lock().await.contains_key(&bot));
        assert_eq!(gates.entered.available_permits(), 0);
        backend.detach_login_pollers().await;
    }
}

#[tokio::test(start_paused = true)]
async fn reconcile_reuse_rejects_http_failure_or_missing_client_without_disposing_poller() {
    let (backend, _, gates) = fixture();
    let bot = BotId::new("10001");
    let client = install_reusable_poller(&backend, &bot).await;
    client.failed.store(true, Ordering::SeqCst);
    assert!(!backend.reuse_reconciled_poller(&bot, 42).await);
    backend.login_clients.lock().await.remove(&bot);
    assert!(!backend.reuse_reconciled_poller(&bot, 42).await);
    assert!(backend.pollers.lock().await.contains_key(&bot));
    assert_eq!(gates.entered.available_permits(), 0);
    backend.detach_login_pollers().await;
}
