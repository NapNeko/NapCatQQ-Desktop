//! DownloadHelper:HTTP 下载 + SHA256 校验 + 多镜像 race + 切片并行
//!
//! 单 URL 与多镜像路径共用 ncd-network 的自适应并发下载；单 URL 只探测
//! Range 能力，多镜像先测速选源。SHA256 在所有路径下载完成后校验。
//!
//! 设计:
//! - 进度桥接:实现一个 CtxSink,把 ncd_network::ProgressUpdate 翻成
//!   ProgressKind::StepProgress + speed_bps,emit 到 ActionCtx
//! - 单 URL 校验在暂存文件内完成，失败保留原目标并返回 ChecksumMismatch
//! - 取消:cancel token 来自 ctx,原子传递给 ncd_network

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_network::{
    ChunkedConfig, DownloadProgressSink, DownloadStage, MirrorRaceConfig, NetworkError,
    ProgressUpdate, download_smart, download_url,
};
use tokio::fs;

use crate::context::{ActionCtx, ProgressKind};
use crate::error::ActionError;

/// 下载辅助,保留 stateful 接口以兼容旧调用点;内部不再持有 reqwest::Client,
/// 走 ncd_network::shared_client() 共享连接池
pub struct DownloadHelper;

impl DownloadHelper {
    pub fn new() -> Result<Self, ActionError> {
        Ok(Self)
    }

    /// 兼容旧 API:单 URL 下载到本地,可选 SHA256
    ///
    /// 支持 Range 的大文件并发下载，小文件和不支持 Range 的端点自动单流。
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

        emit_step(
            ctx,
            step,
            0,
            format!("准备下载 · {}", source_label(url, url)),
            None,
        )
        .await;

        let sink: Arc<dyn DownloadProgressSink> =
            Arc::new(CtxSink::new(ctx.clone(), step, url.to_string()));

        match download_url(
            url,
            dest_path,
            sink,
            ctx.cancel_token(),
            ChunkedConfig {
                expected_sha256: expected_sha256.map(str::to_owned),
                ..Default::default()
            },
        )
        .await
        {
            Ok(_) => {}
            Err(NetworkError::Cancelled) => return Err(ActionError::Cancelled),
            Err(e) => return Err(map_network_err(url, e)),
        }

        emit_step(
            ctx,
            step,
            100,
            format!("下载完成 {}", file_size_label(dest_path).await),
            None,
        )
        .await;
        Ok(())
    }

    /// 多镜像下载：测速选源，断流时切镜像，大文件自动并发下载。
    ///
    /// mirrors:候选 URL 列表(一般用 ncd_network::build_mirror_urls(原始 URL) 生成)
    /// 第一个 URL 用作进度上报里的 "primary" 标识
    ///
    /// expected_sha256:Some 时下载完成后立即在 ncd-network 内部校验 sha256;
    /// mismatch 会切下家而不是直接报 ChecksumMismatch(堵代理"返完整长度
    /// 的垃圾字节"投毒洞,前 4 轮字节级防御都防不住)
    /// 所有镜像都失败才返 AllMirrorsFailed,None 跳过校验(兼容上游 release 还没 digest 的老仓库)
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
            format!("准备下载，正在比较 {} 个下载源", mirrors.len()),
            None,
        )
        .await;

        let sink: Arc<dyn DownloadProgressSink> =
            Arc::new(CtxSink::new(ctx.clone(), step, mirrors[0].clone()));

        let sha = expected_sha256.map(|s| s.to_string());
        // sha256 同时塞 chunked + race fallback,保证 race fallback 链路也能
        // 检出投毒(race 自己也会校验)
        let cfg = ChunkedConfig {
            expected_sha256: sha.clone(),
            race_cfg: MirrorRaceConfig {
                expected_sha256: sha,
                ..Default::default()
            },
            ..Default::default()
        };
        match download_smart(mirrors, dest_path, sink, ctx.cancel_token(), cfg).await {
            Ok(_) => {}
            Err(NetworkError::Cancelled) => return Err(ActionError::Cancelled),
            Err(e) => return Err(map_network_err(&mirrors[0], e)),
        }

        emit_step(
            ctx,
            step,
            100,
            format!("下载完成 {}", file_size_label(dest_path).await),
            None,
        )
        .await;
        Ok(())
    }

    /// 多镜像下载,但强制只走 race + 单流(不切片)给小文件 / 不支持 Range
    /// 的端点(部分镜像 reverse-proxy 会丢 Range header)专用
    ///
    /// sha256 校验语义同 [Self::download_with_mirrors]:mismatch 切下家
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
            format!("准备下载，正在比较 {} 个下载源", mirrors.len()),
            None,
        )
        .await;

        let sink: Arc<dyn DownloadProgressSink> =
            Arc::new(CtxSink::new(ctx.clone(), step, mirrors[0].clone()));

        let sha = expected_sha256.map(str::to_owned);
        let cfg = ChunkedConfig {
            parts: 1,
            expected_sha256: sha.clone(),
            race_cfg: MirrorRaceConfig {
                expected_sha256: sha,
                ..Default::default()
            },
            ..Default::default()
        };
        match download_smart(mirrors, dest_path, sink, ctx.cancel_token(), cfg).await {
            Ok(_) => {}
            Err(NetworkError::Cancelled) => return Err(ActionError::Cancelled),
            Err(e) => return Err(map_network_err(&mirrors[0], e)),
        }

        emit_step(
            ctx,
            step,
            100,
            format!("下载完成 {}", file_size_label(dest_path).await),
            None,
        )
        .await;
        Ok(())
    }
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

/// 只展示主机，避免进度消息暴露签名参数、认证信息或很长的资产路径。
fn source_label(url: &str, primary_url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else {
        return "下载源".into();
    };
    let Some(host) = parsed.host_str() else {
        return "下载源".into();
    };
    let is_mirror = url != primary_url
        || parsed.path().starts_with("/https://")
        || parsed.path().starts_with("/http://")
        || (host == "registry.npmmirror.com" && parsed.path().starts_with("/-/binary/node/"));
    format!("{} {host}", if is_mirror { "镜像" } else { "直连" })
}

/// 把 ncd_network::ProgressUpdate 翻成 ActionCtx::emit
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

        let readable_message = update
            .message
            .chars()
            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
            .then(|| update.message.trim());
        let stage_label = match update.stage {
            DownloadStage::Racing => readable_message.unwrap_or("正在测速选择下载源"),
            DownloadStage::Streaming => readable_message.unwrap_or("下载中"),
            DownloadStage::SwitchingMirror => {
                let reason = update.message.to_ascii_lowercase();
                if let Some(message) = readable_message {
                    message
                } else if reason.contains("slow")
                    || reason.contains("low-speed")
                    || reason.contains("低速")
                    || reason.contains("偏低")
                {
                    "下载速度持续偏低，正在切换镜像"
                } else {
                    "正在切换下载源"
                }
            }
            DownloadStage::Resuming => "断点续传中",
        };
        let stage_id = match update.stage {
            DownloadStage::Racing => "racing",
            DownloadStage::Streaming => "streaming",
            DownloadStage::SwitchingMirror => "switching_mirror",
            DownloadStage::Resuming => "resuming",
        };

        let mirror = update.mirror_url.as_deref().unwrap_or(&self.primary_url);
        let metrics = match (update.total, update.speed_bps) {
            (Some(t), Some(bps)) => format!(
                "{} / {} · {}",
                fmt_bytes(update.downloaded),
                fmt_bytes(t),
                fmt_bps(bps)
            ),
            (Some(t), None) => format!("{} / {}", fmt_bytes(update.downloaded), fmt_bytes(t)),
            (None, Some(bps)) => format!("{} · {}", fmt_bytes(update.downloaded), fmt_bps(bps)),
            (None, None) => fmt_bytes(update.downloaded),
        };
        let message = if update.stage == DownloadStage::Racing {
            stage_label.to_string()
        } else {
            let source = source_label(mirror, &self.primary_url);
            if update.stage == DownloadStage::SwitchingMirror {
                format!("{source} · {stage_label}")
            } else {
                format!("{source} · {stage_label} · {metrics}")
            }
        };

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
    async fn progress_exposes_selected_source_without_url_secrets() {
        let (ctx, mut rx) = ActionCtx::new();
        let primary = "https://github.com/foo/bar/releases/download/v1/archive.zip";
        let sink = CtxSink::new(ctx, 1, primary.into());
        sink.tick(ProgressUpdate {
            stage: DownloadStage::Streaming,
            downloaded: 1024,
            total: Some(4096),
            speed_bps: Some(2048),
            mirror_url: Some(
                "https://user:secret@gh-proxy.com/https://github.com/archive.zip?token=secret"
                    .into(),
            ),
            message: "streaming".into(),
        })
        .await;

        let event = rx.recv().await.unwrap();
        let ProgressKind::StepProgress {
            message,
            downloaded_bytes,
            speed_bps,
            ..
        } = event.kind
        else {
            panic!("expected download progress");
        };
        assert!(message.starts_with("镜像 gh-proxy.com · 下载中"));
        assert!(!message.contains("secret"));
        assert!(!message.contains("archive.zip"));
        assert_eq!(downloaded_bytes, Some(1024));
        assert_eq!(speed_bps, Some(2048));
        assert_eq!(source_label(primary, primary), "直连 github.com");
    }

    #[tokio::test]
    async fn low_speed_switch_reason_reaches_progress_message() {
        let (ctx, mut rx) = ActionCtx::new();
        let sink = CtxSink::new(ctx, 1, "https://github.com/archive.zip".into());
        sink.tick(ProgressUpdate {
            stage: DownloadStage::SwitchingMirror,
            downloaded: 0,
            total: None,
            speed_bps: None,
            mirror_url: Some("https://gh-proxy.com/https://github.com/archive.zip".into()),
            message: "持续低速（80.0 KB/s），自动更换下载源".into(),
        })
        .await;

        let event = rx.recv().await.unwrap();
        let ProgressKind::StepProgress {
            message,
            download_stage,
            ..
        } = event.kind
        else {
            panic!("expected download progress");
        };
        assert!(message.contains("持续低速（80.0 KB/s），自动更换下载源"));
        assert!(message.contains("镜像 gh-proxy.com"));
        assert_eq!(download_stage.as_deref(), Some("switching_mirror"));
    }

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
        let expected = "0".repeat(64);
        let err = helper
            .download_to_file(&url, &dest, Some(&expected), &ctx, 1)
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
    async fn download_checksum_mismatch_preserves_existing_destination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/payload"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"replacement".to_vec()))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let dest = dir.path().join("existing.bin");
        fs::write(&dest, b"original").await.unwrap();
        let (ctx, _rx) = ActionCtx::new();
        let expected = "0".repeat(64);
        let err = DownloadHelper::new()
            .unwrap()
            .download_to_file(
                &format!("{}/payload", server.uri()),
                &dest,
                Some(&expected),
                &ctx,
                1,
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ActionError::ChecksumMismatch { .. }));
        assert_eq!(fs::read(&dest).await.unwrap(), b"original");
    }

    #[tokio::test]
    async fn encoded_single_stream_preserves_existing_destination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/encoded"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Encoding", "gzip")
                    .set_body_bytes(b"encoded bytes".to_vec()),
            )
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let dest = dir.path().join("existing.bin");
        fs::write(&dest, b"original").await.unwrap();
        let (ctx, _rx) = ActionCtx::new();
        let err = DownloadHelper::new()
            .unwrap()
            .download_to_file(&format!("{}/encoded", server.uri()), &dest, None, &ctx, 1)
            .await
            .unwrap_err();

        assert!(matches!(err, ActionError::DownloadFailed { reason, .. }
            if reason.contains("Content-Encoding")));
        assert_eq!(fs::read(&dest).await.unwrap(), b"original");
    }

    #[tokio::test]
    async fn no_chunk_checksum_mismatch_preserves_existing_destination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/payload"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"replacement".to_vec()))
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let dest = dir.path().join("existing.bin");
        fs::write(&dest, b"original").await.unwrap();
        let (ctx, _rx) = ActionCtx::new();
        let expected = "0".repeat(64);
        let url = format!("{}/payload", server.uri());
        let err = DownloadHelper::new()
            .unwrap()
            .download_with_mirrors_no_chunk(
                std::slice::from_ref(&url),
                &dest,
                Some(&expected),
                &ctx,
                1,
            )
            .await
            .unwrap_err();

        assert!(
            matches!(err, ActionError::DownloadFailed { url: failed_url, reason }
            if failed_url == url && reason.starts_with("all mirrors failed:")
                && reason.contains("checksum mismatch"))
        );
        assert_eq!(fs::read(&dest).await.unwrap(), b"original");
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
