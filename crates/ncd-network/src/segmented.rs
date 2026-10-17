//! 有界 worker 动态领取片段，直接定位写入；断流只重取尚未写入的后缀。

use std::io::SeekFrom;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::{Client, header};
use tokio::fs::{self, File};
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::NetworkError;
use crate::chunked::ChunkedConfig;
use crate::client::shared_client;
use crate::progress::{DownloadProgressSink, DownloadStage, ProgressUpdate};
use crate::range::{content_range, part_path};
use crate::speed::{LowSpeedMonitor, SpeedSampler};
use crate::staging::StagedDownload;

const MIB: u64 = 1024 * 1024;
static CONNECTIONS: OnceLock<Semaphore> = OnceLock::new();

struct Transfer {
    client: Client,
    url: String,
    target: Arc<StagedDownload>,
    total: u64,
    piece: u64,
    next: AtomicU64,
    downloaded: AtomicU64,
    etag: Option<header::HeaderValue>,
    idle: Duration,
    cancel: CancellationToken,
}

pub(crate) async fn download_segments(
    url: &str,
    target: Arc<StagedDownload>,
    total: u64,
    etag: Option<header::HeaderValue>,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: &ChunkedConfig,
) -> Result<u64, NetworkError> {
    let piece = piece_size(total, cfg.parts);
    let workers = cfg.parts.min(total.div_ceil(piece) as usize);
    let file = File::create(part_path(&target.file)).await?;
    file.set_len(total).await?;
    drop(file);
    let transfer = Arc::new(Transfer {
        client: shared_client(),
        url: url.into(),
        target: target.clone(),
        total,
        piece,
        next: AtomicU64::new(0),
        downloaded: AtomicU64::new(0),
        etag,
        idle: cfg.idle_timeout,
        cancel: cancel.child_token(),
    });
    let mut tasks = JoinSet::new();
    for _ in 0..workers {
        let transfer = transfer.clone();
        tasks.spawn(async move { worker(transfer).await });
    }
    let mut sampler = SpeedSampler::new();
    sampler.record(Instant::now(), 0);
    let mut monitor = cfg
        .race_cfg
        .low_speed
        .clone()
        .map(|cfg| LowSpeedMonitor::new(cfg, Instant::now(), 0));
    let mut ticker = tokio::time::interval(Duration::from_millis(250));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut error = None;
    while !tasks.is_empty() {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => { error = Some(NetworkError::Cancelled); break; }
            result = tasks.join_next() => match result {
                Some(Ok(Ok(()))) => {},
                Some(Ok(Err(err))) => { error = Some(err); break; },
                Some(Err(err)) => { error = Some(NetworkError::Http(format!("download worker: {err}"))); break; },
                None => break,
            },
            _ = ticker.tick() => {
                let downloaded = transfer.downloaded.load(Ordering::Relaxed);
                let now = Instant::now();
                sampler.record(now, downloaded);
                if let Some(speed_bps) = monitor.as_mut().and_then(|m| m.check(now, downloaded, Some(total))) {
                    error = Some(NetworkError::SlowDownload { speed_bps,
                        min_bps: cfg.race_cfg.low_speed.as_ref().map_or(0, |c| c.min_bps) });
                    break;
                }
                sink.tick(ProgressUpdate { stage: DownloadStage::Streaming, downloaded,
                    total: Some(total), speed_bps: sampler.current_bps(), mirror_url: Some(url.into()),
                    message: format!("parallel: {workers} connections") }).await;
            }
        }
    }
    transfer.cancel.cancel();
    // 回退单流前必须排空 writer，不能让旧请求往重建后的文件继续写。
    while tasks.join_next().await.is_some() {}
    if let Some(error) = error {
        return Err(error);
    }
    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    let downloaded = transfer.downloaded.load(Ordering::Relaxed);
    if downloaded != total {
        return Err(NetworkError::Truncated { downloaded, total });
    }
    fs::rename(part_path(&target.file), &target.file).await?;
    sink.tick(ProgressUpdate {
        stage: DownloadStage::Streaming,
        downloaded,
        total: Some(total),
        speed_bps: sampler.current_bps(),
        mirror_url: Some(url.into()),
        message: "parallel: done".into(),
    })
    .await;
    Ok(downloaded)
}

fn piece_size(total: u64, workers: usize) -> u64 {
    let workers = workers.max(1) as u64;
    (total / workers / 8)
        .clamp(MIB, 8 * MIB)
        .min(total.div_ceil(workers).max(1))
}

async fn worker(transfer: Arc<Transfer>) -> Result<(), NetworkError> {
    let file = fs::OpenOptions::new()
        .write(true)
        .open(part_path(&transfer.target.file))
        .await?;
    // 每个 worker 使用独立文件句柄/游标，既不串行锁磁盘写入，也不额外合并整份文件。
    let mut file = BufWriter::with_capacity(256 * 1024, file);
    loop {
        if transfer.cancel.is_cancelled() {
            return Err(NetworkError::Cancelled);
        }
        let start = transfer.next.fetch_add(transfer.piece, Ordering::Relaxed);
        if start >= transfer.total {
            break;
        }
        let end = start.saturating_add(transfer.piece).min(transfer.total) - 1;
        let mut offset = start;
        file.seek(SeekFrom::Start(start)).await?;
        for attempt in 0..3 {
            let result = stream_piece(&transfer, &mut file, &mut offset, end).await;
            // flush 也确保重试 offset 对应的字节已经离开用户态缓冲区。
            file.flush().await?;
            match result {
                Ok(()) => break,
                Err(failure) if failure.retry && attempt < 2 => {
                    let delay = failure.delay.unwrap_or_else(|| {
                        Duration::from_millis(200 * (1 << attempt) + start % 97)
                    });
                    tokio::select! {
                        biased;
                        _ = transfer.cancel.cancelled() => return Err(NetworkError::Cancelled),
                        _ = tokio::time::sleep(delay) => {},
                    }
                }
                Err(failure) => return Err(failure.error),
            }
        }
    }
    file.flush().await?;
    Ok(())
}

struct Failure {
    error: NetworkError,
    retry: bool,
    delay: Option<Duration>,
}

impl Failure {
    fn fatal(error: NetworkError) -> Self {
        Self {
            error,
            retry: false,
            delay: None,
        }
    }
    fn transient(error: NetworkError) -> Self {
        Self {
            error,
            retry: true,
            delay: None,
        }
    }
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::fatal(NetworkError::Io(error))
    }
}

async fn stream_piece(
    transfer: &Transfer,
    file: &mut BufWriter<File>,
    offset: &mut u64,
    end: u64,
) -> Result<(), Failure> {
    if *offset > end {
        return Ok(());
    }
    let _permit = tokio::select! {
        biased;
        _ = transfer.cancel.cancelled() => return Err(Failure::fatal(NetworkError::Cancelled)),
        permit = CONNECTIONS.get_or_init(|| Semaphore::new(16)).acquire() =>
            permit.map_err(|_| Failure::fatal(NetworkError::Cancelled))?,
    };
    let mut request = transfer
        .client
        .get(&transfer.url)
        .header(header::ACCEPT_ENCODING, "identity")
        .header(header::RANGE, format!("bytes={}-{}", *offset, end));
    if let Some(etag) = &transfer.etag {
        request = request.header(header::IF_RANGE, etag);
    }
    let response = tokio::select! {
        biased;
        _ = transfer.cancel.cancelled() => return Err(Failure::fatal(NetworkError::Cancelled)),
        result = tokio::time::timeout(transfer.idle, request.send()) => result
            .map_err(|_| Failure::transient(NetworkError::IdleTimeout(transfer.idle)))?
            .map_err(|err| Failure::transient(err.into()))?,
    };
    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        let code = response.status().as_u16();
        let delay = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .map(|s| Duration::from_secs(s.min(10)));
        return Err(Failure {
            error: NetworkError::Status(code),
            retry: matches!(code, 429 | 502 | 503 | 504),
            delay,
        });
    }
    if content_range(&response) != Some((*offset, end, transfer.total))
        || response
            .content_length()
            .is_some_and(|len| len != end - *offset + 1)
        || response
            .headers()
            .get(header::CONTENT_ENCODING)
            .is_some_and(|v| v != "identity")
        || transfer
            .etag
            .as_ref()
            .is_some_and(|etag| response.headers().get(header::ETAG) != Some(etag))
    {
        return Err(Failure::fatal(NetworkError::Http(
            "range representation changed".into(),
        )));
    }
    let mut stream = response.bytes_stream();
    loop {
        let next = tokio::select! {
            biased;
            _ = transfer.cancel.cancelled() => return Err(Failure::fatal(NetworkError::Cancelled)),
            next = tokio::time::timeout(transfer.idle, stream.next()) => next
                .map_err(|_| Failure::transient(NetworkError::IdleTimeout(transfer.idle)))?,
        };
        let bytes = match next {
            Some(Ok(bytes)) => bytes,
            Some(Err(error)) => return Err(Failure::transient(error.into())),
            None => break,
        };
        if bytes.len() as u64 > end + 1 - *offset {
            return Err(Failure::fatal(NetworkError::Http(
                "range response exceeds requested size".into(),
            )));
        }
        file.write_all(&bytes).await?;
        *offset += bytes.len() as u64;
        transfer
            .downloaded
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
    }
    if *offset != end + 1 {
        return Err(Failure::transient(NetworkError::Truncated {
            downloaded: *offset,
            total: end + 1,
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_balance_tail_work_without_tiny_requests() {
        assert_eq!(piece_size(32 * MIB, 8), MIB);
        assert_eq!(piece_size(1024 * MIB, 8), 8 * MIB);
        assert_eq!(piece_size(2, 8), 1);
    }
}
