//! 切片并行下载:先按实测吞吐量选择镜像,再决定是否对同一源并行请求 Range。
//!
//! 最快镜像不支持 Range 或文件小于阈值时使用单流。分片失败或持续低速时
//! 保留调用者的取消状态,按既有测速排名切换镜像。每片独立跟踪当前尝试的
//! 有效字节,重试丢弃的字节仅计入传输速度,避免进度重复累计。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::client::{network_snapshot, shared_client, with_network_snapshot, with_snapshot};
use crate::download::{AggregatedProgress, DEFAULT_IDLE_TIMEOUT, download_byte_range};
use crate::error::NetworkError;
use crate::progress::{DownloadProgressSink, DownloadStage, ProgressUpdate};
use crate::race::{
    MirrorRaceConfig, download_ranked_mirrors, download_ranked_mirrors_with_fallback,
    select_mirrors,
};
use crate::speed::{LowSpeedMonitor, SpeedSampler};
use crate::verify::verify_sha256_if_needed;

/// 切片下载阈值:< 16MB 不切片(启动开销 > 收益)
pub const CHUNKED_THRESHOLD: u64 = 16 * 1024 * 1024;
/// 默认切片数,aria2 默认 5,我们取 4:连接太多容易触发镜像 rate limit
pub const DEFAULT_CHUNK_PARTS: usize = 4;
/// 单片最多在同一镜像重试几次
const PER_CHUNK_MAX_RETRIES: usize = 3;
/// 进度聚合 ticker 间隔
const AGGREGATE_TICK_INTERVAL: Duration = Duration::from_millis(250);
const LOW_SPEED_TICK_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct ChunkedConfig {
    pub parts: usize,
    pub threshold: u64,
    pub idle_timeout: Duration,
    /// 不切片时(小文件 / 不支持 Range),fallback 到 race 模式的配置
    pub race_cfg: MirrorRaceConfig,
    /// 期望 SHA256(64-hex 小写);Some 时切片 merge 完整体之后立即校验,
    /// mismatch 视为该 primary mirror 投毒(返完整字节数但内容是另一份缓存
    /// 对象),删 dest 切回 race 让其它 mirror 接力;None 跳过校验
    /// race_cfg.expected_sha256 未设置时采用此值,使切片与单流回退使用
    /// 同一校验要求
    pub expected_sha256: Option<String>,
}

impl Default for ChunkedConfig {
    fn default() -> Self {
        Self {
            parts: DEFAULT_CHUNK_PARTS,
            threshold: CHUNKED_THRESHOLD,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            race_cfg: MirrorRaceConfig::default(),
            expected_sha256: None,
        }
    }
}

/// 智能下载入口:所有文件先比较镜像吞吐量,只有最快源适合时才切片。
pub async fn download_smart(
    mirrors: &[String],
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: ChunkedConfig,
) -> Result<u64, NetworkError> {
    with_network_snapshot(download_smart_inner(mirrors, dest, sink, cancel, cfg)).await
}

async fn download_smart_inner(
    mirrors: &[String],
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    mut cfg: ChunkedConfig,
) -> Result<u64, NetworkError> {
    if mirrors.is_empty() {
        return Err(NetworkError::InvalidArgument("mirrors is empty".into()));
    }
    if cfg.parts == 0 {
        return Err(NetworkError::InvalidArgument("parts must be >= 1".into()));
    }
    if cfg.race_cfg.expected_sha256.is_none() {
        cfg.race_cfg.expected_sha256 = cfg.expected_sha256.clone();
    }

    // Range 支持只决定能否切片,不能让响应快但吞吐量低的直连绕过镜像选择。
    let mut ranked = select_mirrors(mirrors, &sink, &cancel, &cfg.race_cfg).await?;
    let Some(primary) = ranked.first().cloned() else {
        return Err(NetworkError::AllMirrorsFailed("no usable mirror".into()));
    };
    let total = match primary.total {
        Some(total)
            if primary.supports_ranges && total > 0 && total >= cfg.threshold && cfg.parts > 1 =>
        {
            total
        }
        _ => {
            return download_ranked_mirrors(mirrors, ranked, dest, sink, cancel, cfg.race_cfg)
                .await;
        }
    };
    let primary_idx = primary.idx;
    let primary_mirror = &mirrors[primary_idx];

    let mut chunk_cfg = cfg.clone();
    if ranked.len() == 1 {
        chunk_cfg.race_cfg.low_speed = None;
    }
    let chunked_result = download_chunked_inner(
        primary_mirror,
        dest,
        total,
        sink.clone(),
        cancel.clone(),
        chunk_cfg,
    )
    .await;
    let merged = chunked_result.is_ok();
    let slow_fallback = match &chunked_result {
        Err(NetworkError::SlowDownload { .. }) => Some(primary.clone()),
        _ => None,
    };
    let result = match chunked_result {
        Ok(n) => tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(NetworkError::Cancelled),
            result = verify_sha256_if_needed(dest, cfg.race_cfg.expected_sha256.as_deref()) => result.map(|_| n),
        },
        Err(e) => Err(e),
    };
    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    match result {
        Ok(n) => Ok(n),
        Err(NetworkError::Cancelled) => Err(NetworkError::Cancelled),
        Err(e) => {
            warn!(
                target: "ncd_network::chunked",
                primary=%primary_mirror, err=%e,
                "chunked failed or slowed down; try other ranked mirrors"
            );
            if merged {
                let _ = fs::remove_file(dest).await;
            }
            let _ = fs::remove_file(crate::range::part_path(dest)).await;
            // 同源拼片的失败不会污染外层 token;备用源沿用测速排序,无需重新测速。
            ranked.retain(|mirror| mirror.idx != primary_idx);
            if ranked.is_empty() {
                // 没有备用源时允许以单流重新尝试该源,但停止低速切换以免误失败。
                let mut race_cfg = cfg.race_cfg;
                race_cfg.low_speed = None;
                let single = vec![primary];
                return download_ranked_mirrors(mirrors, single, dest, sink, cancel, race_cfg)
                    .await;
            }
            download_ranked_mirrors_with_fallback(
                mirrors,
                ranked,
                dest,
                sink,
                cancel,
                cfg.race_cfg,
                slow_fallback,
            )
            .await
        }
    }
}

async fn download_chunked_inner(
    primary_mirror: &str,
    dest: &Path,
    total: u64,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: ChunkedConfig,
) -> Result<u64, NetworkError> {
    let ranges = split_ranges(total, cfg.parts);
    let parts = ranges.len();
    let chunk_paths: Vec<PathBuf> = (0..parts).map(|i| chunk_path(dest, i)).collect();
    let progress: Vec<ChunkProgress> = ranges
        .iter()
        .map(|&(start, end)| ChunkProgress::new(end - start + 1))
        .collect();

    sink.tick(ProgressUpdate {
        stage: DownloadStage::Streaming,
        downloaded: 0,
        total: Some(total),
        speed_bps: None,
        mirror_url: Some(primary_mirror.to_string()),
        message: format!("chunked: {parts} parts, total {total} bytes"),
    })
    .await;

    // 此 token 只控制本轮切片;失败/低速取消不会向上传播到调用者。
    let chunk_cancel = cancel.child_token();
    let mut tasks: JoinSet<(usize, Result<u64, NetworkError>)> = JoinSet::new();
    // 全部切片固定使用同一镜像,避免不同代理缓存版本差异造成错位拼接。
    for (idx, range) in ranges.iter().enumerate() {
        let dest_chunk = chunk_paths[idx].clone();
        let cancel = chunk_cancel.child_token();
        let progress = progress[idx].clone();
        let idle_timeout = cfg.idle_timeout;
        let range = *range;
        let mirror = primary_mirror.to_string();

        let snapshot = network_snapshot();
        tasks.spawn(with_snapshot(snapshot, async move {
            let res = download_chunk_with_retry(
                idx,
                &mirror,
                &dest_chunk,
                range,
                cancel,
                idle_timeout,
                progress,
            )
            .await;
            (idx, res)
        }));
    }

    let mut sampler = SpeedSampler::new();
    sampler.record(Instant::now(), 0);
    let min_bps = cfg
        .race_cfg
        .low_speed
        .as_ref()
        .map_or(0, |config| config.min_bps);
    let mut low_speed = cfg
        .race_cfg
        .low_speed
        .clone()
        .map(|config| LowSpeedMonitor::new(config, Instant::now(), 0));
    let mut ticker = tokio::time::interval(AGGREGATE_TICK_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut low_speed_ticker = tokio::time::interval(LOW_SPEED_TICK_INTERVAL);
    low_speed_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut first_err: Option<NetworkError> = None;
    while !tasks.is_empty() {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                first_err = Some(NetworkError::Cancelled);
                break;
            }
            joined = tasks.join_next() => {
                match joined {
                    Some(Ok((idx, Ok(_)))) => {
                        debug!(target: "ncd_network::chunked", idx, "chunk done");
                    }
                    Some(Ok((idx, Err(e)))) => {
                        warn!(target: "ncd_network::chunked", idx, err=%e, "chunk failed");
                        first_err = Some(e);
                        break;
                    }
                    Some(Err(join_err)) => {
                        first_err = Some(NetworkError::Http(format!("chunk task panic: {join_err}")));
                        break;
                    }
                    None => break,
                }
            }
            _ = low_speed_ticker.tick(), if low_speed.is_some() => {
                let (_, transferred, discarded) = snapshot_chunks(&progress).await;
                // 速度计保留重试实际传输的字节,总量加上丢弃字节后剩余量仍准确。
                if let Some(speed_bps) = low_speed.as_mut().and_then(|monitor| {
                    monitor.check(Instant::now(), transferred, Some(total.saturating_add(discarded)))
                }) {
                    first_err = Some(NetworkError::SlowDownload {
                        speed_bps,
                        min_bps,
                    });
                    break;
                }
            }
            _ = ticker.tick() => {
                let (downloaded, transferred, _) = snapshot_chunks(&progress).await;
                sampler.record(Instant::now(), transferred);
                sink.tick(ProgressUpdate {
                    stage: DownloadStage::Streaming,
                    downloaded,
                    total: Some(total),
                    speed_bps: sampler.current_bps(),
                    mirror_url: Some(primary_mirror.to_string()),
                    message: "chunked".into(),
                }).await;
            }
        }
    }
    chunk_cancel.cancel();
    while tasks.join_next().await.is_some() {}

    if let Some(err) = first_err {
        cleanup_chunks(&chunk_paths).await;
        return Err(err);
    }
    if cancel.is_cancelled() {
        cleanup_chunks(&chunk_paths).await;
        return Err(NetworkError::Cancelled);
    }
    if let Err(err) = merge_chunks(dest, &chunk_paths, &cancel).await {
        cleanup_chunks(&chunk_paths).await;
        let _ = fs::remove_file(crate::range::part_path(dest)).await;
        return Err(err);
    }

    sink.tick(ProgressUpdate {
        stage: DownloadStage::Streaming,
        downloaded: total,
        total: Some(total),
        speed_bps: None,
        mirror_url: Some(primary_mirror.to_string()),
        message: "chunked: done".into(),
    })
    .await;

    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    Ok(total)
}

/// 每片只把当前尝试算入完成进度;之前丢弃的数据保留给速度采样。
#[derive(Clone)]
struct ChunkProgress {
    expected: u64,
    inner: Arc<Mutex<ChunkProgressInner>>,
}

#[derive(Default)]
struct ChunkProgressInner {
    current: AggregatedProgress,
    discarded: u64,
}

impl ChunkProgress {
    fn new(expected: u64) -> Self {
        Self {
            expected,
            inner: Arc::new(Mutex::new(ChunkProgressInner::default())),
        }
    }

    async fn begin_attempt(&self) -> AggregatedProgress {
        let mut inner = self.inner.lock().await;
        let (previous, _, _) = inner.current.snapshot().await;
        inner.discarded = inner.discarded.saturating_add(previous);
        inner.current = AggregatedProgress::new();
        inner.current.clone()
    }

    async fn snapshot(&self) -> (u64, u64, u64) {
        let inner = self.inner.lock().await;
        let (downloaded, _, _) = inner.current.snapshot().await;
        (
            downloaded.min(self.expected),
            inner.discarded.saturating_add(downloaded),
            inner.discarded,
        )
    }
}

async fn snapshot_chunks(progress: &[ChunkProgress]) -> (u64, u64, u64) {
    let mut downloaded = 0u64;
    let mut transferred = 0u64;
    let mut discarded = 0u64;
    for chunk in progress {
        let (chunk_downloaded, chunk_transferred, chunk_discarded) = chunk.snapshot().await;
        downloaded = downloaded.saturating_add(chunk_downloaded);
        transferred = transferred.saturating_add(chunk_transferred);
        discarded = discarded.saturating_add(chunk_discarded);
    }
    (downloaded, transferred, discarded)
}

async fn download_chunk_with_retry(
    chunk_idx: usize,
    mirror: &str,
    dest_chunk: &Path,
    range: (u64, u64),
    cancel: CancellationToken,
    idle_timeout: Duration,
    progress: ChunkProgress,
) -> Result<u64, NetworkError> {
    let client = shared_client();

    let mut attempts = 0;
    let mut last_err: Option<NetworkError> = None;

    // 切片现在固定走单一 mirror(probe winner)这里的 retry 主要是给临时
    // 抖动(短暂网络故障 / 服务端 429)兜底;如果是 mirror 真的坏了,外层
    // download_smart 会接住整片失败后 fallback 到 race 走其它 mirror
    while attempts < PER_CHUNK_MAX_RETRIES {
        // 每次重试前清掉残片,下次 download_byte_range 重新开 file
        let _ = fs::remove_file(dest_chunk).await;

        let aggregated = progress.begin_attempt().await;
        // 同时覆盖等待响应头和文件 IO 阶段,确保内部取消后能及时排空任务。
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(NetworkError::Cancelled),
            result = download_byte_range(
                &client,
                mirror,
                dest_chunk,
                range,
                cancel.clone(),
                idle_timeout,
                Some(aggregated),
            ) => result,
        };
        match result {
            Ok(n) => return Ok(n),
            Err(NetworkError::Cancelled) => return Err(NetworkError::Cancelled),
            Err(e) => {
                warn!(
                    target: "ncd_network::chunked",
                    chunk_idx, url=%mirror, attempt=attempts, err=%e,
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
    if total == 0 {
        return Vec::new();
    }
    let parts = parts
        .max(1)
        .min(usize::try_from(total).unwrap_or(usize::MAX));
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

async fn merge_chunks(
    dest: &Path,
    chunk_paths: &[PathBuf],
    cancel: &CancellationToken,
) -> Result<(), NetworkError> {
    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).await?;
        }
    }

    let part_path = crate::range::part_path(dest);

    {
        let mut out = fs::File::create(&part_path).await?;
        let mut buf = vec![0u8; 256 * 1024];
        for cp in chunk_paths {
            if cancel.is_cancelled() {
                return Err(NetworkError::Cancelled);
            }
            let mut input = fs::File::open(cp).await?;
            loop {
                if cancel.is_cancelled() {
                    return Err(NetworkError::Cancelled);
                }
                let n = input.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                if cancel.is_cancelled() {
                    return Err(NetworkError::Cancelled);
                }
                out.write_all(&buf[..n]).await?;
            }
        }
        if cancel.is_cancelled() {
            return Err(NetworkError::Cancelled);
        }
        out.flush().await?;
    }

    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    fs::rename(&part_path, dest).await?;
    cleanup_chunks(chunk_paths).await;
    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    Ok(())
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
    fn split_ranges_clamps_parts_to_bytes() {
        assert_eq!(split_ranges(2, 4), vec![(0, 0), (1, 1)]);
        assert!(split_ranges(0, 4).is_empty());
    }

    #[tokio::test]
    async fn chunk_retry_does_not_count_discarded_bytes_as_progress() {
        let progress = ChunkProgress::new(20);
        let first = progress.begin_attempt().await;
        first.add_bytes(10).await;
        assert_eq!(progress.snapshot().await, (10, 10, 0));
        let retry = progress.begin_attempt().await;
        retry.add_bytes(5).await;
        assert_eq!(progress.snapshot().await, (5, 15, 10));
    }

    #[test]
    fn split_ranges_single_part() {
        let r = split_ranges(42, 1);
        assert_eq!(r, vec![(0, 41)]);
    }
}
