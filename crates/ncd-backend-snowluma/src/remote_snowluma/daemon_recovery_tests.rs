use super::*;

use std::path::Path;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::remote::tunnel::{TunnelHandle, TunnelSpec};
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostPath, HostProcess,
    HostShell, Locality, Os,
};
use ncd_traits::events::EventFilter;
use tokio::sync::{Notify, Semaphore};

struct RecoveryHost {
    node_running: AtomicBool,
    starts: AtomicUsize,
    fail_start: bool,
    qq_probe: &'static str,
    commands: StdMutex<Vec<String>>,
    files: StdMutex<BTreeMap<String, Vec<u8>>>,
    reads: AtomicUsize,
    identity: AtomicUsize,
    tunnels: AtomicUsize,
    identity_gate: StdMutex<Option<Arc<Semaphore>>>,
    identity_entered: Notify,
    tunnel_gate: StdMutex<Option<Arc<Semaphore>>>,
    tunnel_entered: Notify,
    fail_prereqs: AtomicBool,
    fail_tunnels: AtomicBool,
}

impl RecoveryHost {
    fn new(node_running: bool, qq_probe: &'static str, fail_start: bool) -> Self {
        Self {
            node_running: AtomicBool::new(node_running),
            starts: AtomicUsize::new(0),
            fail_start,
            qq_probe,
            commands: StdMutex::new(Vec::new()),
            files: StdMutex::new(BTreeMap::new()),
            reads: AtomicUsize::new(0),
            identity: AtomicUsize::new(1),
            tunnels: AtomicUsize::new(0),
            identity_gate: StdMutex::new(None),
            identity_entered: Notify::new(),
            tunnel_gate: StdMutex::new(None),
            tunnel_entered: Notify::new(),
            fail_prereqs: AtomicBool::new(false),
            fail_tunnels: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl Host for RecoveryHost {
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
        "recovery-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }

    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        let path_str = path.as_posix();
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(bytes) = self.files.lock().unwrap().get(path_str) {
            return Ok(Bytes::copy_from_slice(bytes));
        }
        if path_str.ends_with("runtime.json") {
            return Ok(Bytes::from_static(br#"{"webuiPort":13105}"#));
        }
        if path_str.ends_with(".secret") {
            return Ok(Bytes::from_static(b"test-secret"));
        }
        Err(HostError::PathNotFound { path: path.clone() })
    }
    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.as_posix().into(), bytes.to_vec());
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
        panic!("recovery must not install components")
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
        panic!("recovery must not install components")
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("recovery must not spawn QQ")
    }
    async fn which(&self, _: &str) -> Result<Option<String>, HostError> {
        Ok(Some("/bin/bash".into()))
    }
    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        self.tunnels.fetch_add(1, Ordering::SeqCst);
        let gate = self.tunnel_gate.lock().unwrap().take();
        if let Some(gate) = gate {
            self.tunnel_entered.notify_one();
            gate.acquire().await.unwrap().forget();
        }
        if self.fail_tunnels.load(Ordering::SeqCst) {
            return Err(HostError::Cancelled);
        }
        Ok(TunnelHandle::detached(spec.local_port, 0))
    }
    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        let script = cmd.args.last().cloned().unwrap_or_default();
        self.commands.lock().unwrap().push(script.clone());
        let mut out = CommandOutput {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        };
        if cmd.program == "python3" {
            if self.fail_start {
                out.exit_code = Some(1);
                out.stderr = "VNC service unavailable".into();
            } else {
                out.stdout = r#"{"port":6099}"#.into();
            }
        } else if script == super::super::probe::stack_ready_script(&layout().paths) {
            out.exit_code = Some(if self.node_running.load(Ordering::SeqCst) {
                0
            } else {
                1
            });
        } else if script.starts_with("# ncd-snowluma-node-identity") {
            let identity = self.identity.load(Ordering::SeqCst);
            let gate = self.identity_gate.lock().unwrap().take();
            if let Some(gate) = gate {
                self.identity_entered.notify_one();
                gate.acquire().await.unwrap().forget();
            }
            if self.node_running.load(Ordering::SeqCst) {
                out.stdout = format!("901:{identity}");
            } else {
                out.exit_code = Some(1);
            }
        } else if script.starts_with("QQ_BIN=") {
            out.stdout = self.qq_probe.into();
        } else if script.contains("node_pid=$!") {
            self.starts.fetch_add(1, Ordering::SeqCst);
            if self.fail_start {
                out.exit_code = Some(1);
                out.stderr = "node launch failed".into();
            } else {
                self.node_running.store(true, Ordering::SeqCst);
                self.identity.fetch_add(1, Ordering::SeqCst);
                out.stdout = "901\n".into();
            }
        } else if script.contains("node_not_started") {
            out.stdout = "13105\n".into();
        } else if script.contains("\"running\":false,\"ready\":false") {
            self.node_running.store(false, Ordering::SeqCst);
        } else if script.contains("command -v Xvfb") && self.fail_prereqs.load(Ordering::SeqCst) {
            out.exit_code = Some(1);
        }
        Ok(out)
    }
}

#[tokio::test]
async fn batch_start_reuses_one_bootstrap_and_only_probes_node_for_following_accounts() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    let ids = [
        BotId::new("10001"),
        BotId::new("10002"),
        BotId::new("10003"),
    ];
    let (a, b, c) = tokio::join!(
        daemon.ensure_running_for_bot(&ids[0], None, true, Some("test-secret")),
        daemon.ensure_running_for_bot(&ids[1], None, true, Some("test-secret")),
        daemon.ensure_running_for_bot(&ids[2], None, true, Some("test-secret")),
    );
    a.unwrap();
    b.unwrap();
    c.unwrap();
    assert_eq!(daemon.leases.lock().await.count(), 3);
    assert_eq!(host.starts.load(Ordering::SeqCst), 1);
    assert_eq!(host.tunnels.load(Ordering::SeqCst), 2);
    assert_eq!(
        host.commands
            .lock()
            .unwrap()
            .iter()
            .filter(|cmd| cmd.contains("command -v Xvfb"))
            .count(),
        1
    );

    let reads = host.reads.load(Ordering::SeqCst);
    let commands = host.commands.lock().unwrap().len();
    daemon
        .ensure_running_for_bot(&BotId::new("10004"), None, true, None)
        .await
        .unwrap();
    assert_eq!(
        host.reads.load(Ordering::SeqCst),
        reads,
        "warm start must not reread shared credentials/config"
    );
    assert_eq!(
        host.commands.lock().unwrap().len(),
        commands + 1,
        "warm start needs one identity probe"
    );
}

#[tokio::test]
async fn changed_node_or_config_identity_revalidates_shared_startup() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    daemon
        .ensure_running_for_bot(&BotId::new("10001"), None, false, None)
        .await
        .unwrap();
    let reads = host.reads.load(Ordering::SeqCst);
    host.identity.fetch_add(1, Ordering::SeqCst);
    daemon
        .ensure_running_for_bot(&BotId::new("10002"), None, false, None)
        .await
        .unwrap();
    assert!(host.reads.load(Ordering::SeqCst) > reads);
    assert_eq!(daemon.leases.lock().await.count(), 2);
}

#[tokio::test]
async fn dropped_tunnel_bundle_is_recreated_without_stopping_existing_bots() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    daemon
        .ensure_running_for_bot(&BotId::new("10001"), None, false, None)
        .await
        .unwrap();
    let starts = host.starts.load(Ordering::SeqCst);
    daemon.tunnels.drop_server("server").await;
    daemon
        .ensure_running_for_bot(&BotId::new("10002"), None, false, None)
        .await
        .unwrap();
    assert_eq!(host.tunnels.load(Ordering::SeqCst), 4);
    assert_eq!(host.starts.load(Ordering::SeqCst), starts);
    assert!(
        daemon
            .tunnels
            .endpoints_for_server("server")
            .await
            .is_some()
    );
    assert_eq!(daemon.leases.lock().await.count(), 2);
}

#[tokio::test]
async fn reconnect_invalidates_success_and_bootstrap_uses_new_host() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    daemon
        .ensure_running_for_bot(&BotId::new("10001"), None, false, None)
        .await
        .unwrap();
    let replacement = Arc::new(RecoveryHost::new(true, "running", false));
    daemon.replace_host(replacement.clone()).await;
    assert!(daemon.ready_node_identity.lock().await.is_none());
    assert!(daemon.tunnel_endpoints().await.is_none());
    daemon
        .ensure_running_for_bot(&BotId::new("10002"), None, false, None)
        .await
        .unwrap();
    assert!(replacement.reads.load(Ordering::SeqCst) > 0);
    assert_eq!(replacement.tunnels.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn stale_host_lookup_cannot_cancel_the_current_connection_or_drop_its_cache() {
    let old = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&old), Arc::new(BroadcastEventBus::default()));
    let current: Arc<dyn Host> = Arc::new(RecoveryHost::new(true, "running", false));
    daemon.replace_host(Arc::clone(&current)).await;
    daemon
        .ensure_running_for_bot(&BotId::new("10001"), None, false, None)
        .await
        .unwrap();
    let session = daemon.session.lock().await.clone();
    let old: Arc<dyn Host> = old;
    let expected = Arc::clone(&old);
    assert!(
        daemon
            .replace_host_if_current(old, async { Arc::ptr_eq(&current, &expected) })
            .await
            .is_err()
    );
    assert!(!session.is_cancelled());
    assert!(Arc::ptr_eq(&daemon.current_host().await, &current));
    assert!(daemon.ready_node_identity.lock().await.is_some());
    assert!(daemon.tunnel_endpoints().await.is_some());

    // An unchanged daemon Host must still be rejected if the registry retired it.
    assert!(
        daemon
            .replace_host_if_current(Arc::clone(&current), async { false })
            .await
            .is_err()
    );
    assert!(!session.is_cancelled());
}

#[tokio::test]
async fn host_identity_check_and_replacement_share_one_publication_gate() {
    let initial = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = Arc::new(daemon(initial, Arc::new(BroadcastEventBus::default())));
    let old: Arc<dyn Host> = Arc::new(RecoveryHost::new(true, "running", false));
    let fresh: Arc<dyn Host> = Arc::new(RecoveryHost::new(true, "running", false));
    let authority = Arc::new(tokio::sync::RwLock::new(Arc::clone(&old)));
    let validated = Arc::new(Notify::new());
    let resume = Arc::new(Semaphore::new(0));
    let delayed = tokio::spawn({
        let daemon = Arc::clone(&daemon);
        let authority = Arc::clone(&authority);
        let validated = Arc::clone(&validated);
        let resume = Arc::clone(&resume);
        async move {
            let expected = Arc::clone(&old);
            daemon
                .replace_host_if_current(old, async {
                    let accepted = Arc::ptr_eq(&*authority.read().await, &expected);
                    validated.notify_one();
                    resume.acquire().await.unwrap().forget();
                    accepted
                })
                .await
        }
    });
    validated.notified().await;
    *authority.write().await = Arc::clone(&fresh);
    let validator_polled = AtomicBool::new(false);
    let expected = Arc::clone(&fresh);
    let newer = daemon.replace_host_if_current(Arc::clone(&fresh), async {
        validator_polled.store(true, Ordering::SeqCst);
        Arc::ptr_eq(&*authority.read().await, &expected)
    });
    tokio::pin!(newer);
    tokio::select! {
        biased;
        _ = &mut newer => panic!("new publication crossed the old validation gate"),
        _ = std::future::ready(()) => {}
    }
    assert!(!validator_polled.load(Ordering::SeqCst));
    resume.add_permits(1);
    delayed.await.unwrap().unwrap();
    newer.await.unwrap();
    assert!(Arc::ptr_eq(&daemon.current_host().await, &fresh));
}

#[tokio::test]
async fn detach_cancels_pending_validation_and_does_not_revive_success() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = Arc::new(daemon(
        Arc::clone(&host),
        Arc::new(BroadcastEventBus::default()),
    ));
    daemon
        .ensure_running_for_bot(&BotId::new("10001"), None, false, None)
        .await
        .unwrap();
    daemon.confirm_bot_started(&BotId::new("10001")).await;
    let gate = Arc::new(Semaphore::new(0));
    *host.identity_gate.lock().unwrap() = Some(Arc::clone(&gate));
    let pending = tokio::spawn({
        let daemon = Arc::clone(&daemon);
        async move {
            daemon
                .ensure_running_for_bot(&BotId::new("10002"), None, false, None)
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), host.identity_entered.notified())
        .await
        .unwrap();

    // A cached router lookup must not queue behind a remote validation for the same Host.
    tokio::time::timeout(Duration::from_secs(1), daemon.replace_host(host.clone()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), daemon.detach_local_sessions())
        .await
        .unwrap();
    assert!(pending.await.unwrap().is_err());
    gate.add_permits(1);
    assert!(daemon.ready_node_identity.lock().await.is_none());
    assert!(daemon.tunnel_endpoints().await.is_none());
    assert!(
        daemon
            .tunnels
            .endpoints_for_server("server")
            .await
            .is_none()
    );
    assert_eq!(daemon.leases.lock().await.count(), 1);
    assert!(host.node_running.load(Ordering::SeqCst));

    let reads = host.reads.load(Ordering::SeqCst);
    daemon
        .ensure_running_for_bot(&BotId::new("10003"), None, false, None)
        .await
        .unwrap();
    assert!(host.reads.load(Ordering::SeqCst) > reads);
    assert_eq!(host.tunnels.load(Ordering::SeqCst), 4);
    assert_eq!(daemon.leases.lock().await.count(), 2);
}

#[tokio::test]
async fn host_replacement_cancels_old_validation_before_accepting_new_success() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = Arc::new(daemon(
        Arc::clone(&host),
        Arc::new(BroadcastEventBus::default()),
    ));
    daemon
        .ensure_running_for_bot(&BotId::new("10001"), None, false, None)
        .await
        .unwrap();
    daemon.confirm_bot_started(&BotId::new("10001")).await;
    *host.identity_gate.lock().unwrap() = Some(Arc::new(Semaphore::new(0)));
    let pending = tokio::spawn({
        let daemon = Arc::clone(&daemon);
        async move {
            daemon
                .ensure_running_for_bot(&BotId::new("10002"), None, false, None)
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), host.identity_entered.notified())
        .await
        .unwrap();
    let replacement = Arc::new(RecoveryHost::new(true, "running", false));
    tokio::time::timeout(
        Duration::from_secs(1),
        daemon.replace_host(replacement.clone()),
    )
    .await
    .unwrap();
    assert!(pending.await.unwrap().is_err());
    daemon
        .ensure_running_for_bot(&BotId::new("10003"), None, false, None)
        .await
        .unwrap();
    assert_eq!(replacement.tunnels.load(Ordering::SeqCst), 2);
    assert_eq!(daemon.leases.lock().await.count(), 2);
}

#[tokio::test]
async fn last_lease_clears_local_cache_even_when_external_qq_keeps_node_alive() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    let bot = BotId::new("10001");
    daemon
        .ensure_running_for_bot(&bot, None, false, None)
        .await
        .unwrap();
    daemon.release_bot(&bot).await;
    assert!(daemon.ready_node_identity.lock().await.is_none());
    assert!(daemon.tunnel_endpoints().await.is_none());
    assert!(
        daemon
            .tunnels
            .endpoints_for_server("server")
            .await
            .is_none()
    );
    assert_eq!(daemon.leases.lock().await.count(), 0);
    assert!(host.node_running.load(Ordering::SeqCst));
}

#[tokio::test]
async fn detach_returns_unfinished_start_leases_but_preserves_confirmed_bots() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(host, Arc::new(BroadcastEventBus::default()));
    let running = BotId::new("10001");
    let unfinished = BotId::new("10002");
    daemon
        .ensure_running_for_bot(&running, None, false, None)
        .await
        .unwrap();
    daemon.confirm_bot_started(&running).await;
    daemon
        .ensure_running_for_bot(&unfinished, None, false, None)
        .await
        .unwrap();
    assert_eq!(daemon.leases.lock().await.count(), 2);
    daemon.detach_local_sessions().await;
    let leases = daemon.leases.lock().await;
    assert_eq!(leases.count(), 1);
    assert!(leases.bots.contains(&running));
    assert!(!leases.bots.contains(&unfinished));
    assert!(leases.pending_bots.is_empty());
}

#[tokio::test]
async fn repeated_anonymous_reconcile_reuses_without_accumulating_leases() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    daemon.ensure_running_for_reconcile().await.unwrap();
    let reads = host.reads.load(Ordering::SeqCst);
    daemon.ensure_running_for_reconcile().await.unwrap();
    assert_eq!(host.reads.load(Ordering::SeqCst), reads);
    assert_eq!(daemon.leases.lock().await.count(), 1);
    assert_eq!(host.tunnels.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn confirmed_stopped_bot_releases_its_lease_without_releasing_another_bot() {
    let host = Arc::new(RecoveryHost::new(true, "stopped", false));
    let bus = Arc::new(BroadcastEventBus::default());
    let daemon = Arc::new(daemon(Arc::clone(&host), Arc::clone(&bus)));
    let backend = super::super::backend::RemoteSnowLumaBackend::new(
        "server",
        Arc::clone(&daemon),
        bus,
        Arc::clone(&daemon.tunnels),
        Arc::new(ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator::default()),
    );
    let first = BotId::new("10001");
    let second = BotId::new("10002");
    for bot in [&first, &second] {
        daemon
            .ensure_running_for_bot(bot, None, false, None)
            .await
            .unwrap();
        daemon.confirm_bot_started(bot).await;
    }
    daemon.detach_local_sessions().await;
    assert_eq!(daemon.leases.lock().await.count(), 2);
    backend.detach_status_poller(&first).await;
    assert_eq!(daemon.leases.lock().await.count(), 1);
    assert!(host.node_running.load(Ordering::SeqCst));
    backend.detach_status_poller(&second).await;
    assert_eq!(daemon.leases.lock().await.count(), 0);
    assert!(!host.node_running.load(Ordering::SeqCst));
}

#[tokio::test]
async fn failed_revalidation_clears_success_and_a_later_start_can_retry() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    daemon
        .ensure_running_for_bot(&BotId::new("10001"), None, false, None)
        .await
        .unwrap();
    host.identity.fetch_add(1, Ordering::SeqCst);
    host.fail_prereqs.store(true, Ordering::SeqCst);
    assert!(
        daemon
            .ensure_running_for_bot(&BotId::new("10002"), None, false, None)
            .await
            .is_err()
    );
    assert!(daemon.ready_node_identity.lock().await.is_none());
    assert!(daemon.tunnel_endpoints().await.is_none());
    assert_eq!(daemon.leases.lock().await.count(), 1);
    host.fail_prereqs.store(false, Ordering::SeqCst);
    daemon
        .ensure_running_for_bot(&BotId::new("10002"), None, false, None)
        .await
        .unwrap();
    assert!(daemon.ready_node_identity.lock().await.is_some());
    assert_eq!(daemon.leases.lock().await.count(), 2);
}

#[tokio::test]
async fn slow_tunnel_open_does_not_block_another_server() {
    let slow = Arc::new(RecoveryHost::new(true, "running", false));
    let gate = Arc::new(Semaphore::new(0));
    *slow.tunnel_gate.lock().unwrap() = Some(Arc::clone(&gate));
    let registry = Arc::new(RemoteSnowLumaTunnelRegistry::new());
    let pending = tokio::spawn({
        let registry = Arc::clone(&registry);
        let slow = Arc::clone(&slow);
        async move {
            registry
                .acquire(
                    "slow",
                    slow.as_ref(),
                    "secret".into(),
                    "vnc".into(),
                    5099,
                    6081,
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), slow.tunnel_entered.notified())
        .await
        .unwrap();
    let fast = RecoveryHost::new(true, "running", false);
    tokio::time::timeout(
        Duration::from_secs(1),
        registry.acquire("fast", &fast, "secret".into(), "vnc".into(), 5099, 6081),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(registry.endpoints_for_server("fast").await.is_some());
    gate.add_permits(1);
    pending.await.unwrap().unwrap();
}

#[tokio::test]
async fn invalidated_tunnel_open_cannot_replace_a_new_generation() {
    for shutdown_all in [false, true] {
        let old = Arc::new(RecoveryHost::new(true, "running", false));
        let gate = Arc::new(Semaphore::new(0));
        *old.tunnel_gate.lock().unwrap() = Some(Arc::clone(&gate));
        let registry = Arc::new(RemoteSnowLumaTunnelRegistry::new());
        let pending = tokio::spawn({
            let registry = Arc::clone(&registry);
            let old = Arc::clone(&old);
            async move {
                registry
                    .acquire(
                        "server",
                        old.as_ref(),
                        "old".into(),
                        "vnc".into(),
                        5099,
                        6081,
                    )
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(5), old.tunnel_entered.notified())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            if shutdown_all {
                registry.shutdown_all().await;
            } else {
                registry.drop_server("server").await;
            }
        })
        .await
        .unwrap();
        let fresh = RecoveryHost::new(true, "running", false);
        registry
            .acquire("server", &fresh, "new".into(), "vnc".into(), 6100, 6081)
            .await
            .unwrap();
        gate.add_permits(1);
        assert!(matches!(pending.await.unwrap(), Err(HostError::Cancelled)));
        assert_eq!(
            registry
                .endpoints_for_server("server")
                .await
                .unwrap()
                .webui_password,
            "new"
        );
    }
}

#[tokio::test]
async fn failed_tunnel_open_does_not_poison_retry_or_retain_an_endpoint() {
    let host = RecoveryHost::new(true, "running", false);
    let registry = RemoteSnowLumaTunnelRegistry::new();
    host.fail_tunnels.store(true, Ordering::SeqCst);
    assert!(
        registry
            .acquire("server", &host, "secret".into(), "vnc".into(), 5099, 6081)
            .await
            .is_err()
    );
    assert!(registry.endpoints_for_server("server").await.is_none());
    host.fail_tunnels.store(false, Ordering::SeqCst);
    registry
        .acquire("server", &host, "secret".into(), "vnc".into(), 5099, 6081)
        .await
        .unwrap();
    registry.release("server").await;
    assert!(registry.endpoints_for_server("server").await.is_none());
}

fn layout() -> RemoteSnowLumaLayout {
    RemoteSnowLumaLayout {
        home: "/root".into(),
        paths: SnowLumaRemotePaths::from_workspace_and_dir("/opt/sl", "/opt/sl"),
        node_bin: "/opt/sl/node".into(),
        qq_bin: "/opt/QQ/qq".into(),
        qq_install_base: "/".into(),
    }
}

fn daemon(host: Arc<RecoveryHost>, bus: Arc<BroadcastEventBus>) -> RemoteSnowLumaDaemon {
    RemoteSnowLumaDaemon::from_layout(
        "server".into(),
        host,
        Arc::new(RemoteSnowLumaTunnelRegistry::new()),
        bus,
        layout(),
    )
}

#[tokio::test]
async fn shared_metrics_update_preserves_running_node_and_is_used_for_recovery() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    let env = BTreeMap::from([
        ("NCD_METRICS_ENABLED".into(), "1".into()),
        (
            "NCD_METRICS_REGISTRY_DIR".into(),
            "/root/ncd-watch/metrics/snowluma-shared".into(),
        ),
        (
            "NODE_OPTIONS".into(),
            "--require /root/ncd-watch/metrics/ncd-ob11-stats.cjs".into(),
        ),
    ]);
    daemon
        .apply_metrics_node_env(Some(env.clone()))
        .await
        .unwrap();
    assert_eq!(host.starts.load(Ordering::SeqCst), 0);
    assert!(host.node_running.load(Ordering::SeqCst));
    assert_eq!(*daemon.metrics_node_env.lock().await, Some(env));
    host.node_running.store(false, Ordering::SeqCst);
    daemon
        .ensure_running_for_reconcile_bot(&BotId::new("10001"))
        .await
        .unwrap();
    assert_eq!(host.starts.load(Ordering::SeqCst), 1);
    let commands = host.commands.lock().unwrap();
    let startup = commands
        .iter()
        .find(|cmd| cmd.contains("node_pid=$!"))
        .unwrap();
    assert!(startup.contains("NCD_METRICS_REGISTRY_DIR"));
    assert!(startup.contains("snowluma-shared"));
    assert!(!commands.iter().any(|cmd| cmd.contains("kill \"$pid\"")));
}

#[tokio::test]
async fn novnc_recovery_uses_verified_custom_port_without_starting_node_or_acquiring_bot_lease() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    daemon.ensure_novnc_ready().await.unwrap();
    assert_eq!(
        *daemon.tunnel_remote_ports.lock().await,
        Some((13105, 6099))
    );
    assert_eq!(host.starts.load(Ordering::SeqCst), 0);
    assert_eq!(daemon.leases.lock().await.count(), 0);
}

#[tokio::test]
async fn failed_novnc_recovery_does_not_publish_an_endpoint() {
    let host = Arc::new(RecoveryHost::new(true, "running", true));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    assert!(daemon.ensure_novnc_ready().await.is_err());
    assert!(daemon.tunnel_endpoints().await.is_none());
    assert!(host.node_running.load(Ordering::SeqCst));
    assert_eq!(host.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reconcile_recovers_missing_node_once_and_preserves_other_bot() {
    let host = Arc::new(RecoveryHost::new(false, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    let a = BotId::new("10001");
    let b = BotId::new("10002");
    let (first, second) = tokio::join!(
        daemon.ensure_running_for_reconcile_bot(&a),
        daemon.ensure_running_for_reconcile_bot(&b),
    );
    first.unwrap();
    second.unwrap();
    daemon.ensure_running_for_reconcile_bot(&a).await.unwrap();
    assert_eq!(host.starts.load(Ordering::SeqCst), 1);
    assert_eq!(daemon.leases.lock().await.count(), 2);
    assert_eq!(
        *daemon.tunnel_remote_ports.lock().await,
        Some((13105, 6081))
    );
    daemon.release_bot(&a).await;
    assert!(host.node_running.load(Ordering::SeqCst));
    assert!(daemon.tunnel_endpoints().await.is_some());
    let commands = host.commands.lock().unwrap();
    for script in commands.iter() {
        assert!(!script.contains("nohup setsid Xvfb"));
        assert!(!script.contains("x11vnc -display"));
        assert!(!script.contains("--no-sandbox"));
        assert!(!script.contains("kill \"$pid\""));
    }
}

#[tokio::test]
async fn last_local_lease_does_not_stop_unreconciled_remote_qq() {
    for probe in ["running", "unknown"] {
        let host = Arc::new(RecoveryHost::new(true, probe, false));
        let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
        let bot = BotId::new("10001");
        daemon.leases.lock().await.acquire(Some(&bot));
        daemon.release_bot(&bot).await;
        assert!(host.node_running.load(Ordering::SeqCst), "probe={probe}");
    }
}

#[tokio::test]
async fn last_local_lease_stops_daemon_after_all_remote_qq_exit() {
    let host = Arc::new(RecoveryHost::new(true, "stopped", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    let bot = BotId::new("10001");
    daemon.leases.lock().await.acquire(Some(&bot));
    daemon.release_bot(&bot).await;
    assert!(!host.node_running.load(Ordering::SeqCst));
}

#[tokio::test]
async fn anonymous_reconcile_does_not_start_node_without_a_running_bot() {
    let host = Arc::new(RecoveryHost::new(false, "stopped", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    assert!(daemon.ensure_running_for_reconcile().await.is_err());
    assert_eq!(host.starts.load(Ordering::SeqCst), 0);
    assert_eq!(daemon.leases.lock().await.count(), 0);
}

#[tokio::test]
async fn failed_recovery_returns_error_and_publishes_unavailable() {
    let host = Arc::new(RecoveryHost::new(false, "running", true));
    let bus = Arc::new(BroadcastEventBus::default());
    let daemon = Arc::new(daemon(host, Arc::clone(&bus)));
    let backend = super::super::backend::RemoteSnowLumaBackend::new(
        "server",
        Arc::clone(&daemon),
        Arc::clone(&bus),
        Arc::clone(&daemon.tunnels),
        Arc::new(ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator::default()),
    );
    let bot = BotId::new("10001");
    let mut events = bus.subscribe(EventFilter::all());
    let config = ncd_test_support::BotConfigBuilder::new()
        .backend_type(ncd_domain::BackendType::SnowLuma)
        .build();
    let error = backend
        .attach_reconciled_running(bot.clone(), 42, &config)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("node launch failed"));
    assert_eq!(daemon.leases.lock().await.count(), 1);
    tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(event) = events.next().await {
            match event {
                DomainEvent::SnowLumaDaemonStateChanged {
                    state: DaemonState::Crashed,
                    ..
                } => panic!("WebUI failure must not crash running QQ actors"),
                DomainEvent::SnowLumaLoginProbeUnavailable { bot_id } if bot_id == bot => {
                    return;
                }
                _ => {}
            }
        }
        panic!("missing recovery failure event");
    })
    .await
    .unwrap();
}
