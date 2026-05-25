//! 切片并行下载：≥16MB 文件切 4 片，每片独立 byte range + idle timeout。
//!
//! 流程：
//! 1. 先 [`probe_size_and_range`] 探总大小 + Range 支持
//! 2. 不支持 Range 或文件 < [`CHUNKED_THRESHOLD`] 字节 → fallback 到
//!    [`download_with_mirror_race`] 单流下载
//! 3. 支持 Range → 文件切 [`DEFAULT_CHUNK_PARTS`] 片，每片在 mirrors 里轮询
//!    选一个 url（轮询而不是 race，因为切片本身就是为了并行带宽，多 racer
//!    抢字节没意义）
//! 4. 每片独立 [`download_byte_range`]，写到 `<dest>.chunk-<idx>` 临时文件
//! 5. 任一片硬错（idle timeout / status 5xx）→ 该片自动切下个 mirror 重试
//!    （最多换 3 次 mirror）；连续失败则取消整体并清理已下载片
//! 6. 全部片完成 → 顺序拼接到 `<dest>.part`，rename 到 dest，删除 chunk 临时文件
//!
//! 进度聚合：[`AggregatedProgress`] 给所有片共享，单独的 ticker task 每 250ms
//! 读 snapshot 推给 sink，避免每片自己往 sink 推导致 UI 数字回退。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::client::shared_client;
use crate::download::{
    download_byte_range, probe_size_and_range, AggregatedProgress, DEFAULT_IDLE_TIMEOUT,
};
use crate::error::NetworkError;
use crate::progress::{DownloadProgressSink, DownloadStage, ProgressUpdate};
use crate::race::{download_with_mirror_race, MirrorRaceConfig};

/// 切片下载阈值：< 16MB 不切片（启动开销 > 收益）。
pub const CHUNKED_THRESHOLD: u64 = 16 * 1024 * 1024;
/// 默认切片数。aria2 默认 5，我们取 4：连接太多容易触发镜像 rate limit。
pub const DEFAULT_CHUNK_PARTS: usize = 4;
/// 单片最多换几次 mirror。
const PER_CHUNK_MAX_RETRIES: usize = 3;
/// 进度聚合 ticker 间隔。
const AGGREGATE_TICK_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct ChunkedConfig {
    pub parts: usize,
    pub threshold: u64,
    pub idle_timeout: Duration,
    /// 不切片时（小文件 / 不支持 Range），fallback 到 race 模式的配置。
    pub race_cfg: MirrorRaceConfig,
}

impl Default for ChunkedConfig {
    fn default() -> Self {
        Self {
            parts: DEFAULT_CHUNK_PARTS,
            threshold: CHUNKED_THRESHOLD,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            race_cfg: MirrorRaceConfig::default(),
        }
    }
}

/// 智能下载入口：先决定要不要切片，再分派到对应实现。
///
/// 这是 ncd-component 应该调的顶层函数。
pub async fn download_smart(
    mirrors: &[String],
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: ChunkedConfig,
) -> Result<u64, NetworkError> {
    if mirrors.is_empty() {
        return Err(NetworkError::InvalidArgument("mirrors is empty".into()));
    }
    if cfg.parts == 0 {
        return Err(NetworkError::InvalidArgument("parts must be >= 1".into()));
    }

    let client = shared_client();
    let probe_url = &mirrors[0];

    let (total, accept_ranges) = match probe_size_and_range(
        client,
        probe_url,
        &cancel,
        cfg.idle_timeout,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            warn!(target: "ncd_network::chunked", url=%probe_url, err=%e, "probe failed; fallback to race");
            return download_with_mirror_race(mirrors, dest, sink, cancel, cfg.race_cfg).await;
        }
    };

    let total = match total {
        Some(t) if t >= cfg.threshold && accept_ranges && cfg.parts > 1 => t,
        _ => {
            debug!(
                target: "ncd_network::chunked",
                total=?total, accept_ranges, parts=cfg.parts,
                "fallback to race (small file or no range)"
            );
            return download_with_mirror_race(mirrors, dest, sink, cancel, cfg.race_cfg).await;
        }
    };

    download_chunked_inner(mirrors, dest, total, sink, cancel, cfg).await
}

async fn download_chunked_inner(
    mirrors: &[String],
    dest: &Path,
    total: u64,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: ChunkedConfig,
) -> Result<u64, NetworkError> {
    let parts = cfg.parts;
    let ranges = split_ranges(total, parts);
    let chunk_paths: Vec<PathBuf> = (0..parts).map(|i| chunk_path(dest, i)).collect();

    let aggregated = AggregatedProgress::new();
    aggregated.set_total(Some(total)).await;

    sink.tick(ProgressUpdate {
        stage: DownloadStage::Streaming,
        downloaded: 0,
        total: Some(total),
        speed_bps: None,
        mirror_url: Some(mirrors[0].clone()),
        message: format!("chunked: {parts} parts, total {total} bytes"),
    })
    .await;

    let ticker_cancel = cancel.child_token();
    let ticker_handle = spawn_progress_ticker(
        sink.clone(),
        aggregated.clone(),
        Some(mirrors[0].clone()),
        ticker_cancel.clone(),
    );

    let mut tasks: JoinSet<(usize, Result<u64, NetworkError>)> = JoinSet::new();
    for (idx, range) in ranges.iter().enumerate() {
        let mirrors = mirrors.to_vec();
        let dest_chunk = chunk_paths[idx].clone();
        let cancel = cancel.child_token();
        let agg = aggregated.clone();
        let idle_timeout = cfg.idle_timeout;
        let range = *range;

        tasks.spawn(async move {
            let res = download_chunk_with_retry(
                idx, &mirrors, &dest_chunk, range, cancel, idle_timeout, agg,
            )
            .await;
            (idx, res)
        });
    }

    let mut first_err: Option<NetworkError> = None;
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((idx, Ok(_))) => {
                debug!(target: "ncd_network::chunked", idx, "chunk done");
            }
            Ok((idx, Err(e))) => {
                warn!(target: "ncd_network::chunked", idx, err=%e, "chunk failed");
                if first_err.is_none() {
                    first_err = Some(e);
                }
                cancel.cancel();
            }
            Err(join_err) => {
                warn!(target: "ncd_network::chunked", err=%join_err, "chunk task panicked");
                if first_err.is_none() {
                    first_err = Some(NetworkError::Http(format!("chunk task panic: {join_err}")));
                }
                cancel.cancel();
            }
        }
    }

    ticker_cancel.cancel();
    let _ = ticker_handle.await;

    if let Some(err) = first_err {
        cleanup_chunks(&chunk_paths).await;
        return Err(err);
    }

    merge_chunks(dest, &chunk_paths).await?;

    sink.tick(ProgressUpdate {
        stage: DownloadStage::Streaming,
        downloaded: total,
        total: Some(total),
        speed_bps: None,
        mirror_url: Some(mirrors[0].clone()),
        message: "chunked: done".into(),
    })
    .await;

    Ok(total)
}

async fn download_chunk_with_retry(
    chunk_idx: usize,
    mirrors: &[String],
    dest_chunk: &Path,
    range: (u64, u64),
    cancel: CancellationToken,
    idle_timeout: Duration,
    aggregated: AggregatedProgress,
) -> Result<u64, NetworkError> {
    let client = shared_client();
    // 切片之间错开起点：chunk_idx 的初始 mirror = chunk_idx % N，
    // 让 4 片分散到不同 mirror 上，否则前 4 片全打 mirror[0]。
    let start_mirror = chunk_idx % mirrors.len();

    let mut attempts = 0;
    let mut last_err: Option<NetworkError> = None;

    while attempts < PER_CHUNK_MAX_RETRIES.min(mirrors.len()) {
        let mirror_idx = (start_mirror + attempts) % mirrors.len();
        let url = &mirrors[mirror_idx];

        // 每次重试前清掉残片，下次 download_byte_range 重新开 file
        let _ = fs::remove_file(dest_chunk).await;

        match download_byte_range(
            client,
            url,
            dest_chunk,
            range,
            cancel.clone(),
            idle_timeout,
            Some(aggregated.clone()),
        )
        .await
        {
            Ok(n) => return Ok(n),
            Err(NetworkError::Cancelled) => return Err(NetworkError::Cancelled),
            Err(e) => {
                warn!(
                    target: "ncd_network::chunked",
                    chunk_idx, url=%url, attempt=attempts, err=%e,
                    "chunk attempt failed"
                );
                last_err = Some(e);
                attempts += 1;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| NetworkError::Http("chunk: no attempt was made".into())))
}

fn split_ranges(total: u64, parts: usize) -> Vec<(u64, u64)> {
    let parts = parts.max(1);
    let base = total / parts as u64;
    let mut ranges = Vec::with_capacity(parts);
    let mut start: u64 = 0;
    for i in 0..parts {
        let end = if i + 1 == parts {
            total - 1
        } else {
            start + base - 1
        };
        ranges.push((start, end));
        start = end + 1;
    }
    ranges
}

fn chunk_path(dest: &Path, idx: usize) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(format!(".chunk-{idx}"));
    PathBuf::from(p)
}

async fn cleanup_chunks(paths: &[PathBuf]) {
    for p in paths {
        let _ = fs::remove_file(p).await;
    }
}

async fn merge_chunks(dest: &Path, chunk_paths: &[PathBuf]) -> Result<(), NetworkError> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).await?;
        }
    }

    let mut part_path = dest.as_os_str().to_owned();
    part_path.push(".part");
    let part_path = PathBuf::from(part_path);

    {
        let mut out = fs::File::create(&part_path).await?;
        let mut buf = vec![0u8; 256 * 1024];
        for cp in chunk_paths {
            let mut input = fs::File::open(cp).await?;
            loop {
                let n = input.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).await?;
            }
        }
        out.flush().await?;
    }

    fs::rename(&part_path, dest).await?;
    cleanup_chunks(chunk_paths).await;
    Ok(())
}

fn spawn_progress_ticker(
    sink: Arc<dyn DownloadProgressSink>,
    aggregated: AggregatedProgress,
    mirror: Option<String>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut iv = tokio::time::interval(AGGREGATE_TICK_INTERVAL);
        iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                _ = iv.tick() => {
                    let (downloaded, total, bps) = aggregated.snapshot().await;
                    sink.tick(ProgressUpdate {
                        stage: DownloadStage::Streaming,
                        downloaded,
                        total,
                        speed_bps: bps,
                        mirror_url: mirror.clone(),
                        message: "chunked".into(),
                    }).await;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_ranges_covers_full_file() {
        let r = split_ranges(100, 4);
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].0, 0);
        assert_eq!(r.last().unwrap().1, 99);
        for w in r.windows(2) {
            assert_eq!(w[0].1 + 1, w[1].0);
        }
    }

    #[test]
    fn split_ranges_handles_remainder() {
        let r = split_ranges(10, 3);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], (0, 2));
        assert_eq!(r[1], (3, 5));
        assert_eq!(r[2], (6, 9));
    }

    #[test]
    fn split_ranges_single_part() {
        let r = split_ranges(42, 1);
        assert_eq!(r, vec![(0, 41)]);
    }
}
