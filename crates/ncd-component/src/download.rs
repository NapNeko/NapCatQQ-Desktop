//! `DownloadHelper`：HTTP 下载 + SHA256 校验 + 多镜像 race + 切片并行。
//!
//! 单 URL 路径走 ncd_network::download_with_resume（idle timeout + Range
//! 续传）；调用 [`DownloadHelper::download_with_mirrors`] 走 ncd_network 的
//! mirror race + ≥16MB 切片并行下载。SHA256 在所有路径下载完成后做。
//!
//! 设计：
//! - 进度桥接：实现一个 `CtxSink`，把 ncd_network::ProgressUpdate 翻成
//!   ProgressKind::StepProgress + speed_bps，emit 到 ActionCtx
//! - 校验失败：删除已落盘的 dest 文件，再返回 ChecksumMismatch
//! - 取消：cancel token 来自 ctx，原子传递给 ncd_network

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_network::{
    download_smart, download_with_mirror_race, download_with_resume, ChunkedConfig,
    DownloadConfig, DownloadProgressSink, DownloadStage, MirrorRaceConfig, NetworkError,
    ProgressUpdate,
};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::context::{ActionCtx, ProgressKind};
use crate::error::ActionError;

/// 下载辅助。保留 stateful 接口以兼容旧调用点；内部不再持有 reqwest::Client，
/// 走 ncd_network::shared_client() 共享连接池。
pub struct DownloadHelper;

impl DownloadHelper {
    pub fn new() -> Result<Self, ActionError> {
        Ok(Self)
    }

    /// 兼容旧 API：单 URL 下载到本地，可选 SHA256。
    ///
    /// 内部走 ncd_network::download_with_resume（带 idle timeout + 续传）。
    pub async fn download_to_file(
        &self,
        url: &str,
        dest_path: &Path,
        expected_sha256: Option<&str>,
        ctx: &ActionCtx,
        step: u32,
    ) -> Result<(), ActionError> {
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }

        emit_step(ctx, step, 0, format!("download {url}"), None).await;

        let sink: Arc<dyn DownloadProgressSink> =
            Arc::new(CtxSink::new(ctx.clone(), step, url.to_string()));

        let cfg = DownloadConfig {
            mirror_url: Some(url.to_string()),
            ..Default::default()
        };

        match download_with_resume(url, dest_path, sink, ctx.cancel_token(), cfg).await {
            Ok(_) => {}
            Err(NetworkError::Cancelled) => return Err(ActionError::Cancelled),
            Err(e) => return Err(map_network_err(url, e)),
        }

        if let Some(expected) = expected_sha256 {
            verify_sha256(dest_path, expected).await?;
        }

        emit_step(
            ctx,
            step,
            100,
            format!("downloaded {}", file_size_label(dest_path).await),
            None,
        )
        .await;
        Ok(())
    }

    /// 多镜像下载：自动 race 选 winner，stall 时切镜像，≥16MB 自动切片。
    ///
    /// `mirrors`：候选 URL 列表（一般用 `ncd_network::build_mirror_urls(原始 URL)`
    /// 生成）。第一个 URL 用作进度上报里的 "primary" 标识。
    ///
    /// `expected_sha256`：Some 时下载完成后立即在 ncd-network 内部校验 sha256；
    /// mismatch **会切下家**而不是直接报 ChecksumMismatch（堵代理"返完整长度
    /// 的垃圾字节"投毒洞，前 4 轮字节级防御都防不住）。所有镜像都失败才返
    /// AllMirrorsFailed。None 跳过校验（兼容上游 release 还没 digest 的老仓库）。
    pub async fn download_with_mirrors(
        &self,
        mirrors: &[String],
        dest_path: &Path,
        expected_sha256: Option<&str>,
        ctx: &ActionCtx,
        step: u32,
    ) -> Result<(), ActionError> {
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        if mirrors.is_empty() {
            return Err(ActionError::InvalidConfig {
                reason: "mirrors is empty".into(),
            });
        }

        emit_step(
            ctx,
            step,
            0,
            format!("download {} (race {} mirrors)", &mirrors[0], mirrors.len()),
            None,
        )
        .await;

        let sink: Arc<dyn DownloadProgressSink> =
            Arc::new(CtxSink::new(ctx.clone(), step, mirrors[0].clone()));

        let mut cfg = ChunkedConfig::default();
        // sha256 同时塞 chunked + race fallback，保证 race fallback 链路也能
        // 检出投毒（race 自己也会校验）。
        cfg.expected_sha256 = expected_sha256.map(|s| s.to_string());
        cfg.race_cfg.expected_sha256 = expected_sha256.map(|s| s.to_string());
        match download_smart(mirrors, dest_path, sink, ctx.cancel_token(), cfg).await {
            Ok(_) => {}
            Err(NetworkError::Cancelled) => return Err(ActionError::Cancelled),
            Err(e) => return Err(map_network_err(&mirrors[0], e)),
        }

        emit_step(
            ctx,
            step,
            100,
            format!("downloaded {}", file_size_label(dest_path).await),
            None,
        )
        .await;
        Ok(())
    }

    /// 多镜像下载，但强制只走 race + 单流（不切片）。给小文件 / 不支持 Range
    /// 的端点（部分镜像 reverse-proxy 会丢 Range header）专用。
    ///
    /// sha256 校验语义同 [`Self::download_with_mirrors`]：mismatch 切下家。
    pub async fn download_with_mirrors_no_chunk(
        &self,
        mirrors: &[String],
        dest_path: &Path,
        expected_sha256: Option<&str>,
        ctx: &ActionCtx,
        step: u32,
    ) -> Result<(), ActionError> {
        if ctx.is_cancelled() {
            return Err(ActionError::Cancelled);
        }
        if mirrors.is_empty() {
            return Err(ActionError::InvalidConfig {
                reason: "mirrors is empty".into(),
            });
        }

        emit_step(
            ctx,
            step,
            0,
            format!("download {} (race {} mirrors)", &mirrors[0], mirrors.len()),
            None,
        )
        .await;

        let sink: Arc<dyn DownloadProgressSink> =
            Arc::new(CtxSink::new(ctx.clone(), step, mirrors[0].clone()));

        let mut race_cfg = MirrorRaceConfig::default();
        race_cfg.expected_sha256 = expected_sha256.map(|s| s.to_string());
        match download_with_mirror_race(
            mirrors,
            dest_path,
            sink,
            ctx.cancel_token(),
            race_cfg,
        )
        .await
        {
            Ok(_) => {}
            Err(NetworkError::Cancelled) => return Err(ActionError::Cancelled),
            Err(e) => return Err(map_network_err(&mirrors[0], e)),
        }

        emit_step(
            ctx,
            step,
            100,
            format!("downloaded {}", file_size_label(dest_path).await),
            None,
        )
        .await;
        Ok(())
    }
}

async fn verify_sha256(dest_path: &Path, expected: &str) -> Result<(), ActionError> {
    let mut file = fs::File::open(dest_path).await.map_err(|e| {
        ActionError::install_step("open_for_hash", format!("{e}"))
    })?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| ActionError::install_step("read_for_hash", format!("{e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = hex::encode(hasher.finalize());
    if !expected.eq_ignore_ascii_case(&actual) {
        let _ = fs::remove_file(dest_path).await;
        return Err(ActionError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

fn map_network_err(url: &str, e: NetworkError) -> ActionError {
    match e {
        NetworkError::Cancelled => ActionError::Cancelled,
        NetworkError::ChecksumMismatch { expected, actual } => {
            ActionError::ChecksumMismatch { expected, actual }
        }
        other => ActionError::DownloadFailed {
            url: url.to_string(),
            reason: other.to_string(),
        },
    }
}

async fn file_size_label(path: &Path) -> String {
    match fs::metadata(path).await {
        Ok(m) => fmt_bytes(m.len()),
        Err(_) => "?".into(),
    }
}

async fn emit_step(
    ctx: &ActionCtx,
    step: u32,
    percent: u8,
    message: String,
    speed_bps: Option<u64>,
) {
    ctx.emit(ProgressKind::StepProgress {
        step,
        percent,
        message,
        speed_bps,
        downloaded_bytes: None,
        total_bytes: None,
        download_stage: None,
        docker_layers: None,
    })
    .await;
}

fn fmt_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.2} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

fn fmt_bps(bps: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bps >= MB {
        format!("{:.2} MB/s", bps as f64 / MB as f64)
    } else if bps >= KB {
        format!("{:.0} KB/s", bps as f64 / KB as f64)
    } else {
        format!("{bps} B/s")
    }
}

/// 把 ncd_network::ProgressUpdate 翻成 ActionCtx::emit。
struct CtxSink {
    ctx: ActionCtx,
    step: u32,
    primary_url: String,
}

impl CtxSink {
    fn new(ctx: ActionCtx, step: u32, primary_url: String) -> Self {
        Self {
            ctx,
            step,
            primary_url,
        }
    }
}

#[async_trait]
impl DownloadProgressSink for CtxSink {
    async fn tick(&self, update: ProgressUpdate) {
        let pct = match (update.total, update.downloaded) {
            (Some(t), d) if t > 0 => ((d as f64 / t as f64) * 100.0).clamp(0.0, 99.0) as u8,
            _ => 0,
        };

        let stage_label = match update.stage {
            DownloadStage::Racing => "race",
            DownloadStage::Streaming => "download",
            DownloadStage::SwitchingMirror => "switch mirror",
            DownloadStage::Resuming => "resume",
        };
        let stage_id = match update.stage {
            DownloadStage::Racing => "racing",
            DownloadStage::Streaming => "streaming",
            DownloadStage::SwitchingMirror => "switching_mirror",
            DownloadStage::Resuming => "resuming",
        };

        let mirror = update.mirror_url.as_deref().unwrap_or(&self.primary_url);
        let mut message = match (update.total, update.speed_bps) {
            (Some(t), Some(bps)) => format!(
                "{stage_label} {} / {} - {}",
                fmt_bytes(update.downloaded),
                fmt_bytes(t),
                fmt_bps(bps)
            ),
            (Some(t), None) => format!(
                "{stage_label} {} / {}",
                fmt_bytes(update.downloaded),
                fmt_bytes(t)
            ),
            (None, Some(bps)) => format!(
                "{stage_label} {} - {}",
                fmt_bytes(update.downloaded),
                fmt_bps(bps)
            ),
            (None, None) => format!("{stage_label} {}", fmt_bytes(update.downloaded)),
        };
        if matches!(
            update.stage,
            DownloadStage::Racing | DownloadStage::SwitchingMirror
        ) {
            message.push_str(" @ ");
            message.push_str(mirror);
        }

        self.ctx
            .emit(ProgressKind::StepProgress {
                step: self.step,
                percent: pct,
                message,
                speed_bps: update.speed_bps,
                downloaded_bytes: Some(update.downloaded),
                total_bytes: update.total,
                download_stage: Some(stage_id.to_string()),
                docker_layers: None,
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn download_writes_file_and_returns_ok() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/payload"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello world".to_vec()))
            .mount(&server)
            .await;

        let helper = DownloadHelper::new().unwrap();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("payload.bin");
        let (ctx, _rx) = ActionCtx::new();

        let url = format!("{}/payload", server.uri());
        helper
            .download_to_file(&url, &dest, None, &ctx, 1)
            .await
            .unwrap();

        let contents = fs::read(&dest).await.unwrap();
        assert_eq!(contents, b"hello world");
    }

    #[tokio::test]
    async fn download_fails_on_non_2xx() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let helper = DownloadHelper::new().unwrap();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("x");
        let (ctx, _rx) = ActionCtx::new();

        let url = format!("{}/missing", server.uri());
        let err = helper
            .download_to_file(&url, &dest, None, &ctx, 1)
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::DownloadFailed { .. }));
    }

    #[tokio::test]
    async fn download_validates_sha256_and_deletes_on_mismatch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/payload"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
            .mount(&server)
            .await;

        let helper = DownloadHelper::new().unwrap();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("p.bin");
        let (ctx, _rx) = ActionCtx::new();

        let url = format!("{}/payload", server.uri());
        let err = helper
            .download_to_file(&url, &dest, Some("0000bad000sha"), &ctx, 1)
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::ChecksumMismatch { .. }));
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn download_passes_correct_sha256() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/payload"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
            .mount(&server)
            .await;

        let helper = DownloadHelper::new().unwrap();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("p.bin");
        let (ctx, _rx) = ActionCtx::new();

        let url = format!("{}/payload", server.uri());
        helper
            .download_to_file(
                &url,
                &dest,
                Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"),
                &ctx,
                1,
            )
            .await
            .unwrap();
        assert!(dest.exists());
    }

    #[tokio::test]
    async fn download_respects_cancel() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(vec![0u8; 1024 * 1024])
                    .set_delay(std::time::Duration::from_millis(500)),
            )
            .mount(&server)
            .await;

        let helper = DownloadHelper::new().unwrap();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("slow.bin");
        let (ctx, _rx) = ActionCtx::new();

        ctx.cancel();

        let url = format!("{}/slow", server.uri());
        let err = helper
            .download_to_file(&url, &dest, None, &ctx, 1)
            .await
            .unwrap_err();
        assert!(matches!(err, ActionError::Cancelled));
    }
}
