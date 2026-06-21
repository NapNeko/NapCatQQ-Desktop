//! 切片并行下载:≥16MB 文件切 4 片,每片独立 byte range + idle timeout
//!
//! 不支持 Range 或文件 < CHUNKED_THRESHOLD 时 fallback 到 race 单流支持时切
//! DEFAULT_CHUNK_PARTS 片,每片轮询选一个 mirror(切片已并行,多 racer 抢字节
//! 没意义)片硬错自动切下个 mirror 重试(最多 3 次),全失败 fallback race
//! 进度聚合到 AggregatedProgress,ticker 每 250ms 推 sink 避免 UI 数字回退

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
use crate::verify::verify_sha256_if_needed;

/// 切片下载阈值:< 16MB 不切片(启动开销 > 收益)
pub const CHUNKED_THRESHOLD: u64 = 16 * 1024 * 1024;
/// 默认切片数aria2 默认 5,我们取 4:连接太多容易触发镜像 rate limit
pub const DEFAULT_CHUNK_PARTS: usize = 4;
/// 单片最多换几次 mirror
const PER_CHUNK_MAX_RETRIES: usize = 3;
/// 进度聚合 ticker 间隔
const AGGREGATE_TICK_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct ChunkedConfig {
    pub parts: usize,
    pub threshold: u64,
    pub idle_timeout: Duration,
    /// 不切片时(小文件 / 不支持 Range),fallback 到 race 模式的配置
    pub race_cfg: MirrorRaceConfig,
    /// 期望 SHA256(64-hex 小写)Some 时切片 merge 完整体之后立即校验,
    /// mismatch 视为该 primary mirror 投毒(返完整字节数但内容是另一份缓存
    /// 对象),删 dest 切回 race 让其它 mirror 接力None 跳过校验
    /// 注意 ChunkedConfig 与 race_cfg.expected_sha256 在外层调用入口由
    /// 同一个值同步设置;任一为空都会跳过对应阶段的校验
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

/// 智能下载入口:先决定要不要切片,再分派到对应实现
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

    // 找到第一个能用的 mirror 来切片:probe 通过且返回 total + accept_ranges
    // 切片必须**所有片用同一个 mirror**,否则代理之间缓存差异 / Range
    // 实现差异会导致拼出来的字节错位(zip "Could not find EOCD" 的根因)
    // 任何 mirror probe 失败 → 试下一个;全部失败或都不支持 Range / 总大小不
    // 够阈值 → fallback 到 race 单流(race 自己也会按需切 mirror,不切片)
    let mut chunk_mirror_idx: Option<usize> = None;
    let mut chunk_total: Option<u64> = None;
    for (i, m) in mirrors.iter().enumerate() {
        match probe_size_and_range(client, m, &cancel, cfg.idle_timeout).await {
            Ok((Some(t), true)) if t >= cfg.threshold && cfg.parts > 1 => {
                chunk_mirror_idx = Some(i);
                chunk_total = Some(t);
                break;
            }
            Ok((total, accept_ranges)) => {
                debug!(
                    target: "ncd_network::chunked",
                    mirror=%m, total=?total, accept_ranges,
                    "mirror not eligible for chunked, try next"
                );
            }
            Err(e) => {
                warn!(target: "ncd_network::chunked", url=%m, err=%e, "probe failed; try next mirror");
            }
        }
        if cancel.is_cancelled() {
            return Err(NetworkError::Cancelled);
        }
    }

    let (chunk_mirror_idx, total) = match (chunk_mirror_idx, chunk_total) {
        (Some(i), Some(t)) => (i, t),
        _ => {
            debug!(
                target: "ncd_network::chunked",
                "no chunked-capable mirror found; fallback to race"
            );
            return download_with_mirror_race(mirrors, dest, sink, cancel, cfg.race_cfg).await;
        }
    };

    // 主选 mirror 切片下载;如果整个切片失败,fallback 到 race 用剩下 mirror
    // 走单流race 内部还会再做整文件级别的 mirror 切换 + 续传
    let primary_mirror = &mirrors[chunk_mirror_idx];
    match download_chunked_inner(
        primary_mirror,
        dest,
        total,
        sink.clone(),
        cancel.clone(),
        cfg.clone(),
    )
    .await
    {
        Ok(n) => match verify_sha256_if_needed(dest, cfg.expected_sha256.as_deref()).await {
            Ok(_) => Ok(n),
            Err(NetworkError::Cancelled) => Err(NetworkError::Cancelled),
            Err(e) => {
                // 切片完成,字节数对,但 sha256 不一致:primary mirror 的缓存被
                // 投毒("长度对,Content-Range 对,流不截断" 都骗过去了)
                // 删 dest + .part 切回 race,让其它 mirror 接力,同时把 sha256
                // 透给 race 让它继续校验(race_cfg.expected_sha256 已含)
                warn!(
                    target: "ncd_network::chunked",
                    primary=%primary_mirror, err=%e,
                    "chunked sha256 mismatch on primary, fallback to race over other mirrors"
                );
                let _ = fs::remove_file(dest).await;
                let chunk_paths: Vec<PathBuf> =
                    (0..cfg.parts).map(|i| chunk_path(dest, i)).collect();
                cleanup_chunks(&chunk_paths).await;
                let mut race_cfg = cfg.race_cfg.clone();
                if race_cfg.expected_sha256.is_none() {
                    race_cfg.expected_sha256 = cfg.expected_sha256.clone();
                }
                download_with_mirror_race(mirrors, dest, sink, cancel, race_cfg).await
            }
        },
        Err(NetworkError::Cancelled) => Err(NetworkError::Cancelled),
        Err(e) => {
            warn!(
                target: "ncd_network::chunked",
                primary=%primary_mirror, err=%e,
                "chunked failed on primary mirror, fallback to race over all mirrors"
            );
            // 清理可能残留的 .chunk-N
            let chunk_paths: Vec<PathBuf> = (0..cfg.parts).map(|i| chunk_path(dest, i)).collect();
            cleanup_chunks(&chunk_paths).await;
            let mut race_cfg = cfg.race_cfg.clone();
            if race_cfg.expected_sha256.is_none() {
                race_cfg.expected_sha256 = cfg.expected_sha256.clone();
            }
            download_with_mirror_race(mirrors, dest, sink, cancel, race_cfg).await
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
        mirror_url: Some(primary_mirror.to_string()),
        message: format!("chunked: {parts} parts, total {total} bytes"),
    })
    .await;

    let ticker_cancel = cancel.child_token();
    let ticker_handle = spawn_progress_ticker(
        sink.clone(),
        aggregated.clone(),
        Some(primary_mirror.to_string()),
        ticker_cancel.clone(),
    );

    let mut tasks: JoinSet<(usize, Result<u64, NetworkError>)> = JoinSet::new();
    for (idx, range) in ranges.iter().enumerate() {
        let dest_chunk = chunk_paths[idx].clone();
        let cancel = cancel.child_token();
        let agg = aggregated.clone();
        let idle_timeout = cfg.idle_timeout;
        let range = *range;
        let mirror = primary_mirror.to_string();

        tasks.spawn(async move {
            let res = download_chunk_with_retry(
                idx, &mirror, &dest_chunk, range, cancel, idle_timeout, agg,
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
        mirror_url: Some(primary_mirror.to_string()),
        message: "chunked: done".into(),
    })
    .await;

    Ok(total)
}

async fn download_chunk_with_retry(
    chunk_idx: usize,
    mirror: &str,
    dest_chunk: &Path,
    range: (u64, u64),
    cancel: CancellationToken,
    idle_timeout: Duration,
    aggregated: AggregatedProgress,
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

        match download_byte_range(
            client,
            mirror,
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
