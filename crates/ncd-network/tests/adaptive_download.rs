//! Local HTTP regressions for mirror throughput selection and adaptive downloads.
//!
//! The server deliberately controls response headers and body pacing. No test
//! contacts a public mirror or depends on the machine's internet connection.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ncd_network::{
    ChunkedConfig, DownloadConfig, DownloadProgressSink, DownloadStage, LowSpeedConfig,
    MirrorRaceConfig, NetworkError, NoopProgressSink, ProgressUpdate, download_smart,
    download_with_mirror_race, download_with_resume,
};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Clone, Debug)]
struct Request {
    method: String,
    range: Option<(usize, Option<usize>)>,
}

impl Request {
    fn range_len(&self) -> Option<usize> {
        let (start, end) = self.range?;
        end?.checked_sub(start)?.checked_add(1)
    }
}

#[derive(Clone)]
struct Transfer {
    header_delay: Duration,
    first_byte_delay: Duration,
    chunk_bytes: usize,
    chunk_delay: Duration,
    status: Option<u16>,
}

impl Default for Transfer {
    fn default() -> Self {
        Self {
            header_delay: Duration::ZERO,
            first_byte_delay: Duration::ZERO,
            chunk_bytes: 64 * 1024,
            chunk_delay: Duration::ZERO,
            status: None,
        }
    }
}

struct LocalServer {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stopped: Arc<AtomicBool>,
    listener_task: Option<thread::JoinHandle<()>>,
}

impl LocalServer {
    fn start(
        body: Arc<Vec<u8>>,
        transfer: impl Fn(&Request) -> Transfer + Send + Sync + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local download fixture");
        listener
            .set_nonblocking(true)
            .expect("set fixture listener nonblocking");
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server_stopped = stopped.clone();
        let server_requests = requests.clone();
        let transfer = Arc::new(transfer);
        let listener_task = thread::spawn(move || {
            while !server_stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let stopped = server_stopped.clone();
                        let requests = server_requests.clone();
                        let transfer = transfer.clone();
                        let body = body.clone();
                        thread::spawn(move || {
                            serve_request(stream, &body, &requests, &stopped, &*transfer);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url,
            requests,
            stopped,
            listener_task: Some(listener_task),
        }
    }

    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    async fn wait_for_request(&self) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while self.requests().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("download should reach the local server");
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(task) = self.listener_task.take() {
            task.join().expect("fixture listener should exit cleanly");
        }
    }
}

fn serve_request(
    mut stream: TcpStream,
    body: &[u8],
    requests: &Mutex<Vec<Request>>,
    stopped: &AtomicBool,
    transfer: &dyn Fn(&Request) -> Transfer,
) {
    // Winsock accept inherits the nonblocking listener mode; a full send buffer
    // would make write_all return WouldBlock and truncate the HTTP response.
    stream
        .set_nonblocking(false)
        .expect("set fixture connection blocking");
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let mut raw = Vec::new();
    let mut buffer = [0; 1024];
    while !raw.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(count) => raw.extend_from_slice(&buffer[..count]),
        }
        if raw.len() > 16 * 1024 {
            return;
        }
    }
    let raw = String::from_utf8_lossy(&raw);
    let request = Request {
        method: raw
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().next())
            .unwrap_or("GET")
            .to_string(),
        range: raw.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if !name.eq_ignore_ascii_case("range") {
                return None;
            }
            let (start, end) = value.trim().strip_prefix("bytes=")?.split_once('-')?;
            Some((start.parse().ok()?, end.parse().ok()))
        }),
    };
    requests.lock().unwrap().push(request.clone());
    let plan = transfer(&request);
    if !pause(plan.header_delay, stopped) {
        return;
    }
    if let Some(status) = plan.status {
        let _ = write!(
            stream,
            "HTTP/1.1 {status} Fixture Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        return;
    }
    let (start, end) = request
        .range
        .map(|(start, end)| (start, end.unwrap_or(body.len() - 1).min(body.len() - 1)))
        .unwrap_or((0, body.len() - 1));
    if start > end {
        let _ = stream.write_all(
            b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }
    let status = if request.range.is_some() {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let content_range = if request.range.is_some() {
        format!("Content-Range: bytes {start}-{end}/{}\r\n", body.len())
    } else {
        String::new()
    };
    if write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n{content_range}Connection: close\r\n\r\n",
        end - start + 1,
    )
    .is_err()
        || request.method == "HEAD"
        || !pause(plan.first_byte_delay, stopped)
    {
        return;
    }
    for (index, chunk) in body[start..=end]
        .chunks(plan.chunk_bytes.max(1))
        .enumerate()
    {
        if index > 0 && !pause(plan.chunk_delay, stopped) {
            return;
        }
        if stopped.load(Ordering::Relaxed) || stream.write_all(chunk).is_err() {
            return;
        }
    }
}

fn pause(duration: Duration, stopped: &AtomicBool) -> bool {
    let started = Instant::now();
    while started.elapsed() < duration {
        if stopped.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep((duration - started.elapsed().min(duration)).min(Duration::from_millis(5)));
    }
    !stopped.load(Ordering::Relaxed)
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<ProgressUpdate>>);

#[async_trait]
impl DownloadProgressSink for RecordingSink {
    async fn tick(&self, update: ProgressUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

impl RecordingSink {
    fn first_streaming_mirror(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|update| update.stage == DownloadStage::Streaming)
            .and_then(|update| update.mirror_url.clone())
    }

    fn switched_to(&self, mirror: &str) -> bool {
        self.0.lock().unwrap().iter().any(|update| {
            update.stage == DownloadStage::SwitchingMirror
                && update.mirror_url.as_deref() == Some(mirror)
        })
    }
}

fn payload(bytes: usize) -> Arc<Vec<u8>> {
    Arc::new((0..bytes).map(|offset| (offset % 251) as u8).collect())
}

#[test]
fn fixture_handles_nonblocking_connections_with_delayed_headers_and_backpressure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let (stream, _) = listener.accept().unwrap();
    // Reproduce Winsock inheritance on every platform, before any request bytes.
    stream.set_nonblocking(true).unwrap();
    let body = payload(4 * 1024 * 1024);
    let response_body = body.clone();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let task = thread::spawn(move || {
        serve_request(
            stream,
            &response_body,
            &Mutex::new(Vec::new()),
            &AtomicBool::new(false),
            &|_| Transfer::default(),
        );
        done_tx.send(()).unwrap();
    });

    assert!(
        matches!(
            done_rx.recv_timeout(Duration::from_millis(50)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ),
        "an accepted connection must wait for request headers"
    );
    client
        .write_all(b"GET /asset HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    // Leave the response unread long enough for the socket send buffer to fill.
    thread::sleep(Duration::from_millis(50));
    let mut response = Vec::new();
    client.read_to_end(&mut response).unwrap();
    task.join().unwrap();
    let body_start = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .expect("response should contain complete headers")
        + 4;
    assert_eq!(&response[body_start..], body.as_slice());
}

fn race_config() -> MirrorRaceConfig {
    MirrorRaceConfig {
        initial_parallel: 2,
        stagger: Duration::from_millis(30),
        probe_first_chunk_timeout: Duration::from_secs(1),
        probe_duration: Duration::from_millis(200),
        probe_bytes: 16 * 1024,
        idle_timeout: Duration::from_secs(1),
        low_speed: None,
        ..Default::default()
    }
}

fn low_speed_config() -> LowSpeedConfig {
    LowSpeedConfig {
        min_bps: 128 * 1024,
        grace_period: Duration::from_millis(100),
        window: Duration::from_millis(200),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn faster_mirror_wins_even_when_direct_source_sends_first_byte_first() {
    let body = payload(128 * 1024);
    let direct = LocalServer::start(body.clone(), |_| Transfer {
        chunk_bytes: 512,
        chunk_delay: Duration::from_millis(35),
        ..Default::default()
    });
    let mirror = LocalServer::start(body.clone(), |_| Transfer {
        first_byte_delay: Duration::from_millis(60),
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let sink = Arc::new(RecordingSink::default());
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        download_with_mirror_race(
            &[direct.url.clone(), mirror.url.clone()],
            &dest,
            sink.clone(),
            CancellationToken::new(),
            race_config(),
        ),
    )
    .await
    .expect("throughput selection should avoid a long slow direct download")
    .unwrap();

    assert_eq!(result, body.len() as u64);
    assert_eq!(
        sink.first_streaming_mirror().as_deref(),
        Some(mirror.url.as_str())
    );
    assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_real_stream_switches_mirror_after_a_fast_probe() {
    let body = payload(128 * 1024);
    let direct = LocalServer::start(body.clone(), |request| {
        if request.range.is_some() {
            Transfer::default()
        } else {
            Transfer {
                chunk_bytes: 1024,
                chunk_delay: Duration::from_millis(40),
                ..Default::default()
            }
        }
    });
    let mirror = LocalServer::start(body.clone(), |_| Transfer {
        first_byte_delay: Duration::from_millis(60),
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let sink = Arc::new(RecordingSink::default());
    let mut cfg = race_config();
    cfg.low_speed = Some(low_speed_config());
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        download_with_mirror_race(
            &[direct.url.clone(), mirror.url.clone()],
            &dest,
            sink.clone(),
            CancellationToken::new(),
            cfg,
        ),
    )
    .await
    .expect("low throughput should trigger a mirror switch before idle timeout")
    .unwrap();

    assert_eq!(result, body.len() as u64);
    assert_eq!(
        sink.first_streaming_mirror().as_deref(),
        Some(direct.url.as_str())
    );
    assert!(sink.switched_to(&mirror.url));
    assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smart_download_uses_the_fastest_source_for_all_slices() {
    let body = payload(256 * 1024);
    let direct = LocalServer::start(body.clone(), |_| Transfer {
        chunk_bytes: 512,
        chunk_delay: Duration::from_millis(35),
        ..Default::default()
    });
    let mirror = LocalServer::start(body.clone(), |_| Transfer {
        first_byte_delay: Duration::from_millis(60),
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let sink = Arc::new(RecordingSink::default());
    let cfg = ChunkedConfig {
        threshold: 64 * 1024,
        parts: 4,
        idle_timeout: Duration::from_secs(1),
        race_cfg: race_config(),
        ..Default::default()
    };
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        download_smart(
            &[direct.url.clone(), mirror.url.clone()],
            &dest,
            sink.clone(),
            CancellationToken::new(),
            cfg,
        ),
    )
    .await
    .expect("smart download should select throughput before splitting")
    .unwrap();

    assert_eq!(result, body.len() as u64);
    assert_eq!(
        sink.first_streaming_mirror().as_deref(),
        Some(mirror.url.as_str())
    );
    let mut slices: Vec<_> = mirror
        .requests()
        .into_iter()
        .filter(|request| request.range_len() == Some(64 * 1024))
        .map(|request| request.range.unwrap())
        .collect();
    slices.sort_unstable();
    assert_eq!(
        slices,
        vec![
            (0, Some(65535)),
            (65536, Some(131071)),
            (131072, Some(196607)),
            (196608, Some(262143)),
        ]
    );
    assert!(
        direct
            .requests()
            .iter()
            .all(|request| request.range_len().is_some_and(|len| len <= 16 * 1024))
    );
    assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slice_failure_keeps_caller_alive_and_falls_back_to_streaming() {
    let body = payload(256 * 1024);
    let broken = LocalServer::start(body.clone(), |request| Transfer {
        status: if request.range_len().is_some_and(|len| len <= 16 * 1024) {
            None
        } else {
            Some(503)
        },
        ..Default::default()
    });
    let mirror = LocalServer::start(body.clone(), |_| Transfer {
        first_byte_delay: Duration::from_millis(60),
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let cancel = CancellationToken::new();
    let cfg = ChunkedConfig {
        threshold: 64 * 1024,
        parts: 4,
        idle_timeout: Duration::from_secs(1),
        race_cfg: race_config(),
        ..Default::default()
    };
    let result = tokio::time::timeout(
        Duration::from_secs(4),
        download_smart(
            &[broken.url.clone(), mirror.url.clone()],
            &dest,
            Arc::new(NoopProgressSink),
            cancel.clone(),
            cfg,
        ),
    )
    .await
    .expect("slice failure should finish through the streaming fallback")
    .unwrap();

    assert_eq!(result, body.len() as u64);
    assert!(
        !cancel.is_cancelled(),
        "internal slice failure must not cancel its caller"
    );
    assert!(
        broken
            .requests()
            .iter()
            .any(|request| request.range_len() == Some(64 * 1024))
    );
    assert!(
        mirror
            .requests()
            .iter()
            .any(|request| request.range.is_none())
    );
    assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
    for index in 0..4 {
        assert!(!dir.path().join(format!("asset.bin.chunk-{index}")).exists());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn all_slow_sources_still_finish_instead_of_failing_on_speed_alone() {
    // Keep more than min_bps * window outstanding when the monitor samples;
    // otherwise the useful "nearly complete" exemption would skip switching.
    let body = payload(96 * 1024);
    let slow = || Transfer {
        chunk_bytes: 1024,
        chunk_delay: Duration::from_millis(25),
        ..Default::default()
    };
    let direct = LocalServer::start(body.clone(), move |_| slow());
    let mirror = LocalServer::start(body.clone(), move |_| slow());
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let cancel = CancellationToken::new();
    let sink = Arc::new(RecordingSink::default());
    let mut cfg = race_config();
    cfg.probe_bytes = 4 * 1024;
    cfg.low_speed = Some(low_speed_config());
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        download_with_mirror_race(
            &[direct.url.clone(), mirror.url.clone()],
            &dest,
            sink.clone(),
            cancel.clone(),
            cfg,
        ),
    )
    .await
    .expect("available slow sources should remain usable after speed fallback")
    .expect("speed alone must not reject every available source");

    assert_eq!(result, body.len() as u64);
    assert!(!cancel.is_cancelled());
    assert!(sink.switched_to(&direct.url) || sink.switched_to(&mirror.url));
    assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_low_speed_is_distinct_from_idle_timeout() {
    let body = payload(128 * 1024);
    let server = LocalServer::start(body, |_| Transfer {
        chunk_bytes: 1024,
        chunk_delay: Duration::from_millis(40),
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        download_with_resume(
            &server.url,
            &dest,
            Arc::new(NoopProgressSink),
            CancellationToken::new(),
            DownloadConfig {
                idle_timeout: Duration::from_secs(1),
                low_speed: Some(low_speed_config()),
                ..Default::default()
            },
        ),
    )
    .await
    .expect("continuously arriving slow bytes should trigger a speed decision");

    match result {
        Err(NetworkError::SlowDownload { speed_bps, min_bps }) => {
            assert!(speed_bps < min_bps);
            assert_eq!(min_bps, 128 * 1024);
        }
        other => panic!("expected SlowDownload, got {other:?}"),
    }
    assert!(
        !dest.exists(),
        "incomplete bytes must never become the destination"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_while_waiting_for_headers_returns_immediately() {
    let server = LocalServer::start(payload(1024), |_| Transfer {
        header_delay: Duration::from_secs(2),
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let url = server.url.clone();
    let download = tokio::spawn(async move {
        download_with_resume(
            &url,
            &dest,
            Arc::new(NoopProgressSink),
            task_cancel,
            DownloadConfig {
                idle_timeout: Duration::from_secs(3),
                ..Default::default()
            },
        )
        .await
    });
    server.wait_for_request().await;
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_millis(200), download)
        .await
        .expect("cancellation must interrupt response-header wait")
        .expect("download task should not panic");
    assert!(matches!(result, Err(NetworkError::Cancelled)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smart_download_does_not_resume_a_part_file_from_an_unknown_source() {
    let body = payload(64 * 1024);
    let server = LocalServer::start(body.clone(), |_| Transfer::default());
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    // The length is plausible, but these bytes belong to a different source.
    tokio::fs::write(ncd_network::range::part_path(&dest), vec![255; 4096])
        .await
        .unwrap();
    let result = download_smart(
        std::slice::from_ref(&server.url),
        &dest,
        Arc::new(NoopProgressSink),
        CancellationToken::new(),
        ChunkedConfig {
            threshold: u64::MAX,
            race_cfg: race_config(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(result, body.len() as u64);
    assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
    assert!(
        server
            .requests()
            .iter()
            .any(|request| request.range.is_none())
    );
    assert!(
        server
            .requests()
            .iter()
            .all(|request| request.range != Some((4096, None)))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_slices_can_return_to_the_primary_stream_when_backups_fail() {
    let body = payload(512 * 1024);
    let primary = LocalServer::start(body.clone(), |request| {
        if request.range_len().is_some_and(|len| len > 16 * 1024) {
            // Four such slices together remain below the 128 KiB/s floor.
            Transfer {
                chunk_bytes: 1024,
                chunk_delay: Duration::from_millis(50),
                ..Default::default()
            }
        } else {
            Transfer::default()
        }
    });
    let backup = LocalServer::start(body.clone(), |request| Transfer {
        first_byte_delay: Duration::from_millis(60),
        status: if request.range.is_none() {
            Some(503)
        } else {
            None
        },
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let cancel = CancellationToken::new();
    let sink = Arc::new(RecordingSink::default());
    let mut race_cfg = race_config();
    race_cfg.low_speed = Some(low_speed_config());
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        download_smart(
            &[primary.url.clone(), backup.url.clone()],
            &dest,
            sink.clone(),
            cancel.clone(),
            ChunkedConfig {
                parts: 4,
                threshold: 64 * 1024,
                idle_timeout: Duration::from_secs(1),
                race_cfg,
                ..Default::default()
            },
        ),
    )
    .await
    .expect("aggregate low speed should permit a usable primary stream fallback")
    .unwrap();

    assert_eq!(result, body.len() as u64);
    assert_eq!(
        sink.first_streaming_mirror().as_deref(),
        Some(primary.url.as_str())
    );
    assert!(sink.switched_to(&primary.url));
    assert!(!cancel.is_cancelled());
    assert!(
        primary
            .requests()
            .iter()
            .any(|request| request.range_len() == Some(128 * 1024))
    );
    assert!(
        backup
            .requests()
            .iter()
            .any(|request| request.range.is_none())
    );
    assert!(
        primary
            .requests()
            .iter()
            .any(|request| request.range.is_none())
    );
    assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
    for index in 0..4 {
        assert!(!dir.path().join(format!("asset.bin.chunk-{index}")).exists());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_while_waiting_for_probe_headers_returns_immediately() {
    let server = LocalServer::start(payload(1024), |_| Transfer {
        header_delay: Duration::from_secs(2),
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let url = server.url.clone();
    let mut race_cfg = race_config();
    race_cfg.probe_first_chunk_timeout = Duration::from_secs(3);
    let download = tokio::spawn(async move {
        download_smart(
            &[url],
            &dest,
            Arc::new(NoopProgressSink),
            task_cancel,
            ChunkedConfig {
                race_cfg,
                ..Default::default()
            },
        )
        .await
    });
    server.wait_for_request().await;
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_millis(200), download)
        .await
        .expect("cancellation must interrupt probe response-header wait")
        .expect("smart download task should not panic");
    assert!(matches!(result, Err(NetworkError::Cancelled)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn progress_ticks_do_not_extend_the_idle_deadline_without_new_bytes() {
    let server = LocalServer::start(payload(64 * 1024), |_| Transfer {
        chunk_bytes: 1024,
        chunk_delay: Duration::from_secs(3),
        ..Default::default()
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    // The 500 ms progress tick happens before this timeout. It must preserve
    // the deadline measured from the last body byte rather than restart it.
    let idle_timeout = Duration::from_millis(900);
    let result = tokio::time::timeout(
        Duration::from_millis(1500),
        download_with_resume(
            &server.url,
            &dest,
            Arc::new(NoopProgressSink),
            CancellationToken::new(),
            DownloadConfig {
                idle_timeout,
                low_speed: None,
                ..Default::default()
            },
        ),
    )
    .await
    .expect("progress ticker must not keep a stalled stream alive");

    assert!(matches!(result, Err(NetworkError::IdleTimeout(duration)) if duration == idle_timeout));
    assert!(!dest.exists());
}

#[tokio::test]
async fn range_rejections_fall_back_to_a_normal_get_probe() {
    let body = payload(32 * 1024);
    for status in [400, 405, 416, 501] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(|request: &wiremock::Request| request.headers.contains_key("range"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(|request: &wiremock::Request| !request.headers.contains_key("range"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.as_ref().clone()))
            .expect(2)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("asset.bin");
        let result = download_smart(
            &[format!("{}/asset", server.uri())],
            &dest,
            Arc::new(NoopProgressSink),
            CancellationToken::new(),
            ChunkedConfig {
                race_cfg: race_config(),
                ..Default::default()
            },
        )
        .await
        .unwrap_or_else(|error| panic!("Range rejection {status} should allow GET: {error}"));

        assert_eq!(result, body.len() as u64);
        assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
        server.verify().await;
    }
}

#[tokio::test]
async fn a_valid_empty_file_can_be_probed_and_downloaded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(Vec::new()))
        .expect(2)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("empty.bin");
    let result = download_smart(
        &[format!("{}/asset", server.uri())],
        &dest,
        Arc::new(NoopProgressSink),
        CancellationToken::new(),
        ChunkedConfig {
            threshold: 1,
            race_cfg: race_config(),
            ..Default::default()
        },
    )
    .await
    .expect("a declared Content-Length of zero is a legitimate file");

    assert_eq!(result, 0);
    assert_eq!(tokio::fs::metadata(dest).await.unwrap().len(), 0);
    server.verify().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checksum_mismatch_switches_sources_for_streaming_and_slices() {
    let body = payload(256 * 1024);
    let expected = hex::encode(Sha256::digest(body.as_slice()));
    for chunked in [false, true] {
        // Same length and valid ranges are insufficient when a mirror serves
        // another cached asset. The final hash must reject it and try the next.
        let poisoned =
            LocalServer::start(Arc::new(vec![b'X'; body.len()]), |_| Transfer::default());
        let correct = LocalServer::start(body.clone(), |_| Transfer {
            first_byte_delay: Duration::from_millis(60),
            ..Default::default()
        });
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("asset.bin");
        let mut race_cfg = race_config();
        race_cfg.expected_sha256 = Some(expected.clone());
        let result = if chunked {
            download_smart(
                &[poisoned.url.clone(), correct.url.clone()],
                &dest,
                Arc::new(NoopProgressSink),
                CancellationToken::new(),
                ChunkedConfig {
                    threshold: 64 * 1024,
                    race_cfg,
                    ..Default::default()
                },
            )
            .await
        } else {
            download_with_mirror_race(
                &[poisoned.url.clone(), correct.url.clone()],
                &dest,
                Arc::new(NoopProgressSink),
                CancellationToken::new(),
                race_cfg,
            )
            .await
        };
        assert_eq!(result.unwrap(), body.len() as u64);
        assert_eq!(tokio::fs::read(dest).await.unwrap(), *body);
        assert!(
            correct
                .requests()
                .iter()
                .any(|request| request.range.is_none())
        );
    }
}
