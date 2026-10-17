//! 单源快探与镜像测速共用并发分片，文件校验完成后才提交。

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header;
use tokio_util::sync::CancellationToken;

use crate::NetworkError;
use crate::client::{shared_client, with_network_snapshot};
use crate::download::{
    DEFAULT_IDLE_TIMEOUT, DownloadConfig, download_with_resume, stream_response,
};
use crate::progress::{DownloadProgressSink, DownloadStage};
use crate::race::{
    MirrorProbe, MirrorRaceConfig, download_ranked_mirrors, download_ranked_mirrors_with_fallback,
    select_mirrors,
};
use crate::range::{PartFile, content_range, strong_etag};
use crate::segmented::download_segments;
use crate::staging::StagedDownload;
use crate::verify::verify_sha256_if_needed;

pub const CHUNKED_THRESHOLD: u64 = 4 * 1024 * 1024;
pub const DEFAULT_CHUNK_PARTS: usize = 8;

#[derive(Debug, Clone)]
pub struct ChunkedConfig {
    /// 并发上限；实际连接数还受文件大小与全局下载预算限制。
    pub parts: usize,
    pub threshold: u64,
    pub idle_timeout: Duration,
    pub race_cfg: MirrorRaceConfig,
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

impl ChunkedConfig {
    fn validate(&mut self) -> Result<(), NetworkError> {
        if self.parts == 0 || self.idle_timeout.is_zero() {
            return Err(NetworkError::InvalidArgument(
                "invalid parallel download configuration".into(),
            ));
        }
        self.parts = self.parts.min(16);
        for checksum in [
            &mut self.expected_sha256,
            &mut self.race_cfg.expected_sha256,
        ] {
            if checksum.as_ref().is_some_and(String::is_empty) {
                *checksum = None;
            }
            if checksum.as_ref().is_some_and(|hash| {
                hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit())
            }) {
                return Err(NetworkError::InvalidArgument(
                    "SHA256 must contain 64 hexadecimal digits".into(),
                ));
            }
        }
        if let (Some(outer), Some(inner)) = (&self.expected_sha256, &self.race_cfg.expected_sha256)
        {
            if !outer.eq_ignore_ascii_case(inner) {
                return Err(NetworkError::InvalidArgument(
                    "conflicting SHA256 expectations".into(),
                ));
            }
        }
        if self.race_cfg.expected_sha256.is_none() {
            self.race_cfg.expected_sha256 = self.expected_sha256.clone();
        }
        Ok(())
    }

    fn can_split(&self, probe: &MirrorProbe) -> bool {
        self.parts > 1 && probe.supports_ranges
            && probe.total.is_some_and(|size| size > 0 && size >= self.threshold)
            // 无强校验器的片段只能在有完整文件散列时合并，避免拼接两个版本。
            && (probe.etag.is_some() || self.race_cfg.expected_sha256.is_some())
    }
}

/// 单 URL 不测速；普通 200 响应直接流式保存。
pub async fn download_url(
    url: &str,
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    mut cfg: ChunkedConfig,
) -> Result<u64, NetworkError> {
    with_network_snapshot(async {
        cfg.validate()?;
        if cancel.is_cancelled() { return Err(NetworkError::Cancelled); }
        let target = StagedDownload::create(dest).await?;
        let client = shared_client();
        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(NetworkError::Cancelled),
            result = tokio::time::timeout(cfg.idle_timeout, client.get(url)
                .header(header::RANGE, "bytes=0-0").header(header::ACCEPT_ENCODING, "identity").send()) =>
                result.map_err(|_| NetworkError::IdleTimeout(cfg.idle_timeout))??,
        };
        let status = response.status();
        let single_cfg = DownloadConfig { idle_timeout: cfg.idle_timeout,
            stage: DownloadStage::Streaming, mirror_url: Some(url.into()), low_speed: None };
        let downloaded = if status == reqwest::StatusCode::OK {
            let part = PartFile::open_or_create(&target.file).await?;
            stream_response(response, part, &target.file, sink.clone(), cancel.clone(), single_cfg).await?
        } else {
            let bounds = content_range(&response);
            let probe = MirrorProbe { idx: 0, total: bounds.map(|b| b.2),
                supports_ranges: status == reqwest::StatusCode::PARTIAL_CONTENT && bounds.is_some_and(|b| b.0 == 0 && b.1 == 0),
                speed_bps: 0, etag: strong_etag(&response) };
            drop(response);
            if !status.is_success() && !matches!(status.as_u16(), 400 | 405 | 416 | 501) {
                return Err(NetworkError::Status(status.as_u16()));
            }
            if cfg.can_split(&probe) {
                let mut segmented_cfg = cfg.clone();
                segmented_cfg.race_cfg.low_speed = None;
                match download_segments(url, target.clone(), probe.total.unwrap_or(0), probe.etag, sink.clone(), cancel.clone(), &segmented_cfg).await {
                    Ok(n) => n,
                    Err(err @ (NetworkError::Cancelled | NetworkError::Io(_) | NetworkError::InvalidArgument(_))) => return Err(err),
                    Err(_) => {
                        remove_partial(&target.file).await?;
                        download_with_resume(url, &target.file, sink.clone(), cancel.clone(), single_cfg).await?
                    }
                }
            } else {
                download_with_resume(url, &target.file, sink.clone(), cancel.clone(), single_cfg).await?
            }
        };
        verify_and_commit(&target, dest, downloaded, &cancel, cfg.race_cfg.expected_sha256.as_deref()).await
    }).await
}

pub async fn download_smart(
    mirrors: &[String],
    dest: &Path,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    mut cfg: ChunkedConfig,
) -> Result<u64, NetworkError> {
    with_network_snapshot(async {
        cfg.validate()?;
        if cancel.is_cancelled() {
            return Err(NetworkError::Cancelled);
        }
        let target = StagedDownload::create(dest).await?;
        let downloaded =
            download_ranked(mirrors, target.clone(), sink, cancel.clone(), cfg.clone()).await?;
        // 排名下载已校验每个候选，提交前无需再读取整份文件。
        verify_and_commit(&target, dest, downloaded, &cancel, None).await
    })
    .await
}

async fn download_ranked(
    mirrors: &[String],
    target: Arc<StagedDownload>,
    sink: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
    cfg: ChunkedConfig,
) -> Result<u64, NetworkError> {
    let mut ranked = select_mirrors(mirrors, &sink, &cancel, &cfg.race_cfg).await?;
    let primary = ranked
        .first()
        .cloned()
        .ok_or_else(|| NetworkError::AllMirrorsFailed("no usable mirror".into()))?;
    if !cfg.can_split(&primary) {
        return download_ranked_mirrors(mirrors, ranked, &target.file, sink, cancel, cfg.race_cfg)
            .await;
    }
    let mut chunk_cfg = cfg.clone();
    if ranked.len() == 1 {
        chunk_cfg.race_cfg.low_speed = None;
    }
    let result = download_segments(
        &mirrors[primary.idx],
        target.clone(),
        primary.total.unwrap_or(0),
        primary.etag.clone(),
        sink.clone(),
        cancel.clone(),
        &chunk_cfg,
    )
    .await;
    let result = match result {
        Ok(n) => tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(NetworkError::Cancelled),
            verified = verify_sha256_if_needed(&target.file, cfg.race_cfg.expected_sha256.as_deref()) => verified.map(|_| n),
        },
        Err(error) => Err(error),
    };
    match result {
        Ok(n) => Ok(n),
        Err(
            error @ (NetworkError::Cancelled
            | NetworkError::Io(_)
            | NetworkError::InvalidArgument(_)),
        ) => Err(error),
        Err(error) => {
            let slow = matches!(error, NetworkError::SlowDownload { .. }).then(|| primary.clone());
            remove_partial(&target.file).await?;
            let _ = tokio::fs::remove_file(&target.file).await;
            ranked.retain(|p| p.idx != primary.idx);
            if ranked.is_empty() {
                let mut fallback = cfg.race_cfg;
                fallback.low_speed = None;
                download_ranked_mirrors(
                    mirrors,
                    vec![primary],
                    &target.file,
                    sink,
                    cancel,
                    fallback,
                )
                .await
            } else {
                download_ranked_mirrors_with_fallback(
                    mirrors,
                    ranked,
                    &target.file,
                    sink,
                    cancel,
                    cfg.race_cfg,
                    slow,
                )
                .await
            }
        }
    }
}

async fn verify_and_commit(
    target: &StagedDownload,
    destination: &Path,
    downloaded: u64,
    cancel: &CancellationToken,
    checksum: Option<&str>,
) -> Result<u64, NetworkError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(NetworkError::Cancelled),
        result = verify_sha256_if_needed(&target.file, checksum) => { result?; }
    }
    if cancel.is_cancelled() {
        return Err(NetworkError::Cancelled);
    }
    target.commit(destination).await?;
    Ok(downloaded)
}

async fn remove_partial(path: &Path) -> Result<(), NetworkError> {
    match tokio::fs::remove_file(crate::range::part_path(path)).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_checksum_does_not_enable_unvalidated_ranges() {
        let mut cfg = ChunkedConfig {
            expected_sha256: Some(String::new()),
            ..Default::default()
        };
        cfg.validate().unwrap();
        let probe = MirrorProbe {
            idx: 0,
            total: Some(32 * 1024 * 1024),
            supports_ranges: true,
            speed_bps: 0,
            etag: None,
        };
        assert!(!cfg.can_split(&probe));
    }

    #[test]
    fn invalid_or_conflicting_checksums_fail_before_downloading() {
        let mut cfg = ChunkedConfig {
            expected_sha256: Some("invalid".into()),
            ..Default::default()
        };
        assert!(matches!(
            cfg.validate(),
            Err(NetworkError::InvalidArgument(_))
        ));
        cfg.expected_sha256 = Some("00".repeat(32));
        cfg.race_cfg.expected_sha256 = Some("11".repeat(32));
        assert!(matches!(
            cfg.validate(),
            Err(NetworkError::InvalidArgument(_))
        ));
    }
}
