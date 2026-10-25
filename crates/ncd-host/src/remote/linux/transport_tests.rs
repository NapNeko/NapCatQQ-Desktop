use super::*;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use russh::keys::key::KeyPair;
use russh::server::{self, Auth, Msg as ServerMsg, Session};
use russh::{ChannelId, CryptoVec, Disconnect};
use tokio::sync::{Notify, oneshot};

use crate::remote::{HostKeyPolicy, SshCredentials};

#[derive(Clone, Copy)]
enum SftpBehavior {
    HoldHandshake,
    HoldMetadata,
    HoldSubsystem,
    FailFirstRead,
    HoldWriteAcks,
}

#[derive(Clone, Copy)]
enum SessionPolicy {
    Normal,
    RejectSecond,
    LimitFour,
    HoldOpen,
    HoldExec,
}

#[derive(Default)]
struct SessionStats {
    opened: AtomicUsize,
    active: AtomicUsize,
    peak: AtomicUsize,
    rejected: AtomicUsize,
    executions: AtomicUsize,
    file_reads: AtomicUsize,
    kills: AtomicUsize,
    open_requested: Notify,
    release_open: Notify,
    kill_seen: Notify,
    first_exec: Notify,
    release_exec: Notify,
}

struct MockSession {
    behavior: SftpBehavior,
    policy: SessionPolicy,
    stats: Arc<SessionStats>,
    channels: HashMap<ChannelId, Channel<ServerMsg>>,
    sftp_channels: HashSet<ChannelId>,
    buffers: HashMap<ChannelId, Vec<u8>>,
    init_seen: Arc<Notify>,
    metadata_seen: Arc<Notify>,
    subsystem_seen: Arc<Notify>,
    write_seen: Arc<Notify>,
}

fn sftp_packet(session: &mut Session, channel: ChannelId, body: &[u8]) {
    let mut packet = Vec::with_capacity(body.len() + 4);
    packet.extend_from_slice(&(body.len() as u32).to_be_bytes());
    packet.extend_from_slice(body);
    session.data(channel, CryptoVec::from_slice(&packet));
}

fn sftp_status(session: &mut Session, channel: ChannelId, id: &[u8], code: u32) {
    let mut body = vec![101];
    body.extend_from_slice(id);
    body.extend_from_slice(&code.to_be_bytes());
    body.extend_from_slice(&[0; 8]);
    sftp_packet(session, channel, &body);
}

#[async_trait]
impl server::Handler for MockSession {
    type Error = russh::Error;

    async fn auth_password(&mut self, _: &str, _: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<ServerMsg>,
        _: &mut Session,
    ) -> Result<bool, Self::Error> {
        let ordinal = self.stats.opened.fetch_add(1, Ordering::SeqCst) + 1;
        if matches!(self.policy, SessionPolicy::HoldOpen) {
            self.stats.open_requested.notify_one();
            self.stats.release_open.notified().await;
        }
        if (matches!(self.policy, SessionPolicy::RejectSecond) && ordinal == 2)
            || (matches!(self.policy, SessionPolicy::LimitFour)
                && self.stats.active.load(Ordering::SeqCst) >= 4)
        {
            self.stats.rejected.fetch_add(1, Ordering::SeqCst);
            return Ok(false);
        }
        let active = self.stats.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.stats.peak.fetch_max(active, Ordering::SeqCst);
        self.channels.insert(channel.id(), channel);
        Ok(true)
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == "sftp" {
            self.subsystem_seen.notify_one();
            if matches!(self.behavior, SftpBehavior::HoldSubsystem) {
                return Ok(());
            }
            self.sftp_channels.insert(channel);
            session.channel_success(channel);
        } else {
            session.channel_failure(channel);
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        _: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel);
        let first = self.stats.executions.fetch_add(1, Ordering::SeqCst) == 0;
        if matches!(self.policy, SessionPolicy::HoldExec) {
            self.stats.first_exec.notify_one();
            return Ok(());
        }
        if matches!(self.policy, SessionPolicy::LimitFour)
            || (matches!(self.policy, SessionPolicy::RejectSecond) && first)
        {
            let handle = session.handle();
            let stats = Arc::clone(&self.stats);
            let policy = self.policy;
            self.stats.first_exec.notify_one();
            tokio::spawn(async move {
                if matches!(policy, SessionPolicy::RejectSecond) {
                    stats.release_exec.notified().await;
                } else {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                let _ = handle
                    .data(channel, CryptoVec::from_slice(b"exec-ok\n"))
                    .await;
                let _ = handle.exit_status_request(channel, 0).await;
                let _ = handle.eof(channel).await;
                stats.active.fetch_sub(1, Ordering::SeqCst);
                let _ = handle.close(channel).await;
            });
            return Ok(());
        }
        session.data(channel, CryptoVec::from_slice(b"exec-ok\n"));
        session.exit_status_request(channel, 0);
        session.eof(channel);
        session.close(channel);
        self.stats.active.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    async fn signal(
        &mut self,
        channel: ChannelId,
        signal: Sig,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if matches!(signal, Sig::KILL) {
            self.stats.kills.fetch_add(1, Ordering::SeqCst);
            self.stats.kill_seen.notify_one();
            session.close(channel);
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.sftp_channels.contains(&channel) {
            return Ok(());
        }
        let buffer = self.buffers.entry(channel).or_default();
        buffer.extend_from_slice(data);
        while buffer.len() >= 4 {
            let size = u32::from_be_bytes(buffer[..4].try_into().unwrap()) as usize;
            if buffer.len() < size + 4 {
                break;
            }
            let packet: Vec<_> = buffer.drain(..size + 4).collect();
            if packet.get(4) == Some(&1) {
                self.init_seen.notify_one();
                if !matches!(self.behavior, SftpBehavior::HoldHandshake) {
                    // SSH_FXP_VERSION, version 3, no extensions or extra requests.
                    session.data(channel, CryptoVec::from_slice(&[0, 0, 0, 5, 2, 0, 0, 0, 3]));
                }
            } else {
                self.metadata_seen.notify_one();
                if matches!(self.behavior, SftpBehavior::HoldMetadata) {
                    continue;
                }
                let id = &packet[5..9];
                match packet[4] {
                    3 => {
                        let mut body = vec![102];
                        body.extend_from_slice(id);
                        body.extend_from_slice(&7_u32.to_be_bytes());
                        body.extend_from_slice(b"fixture");
                        sftp_packet(session, channel, &body);
                    }
                    5 => {
                        let first = self.stats.file_reads.fetch_add(1, Ordering::SeqCst) == 0;
                        if matches!(self.behavior, SftpBehavior::FailFirstRead) && first {
                            sftp_status(session, channel, id, 4);
                            continue;
                        }
                        let handle_len =
                            u32::from_be_bytes(packet[9..13].try_into().unwrap()) as usize;
                        let offset = u64::from_be_bytes(
                            packet[13 + handle_len..21 + handle_len].try_into().unwrap(),
                        );
                        if offset != 0 {
                            sftp_status(session, channel, id, 1);
                        } else {
                            let data = b"fixture-public\n";
                            let mut body = vec![103];
                            body.extend_from_slice(id);
                            body.extend_from_slice(&(data.len() as u32).to_be_bytes());
                            body.extend_from_slice(data);
                            sftp_packet(session, channel, &body);
                        }
                    }
                    6 if matches!(self.behavior, SftpBehavior::HoldWriteAcks) => {
                        self.write_seen.notify_one();
                    }
                    _ => sftp_status(session, channel, id, 0),
                }
            }
        }
        Ok(())
    }
}

struct MockSsh {
    host: Arc<RemoteLinuxHost>,
    peer: server::Handle,
    worker: tokio::task::JoinHandle<()>,
    init_seen: Arc<Notify>,
    metadata_seen: Arc<Notify>,
    subsystem_seen: Arc<Notify>,
    write_seen: Arc<Notify>,
    stats: Arc<SessionStats>,
}

impl MockSsh {
    async fn connect(behavior: SftpBehavior) -> Self {
        Self::connect_with_policy(behavior, SessionPolicy::Normal).await
    }

    async fn connect_with_policy(behavior: SftpBehavior, policy: SessionPolicy) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let init_seen = Arc::new(Notify::new());
        let metadata_seen = Arc::new(Notify::new());
        let subsystem_seen = Arc::new(Notify::new());
        let write_seen = Arc::new(Notify::new());
        let stats = Arc::new(SessionStats::default());
        let handler = MockSession {
            behavior,
            policy,
            stats: Arc::clone(&stats),
            channels: HashMap::new(),
            sftp_channels: HashSet::new(),
            buffers: HashMap::new(),
            init_seen: Arc::clone(&init_seen),
            metadata_seen: Arc::clone(&metadata_seen),
            subsystem_seen: Arc::clone(&subsystem_seen),
            write_seen: Arc::clone(&write_seen),
        };
        let config = Arc::new(server::Config {
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            inactivity_timeout: None,
            keys: vec![KeyPair::generate_ed25519().unwrap()],
            ..Default::default()
        });
        let (ready, peer) = oneshot::channel();
        let worker = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let running = server::run_stream(config, stream, handler).await.unwrap();
            let _ = ready.send(running.handle());
            let _ = running.await;
        });
        let connection = ConnectionConfig::new(
            "127.0.0.1",
            port,
            SshCredentials::password("transport-test", "fixture-only-password"),
            HostKeyPolicy::Insecure,
        )
        .with_connect_timeout(Duration::from_secs(5))
        .with_keepalive(None);
        let host = Arc::new(
            RemoteLinuxHost::connect("transport-test", connection)
                .await
                .unwrap(),
        );
        Self {
            host,
            peer: peer.await.unwrap(),
            worker,
            init_seen,
            metadata_seen,
            subsystem_seen,
            write_seen,
            stats,
        }
    }

    async fn exec(&self) {
        let output = tokio::time::timeout(
            Duration::from_secs(2),
            self.host
                .run_to_string(HostCommand::new("fixture-exec").timeout(Duration::from_secs(2))),
        )
        .await
        .expect("an independent SSH exec was blocked behind SFTP")
        .unwrap();
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "exec-ok\n");
    }

    async fn close(self) {
        let _ = self
            .peer
            .disconnect(
                Disconnect::ByApplication,
                "test complete".into(),
                "en".into(),
            )
            .await;
        let mut worker = self.worker;
        if tokio::time::timeout(Duration::from_secs(2), &mut worker)
            .await
            .is_err()
        {
            worker.abort();
        }
    }
}

#[tokio::test]
async fn stalled_sftp_handshake_does_not_block_an_independent_exec() {
    let fixture = MockSsh::connect(SftpBehavior::HoldHandshake).await;
    let host = Arc::clone(&fixture.host);
    let opening = tokio::spawn(async move { host.sftp_session().await });
    tokio::time::timeout(Duration::from_secs(2), fixture.init_seen.notified())
        .await
        .expect("the server did not receive the SFTP init request");
    fixture.exec().await;
    assert!(!opening.is_finished());
    opening.abort();
    let _ = opening.await;
    fixture.close().await;
}

#[tokio::test]
async fn sftp_handshake_timeout_is_not_an_ssh_disconnect() {
    let fixture = MockSsh::connect(SftpBehavior::HoldHandshake).await;
    let host = Arc::clone(&fixture.host);
    let opening = tokio::spawn(async move { host.sftp_session().await });
    tokio::time::timeout(Duration::from_secs(2), fixture.init_seen.notified())
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(11)).await;
    let result = opening.await.unwrap();
    tokio::time::resume();
    let error = result
        .err()
        .expect("the unanswered SFTP init should time out");
    assert!(matches!(error, HostError::Timeout { .. }));
    assert!(!error.is_disconnect());
    assert!(fixture.host.handle.read().await.is_some());
    fixture.exec().await;
    fixture.close().await;
}

#[tokio::test]
async fn sftp_metadata_timeout_invalidates_only_sftp_and_exec_still_works() {
    let fixture = MockSsh::connect(SftpBehavior::HoldMetadata).await;
    let sftp = fixture.host.sftp_session().await.unwrap();
    sftp.set_timeout(1);
    let host = Arc::clone(&fixture.host);
    let checking =
        tokio::spawn(async move { host.exists(&HostPath::from_posix("/fixture-only")).await });
    tokio::time::timeout(Duration::from_secs(2), fixture.metadata_seen.notified())
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    let result = checking.await.unwrap();
    tokio::time::resume();
    let error = result.expect_err("the unanswered SFTP metadata request should time out");
    assert!(matches!(error, HostError::Timeout { .. }));
    assert!(!error.is_disconnect());
    assert!(fixture.host.handle.read().await.is_some());
    assert!(fixture.host.sftp.lock().await.is_none());
    fixture.exec().await;
    fixture.close().await;
}

#[tokio::test]
async fn concurrent_sftp_initializers_share_their_lock_and_subsystem_deadline() {
    let fixture = MockSsh::connect(SftpBehavior::HoldSubsystem).await;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let host = Arc::clone(&fixture.host);
        tasks.spawn(async move { host.sftp_session().await });
    }
    tokio::time::timeout(Duration::from_secs(2), fixture.subsystem_seen.notified())
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(SFTP_SESSION_TIMEOUT + Duration::from_secs(1)).await;
    let completed = tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(result) = tasks.join_next().await {
            let error = result.unwrap().err().expect("initialization must time out");
            assert!(matches!(error, HostError::Timeout { .. }));
        }
    })
    .await;
    tokio::time::resume();
    assert!(
        completed.is_ok(),
        "waiters retried full initialization budgets"
    );
    assert_eq!(fixture.stats.opened.load(Ordering::SeqCst), 1);
    assert!(fixture.host.sftp.lock().await.is_none());
    fixture.exec().await;
    fixture.close().await;
}

#[tokio::test]
async fn failed_sftp_file_read_rebuilds_only_the_bad_subsystem() {
    let fixture = MockSsh::connect(SftpBehavior::FailFirstRead).await;
    let previous = fixture.host.sftp_session().await.unwrap();
    let path = HostPath::from_posix("/fixture-public");
    let error = fixture.host.read_file(&path).await.unwrap_err();
    assert!(!error.is_disconnect());
    assert!(fixture.host.sftp.lock().await.is_none());
    fixture.exec().await;
    assert_eq!(
        fixture.host.read_file(&path).await.unwrap().as_ref(),
        b"fixture-public\n"
    );
    let replacement = fixture.host.sftp_session().await.unwrap();
    assert!(!Arc::ptr_eq(&previous, &replacement));
    fixture.close().await;
}

#[tokio::test]
async fn stalled_sftp_write_ack_has_a_budget_and_keeps_ssh_usable() {
    let fixture = MockSsh::connect(SftpBehavior::HoldWriteAcks).await;
    let host = Arc::clone(&fixture.host);
    let writing = tokio::spawn(async move {
        host.write_file(&HostPath::from_posix("/fixture-public"), b"fixture-data")
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), fixture.write_seen.notified())
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(SFTP_FILE_IO_TIMEOUT + Duration::from_secs(1)).await;
    let result = writing.await.unwrap();
    tokio::time::resume();
    assert!(matches!(result, Err(HostError::Timeout { .. })));
    assert!(fixture.host.sftp.lock().await.is_none());
    assert!(fixture.host.handle.read().await.is_some());
    fixture.exec().await;
    fixture.close().await;
}

#[tokio::test]
async fn channel_open_rejection_does_not_disconnect_an_existing_exec() {
    let fixture =
        MockSsh::connect_with_policy(SftpBehavior::HoldMetadata, SessionPolicy::RejectSecond).await;
    let host = Arc::clone(&fixture.host);
    let running = tokio::spawn(async move {
        host.run_to_string(HostCommand::new("first").timeout(Duration::from_secs(3)))
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), fixture.stats.first_exec.notified())
        .await
        .unwrap();
    let error = fixture
        .host
        .run_to_string(HostCommand::new("rejected").timeout(Duration::from_secs(2)))
        .await
        .expect_err("the second SSH session must be refused by the fixture");
    assert!(!error.is_disconnect());
    assert!(fixture.host.handle.read().await.is_some());
    fixture.stats.release_exec.notify_one();
    assert_eq!(running.await.unwrap().unwrap().stdout, "exec-ok\n");
    fixture.exec().await;
    fixture.close().await;
}

#[tokio::test]
async fn sixteen_mixed_short_execs_respect_session_capacity_without_failures() {
    let fixture =
        MockSsh::connect_with_policy(SftpBehavior::HoldMetadata, SessionPolicy::LimitFour).await;
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..16 {
        let host = Arc::clone(&fixture.host);
        tasks.spawn(async move {
            let command = HostCommand::new("fixture-exec").timeout(Duration::from_secs(3));
            if index % 2 == 0 {
                host.run_to_string(command).await
            } else {
                host.run_raw("fixture-exec", &command, None, 4096).await
            }
        });
    }
    while let Some(result) = tasks.join_next().await {
        let output = result.unwrap().unwrap();
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, "exec-ok\n");
    }
    assert_eq!(fixture.stats.executions.load(Ordering::SeqCst), 16);
    assert_eq!(fixture.stats.rejected.load(Ordering::SeqCst), 0);
    assert!(fixture.stats.peak.load(Ordering::SeqCst) <= 4);
    fixture.close().await;
}

#[tokio::test]
async fn streaming_precancelled_command_never_opens_a_session() {
    let fixture = MockSsh::connect(SftpBehavior::HoldMetadata).await;
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let result = fixture
        .host
        .run_streaming(
            HostCommand::new("must-not-run").cancel_token(cancel),
            Box::new(|_, _| {}),
        )
        .await;
    assert!(matches!(result, Err(HostError::Cancelled)));
    assert_eq!(fixture.stats.opened.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.stats.executions.load(Ordering::SeqCst), 0);
    fixture.close().await;
}

#[tokio::test]
async fn streaming_timeout_includes_waiting_for_channel_open_confirmation() {
    let fixture =
        MockSsh::connect_with_policy(SftpBehavior::HoldMetadata, SessionPolicy::HoldOpen).await;
    let host = Arc::clone(&fixture.host);
    let running = tokio::spawn(async move {
        host.run_streaming(
            HostCommand::new("must-not-run").timeout(Duration::from_secs(1)),
            Box::new(|_, _| {}),
        )
        .await
    });
    tokio::time::timeout(
        Duration::from_secs(2),
        fixture.stats.open_requested.notified(),
    )
    .await
    .unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    let result = tokio::time::timeout(Duration::from_secs(1), running).await;
    tokio::time::resume();
    assert!(matches!(
        result.unwrap().unwrap(),
        Err(HostError::Timeout { .. })
    ));
    assert_eq!(fixture.stats.executions.load(Ordering::SeqCst), 0);
    fixture.stats.release_open.notify_one();
    fixture.close().await;
}

#[tokio::test]
async fn streaming_cancel_during_channel_open_never_submits_exec() {
    let fixture =
        MockSsh::connect_with_policy(SftpBehavior::HoldMetadata, SessionPolicy::HoldOpen).await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let command_cancel = cancel.clone();
    let host = Arc::clone(&fixture.host);
    let running = tokio::spawn(async move {
        host.run_streaming(
            HostCommand::new("must-not-run")
                .timeout(Duration::from_secs(60))
                .cancel_token(command_cancel),
            Box::new(|_, _| {}),
        )
        .await
    });
    tokio::time::timeout(
        Duration::from_secs(2),
        fixture.stats.open_requested.notified(),
    )
    .await
    .unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), running)
        .await
        .expect("cancellation was blocked behind channel confirmation")
        .unwrap();
    assert!(matches!(result, Err(HostError::Cancelled)));
    assert_eq!(fixture.stats.executions.load(Ordering::SeqCst), 0);
    fixture.stats.release_open.notify_one();
    fixture.close().await;
}

#[tokio::test]
async fn streaming_cancel_after_exec_signals_the_remote_process() {
    let fixture =
        MockSsh::connect_with_policy(SftpBehavior::HoldMetadata, SessionPolicy::HoldExec).await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let command_cancel = cancel.clone();
    let host = Arc::clone(&fixture.host);
    let running = tokio::spawn(async move {
        host.run_streaming(
            HostCommand::new("fixture-held-exec").cancel_token(command_cancel),
            Box::new(|_, _| {}),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(2), fixture.stats.first_exec.notified())
        .await
        .unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), running)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(HostError::Cancelled)));
    tokio::time::timeout(Duration::from_secs(2), fixture.stats.kill_seen.notified())
        .await
        .expect("the remote command was abandoned without SIGKILL");
    assert_eq!(fixture.stats.kills.load(Ordering::SeqCst), 1);
    fixture.close().await;
}
