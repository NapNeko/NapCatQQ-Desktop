//! 远端历史隔离、启动事件和取消边界的可控传输回归。
use super::*;
use bytes::Bytes;
use ncd_domain::DomainEvent;
use ncd_domain::windows_bot::{WindowsBotEndpoint, WindowsBotEventBatch};
use ncd_domain::windows_remote::WindowsAgentResponse;
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::shell::{HostShell, PowerShellShell};
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostProcess, Locality,
};
use ncd_traits::events::{EventBus, EventFilter};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Notify, oneshot};

async fn next_event(subscription: &mut ncd_traits::events::EventSubscription) -> DomainEvent {
    tokio::time::timeout(Duration::from_secs(1), subscription.next())
        .await
        .expect("expected event was not published")
        .expect("event bus closed")
}

#[derive(Default)]
struct ScriptHost {
    replies: Mutex<VecDeque<WindowsAgentResult>>,
    requests: Mutex<Vec<WindowsAgentAction>>,
    endpoint_gate: Mutex<Option<oneshot::Receiver<()>>>,
    event_gate: Mutex<Option<oneshot::Receiver<()>>>,
    tunnel_gate: Mutex<Option<oneshot::Receiver<()>>>,
    entered: Notify,
    tunnels_completed: AtomicUsize,
}

fn unused() -> HostError {
    HostError::Unsupported {
        operation: "unused test operation",
    }
}

#[async_trait]
impl Host for ScriptHost {
    fn os(&self) -> Os {
        Os::Windows
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        Locality::Remote
    }
    fn id(&self) -> &str {
        "windows-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &PowerShellShell
    }
    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        Err(unused())
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        Err(unused())
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Err(unused())
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Err(unused())
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        Err(unused())
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Err(unused())
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        Err(unused())
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        Err(unused())
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        Err(unused())
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        Err(unused())
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        Err(unused())
    }
    async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
        Err(unused())
    }

    async fn windows_agent_request(
        &self,
        request: WindowsAgentRequest,
    ) -> Result<WindowsAgentResponse, HostError> {
        self.requests.lock().await.push(request.action.clone());
        let gate = match request.action {
            WindowsAgentAction::BotEndpoint { .. } => self.endpoint_gate.lock().await.take(),
            WindowsAgentAction::BotEvents { after, .. } if after != u64::MAX => {
                self.event_gate.lock().await.take()
            }
            _ => None,
        };
        if let Some(gate) = gate {
            self.entered.notify_one();
            let _ = gate.await;
        }
        let result = self.replies.lock().await.pop_front().ok_or_else(unused)?;
        Ok(WindowsAgentResponse {
            v: WINDOWS_AGENT_PROTOCOL,
            request_id: request.request_id,
            result,
        })
    }

    async fn open_tunnel(&self, _: TunnelSpec) -> Result<TunnelHandle, HostError> {
        let gate = self.tunnel_gate.lock().await.take();
        if let Some(gate) = gate {
            self.entered.notify_one();
            let _ = gate.await;
        }
        self.tunnels_completed.fetch_add(1, Ordering::SeqCst);
        Ok(TunnelHandle::detached(47001, 0))
    }
}

struct Resolver(Arc<ScriptHost>);

#[async_trait]
impl HostResolver for Resolver {
    async fn resolve(&self, _: &RuntimeTarget) -> Result<Arc<dyn Host>, crate::HostResolveError> {
        Ok(self.0.clone())
    }
}

fn backend(host: &Arc<ScriptHost>, bus: &Arc<BroadcastEventBus>) -> Arc<RemoteWindowsBotBackend> {
    Arc::new(RemoteWindowsBotBackend {
        id: BotId::new("123"),
        server: "server".into(),
        flavor: BackendType::NapCat,
        resolver: Arc::new(Resolver(host.clone())),
        bus: bus.clone(),
        local_data_root: None,
        selected: Mutex::new(ncd_domain::RemoteSelectedPaths::default()),
        runtime: Mutex::new(None),
        endpoint: Mutex::new(None),
        cancel: CancellationToken::new(),
        following: Mutex::new(None),
        cursor: Mutex::new(events::EventCursor::default()),
        daemon_events: Arc::new(Mutex::new(events::DaemonEvents::default())),
    })
}

fn batch(generation: &str, next: u64, events: Vec<(u64, DomainEvent)>) -> WindowsAgentResult {
    let (event_sequences, events) = events.into_iter().unzip();
    WindowsAgentResult::BotEvents(WindowsBotEventBatch {
        generation: generation.into(),
        next,
        events,
        event_sequences,
    })
}

fn spec() -> WindowsBotSpec {
    WindowsBotSpec {
        intent_revision: None,
        remote_qr_login_enabled: false,
        metrics_environment: Default::default(),
        bot_id: "123".into(),
        config: ncd_test_support::BotConfigBuilder::new().build(),
        napcat_dir: "C:\\NapCat".into(),
        qq_executable: "C:\\QQ\\QQ.exe".into(),
        snowluma_dir: String::new(),
        node_executable: String::new(),
        data_root: "C:\\data".into(),
        webui_password: String::new(),
        webui_port: 0,
    }
}

#[tokio::test]
async fn attachment_skips_history_and_start_preserves_all_unread_events() {
    let host = Arc::new(ScriptHost::default());
    let bus = Arc::new(BroadcastEventBus::default());
    let mut subscription = bus.subscribe(EventFilter::all());
    let backend = backend(&host, &bus);
    let unread = DomainEvent::bot_log("123", "before start");
    let started = DomainEvent::bot_log("123", "this start");
    host.replies.lock().await.extend([
        batch("worker", 10, vec![]),
        batch("worker", 11, vec![]),
        WindowsAgentResult::BotStatus(BotStatus::running("123", 42, 1)),
        batch(
            "worker",
            12,
            vec![(11, unread.clone()), (12, started.clone())],
        ),
    ]);
    backend.ensure_event_baseline(false).await.unwrap();
    backend.start_with_event_baseline(spec()).await.unwrap();
    backend.poll_events().await.unwrap();
    assert_eq!(next_event(&mut subscription).await, unread);
    assert_eq!(next_event(&mut subscription).await, started);
    let requests = host.requests.lock().await;
    assert!(matches!(
        requests[0],
        WindowsAgentAction::BotEvents {
            after: u64::MAX,
            ..
        }
    ));
    assert!(matches!(requests[2], WindowsAgentAction::BotStart(_)));
    assert!(matches!(
        requests[3],
        WindowsAgentAction::BotEvents { after: 10, .. }
    ));
}

#[tokio::test]
async fn failed_baseline_does_not_submit_start() {
    let host = Arc::new(ScriptHost::default());
    let bus = Arc::new(BroadcastEventBus::default());
    let backend = backend(&host, &bus);
    assert!(backend.start_with_event_baseline(spec()).await.is_err());
    assert_eq!(host.requests.lock().await.len(), 1);
}

#[tokio::test]
async fn restarted_worker_history_is_not_published_as_live_lifecycle() {
    let host = Arc::new(ScriptHost::default());
    let bus = Arc::new(BroadcastEventBus::default());
    let mut subscription = bus.subscribe(EventFilter::all());
    let backend = backend(&host, &bus);
    let old_exit = DomainEvent::BotProcessExited {
        bot_id: BotId::new("123"),
        exit_code: Some(0),
        reason: None,
    };
    let live = DomainEvent::bot_log("123", "new worker live");
    host.replies.lock().await.extend([
        batch("old", 100, vec![]),
        batch("new", 5, vec![(1, old_exit)]),
        batch("new", 5, vec![]),
        batch("new", 6, vec![(6, live.clone())]),
    ]);
    backend.ensure_event_baseline(false).await.unwrap();
    backend.poll_events().await.unwrap();
    backend.poll_events().await.unwrap();
    assert_eq!(next_event(&mut subscription).await, live);
    assert!(matches!(
        host.requests.lock().await[3],
        WindowsAgentAction::BotEvents { after: 5, .. }
    ));
}

#[tokio::test]
async fn sibling_followers_scope_daemon_logs_per_bot_and_deduplicate_state() {
    let host = Arc::new(ScriptHost::default());
    let bus = Arc::new(BroadcastEventBus::default());
    let mut subscription = bus.subscribe(EventFilter::all());
    let first = backend(&host, &bus);
    let mut second = backend(&host, &bus);
    let second_fields = Arc::get_mut(&mut second).unwrap();
    second_fields.daemon_events = first.daemon_events.clone();
    second_fields.id = BotId::new("456");
    let daemon = DomainEvent::SnowLumaDaemonLog {
        line: "same text".into(),
    };
    let state =
        DomainEvent::snowluma_daemon_state_changed(ncd_domain::DaemonState::Ready, 2, None, None);
    host.replies.lock().await.extend([
        batch("worker", 0, vec![]),
        batch("worker", 0, vec![]),
        batch("worker", 10, vec![(1, daemon.clone()), (10, state.clone())]),
        batch("worker", 20, vec![(1, daemon.clone()), (10, state.clone())]),
        batch("worker", 20, vec![(20, daemon)]),
    ]);
    first.ensure_event_baseline(false).await.unwrap();
    second.ensure_event_baseline(false).await.unwrap();
    first.poll_events().await.unwrap();
    second.poll_events().await.unwrap();
    first.poll_events().await.unwrap();
    assert_eq!(
        next_event(&mut subscription).await,
        DomainEvent::bot_log("123", "same text")
    );
    assert_eq!(
        next_event(&mut subscription).await,
        DomainEvent::snowluma_daemon_state_changed(
            ncd_domain::DaemonState::Ready,
            2,
            None,
            Some("server".into()),
        )
    );
    assert_eq!(
        next_event(&mut subscription).await,
        DomainEvent::bot_log("456", "same text")
    );
    assert_eq!(
        next_event(&mut subscription).await,
        DomainEvent::bot_log("123", "same text")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), subscription.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn late_exit_requires_current_terminal_status_and_preserves_unread_logs() {
    for terminal in [false, true] {
        for unknown in [false, true] {
            let host = Arc::new(ScriptHost::default());
            let bus = Arc::new(BroadcastEventBus::default());
            let mut subscription = bus.subscribe(EventFilter::all());
            let backend = backend(&host, &bus);
            let exited = DomainEvent::BotProcessExited {
                bot_id: BotId::new("123"),
                exit_code: Some(0),
                reason: None,
            };
            let unread = DomainEvent::bot_log("123", "unread before start");
            let status = if unknown {
                WindowsAgentResult::Error(ncd_domain::windows_remote::WindowsAgentError {
                    code: "ProcessIdentityUnknown".into(),
                    message: "query denied".into(),
                    retryable: true,
                })
            } else if terminal {
                WindowsAgentResult::BotStatus(BotStatus::stopped("123"))
            } else {
                WindowsAgentResult::BotStatus(BotStatus::running("123", 42, 1))
            };
            host.replies.lock().await.extend([
                batch("worker", 10, vec![]),
                batch("worker", 12, vec![]),
                WindowsAgentResult::BotStatus(BotStatus::running("123", 42, 1)),
                batch(
                    "worker",
                    12,
                    vec![(11, exited.clone()), (12, unread.clone())],
                ),
                status,
            ]);
            backend.ensure_event_baseline(false).await.unwrap();
            backend.start_with_event_baseline(spec()).await.unwrap();
            backend.poll_events().await.unwrap();
            if terminal && !unknown {
                assert_eq!(next_event(&mut subscription).await, exited);
            }
            assert_eq!(next_event(&mut subscription).await, unread);
            assert!(
                tokio::time::timeout(Duration::from_millis(10), subscription.next())
                    .await
                    .is_err()
            );
            assert!(
                matches!(host.requests.lock().await.last(), Some(WindowsAgentAction::BotStatus { bot_id }) if bot_id == "123")
            );
        }
    }
}

#[tokio::test]
async fn closing_cancels_an_inflight_endpoint_without_recreating_a_tunnel() {
    for block_tunnel in [false, true] {
        let host = Arc::new(ScriptHost::default());
        let bus = Arc::new(BroadcastEventBus::default());
        let mut subscription = bus.subscribe(EventFilter::all());
        let backend = backend(&host, &bus);
        let (release, gate) = oneshot::channel();
        if block_tunnel {
            *host.tunnel_gate.lock().await = Some(gate);
        } else {
            *host.endpoint_gate.lock().await = Some(gate);
        }
        host.replies
            .lock()
            .await
            .push_back(WindowsAgentResult::BotEndpoint(WindowsBotEndpoint {
                backend: BackendType::NapCat,
                host_port: 6099,
                secret: "test".into(),
            }));
        let active = backend.clone();
        let request = tokio::spawn(async move { active.ensure_endpoint().await });
        tokio::time::timeout(Duration::from_secs(1), host.entered.notified())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), backend.close())
            .await
            .unwrap();
        let _ = release.send(());
        assert!(
            tokio::time::timeout(Duration::from_secs(1), request)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(backend.endpoint.lock().await.is_none());
        assert_eq!(host.tunnels_completed.load(Ordering::SeqCst), 0);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), subscription.next())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn closing_waits_for_the_follower_to_stop_before_returning() {
    let host = Arc::new(ScriptHost::default());
    let bus = Arc::new(BroadcastEventBus::default());
    let mut subscription = bus.subscribe(EventFilter::all());
    let backend = backend(&host, &bus);
    let (release, gate) = oneshot::channel();
    *host.event_gate.lock().await = Some(gate);
    host.replies.lock().await.extend([
        batch("worker", 0, vec![]),
        batch(
            "worker",
            1,
            vec![(1, DomainEvent::bot_log("123", "late event"))],
        ),
    ]);
    backend.ensure_follow().await;
    tokio::time::timeout(Duration::from_secs(1), host.entered.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), backend.close())
        .await
        .unwrap();
    let _ = release.send(());
    assert!(backend.following.lock().await.is_none());
    assert!(
        tokio::time::timeout(Duration::from_millis(10), subscription.next())
            .await
            .is_err()
    );
}
