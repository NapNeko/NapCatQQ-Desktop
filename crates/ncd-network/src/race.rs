//! Mirror Race：多镜像并发探测，谁先吐第一字节用谁。
//!
//! 流程（Top-2 阶梯加码 + first-chunk 判赢）：
//! 1. 启动前 2 个镜像的 probe（不写盘，只发 GET 等首 chunk）
//! 2. 每 [`MirrorRaceConfig::stagger`] 增加一个 racer，直到镜像耗尽或有胜者
//! 3. 第一个收到 chunk 的 racer 报告 url，主控 cancel 其他 racer
//! 4. 主控用 winner url 启动正式下载（[`download_with_resume`]），stage=Streaming
//! 5. 若中途 idle timeout 或硬错，主控切到下一个未尝试镜像，stage=SwitchingMirror，
//!    自动从 `.part` 续传
//! 6. 全部镜像跑挂 → [`NetworkError::AllMirrorsFailed`]
//!
//! 不在 race 阶段复用 stream / 字节：
//! - probe 阶段拿到首 chunk 就 abort 自己（不写盘）
//! - winner 重新发 GET 走标准 [`download_with_resume`] 路径
//! - 代价：每个 mirror 多收几十 KB，但代码简洁、正确性强（避免多 racer 抢
//!   写同一 .part 的并发问题）

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::client::shared_client;
use crate::download::{download_with_resume, DownloadConfig, DEFAULT_IDLE_TIMEOUT};
use crate::error::NetworkError;
use crate::progress::{DownloadProgressSink, DownloadStage, ProgressUpdate};

#[derive(Debug, Clone)]
pub struct MirrorRaceConfig {
    /// 初始并发探测的镜像数。默认 2。
    pub initial_parallel: usize,
    /// 阶梯加码间隔：每 stagger 加 1 个 racer。默认 3s。
    pub stagger: Duration,
    /// 单 mirror 等首字节最长时间。默认 30s。
    pub probe_first_chunk_timeout: Duration,
    /// 正式下载阶段每 chunk 的 idle timeout。默认 20s。
    pub idle_timeout: Duration,
}

impl Default for MirrorRaceConfig {
    fn default() -> Self {
        Self {
            initial_parallel: 2,
            stagger: Duration::from_secs(3),
            probe_first_chunk_timeout: Duration::from_secs(30),
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        }
    }
}

#[derive(Debug)]
enum RacerOutcome {
    FirstChunk { idx: usize, url: String },
    Failed { idx: usize, url: String, err: String },
}

/// Mirror race 主入口。
///
/// `mirrors` 顺序即偏好顺序：前 [`MirrorRaceConfig::initial_parallel`] 个先并发，
/// 之后按 stagger 节奏依次加入。
pub async fn download_with_mirror_race(
    mirrors: &[String],
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: MirrorRaceConfig,
) -> Result<u64, NetworkError> {
    if mirrors.is_empty() {
        return Err(NetworkError::InvalidArgument("mirrors is empty".into()));
    }
    if cfg.initial_parallel == 0 {
        return Err(NetworkError::InvalidArgument(
            "initial_parallel must be >= 1".into(),
        ));
    }

    let winner_idx = match run_race(mirrors, &sink, &cancel, &cfg).await? {
        Some(idx) => idx,
        None => {
            return Err(NetworkError::AllMirrorsFailed(
                "no mirror produced first chunk".into(),
            ));
        }
    };

    let mut tried: Vec<usize> = vec![winner_idx];
    #[allow(unused_assignments)]
    let mut last_err: Option<NetworkError> = None;

    let primary = &mirrors[winner_idx];
    push_stage(&sink, DownloadStage::Streaming, primary, "winner").await;

    let dl_cfg = DownloadConfig {
        idle_timeout: cfg.idle_timeout,
        stage: DownloadStage::Streaming,
        mirror_url: Some(primary.clone()),
    };
    match download_with_resume(primary, dest, sink.clone(), cancel.clone(), dl_cfg).await {
        Ok(n) => return Ok(n),
        Err(NetworkError::Cancelled) => return Err(NetworkError::Cancelled),
        Err(e) => {
            warn!(target: "ncd_network::race", url = %primary, err = %e, "primary mirror failed");
            last_err = Some(e);
        }
    }

    for (idx, url) in mirrors.iter().enumerate() {
        if tried.contains(&idx) {
            continue;
        }
        tried.push(idx);
        push_stage(&sink, DownloadStage::SwitchingMirror, url, "switch").await;
        let dl_cfg = DownloadConfig {
            idle_timeout: cfg.idle_timeout,
            stage: DownloadStage::Streaming,
            mirror_url: Some(url.clone()),
        };
        match download_with_resume(url, dest, sink.clone(), cancel.clone(), dl_cfg).await {
            Ok(n) => return Ok(n),
            Err(NetworkError::Cancelled) => return Err(NetworkError::Cancelled),
            Err(e) => {
                warn!(target: "ncd_network::race", url = %url, err = %e, "fallback mirror failed");
                last_err = Some(e);
            }
        }
    }

    Err(NetworkError::AllMirrorsFailed(
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "no further mirrors".into()),
    ))
}

async fn run_race(
    mirrors: &[String],
    sink: &Arc<dyn DownloadProgressSink>,
    cancel: &CancellationToken,
    cfg: &MirrorRaceConfig,
) -> Result<Option<usize>, NetworkError> {
    let race_cancel = cancel.child_token();
    // capacity 留余量：每个 mirror 最多发 1 条 outcome
    let (tx, mut rx) = mpsc::channel::<RacerOutcome>(mirrors.len().max(1) + 4);
    let mut tasks: JoinSet<()> = JoinSet::new();
    let mut next_idx: usize = 0;

    let initial = cfg.initial_parallel.min(mirrors.len());
    for _ in 0..initial {
        if next_idx >= mirrors.len() {
            break;
        }
        spawn_racer(
            &mut tasks,
            next_idx,
            mirrors[next_idx].clone(),
            race_cancel.child_token(),
            tx.clone(),
            cfg.probe_first_chunk_timeout,
        );
        next_idx += 1;
    }

    push_stage(
        sink,
        DownloadStage::Racing,
        &mirrors[0],
        &format!("probing {initial} mirrors"),
    )
    .await;

    let mut active = initial;
    let mut stagger = tokio::time::interval(cfg.stagger);
    stagger.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    stagger.tick().await;

    let winner = loop {
        if active == 0 && next_idx >= mirrors.len() {
            break None;
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                race_cancel.cancel();
                drop(tx);
                while tasks.join_next().await.is_some() {}
                return Err(NetworkError::Cancelled);
            }
            Some(outcome) = rx.recv() => {
                active = active.saturating_sub(1);
                match outcome {
                    RacerOutcome::FirstChunk { idx, url } => {
                        debug!(target: "ncd_network::race", url=%url, "race winner");
                        break Some(idx);
                    }
                    RacerOutcome::Failed { idx, url, err } => {
                        debug!(target: "ncd_network::race", url=%url, idx, err, "racer failed");
                        if next_idx < mirrors.len() {
                            spawn_racer(
                                &mut tasks,
                                next_idx,
                                mirrors[next_idx].clone(),
                                race_cancel.child_token(),
                                tx.clone(),
                                cfg.probe_first_chunk_timeout,
                            );
                            next_idx += 1;
                            active += 1;
                        }
                    }
                }
            }
            _ = stagger.tick() => {
                if next_idx < mirrors.len() {
                    spawn_racer(
                        &mut tasks,
                        next_idx,
                        mirrors[next_idx].clone(),
                        race_cancel.child_token(),
                        tx.clone(),
                        cfg.probe_first_chunk_timeout,
                    );
                    next_idx += 1;
                    active += 1;
                }
            }
        }
    };

    race_cancel.cancel();
    drop(tx);
    while tasks.join_next().await.is_some() {}
    Ok(winner)
}

fn spawn_racer(
    tasks: &mut JoinSet<()>,
    idx: usize,
    url: String,
    cancel: CancellationToken,
    tx: mpsc::Sender<RacerOutcome>,
    first_chunk_timeout: Duration,
) {
    tasks.spawn(async move {
        let outcome = probe_first_chunk(&url, &cancel, first_chunk_timeout).await;
        let msg = match outcome {
            Ok(()) => RacerOutcome::FirstChunk {
                idx,
                url: url.clone(),
            },
            Err(e) => RacerOutcome::Failed {
                idx,
                url: url.clone(),
                err: e.to_string(),
            },
        };
        let _ = tx.send(msg).await;
    });
}

/// 单 racer：发 GET，等第一个 chunk 到达就成功返回。
///
/// 不写盘、不消费后续字节：拿到首字节立刻 drop stream 释放连接。
async fn probe_first_chunk(
    url: &str,
    cancel: &CancellationToken,
    first_chunk_timeout: Duration,
) -> Result<(), NetworkError> {
    let client = shared_client();
    let req = client.get(url);
    let resp_fut = req.send();

    let resp = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(NetworkError::Cancelled),
        r = timeout(first_chunk_timeout, resp_fut) => {
            match r {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => return Err(NetworkError::from(e)),
                Err(_) => return Err(NetworkError::IdleTimeout(first_chunk_timeout)),
            }
        }
    };

    let status = resp.status();
    if !status.is_success() {
        return Err(NetworkError::Status(status.as_u16()));
    }

    let mut stream = resp.bytes_stream();
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(NetworkError::Cancelled),
        r = timeout(first_chunk_timeout, stream.next()) => {
            match r {
                Ok(Some(Ok(chunk))) if !chunk.is_empty() => Ok(()),
                Ok(Some(Ok(_))) => Err(NetworkError::Http("empty first chunk".into())),
                Ok(Some(Err(e))) => Err(NetworkError::from(e)),
                Ok(None) => Err(NetworkError::Http("stream ended before first chunk".into())),
                Err(_) => Err(NetworkError::IdleTimeout(first_chunk_timeout)),
            }
        }
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
