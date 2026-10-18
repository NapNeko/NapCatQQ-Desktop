//! 单 URL 下载:HTTP Range 续传 + idle / 持续低速检测 + 进度节流
//!
//! 与 race / chunked 解耦,只做"给定 URL,把字节灌到 .part,写完 rename 到 dest"
//! idle / 低速超时由 caller(race)处理换源;dest 已有 .part 时取其大小作
//! Range offset;200 表示不支持续传则重下,206 验证起点后 append

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use reqwest::header;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::client::shared_client;
use crate::error::NetworkError;
use crate::progress::{DownloadProgressSink, DownloadStage, ProgressUpdate};
use crate::range::{
    PartFile, parse_content_length_from_range, parse_content_range_bounds, range_header_value,
    supports_resume,
};
use crate::speed::{LowSpeedConfig, LowSpeedMonitor, SpeedSampler};

pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(20);
pub const PROGRESS_THROTTLE_INTERVAL: Duration = Duration::from_millis(250);
pub const PROGRESS_THROTTLE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub idle_timeout: Duration,
    pub stage: DownloadStage,
    pub mirror_url: Option<String>,
    pub low_speed: Option<LowSpeedConfig>,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            stage: DownloadStage::Streaming,
            mirror_url: None,
            low_speed: None,
        }
    }
}

/// 单 URL 下载主入口.part 已存在则发 Range 续传;206 append / 200 truncate 重下;
/// 写完 rename .part → dest
pub async fn download_with_resume(
    url: &str,
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: DownloadConfig,
) -> Result<u64, NetworkError> {
    let client = shared_client();
    download_with_client(&client, url, dest, sink, cancel, cfg).await
}

pub async fn download_with_client(
    client: &Client,
    url: &str,
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: DownloadConfig,
) -> Result<u64, NetworkError> {
    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }

    let part = PartFile::open_or_create(dest).await?;
    let resume_offset = part.existing_bytes;
    info!(
        target: "ncd_network::download",
        dest = %dest.display(),
        resume_offset,
        stage = ?cfg.stage,
        "download start"
    );

    let mut req = client.get(url).header(header::ACCEPT_ENCODING, "identity");
    if resume_offset > 0 {
        req = req.header(header::RANGE, range_header_value(resume_offset));
    }

    let resp = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(NetworkError::Cancelled),
        result = timeout(cfg.idle_timeout, req.send()) => match result {
            Ok(result) => result?,
            Err(_) => return Err(NetworkError::IdleTimeout(cfg.idle_timeout)),
        },
    };
    stream_response(resp, part, dest, sink, cancel, cfg).await
}

pub(crate) async fn stream_response(
    resp: reqwest::Response,
    mut part: PartFile,
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: DownloadConfig,
) -> Result<u64, NetworkError> {
    let resume_offset = part.existing_bytes;
    let status = resp.status();
    if !status.is_success() {
        return Err(NetworkError::Status(status.as_u16()));
    }
    if resp
        .headers()
        .get(header::CONTENT_ENCODING)
        .is_some_and(|value| value != "identity")
    {
        return Err(NetworkError::Http(
            "unexpected Content-Encoding for file download".into(),
        ));
    }

    let used_resume = supports_resume(status);
    if used_resume {
        match parse_content_range_bounds(&resp) {
            Some((start, _)) if start == resume_offset => {}
            _ => {
                return Err(NetworkError::Http(
                    "invalid Content-Range for resume offset".into(),
                ));
            }
        }
    }
    if resume_offset > 0 && !used_resume {
        part.truncate().await?;
    }

    let total = if used_resume {
        parse_content_length_from_range(&resp, resume_offset)
    } else {
        resp.content_length()
    };

    let mut downloaded = if used_resume { resume_offset } else { 0 };
    let mut sampler = SpeedSampler::new();
    sampler.record(Instant::now(), downloaded);

    let mut last_progress_at = Instant::now();
    let mut last_progress_bytes = downloaded;
    let mut low_speed = cfg
        .low_speed
        .clone()
        .map(|cfg| LowSpeedMonitor::new(cfg, Instant::now(), downloaded));
    let mut ticker = tokio::time::interval(Duration::from_millis(500));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await;
    let mut last_byte_at = tokio::time::Instant::now();

    push_update(
        &sink,
        cfg.stage,
        downloaded,
        total,
        sampler.current_bps(),
        cfg.mirror_url.as_deref(),
        if used_resume { "resume" } else { "start" }.to_string(),
    )
    .await;

    let mut stream = resp.bytes_stream();
    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let _ = part.flush().await;
                return Err(NetworkError::Cancelled);
            }
            r = tokio::time::timeout_at(last_byte_at + cfg.idle_timeout, stream.next()) => r,
            _ = ticker.tick() => {
                let now = Instant::now();
                sampler.record(now, downloaded);
                if let Some(speed_bps) = low_speed.as_mut().and_then(|m| m.check(now, downloaded, total)) {
                    part.flush().await?;
                    return Err(NetworkError::SlowDownload {
                        speed_bps,
                        min_bps: cfg.low_speed.as_ref().map_or(0, |c| c.min_bps),
                    });
                }
                push_update(&sink, cfg.stage, downloaded, total, sampler.current_bps(),
                    cfg.mirror_url.as_deref(), "streaming".into()).await;
                last_progress_at = now;
                last_progress_bytes = downloaded;
                continue;
            }
        };

        let chunk_opt = match next {
            Ok(opt) => opt,
            Err(_) => {
                part.flush().await?;
                return Err(NetworkError::IdleTimeout(cfg.idle_timeout));
            }
        };

        let chunk = match chunk_opt {
            Some(Ok(c)) => c,
            Some(Err(e)) => return Err(NetworkError::from(e)),
            None => break,
        };

        if chunk.is_empty() {
            continue;
        }
        last_byte_at = tokio::time::Instant::now();

        if total.is_some_and(|t| downloaded.saturating_add(chunk.len() as u64) > t) {
            return Err(NetworkError::Http(
                "response exceeds declared file size".into(),
            ));
        }

        part.append(&chunk).await?;
        downloaded += chunk.len() as u64;
        sampler.record(Instant::now(), downloaded);

        let now = Instant::now();
        let bytes_since = downloaded - last_progress_bytes;
        if now.duration_since(last_progress_at) >= PROGRESS_THROTTLE_INTERVAL
            || bytes_since >= PROGRESS_THROTTLE_BYTES
        {
            last_progress_at = now;
            last_progress_bytes = downloaded;
            push_update(
                &sink,
                cfg.stage,
                downloaded,
                total,
                sampler.current_bps(),
                cfg.mirror_url.as_deref(),
                "streaming".to_string(),
            )
            .await;
        }
    }

    part.flush().await?;
    // 流自然结束 ≠ 下完服务端 / 中间代理可能在没发完 Content-Length 字节
    // 的情况下 EOF(连接被掐断,反代上游超时,CDN 缓存只缓了一部分)
    // 不校验就 finalize,会留下残缺的 .part 改名成 dest,下游 zip / tar
    // 解压立刻 "Could not find EOCD". 这里强制对齐 total,差一个字节也
    // 算失败,把 .part 留着让上层切 mirror 时清掉重下(mirror 间内容可能
    // 不一致,续传 offset 是危险操作,所以 race 切 mirror 时也会主动
    // truncate)
    if let Some(t) = total {
        if downloaded != t {
            // .part 仍保留在磁盘上以便观察/调试;finalize() 没被调用,
            // 不会污染 dest
            return Err(NetworkError::Truncated {
                downloaded,
                total: t,
            });
        }
    }

    part.finalize(dest).await?;
    push_update(
        &sink,
        cfg.stage,
        downloaded,
        total,
        sampler.current_bps(),
        cfg.mirror_url.as_deref(),
        "done".into(),
    )
    .await;
    Ok(downloaded)
}

async fn push_update(
    sink: &Arc<dyn DownloadProgressSink>,
    stage: DownloadStage,
    downloaded: u64,
    total: Option<u64>,
    speed: Option<u64>,
    mirror: Option<&str>,
    message: String,
) {
    sink.tick(ProgressUpdate {
        stage,
        downloaded,
        total,
        speed_bps: speed,
        mirror_url: mirror.map(|s| s.to_string()),
        message,
    })
    .await;
}
