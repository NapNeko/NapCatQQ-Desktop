//! 单 URL 下载：HTTP Range 续传 + chunk-level idle timeout + 进度节流。
//!
//! 与 race / chunked 解耦：本模块只做"给定一个 URL，把字节灌到 .part 文件，
//! 写完 rename 到 dest"。
//!
//! idle timeout：每次 `stream.next()` 包一层 [`tokio::time::timeout`]，超过约定
//! 时间没字节到达就报 [`NetworkError::IdleTimeout`]。caller（race）据此切镜像。
//!
//! 续传：caller 传入的 dest 若已有 `<dest>.part`，本模块自动取其大小作 Range
//! offset。服务端 200 表示不支持续传，本模块自动 truncate 重下；206 直接 append。

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::header;
use reqwest::{Client, Response};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::client::shared_client;
use crate::error::NetworkError;
use crate::progress::{DownloadProgressSink, DownloadStage, ProgressUpdate};
use crate::range::{
    parse_content_length_from_range, parse_content_range_bounds, range_header_value,
    supports_resume, PartFile,
};
use crate::speed::SpeedSampler;

pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(20);
pub const PROGRESS_THROTTLE_INTERVAL: Duration = Duration::from_millis(250);
pub const PROGRESS_THROTTLE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub idle_timeout: Duration,
    pub stage: DownloadStage,
    pub mirror_url: Option<String>,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            stage: DownloadStage::Streaming,
            mirror_url: None,
        }
    }
}

/// 单 URL 下载主入口。
///
/// 行为：
/// 1. 若 `<dest>.part` 已存在，发 `Range: bytes=N-`
/// 2. 服务端 206 → append；200 → truncate 后重下
/// 3. 每个 chunk 检查 cancel + idle timeout
/// 4. 进度按 250ms 或 1MB 节流推给 sink
/// 5. 全部下完后 rename `.part` → dest
pub async fn download_with_resume(
    url: &str,
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: DownloadConfig,
) -> Result<u64, NetworkError> {
    let client = shared_client();
    download_with_client(client, url, dest, sink, cancel, cfg).await
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

    let mut part = PartFile::open_or_create(dest).await?;
    let resume_offset = part.existing_bytes;
    info!(
        target: "ncd_network::download",
        dest = %dest.display(),
        resume_offset,
        stage = ?cfg.stage,
        "download start"
    );

    let mut req = client.get(url);
    if resume_offset > 0 {
        req = req.header(header::RANGE, range_header_value(resume_offset));
    }

    let resp = req.send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(NetworkError::Status(status.as_u16()));
    }

    let used_resume = supports_resume(status);
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
                return Err(NetworkError::Cancelled);
            }
            r = timeout(cfg.idle_timeout, stream.next()) => r,
        };

        let chunk_opt = match next {
            Ok(opt) => opt,
            Err(_) => return Err(NetworkError::IdleTimeout(cfg.idle_timeout)),
        };

        let chunk = match chunk_opt {
            Some(Ok(c)) => c,
            Some(Err(e)) => return Err(NetworkError::from(e)),
            None => break,
        };

        if chunk.is_empty() {
            continue;
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
    push_update(
        &sink,
        cfg.stage,
        downloaded,
        total,
        sampler.current_bps(),
        cfg.mirror_url.as_deref(),
        "done".to_string(),
    )
    .await;

    // 流自然结束 ≠ 下完。服务端 / 中间代理可能在没发完 Content-Length 字节
    // 的情况下 EOF（连接被掐断、反代上游超时、CDN 缓存只缓了一部分）。
    // 不校验就 finalize，会留下残缺的 .part 改名成 dest，下游 zip / tar
    // 解压立刻 "Could not find EOCD"。这里强制对齐 total，差一个字节也
    // 算失败，把 .part 留着让上层切 mirror 时清掉重下（mirror 间内容可能
    // 不一致，续传 offset 是危险操作，所以 race 切 mirror 时也会主动
    // truncate）。
    if let Some(t) = total {
        if downloaded < t {
            // .part 仍保留在磁盘上以便观察/调试；finalize() 没被调用，
            // 不会污染 dest。
            return Err(NetworkError::Truncated {
                downloaded,
                total: t,
            });
        }
    }

    part.finalize(dest).await?;
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

/// 共享进度状态：chunked 多个并发切片聚合到同一个 sink 时使用。
///
/// 单纯 download_with_resume 不需要它。chunked 模块跑 4 片并发，每片往
/// `add_bytes` 累加，由独立的进度上报任务定时读 `snapshot` 推给 sink，
/// 避免每片各自往 sink 推导致 UI 闪烁与数字回退。
#[derive(Clone, Default)]
pub struct AggregatedProgress {
    inner: Arc<Mutex<AggregatedInner>>,
}

#[derive(Default)]
struct AggregatedInner {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub sampler: SpeedSampler,
}

impl AggregatedProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn set_total(&self, total: Option<u64>) {
        let mut g = self.inner.lock().await;
        g.total = total;
    }

    pub async fn add_bytes(&self, bytes: u64) -> (u64, Option<u64>, Option<u64>) {
        let mut g = self.inner.lock().await;
        g.downloaded += bytes;
        let downloaded = g.downloaded;
        g.sampler.record(Instant::now(), downloaded);
        (downloaded, g.total, g.sampler.current_bps())
    }

    pub async fn snapshot(&self) -> (u64, Option<u64>, Option<u64>) {
        let g = self.inner.lock().await;
        (g.downloaded, g.total, g.sampler.current_bps())
    }
}

/// 内部 helper：固定 byte range 下载到指定文件（不走 .part 续传协议）。
///
/// 给 chunked.rs 用。每个切片调一次，dest 是该切片的临时路径
/// （例如 `<final-dest>.chunk-0`）。切片范围由 `range` 指定（inclusive 双闭）。
///
/// 不带聚合进度时 sink_aggregated 传 None；chunked 模式下传 Some 让本函数
/// 把每个 chunk 的 bytes 加进去。
pub(crate) async fn download_byte_range(
    client: &Client,
    url: &str,
    dest_chunk: &Path,
    range: (u64, u64),
    cancel: CancellationToken,
    idle_timeout: Duration,
    aggregated: Option<AggregatedProgress>,
) -> Result<u64, NetworkError> {
    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    if range.1 < range.0 {
        return Err(NetworkError::InvalidArgument(format!(
            "invalid byte range: {range:?}"
        )));
    }

    if let Some(parent) = dest_chunk.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let header_val = format!("bytes={}-{}", range.0, range.1);
    let resp = client.get(url).header(header::RANGE, header_val).send().await?;
    let status = resp.status();
    if !supports_resume(status) {
        return Err(NetworkError::Status(status.as_u16()));
    }

    // 服务器声称 206，但代理 / 反代不一定真切了 byte range；有些镜像会
    // 拿一份缓存的"前 N 字节"副本贴 206 头返回，等于 byte range mismatch。
    // 这种情况下盲信会写出错位字节，merge 时拼出无法解析的 zip / tar，
    // EOCD 找不到错的根因。强制比对 Content-Range，不一致直接拒掉，让
    // chunked 上层重试下个 mirror。
    match parse_content_range_bounds(&resp) {
        Some((start, end)) if start == range.0 && end == range.1 => {}
        Some((start, end)) => {
            return Err(NetworkError::Http(format!(
                "Content-Range mismatch: requested {}-{}, got {}-{}",
                range.0, range.1, start, end
            )));
        }
        None => {
            return Err(NetworkError::Http(
                "Content-Range header missing on 206 response".into(),
            ));
        }
    }

    let mut file = tokio::fs::File::create(dest_chunk).await?;
    use tokio::io::AsyncWriteExt;

    let mut downloaded: u64 = 0;
    let expected = range.1 - range.0 + 1;
    let mut stream = resp.bytes_stream();

    loop {
        let next = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                drop(file);
                let _ = tokio::fs::remove_file(dest_chunk).await;
                return Err(NetworkError::Cancelled);
            }
            r = timeout(idle_timeout, stream.next()) => r,
        };

        let chunk_opt = match next {
            Ok(opt) => opt,
            Err(_) => return Err(NetworkError::IdleTimeout(idle_timeout)),
        };

        let chunk: Bytes = match chunk_opt {
            Some(Ok(c)) => c,
            Some(Err(e)) => return Err(NetworkError::from(e)),
            None => break,
        };

        if chunk.is_empty() {
            continue;
        }

        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if let Some(a) = &aggregated {
            a.add_bytes(chunk.len() as u64).await;
        }
    }

    file.flush().await?;

    if downloaded != expected {
        return Err(NetworkError::Http(format!(
            "chunk size mismatch: expected {expected}, got {downloaded}"
        )));
    }

    Ok(downloaded)
}

/// 探测远端文件大小 + 是否支持 Range。
///
/// 用 GET + `Range: bytes=0-0` 的方式（HEAD 在 GitHub releases / objects 上
/// 有镜像不支持）。返回 (total_bytes, accept_ranges)。
pub(crate) async fn probe_size_and_range(
    client: &Client,
    url: &str,
    cancel: &CancellationToken,
    idle_timeout: Duration,
) -> Result<(Option<u64>, bool), NetworkError> {
    let resp_fut = client.get(url).header(header::RANGE, "bytes=0-0").send();
    let resp: Response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(NetworkError::Cancelled),
        r = timeout(idle_timeout, resp_fut) => match r {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(NetworkError::from(e)),
            Err(_) => return Err(NetworkError::IdleTimeout(idle_timeout)),
        }
    };

    let status = resp.status();
    if !status.is_success() {
        return Err(NetworkError::Status(status.as_u16()));
    }

    let accept_ranges = supports_resume(status);
    let total = if accept_ranges {
        parse_content_length_from_range(&resp, 0)
    } else {
        resp.content_length()
    };

    // 立即丢弃 stream，释放连接
    drop(resp);
    Ok((total, accept_ranges))
}
