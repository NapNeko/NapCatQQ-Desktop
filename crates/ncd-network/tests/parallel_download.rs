//! Local HTTP coverage for parallel scheduling, representation safety, and atomic publication.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ncd_network::{
    ChunkedConfig, DownloadProgressSink, DownloadStage, MirrorRaceConfig, NetworkError,
    NoopProgressSink, ProgressUpdate, download_smart, download_url,
};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

const MIB: usize = 1024 * 1024;
const V1: &str = "\"fixture-v1\"";
const V2: &str = "\"fixture-v2\"";
static TEST_DOWNLOADS: OnceLock<tokio::sync::RwLock<()>> = OnceLock::new();

async fn test_guard() -> tokio::sync::RwLockReadGuard<'static, ()> {
    TEST_DOWNLOADS
        .get_or_init(|| tokio::sync::RwLock::new(()))
        .read()
        .await
}

#[derive(Clone, Debug)]
struct Request {
    range: Option<(usize, Option<usize>)>,
    if_range: Option<String>,
    accept_encoding: Option<String>,
}

impl Request {
    fn is_piece(&self) -> bool {
        self.if_range.is_some()
            || self.range.is_some_and(|(start, end)| {
                end.and_then(|end| end.checked_sub(start))
                    .is_some_and(|length| length + 1 >= MIB)
            })
    }
}

struct ResponsePlan {
    body: Arc<Vec<u8>>,
    etag: Option<&'static str>,
    ignore_range: bool,
    reported_total: Option<usize>,
    cut_after: Option<usize>,
    hold_body: Option<Arc<AtomicBool>>,
    hold_after_bytes: Option<usize>,
    chunk_bytes: usize,
    chunk_delay: Duration,
}

impl ResponsePlan {
    fn new(body: Arc<Vec<u8>>) -> Self {
        Self {
            body,
            etag: Some(V1),
            ignore_range: false,
            reported_total: None,
            cut_after: None,
            hold_body: None,
            hold_after_bytes: None,
            chunk_bytes: 64 * 1024,
            chunk_delay: Duration::ZERO,
        }
    }
}

#[derive(Default)]
struct Observations {
    requests: Mutex<Vec<Request>>,
    active_pieces: AtomicUsize,
    peak_pieces: AtomicUsize,
    body_gates: AtomicUsize,
    client_closed_pieces: AtomicUsize,
}

struct ActivePiece<'a>(&'a Observations);

impl Drop for ActivePiece<'_> {
    fn drop(&mut self) {
        self.0.active_pieces.fetch_sub(1, Ordering::SeqCst);
    }
}

struct LocalServer {
    url: String,
    observations: Arc<Observations>,
    stopped: Arc<AtomicBool>,
    task: Option<thread::JoinHandle<()>>,
}

impl LocalServer {
    fn start(plan: impl Fn(&Request) -> ResponsePlan + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/asset", listener.local_addr().unwrap());
        let observations = Arc::new(Observations::default());
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_observations = observations.clone();
        let thread_stopped = stopped.clone();
        let plan = Arc::new(plan);
        let task = thread::spawn(move || {
            let mut connections = Vec::new();
            while !thread_stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let observations = thread_observations.clone();
                        let stopped = thread_stopped.clone();
                        let plan = plan.clone();
                        connections.push(thread::spawn(move || {
                            serve(stream, &observations, &stopped, &*plan);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(_) => break,
                }
            }
            for connection in connections {
                connection.join().expect("HTTP fixture worker panicked");
            }
        });
        Self {
            url,
            observations,
            stopped,
            task: Some(task),
        }
    }

    fn requests(&self) -> Vec<Request> {
        self.observations.requests.lock().unwrap().clone()
    }

    fn pieces(&self) -> Vec<Request> {
        self.requests()
            .into_iter()
            .filter(Request::is_piece)
            .collect()
    }

    fn peak_pieces(&self) -> usize {
        self.observations.peak_pieces.load(Ordering::SeqCst)
    }

    async fn wait_for_pieces(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while self.pieces().len() < count {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("download did not issue the expected piece requests");
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(task) = self.task.take() {
            task.join().expect("HTTP fixture listener panicked");
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut raw = Vec::new();
    let mut buf = [0; 1024];
    while !raw.windows(4).any(|window| window == b"\r\n\r\n") {
        let count = stream.read(&mut buf).ok()?;
        if count == 0 || raw.len() > 16 * 1024 {
            return None;
        }
        raw.extend_from_slice(&buf[..count]);
    }
    let raw = String::from_utf8_lossy(&raw);
    let header = |name: &str| {
        raw.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    };
    let range = header("range").and_then(|value| {
        let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
        Some((start.parse().ok()?, end.parse().ok()))
    });
    Some(Request {
        range,
        if_range: header("if-range"),
        accept_encoding: header("accept-encoding"),
    })
}

fn serve(
    mut stream: TcpStream,
    observations: &Observations,
    stopped: &AtomicBool,
    plan: &dyn Fn(&Request) -> ResponsePlan,
) {
    // Winsock accepted sockets inherit nonblocking mode from the listener.
    stream.set_nonblocking(false).unwrap();
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let _active_piece = request.is_piece().then(|| {
        let count = observations.active_pieces.fetch_add(1, Ordering::SeqCst) + 1;
        observations.peak_pieces.fetch_max(count, Ordering::SeqCst);
        ActivePiece(observations)
    });
    observations.requests.lock().unwrap().push(request.clone());
    let plan = plan(&request);
    let partial = request.range.is_some() && !plan.ignore_range;
    let (start, end) = if partial {
        let (start, end) = request.range.unwrap();
        (
            start,
            end.unwrap_or(plan.body.len() - 1).min(plan.body.len() - 1),
        )
    } else {
        (0, plan.body.len() - 1)
    };
    if start > end {
        let _ = stream.write_all(
            b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        return;
    }
    let status = if partial {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let etag = plan
        .etag
        .map(|etag| format!("ETag: {etag}\r\n"))
        .unwrap_or_default();
    let content_range = if partial {
        format!(
            "Content-Range: bytes {start}-{end}/{}\r\n",
            plan.reported_total.unwrap_or(plan.body.len())
        )
    } else {
        String::new()
    };
    if write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n{etag}{content_range}Connection: close\r\n\r\n",
        end - start + 1
    )
    .is_err()
    {
        return;
    }
    if plan.hold_after_bytes.is_none() {
        if let Some(gate) = &plan.hold_body {
            if !wait_at_body_gate(&mut stream, gate, stopped, observations, request.is_piece()) {
                return;
            }
        }
    }
    let send_len = plan
        .cut_after
        .unwrap_or(end - start + 1)
        .min(end - start + 1);
    let mut sent = 0;
    let mut held = false;
    for chunk in plan.body[start..start + send_len].chunks(plan.chunk_bytes.max(1)) {
        if stopped.load(Ordering::SeqCst) {
            return;
        }
        if !plan.chunk_delay.is_zero() {
            thread::sleep(plan.chunk_delay);
        }
        if stream.write_all(chunk).is_err() {
            return;
        }
        sent += chunk.len();
        if !held && plan.hold_after_bytes.is_some_and(|after| sent >= after) {
            held = true;
            if let Some(gate) = &plan.hold_body {
                if !wait_at_body_gate(&mut stream, gate, stopped, observations, request.is_piece())
                {
                    return;
                }
            }
        }
    }
}

fn wait_at_body_gate(
    stream: &mut TcpStream,
    gate: &AtomicBool,
    stopped: &AtomicBool,
    observations: &Observations,
    is_piece: bool,
) -> bool {
    observations.body_gates.fetch_add(1, Ordering::SeqCst);
    while !gate.load(Ordering::SeqCst) {
        if stopped.load(Ordering::SeqCst) {
            return false;
        }
        // No response body is produced while gated, so peer closure proves cancellation.
        stream.set_nonblocking(true).unwrap();
        let closed = match stream.peek(&mut [0; 1]) {
            Ok(0) => true,
            Err(error) => matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::NotConnected
            ),
            _ => false,
        };
        stream.set_nonblocking(false).unwrap();
        if closed {
            if is_piece {
                observations
                    .client_closed_pieces
                    .fetch_add(1, Ordering::SeqCst);
            }
            return false;
        }
        thread::sleep(Duration::from_millis(2));
    }
    true
}

#[derive(Default)]
struct StreamingTicks(AtomicUsize);

#[async_trait]
impl DownloadProgressSink for StreamingTicks {
    async fn tick(&self, update: ProgressUpdate) {
        if update.stage == DownloadStage::Streaming {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn payload(len: usize) -> Arc<Vec<u8>> {
    Arc::new((0..len).map(|offset| (offset % 251) as u8).collect())
}

fn config(parts: usize) -> ChunkedConfig {
    ChunkedConfig {
        parts,
        threshold: 1,
        idle_timeout: Duration::from_secs(15),
        race_cfg: MirrorRaceConfig {
            initial_parallel: 1,
            stagger: Duration::from_millis(1),
            probe_first_chunk_timeout: Duration::from_secs(5),
            probe_duration: Duration::from_millis(20),
            probe_bytes: 16 * 1024,
            idle_timeout: Duration::from_secs(15),
            low_speed: None,
            ..Default::default()
        },
        ..Default::default()
    }
}

async fn fetch(server: &LocalServer, dest: &Path, cfg: ChunkedConfig) -> Result<u64, NetworkError> {
    tokio::time::timeout(
        Duration::from_secs(30),
        download_url(
            &server.url,
            dest,
            Arc::new(NoopProgressSink),
            CancellationToken::new(),
            cfg,
        ),
    )
    .await
    .expect("local download should terminate")
}

fn assert_only_destination(dir: &Path, name: &str) {
    let entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from(name)]);
}

async fn wait_for_cleanup(dir: &Path, expected_files: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let entries: Vec<_> = std::fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap())
                .collect();
            if entries.len() == expected_files
                && entries
                    .iter()
                    .all(|entry| entry.file_type().unwrap().is_file())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("all staging ownership should be released after the parent exits");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workers_take_more_pieces_while_an_earlier_piece_is_still_running() {
    let _guard = test_guard().await;
    let body = payload(32 * MIB);
    let gate = Arc::new(AtomicBool::new(false));
    let held = Arc::new(AtomicBool::new(false));
    let server_body = body.clone();
    let server_gate = gate.clone();
    let server = LocalServer::start(move |request| {
        let mut plan = ResponsePlan::new(server_body.clone());
        if request.is_piece() && !held.swap(true, Ordering::SeqCst) {
            plan.hold_body = Some(server_gate.clone());
        }
        plan
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let task_dest = dest.clone();
    let url = server.url.clone();
    let download = tokio::spawn(async move {
        download_smart(
            &[url],
            &task_dest,
            Arc::new(NoopProgressSink),
            CancellationToken::new(),
            config(2),
        )
        .await
    });

    // The held worker cannot finish; the third request must come from work redistribution.
    server.wait_for_pieces(3).await;
    assert!(!gate.load(Ordering::SeqCst));
    assert!(server.peak_pieces() >= 2);
    gate.store(true, Ordering::SeqCst);
    let bytes = tokio::time::timeout(Duration::from_secs(20), download)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(bytes, body.len() as u64);
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), *body);
    assert!(server.pieces().iter().all(|request| {
        request.if_range.as_deref() == Some(V1)
            && request.accept_encoding.as_deref() == Some("identity")
    }));
    assert_only_destination(dir.path(), "asset.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_piece_retries_only_its_unwritten_suffix() {
    let _guard = test_guard().await;
    let body = payload(6 * MIB);
    let interrupted = Arc::new(Mutex::new(None::<(usize, usize)>));
    let server_interrupted = interrupted.clone();
    let server_body = body.clone();
    let server = LocalServer::start(move |request| {
        let mut plan = ResponsePlan::new(server_body.clone());
        if request.is_piece() {
            let mut first = server_interrupted.lock().unwrap();
            if first.is_none() {
                let (start, end) = request.range.unwrap();
                *first = Some((start, end.unwrap()));
                plan.cut_after = Some(256 * 1024);
            }
        }
        plan
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    assert_eq!(
        fetch(&server, &dest, config(2)).await.unwrap(),
        body.len() as u64
    );
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), *body);
    let (start, end) = interrupted.lock().unwrap().unwrap();
    let pieces = server.pieces();
    assert!(pieces.iter().any(|request| {
        request.range == Some((start + 256 * 1024, Some(end)))
            && request.if_range.as_deref() == Some(V1)
    }));
    assert_eq!(
        pieces
            .iter()
            .filter(|request| request.range == Some((start, Some(end))))
            .count(),
        1,
        "retry must retain the successfully written prefix"
    );
    assert!(
        server
            .requests()
            .iter()
            .all(|request| request.range.is_some())
    );
    assert_only_destination(dir.path(), "asset.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changed_representation_restarts_without_mixing_versions() {
    let _guard = test_guard().await;
    for ignore_range in [false, true] {
        let old_body = payload(6 * MIB);
        let new_body = Arc::new(vec![0xE7; old_body.len()]);
        let expected = new_body.clone();
        let server = LocalServer::start(move |request| {
            if request.is_piece() || request.range.is_none() {
                let mut plan = ResponsePlan::new(new_body.clone());
                plan.etag = Some(V2);
                plan.ignore_range = ignore_range;
                plan
            } else {
                ResponsePlan::new(old_body.clone())
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("asset.bin");
        tokio::fs::write(&dest, b"previous complete file")
            .await
            .unwrap();
        fetch(&server, &dest, config(4)).await.unwrap();
        assert!(!server.pieces().is_empty());
        assert!(
            server
                .requests()
                .iter()
                .any(|request| request.range.is_none())
        );
        assert_eq!(tokio::fs::read(&dest).await.unwrap(), *expected);
        assert_only_destination(dir.path(), "asset.bin");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inconsistent_range_total_falls_back_to_a_complete_stream() {
    let _guard = test_guard().await;
    let body = payload(6 * MIB);
    let server_body = body.clone();
    let server = LocalServer::start(move |request| {
        let mut plan = ResponsePlan::new(server_body.clone());
        if request.is_piece() {
            plan.reported_total = Some(server_body.len() + 1);
        }
        plan
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    fetch(&server, &dest, config(4)).await.unwrap();
    assert!(!server.pieces().is_empty());
    assert!(
        server
            .requests()
            .iter()
            .any(|request| request.range.is_none())
    );
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), *body);
    assert_only_destination(dir.path(), "asset.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn weak_or_missing_validators_use_one_complete_stream() {
    let _guard = test_guard().await;
    for etag in [None, Some("W/\"fixture-v1\"")] {
        let body = payload(6 * MIB);
        let server_body = body.clone();
        let server = LocalServer::start(move |_| {
            let mut plan = ResponsePlan::new(server_body.clone());
            plan.etag = etag;
            plan
        });
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("asset.bin");
        fetch(&server, &dest, config(8)).await.unwrap();
        assert!(server.pieces().is_empty());
        assert!(
            server
                .requests()
                .iter()
                .any(|request| request.range.is_none())
        );
        assert_eq!(tokio::fs::read(&dest).await.unwrap(), *body);
        assert_only_destination(dir.path(), "asset.bin");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn servers_that_ignore_range_are_downloaded_as_a_complete_stream() {
    let _guard = test_guard().await;
    let body = payload(6 * MIB);
    let server_body = body.clone();
    let server = LocalServer::start(move |_| {
        let mut plan = ResponsePlan::new(server_body.clone());
        plan.ignore_range = true;
        plan
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    fetch(&server, &dest, config(8)).await.unwrap();
    assert!(server.pieces().is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 1, "the full probe response can be reused");
    assert_eq!(requests[0].range, Some((0, Some(0))));
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), *body);
    assert_only_destination(dir.path(), "asset.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_known_checksum_allows_parallel_download_without_an_etag() {
    let _guard = test_guard().await;
    let body = payload(6 * MIB);
    let expected = hex::encode(Sha256::digest(body.as_slice()));
    let server_body = body.clone();
    let server = LocalServer::start(move |_| {
        let mut plan = ResponsePlan::new(server_body.clone());
        plan.etag = None;
        plan
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let mut cfg = config(4);
    cfg.expected_sha256 = Some(expected);
    fetch(&server, &dest, cfg).await.unwrap();
    assert!(server.pieces().len() >= 2);
    assert!(
        server
            .pieces()
            .iter()
            .all(|request| request.if_range.is_none())
    );
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), *body);
    assert_only_destination(dir.path(), "asset.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_keeps_the_previous_file_and_removes_staging_files() {
    let _guard = test_guard().await;
    let body = payload(6 * MIB);
    let gate = Arc::new(AtomicBool::new(false));
    let server_gate = gate.clone();
    let server = LocalServer::start(move |request| {
        let mut plan = ResponsePlan::new(body.clone());
        if request.is_piece() {
            plan.hold_body = Some(server_gate.clone());
        }
        plan
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let old = b"previous complete file";
    tokio::fs::write(&dest, old).await.unwrap();
    let task_dest = dest.clone();
    let url = server.url.clone();
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let download = tokio::spawn(async move {
        download_url(
            &url,
            &task_dest,
            Arc::new(NoopProgressSink),
            task_cancel,
            config(4),
        )
        .await
    });
    server.wait_for_pieces(2).await;
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), old);
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), download)
        .await
        .expect("cancellation must drain workers without waiting for the body")
        .unwrap();
    gate.store(true, Ordering::SeqCst);
    assert!(matches!(result, Err(NetworkError::Cancelled)));
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), old);
    assert_only_destination(dir.path(), "asset.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checksum_failure_never_replaces_a_previously_complete_file() {
    let _guard = test_guard().await;
    let body = payload(6 * MIB);
    let server = LocalServer::start(move |_| ResponsePlan::new(body.clone()));
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let old = b"previous complete file";
    tokio::fs::write(&dest, old).await.unwrap();
    let mut cfg = config(4);
    cfg.expected_sha256 = Some("00".repeat(32));
    assert!(fetch(&server, &dest, cfg).await.is_err());
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), old);
    assert_only_destination(dir.path(), "asset.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborting_the_parent_closes_workers_and_eventually_removes_staging() {
    let _guard = test_guard().await;
    let body = payload(16 * MIB);
    let gate = Arc::new(AtomicBool::new(false));
    let server_gate = gate.clone();
    let server = LocalServer::start(move |request| {
        let mut plan = ResponsePlan::new(body.clone());
        if request.is_piece() {
            plan.hold_body = Some(server_gate.clone());
            plan.hold_after_bytes = Some(256 * 1024);
        }
        plan
    });
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("asset.bin");
    let old = b"previous complete file";
    tokio::fs::write(&dest, old).await.unwrap();
    let task_dest = dest.clone();
    let url = server.url.clone();
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let download = tokio::spawn(async move {
        download_url(
            &url,
            &task_dest,
            Arc::new(NoopProgressSink),
            task_cancel,
            config(4),
        )
        .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while server.observations.body_gates.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("workers should write a prefix before the parent is aborted");
    download.abort();
    assert!(download.await.unwrap_err().is_cancelled());
    assert!(!cancel.is_cancelled(), "only the parent task was aborted");
    wait_for_cleanup(dir.path(), 1).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while server.observations.active_pieces.load(Ordering::SeqCst) != 0
            || server
                .observations
                .client_closed_pieces
                .load(Ordering::SeqCst)
                < 2
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("dropping the parent must close every in-flight range response");
    assert!(!gate.load(Ordering::SeqCst));
    assert!(
        server
            .requests()
            .iter()
            .all(|request| request.range.is_some())
    );
    assert_eq!(tokio::fs::read(&dest).await.unwrap(), old);
    assert_only_destination(dir.path(), "asset.bin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn simultaneous_downloads_share_a_sixteen_connection_budget() {
    // Other gated tests must not hold permits while this fixture waits for the full budget.
    let _guard = TEST_DOWNLOADS
        .get_or_init(|| tokio::sync::RwLock::new(()))
        .write()
        .await;
    let body = payload(32 * MIB);
    let gate = Arc::new(AtomicBool::new(false));
    let server_gate = gate.clone();
    let server = LocalServer::start(move |request| {
        let mut plan = ResponsePlan::new(body.clone());
        if request.is_piece() {
            plan.hold_body = Some(server_gate.clone());
        }
        plan
    });
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let mut downloads = Vec::new();
    let mut ticks = Vec::new();
    for index in 0..3 {
        let dest = dir.path().join(format!("asset-{index}.bin"));
        tokio::fs::write(&dest, b"previous complete file")
            .await
            .unwrap();
        let url = format!("{}?download={index}", server.url);
        let task_cancel = cancel.clone();
        let sink = Arc::new(StreamingTicks::default());
        ticks.push(sink.clone());
        downloads.push(tokio::spawn(async move {
            download_url(&url, &dest, sink, task_cancel, config(16)).await
        }));
    }
    server.wait_for_pieces(16).await;
    // A second heartbeat confirms all three parents remain active while bodies are gated.
    tokio::time::timeout(Duration::from_secs(10), async {
        while ticks.iter().any(|tick| tick.0.load(Ordering::SeqCst) < 2) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("all downloads should remain scheduled while awaiting their ranges");
    let gated_peak = server.peak_pieces();
    cancel.cancel();
    for download in downloads {
        let result = tokio::time::timeout(Duration::from_secs(5), download)
            .await
            .expect("cancellation must also interrupt global permit acquisition")
            .unwrap();
        assert!(matches!(result, Err(NetworkError::Cancelled)));
    }
    wait_for_cleanup(dir.path(), 3).await;
    assert_eq!(
        gated_peak, 16,
        "three downloads must share one global range budget"
    );
    assert!(server.peak_pieces() <= 16);
    for index in 0..3 {
        assert_eq!(
            tokio::fs::read(dir.path().join(format!("asset-{index}.bin")))
                .await
                .unwrap(),
            b"previous complete file"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "manual local bandwidth comparison; run with --release --ignored --nocapture"]
#[allow(clippy::print_stdout)]
async fn local_throttled_throughput_comparison() {
    let _guard = test_guard().await;
    let body = payload(32 * MIB);
    let mut timings = Vec::new();
    for parts in [1, 8] {
        let server_body = body.clone();
        let server = LocalServer::start(move |_| {
            let mut plan = ResponsePlan::new(server_body.clone());
            plan.chunk_bytes = 64 * 1024;
            plan.chunk_delay = Duration::from_millis(4);
            plan
        });
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("asset.bin");
        let started = Instant::now();
        let downloaded = fetch(&server, &dest, config(parts)).await.unwrap();
        let elapsed = started.elapsed();
        assert_eq!(downloaded, body.len() as u64);
        assert_eq!(tokio::fs::read(&dest).await.unwrap(), *body);
        timings.push(elapsed.as_secs_f64());
        println!(
            "parts={parts}, bytes={downloaded}, elapsed={:.3}s, throughput={:.2} MiB/s, piece_requests={}, peak_active_pieces={}",
            elapsed.as_secs_f64(),
            downloaded as f64 / MIB as f64 / elapsed.as_secs_f64(),
            server.pieces().len(),
            server.peak_pieces()
        );
    }
    // This is a report, not a speed gate: heavily loaded hosts can distort wall-clock timings.
    println!("parallel_speedup={:.2}x", timings[0] / timings[1]);
}
