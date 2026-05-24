//! `DownloadHelper`:HTTP 下载 + SHA256 校验工具。
//!
//! fallback 链("远端不通就走本地下载 + SFTP 上传")在 ncd-deploy 实装,
//! 本 helper 只做基础原语:从 URL 下载到本地路径 + 校验 SHA256 + 进度上报。
//!
//! 设计原则:
//! - 复用 `reqwest`(已在 workspace 共享)
//! - 流式下载,不一次性 buffer 全部内存
//! - SHA256 校验失败时自动删除已下载文件,防止下次复用损坏的副本

use std::path::Path;

use futures_util::StreamExt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::context::{ActionCtx, ProgressKind};
use crate::error::ActionError;

/// 下载辅助。
pub struct DownloadHelper {
    client: Client,
}

impl DownloadHelper {
    /// 用默认 reqwest 客户端创建。
    pub fn new() -> Result<Self, ActionError> {
        let client = Client::builder()
            .user_agent("ncd-component/0.1")
            .gzip(true)
            .build()
            .map_err(|e| ActionError::DownloadFailed {
                url: "<client init>".to_string(),
                reason: format!("reqwest client build: {e}"),
            })?;
        Ok(Self { client })
    }

    /// 注入自定义 reqwest::Client(测试用)。
    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// 下载 url 到本地 dest_path,带可选 SHA256 校验和进度上报。
    ///
    /// `step`:进度步骤号(用于 ProgressKind::StepProgress)。
    /// `expected_sha256`:`None` 表示跳过校验。
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

        ctx.emit(ProgressKind::StepProgress {
            step,
            percent: 0,
            message: format!("download {url}"),
        })
        .await;

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ActionError::DownloadFailed {
                url: url.to_string(),
                reason: format!("http get: {e}"),
            })?;

        if !resp.status().is_success() {
            return Err(ActionError::DownloadFailed {
                url: url.to_string(),
                reason: format!("http status: {}", resp.status()),
            });
        }

        let total = resp.content_length();

        if let Some(parent) = dest_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    ActionError::install_step("mkdir_parent", format!("{e}"))
                })?;
            }
        }

        let mut file = fs::File::create(dest_path).await.map_err(|e| {
            ActionError::install_step("create_file", format!("{e}"))
        })?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;

        let mut stream = resp.bytes_stream();
        let cancel_token = ctx.cancel_token();
        loop {
            // 同时等"下一块字节"和"取消信号"。任一就绪就 wake，避免连接 stall
            // 时取消按钮没反应（取消只检查每个 chunk 边界 → 服务器不发字节 →
            // stream.next() 永远挂着 → cancel token 永远不被观测）。
            let chunk_res = tokio::select! {
                biased;
                _ = cancel_token.cancelled() => {
                    drop(file);
                    let _ = fs::remove_file(dest_path).await;
                    return Err(ActionError::Cancelled);
                }
                next = stream.next() => match next {
                    Some(c) => c,
                    None => break,
                },
            };
            let chunk = chunk_res.map_err(|e| ActionError::DownloadFailed {
                url: url.to_string(),
                reason: format!("read chunk: {e}"),
            })?;
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| {
                ActionError::install_step("write_chunk", format!("{e}"))
            })?;
            downloaded += chunk.len() as u64;

            // 进度上报(每 1 MB 一次,避免 channel 风暴)
            if let Some(t) = total {
                if t > 0 {
                    let pct = ((downloaded as f64 / t as f64) * 100.0) as u8;
                    if downloaded % (1024 * 1024) < chunk.len() as u64 {
                        ctx.emit(ProgressKind::StepProgress {
                            step,
                            percent: pct.min(99),
                            message: format!(
                                "download {} / {}",
                                fmt_bytes(downloaded),
                                fmt_bytes(t)
                            ),
                        })
                        .await;
                    }
                }
            }
        }

        file.flush()
            .await
            .map_err(|e| ActionError::install_step("flush", format!("{e}")))?;
        drop(file);

        let actual_hash = hex::encode(hasher.finalize());

        if let Some(expected) = expected_sha256 {
            if !expected.eq_ignore_ascii_case(&actual_hash) {
                let _ = fs::remove_file(dest_path).await;
                return Err(ActionError::ChecksumMismatch {
                    expected: expected.to_string(),
                    actual: actual_hash,
                });
            }
        }

        ctx.emit(ProgressKind::StepProgress {
            step,
            percent: 100,
            message: format!("downloaded {}", fmt_bytes(downloaded)),
        })
        .await;
        Ok(())
    }
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
        let err = helper.download_to_file(&url, &dest, None, &ctx, 1).await.unwrap_err();
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
        // 文件应被删除
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn download_passes_correct_sha256() {
        // sha256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
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
        // 用一个 delay 较长的响应,够我们取消
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

        // 立即取消
        ctx.cancel();

        let url = format!("{}/slow", server.uri());
        let err = helper.download_to_file(&url, &dest, None, &ctx, 1).await.unwrap_err();
        assert!(matches!(err, ActionError::Cancelled));
    }
}
