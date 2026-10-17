//! 有界吞吐量测速选源；正式下载低速或断流后按测速排名换源。
//! 每个候选最多采样 probe_bytes，慢流最多观察 probe_duration；首字节只
//! 标志连接可用，不决定胜者。跨源重新下载，避免不同缓存的字节拼接。

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header;
use tokio::task::JoinSet;
use tokio::time::{Instant, timeout_at};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::client::{network_snapshot, shared_client, with_network_snapshot, with_snapshot};
use crate::download::{DEFAULT_IDLE_TIMEOUT, DownloadConfig, download_with_resume};
use crate::error::NetworkError;
use crate::progress::{DownloadProgressSink, DownloadStage, ProgressUpdate};
use crate::range::{parse_content_length_from_range, parse_content_range_bounds, part_path};
use crate::speed::LowSpeedConfig;
use crate::verify::verify_sha256_if_needed;

#[derive(Debug, Clone)]
pub struct MirrorRaceConfig {
    /// 初始并发探测数；其余候选按 stagger 加入。
    pub initial_parallel: usize,
    pub stagger: Duration,
    /// 单源从建立连接到完成采样的总时间上限。
    pub probe_first_chunk_timeout: Duration,
    pub probe_duration: Duration,
    pub probe_bytes: u64,
    pub idle_timeout: Duration,
    pub low_speed: Option<LowSpeedConfig>,
    pub expected_sha256: Option<String>,
}

impl Default for MirrorRaceConfig {
    fn default() -> Self {
        Self {
            initial_parallel: 3,
            stagger: Duration::from_millis(350),
            probe_first_chunk_timeout: Duration::from_secs(8),
            probe_duration: Duration::from_secs(2),
            probe_bytes: 512 * 1024,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            low_speed: Some(LowSpeedConfig::default()),
            expected_sha256: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MirrorProbe {
    pub idx: usize,
    pub total: Option<u64>,
    pub supports_ranges: bool,
    pub speed_bps: u64,
    pub etag: Option<header::HeaderValue>,
}

pub async fn download_with_mirror_race(
    mirrors: &[String],
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: MirrorRaceConfig,
) -> Result<u64, NetworkError> {
    with_network_snapshot(async {
        let ranked = select_mirrors(mirrors, &sink, &cancel, &cfg).await?;
        download_ranked_mirrors(mirrors, ranked, dest, sink, cancel, cfg).await
    })
    .await
}

/// 大小文件共享同一测速结果；不支持 Range 的最快源仍可用单流下载。
pub(crate) async fn select_mirrors(
    mirrors: &[String],
    sink: &Arc<dyn DownloadProgressSink>,
    cancel: &CancellationToken,
    cfg: &MirrorRaceConfig,
) -> Result<Vec<MirrorProbe>, NetworkError> {
    if mirrors.is_empty()
        || cfg.initial_parallel == 0
        || cfg.stagger.is_zero()
        || cfg.probe_duration.is_zero()
        || cfg.probe_bytes == 0
        || cfg.probe_first_chunk_timeout.is_zero()
        || cfg.idle_timeout.is_zero()
        || cfg
            .low_speed
            .as_ref()
            .is_some_and(|c| c.min_bps == 0 || c.window.is_zero())
    {
        return Err(NetworkError::InvalidArgument(
            "invalid mirror race configuration".into(),
        ));
    }
    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    push_stage(
        sink,
        DownloadStage::Racing,
        &mirrors[0],
        "正在比较下载源速度",
    )
    .await;

    let mut tasks = JoinSet::new();
    let mut next_idx = 0;
    let mut completed = vec![false; mirrors.len()];
    let mut ranked = Vec::new();
    let mut errors = Vec::new();
    for _ in 0..cfg.initial_parallel.min(mirrors.len()) {
        spawn_probe(&mut tasks, next_idx, mirrors[next_idx].clone(), cfg.clone());
        next_idx += 1;
    }
    let mut stagger = tokio::time::interval(cfg.stagger);
    stagger.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    stagger.tick().await;
    // 给其它候选一个完整采样窗口，避免最快首字节抢走胜者位置。
    let mut decision_deadline: Option<Instant> = None;
    loop {
        if tasks.is_empty() && next_idx == mirrors.len() {
            break;
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(NetworkError::Cancelled),
            joined = tasks.join_next(), if !tasks.is_empty() => {
                match joined {
                    Some(Ok((idx, Ok(probe)))) => {
                        completed[idx] = true;
                        debug!(target: "ncd_network::race", url=%mirrors[idx], speed_bps=probe.speed_bps, "throughput sample complete");
                        ranked.push(probe);
                        if decision_deadline.is_none() {
                            decision_deadline = Some(Instant::now() + cfg.probe_duration);
                        }
                    },
                    Some(Ok((idx, Err(err)))) => {
                        completed[idx] = true;
                        errors.push(format!("{}: {err}", mirrors[idx]));
                    },
                    Some(Err(err)) => errors.push(format!("probe task: {err}")),
                    None => {},
                }
                // 空出来的探测连接立即让给后续候选，不必等待下一次 stagger。
                if next_idx < mirrors.len() {
                    spawn_probe(&mut tasks, next_idx, mirrors[next_idx].clone(), cfg.clone());
                    next_idx += 1;
                }
            },
            _ = stagger.tick(), if next_idx < mirrors.len() => {
                spawn_probe(&mut tasks, next_idx, mirrors[next_idx].clone(), cfg.clone());
                next_idx += 1;
            },
            _ = async {
                match decision_deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => break,
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    if ranked.is_empty() {
        return Err(NetworkError::AllMirrorsFailed(errors.join("; ")));
    }
    ranked.sort_by(|a, b| b.speed_bps.cmp(&a.speed_bps).then(a.idx.cmp(&b.idx)));
    // 有界测速结束时仍在连接的源留作备选，已经失败的源不重复等待。
    for (idx, done) in completed.into_iter().enumerate() {
        if !done {
            ranked.push(MirrorProbe {
                idx,
                total: None,
                supports_ranges: false,
                speed_bps: 0,
                etag: None,
            });
        }
    }
    Ok(ranked)
}

fn spawn_probe(
    tasks: &mut JoinSet<(usize, Result<MirrorProbe, NetworkError>)>,
    idx: usize,
    url: String,
    cfg: MirrorRaceConfig,
) {
    let snapshot = network_snapshot();
    tasks.spawn(with_snapshot(snapshot, async move {
        (idx, probe_throughput(idx, &url, &cfg).await)
    }));
}

async fn probe_throughput(
    idx: usize,
    url: &str,
    cfg: &MirrorRaceConfig,
) -> Result<MirrorProbe, NetworkError> {
    let started = Instant::now();
    let deadline = started + cfg.probe_first_chunk_timeout;
    let request = shared_client()
        .get(url)
        .header(header::RANGE, format!("bytes=0-{}", cfg.probe_bytes - 1))
        .header(header::ACCEPT_ENCODING, "identity");
    let mut resp = timeout_at(deadline, request.send())
        .await
        .map_err(|_| NetworkError::IdleTimeout(cfg.probe_first_chunk_timeout))??;
    // 一些下载端点拒绝 Range，却允许普通 GET；仍按相同字节/时间预算采样。
    if matches!(resp.status().as_u16(), 400 | 405 | 416 | 501) {
        drop(resp);
        resp = timeout_at(
            deadline,
            shared_client()
                .get(url)
                .header(header::ACCEPT_ENCODING, "identity")
                .send(),
        )
        .await
        .map_err(|_| NetworkError::IdleTimeout(cfg.probe_first_chunk_timeout))??;
    }
    if !resp.status().is_success() {
        return Err(NetworkError::Status(resp.status().as_u16()));
    }
    let supports_ranges = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if supports_ranges {
        match parse_content_range_bounds(&resp) {
            Some((0, end)) if end < cfg.probe_bytes => {}
            _ => {
                return Err(NetworkError::Http(
                    "invalid Content-Range on throughput probe".into(),
                ));
            }
        }
    }
    let total = if supports_ranges {
        parse_content_length_from_range(&resp, 0)
    } else {
        resp.content_length()
    };
    let etag = crate::range::strong_etag(&resp);
    if total == Some(0) {
        return Ok(MirrorProbe {
            idx,
            total,
            supports_ranges: false,
            speed_bps: 0,
            etag,
        });
    }
    let mut stream = resp.bytes_stream();
    let mut received = 0u64;
    let mut sample_deadline = deadline;
    loop {
        match timeout_at(sample_deadline, stream.next()).await {
            Ok(Some(Ok(chunk))) if !chunk.is_empty() => {
                if received == 0 {
                    sample_deadline = deadline.min(Instant::now() + cfg.probe_duration);
                }
                received += (chunk.len() as u64).min(cfg.probe_bytes - received);
                if received >= cfg.probe_bytes {
                    break;
                }
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(err))) => return Err(err.into()),
            Ok(None) => break,
            Err(_) if received > 0 => break,
            Err(_) => return Err(NetworkError::IdleTimeout(cfg.probe_first_chunk_timeout)),
        }
    }
    if received == 0 {
        return Err(NetworkError::Http("empty download source".into()));
    }
    let seconds = started.elapsed().as_secs_f64().max(0.001);
    Ok(MirrorProbe {
        idx,
        total,
        supports_ranges,
        speed_bps: (received as f64 / seconds) as u64,
        etag,
    })
}

pub(crate) async fn download_ranked_mirrors(
    mirrors: &[String],
    ranked: Vec<MirrorProbe>,
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: MirrorRaceConfig,
) -> Result<u64, NetworkError> {
    download_ranked_mirrors_with_fallback(mirrors, ranked, dest, sink, cancel, cfg, None).await
}

/// 分片的低速主源也可作为最后的单流兜底，正式失败的备用源不能使它丢失。
pub(crate) async fn download_ranked_mirrors_with_fallback(
    mirrors: &[String],
    ranked: Vec<MirrorProbe>,
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: MirrorRaceConfig,
    slow_fallback: Option<MirrorProbe>,
) -> Result<u64, NetworkError> {
    let allow_switch = ranked.len() > 1 || slow_fallback.is_some();
    let mut attempts: Vec<(MirrorProbe, bool)> =
        ranked.into_iter().map(|p| (p, allow_switch)).collect();
    let mut slowest_fallback = slow_fallback;
    let mut retried_slow = false;
    let mut next = 0;
    let mut last_err = None;
    loop {
        if cancel.is_cancelled() {
            return Err(NetworkError::Cancelled);
        }
        if next >= attempts.len() {
            if !retried_slow {
                if let Some(probe) = slowest_fallback.take() {
                    attempts.push((probe, false));
                    retried_slow = true;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let (probe, detect_low_speed) = &attempts[next];
        let url = &mirrors[probe.idx];
        // 智能选源每次胜者都可能变化，旧 .part 没有来源凭据，不能盲目续接。
        // 明确单 URL 的续传仍由 download_with_resume 保留。
        remove_if_exists(&part_path(dest)).await?;
        let message = if retried_slow {
            "候选源均较慢，继续使用可用源完成下载".to_string()
        } else if let Some(NetworkError::SlowDownload { speed_bps, .. }) = &last_err {
            format!(
                "持续低速（{:.1} KB/s），自动更换下载源",
                *speed_bps as f64 / 1024.0
            )
        } else if let Some(error) = &last_err {
            let reason = match error {
                NetworkError::Status(code) => format!("HTTP {code}"),
                NetworkError::IdleTimeout(_) => "等待下载数据超时".into(),
                NetworkError::ChecksumMismatch { .. } => "文件校验失败".into(),
                NetworkError::Truncated { .. } => "下载内容不完整".into(),
                _ => "连接异常".into(),
            };
            format!("下载源不可用，自动更换：{reason}")
        } else {
            format!(
                "已选择下载源，测速 {:.1} KB/s",
                probe.speed_bps as f64 / 1024.0
            )
        };
        push_stage(
            &sink,
            if next == 0 {
                DownloadStage::Streaming
            } else {
                DownloadStage::SwitchingMirror
            },
            url,
            &message,
        )
        .await;
        let dl_cfg = DownloadConfig {
            idle_timeout: cfg.idle_timeout,
            stage: DownloadStage::Streaming,
            mirror_url: Some(url.clone()),
            low_speed: if *detect_low_speed {
                cfg.low_speed.clone()
            } else {
                None
            },
        };
        let result = match download_with_resume(url, dest, sink.clone(), cancel.clone(), dl_cfg)
            .await
        {
            Ok(_) => {
                let result = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => Err(NetworkError::Cancelled),
                    result = verify_sha256_if_needed(dest, cfg.expected_sha256.as_deref()) => result,
                };
                if result.is_err() {
                    remove_if_exists(dest).await?;
                }
                result
            }
            Err(err) => Err(err),
        };
        match result {
            Ok(n) => {
                return if cancel.is_cancelled() {
                    Err(NetworkError::Cancelled)
                } else {
                    Ok(n)
                };
            }
            Err(NetworkError::Cancelled) => return Err(NetworkError::Cancelled),
            Err(err @ (NetworkError::Io(_) | NetworkError::InvalidArgument(_))) => return Err(err),
            Err(err) => {
                if matches!(err, NetworkError::SlowDownload { .. }) && slowest_fallback.is_none() {
                    slowest_fallback = Some(probe.clone());
                }
                warn!(target: "ncd_network::race", url=%url, err=%err, "download source failed; trying ranked fallback");
                last_err = Some(err);
            }
        }
        next += 1;
    }
    Err(NetworkError::AllMirrorsFailed(last_err.map_or_else(
        || "no usable download source".into(),
        |e| e.to_string(),
    )))
}

async fn remove_if_exists(path: &Path) -> Result<(), NetworkError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

async fn push_stage(
    sink: &Arc<dyn DownloadProgressSink>,
    stage: DownloadStage,
    mirror: &str,
    msg: &str,
) {
    sink.tick(ProgressUpdate {
        stage,
        downloaded: 0,
        total: None,
        speed_bps: None,
        mirror_url: Some(mirror.to_string()),
        message: msg.to_string(),
    })
    .await;
}
