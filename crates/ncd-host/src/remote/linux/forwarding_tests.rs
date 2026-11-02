//! 本地 SSH 服务验证转发确认停滞、控制隔离及活动流回收。

use super::*;
use russh::keys::key::KeyPair;
use russh::server::{self, Auth, Session};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
struct State {
    stall: AtomicBool,
    reverse_stall: AtomicBool,
    opened: Notify,
    resume: Notify,
    active: AtomicUsize,
    connections: AtomicUsize,
}

struct Echo {
    state: Arc<State>,
    channels: std::collections::HashSet<russh::ChannelId>,
}

#[async_trait]
impl server::Handler for Echo {
    type Error = russh::Error;

    async fn auth_password(&mut self, _: &str, _: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _: Channel<server::Msg>,
        _: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn tcpip_forward(
        &mut self,
        _: &str,
        port: &mut u32,
        _: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.state.opened.notify_one();
        if self.state.reverse_stall.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        if *port == 0 {
            *port = 43123;
        }
        Ok(true)
    }

    async fn cancel_tcpip_forward(
        &mut self,
        _: &str,
        _: u32,
        _: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn exec_request(
        &mut self,
        channel: russh::ChannelId,
        _: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel);
        session.data(channel, russh::CryptoVec::from_slice(b"control-ok"));
        session.exit_status_request(channel, 0);
        session.eof(channel);
        session.close(channel);
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<server::Msg>,
        _: &str,
        _: u32,
        _: &str,
        _: u32,
        _: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.state.opened.notify_one();
        if self.state.stall.load(Ordering::SeqCst) {
            self.state.resume.notified().await;
        }
        self.channels.insert(channel.id());
        self.state.active.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }

    async fn data(
        &mut self,
        channel: russh::ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, russh::CryptoVec::from_slice(data));
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: russh::ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.close(channel);
        if self.channels.remove(&channel) {
            self.state.active.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: russh::ChannelId,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.channels.remove(&channel) {
            self.state.active.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

impl Drop for Echo {
    fn drop(&mut self) {
        self.state
            .active
            .fetch_sub(self.channels.len(), Ordering::SeqCst);
    }
}

struct Fixture {
    host: Arc<RemoteLinuxHost>,
    state: Arc<State>,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(State::default());
        let worker_state = Arc::clone(&state);
        let config = Arc::new(server::Config {
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            inactivity_timeout: None,
            keys: vec![KeyPair::generate_ed25519().unwrap()],
            ..Default::default()
        });
        let stop = CancellationToken::new();
        let stopped = stop.clone();
        let task = tokio::spawn(async move {
            let mut sessions = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = stopped.cancelled() => break,
                    Some(_) = sessions.join_next(), if !sessions.is_empty() => {},
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.unwrap();
                        worker_state.connections.fetch_add(1, Ordering::SeqCst);
                        let handler = Echo { state: Arc::clone(&worker_state), channels: Default::default() };
                        let config = Arc::clone(&config);
                        sessions.spawn(async move { if let Ok(running) = server::run_stream(config, stream, handler).await { let _ = running.await; } });
                    }
                }
            }
            sessions.shutdown().await;
        });
        let config = ConnectionConfig::new(
            "127.0.0.1",
            port,
            SshCredentials::password("fixture", "fixture-only"),
            HostKeyPolicy::Insecure,
        )
        .with_connect_timeout(Duration::from_secs(3));
        let host = Arc::new(
            RemoteLinuxHost::connect_transport("fixture", config)
                .await
                .unwrap(),
        );
        Self {
            host,
            state,
            stop,
            task,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}

async fn assert_stream_closed(stream: &mut TcpStream) {
    let result = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut [0u8; 1]))
        .await
        .expect("隧道关闭后 TCP 流应及时断开");
    // 取消转发时，Windows 可能通过 TCP reset 而不是 EOF 报告断开。
    assert!(
        matches!(result, Ok(0))
            || matches!(result, Err(ref error) if error.kind() == std::io::ErrorKind::ConnectionReset),
        "隧道关闭后收到意外结果: {result:?}"
    );
}

#[tokio::test]
async fn stalled_forward_confirmation_does_not_block_control_or_tunnel_close() {
    let fixture = Fixture::new().await;
    fixture.state.stall.store(true, Ordering::SeqCst);
    let mut tunnel = fixture
        .host
        .open_tunnel(TunnelSpec::local_to_remote(0, 1234))
        .await
        .unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", tunnel.local_port()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), fixture.state.opened.notified())
        .await
        .unwrap();
    let output = fixture
        .host
        .run_to_string(HostCommand::new("true").timeout(Duration::from_secs(1)))
        .await
        .unwrap();
    assert_eq!(output.stdout, "control-ok");
    tokio::time::timeout(Duration::from_secs(1), tunnel.close_and_wait())
        .await
        .unwrap();
    assert_stream_closed(&mut stream).await;
    assert_eq!(fixture.state.connections.load(Ordering::SeqCst), 2);
    assert!(fixture.host.is_healthy().await);
    fixture.state.stall.store(false, Ordering::SeqCst);
    fixture.state.resume.notify_one();
    let mut next = fixture
        .host
        .open_tunnel(TunnelSpec::local_to_remote(0, 1234))
        .await
        .unwrap();
    assert_eq!(fixture.state.connections.load(Ordering::SeqCst), 3);
    next.close_and_wait().await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture.state.active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn stalled_forward_confirmation_times_out_and_invalidates_its_leases() {
    let fixture = Fixture::new().await;
    let mut changes = fixture.host.transport_changes().unwrap();
    fixture.state.stall.store(true, Ordering::SeqCst);
    let tunnel = fixture
        .host
        .open_tunnel(TunnelSpec::local_to_remote(0, 1234))
        .await
        .unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", tunnel.local_port()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(8), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert!(tunnel.is_closed());
    assert!(fixture.host.is_healthy().await);
    assert_stream_closed(&mut stream).await;
}

#[tokio::test]
async fn tunnel_close_reclaims_an_active_ssh_stream_and_next_lease_reuses_transport() {
    let fixture = Fixture::new().await;
    for _ in 0..100 {
        let mut tunnel = fixture
            .host
            .open_tunnel(TunnelSpec::local_to_remote(0, 1234))
            .await
            .unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", tunnel.local_port()))
            .await
            .unwrap();
        stream.write_all(b"ping").await.unwrap();
        let mut response = [0; 4];
        tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut response))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&response, b"ping");
        tunnel.close_and_wait().await;
        assert_stream_closed(&mut stream).await;
    }
    assert_eq!(fixture.state.connections.load(Ordering::SeqCst), 2);
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture.state.active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    fixture.host.invalidate_connection().await;
    assert!(!fixture.host.is_healthy().await);
}

#[tokio::test]
async fn stalled_reverse_forward_times_out_without_retiring_the_control_connection() {
    let fixture = Fixture::new().await;
    let mut changes = fixture.host.transport_changes().unwrap();
    let existing = fixture
        .host
        .open_tunnel(TunnelSpec::local_to_remote(0, 1234))
        .await
        .unwrap();
    fixture.state.reverse_stall.store(true, Ordering::SeqCst);
    let host = Arc::clone(&fixture.host);
    let opening =
        tokio::spawn(async move { host.open_tunnel(TunnelSpec::remote_to_local(0, 1234)).await });
    tokio::time::timeout(Duration::from_secs(3), fixture.state.opened.notified())
        .await
        .unwrap();
    let output = fixture
        .host
        .run_to_string(HostCommand::new("true").timeout(Duration::from_secs(1)))
        .await
        .unwrap();
    assert_eq!(output.stdout, "control-ok");
    let result = tokio::time::timeout(Duration::from_secs(8), opening)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result,
        Err(HostError::Timeout {
            operation: "reverse_tunnel_open"
        })
    ));
    assert!(fixture.host.is_healthy().await);
    tokio::time::timeout(Duration::from_secs(1), changes.changed())
        .await
        .unwrap()
        .unwrap();
    assert!(
        existing.is_closed(),
        "旧租约必须立即失效，不能等监听任务轮询"
    );
    fixture.state.reverse_stall.store(false, Ordering::SeqCst);
    let mut next = fixture
        .host
        .open_tunnel(TunnelSpec::local_to_remote(0, 1234))
        .await
        .unwrap();
    assert!(!next.is_closed());
    next.close_and_wait().await;
}
