//! ncd-network:NapCatQQ-Desktop 的统一 HTTP 下载层
//!
//! 集中三件事:
//! 1. 全工程共享 reqwest::Client(连接池 + TLS 会话复用)
//! 2. 大文件下载流水线:单镜像续传 + idle timeout → mirror race(Top-2 阶梯
//!    加码 + first-chunk 判赢)→ 切片并行(≥16MB 文件切 4 片并行下载)
//! 3. 进度上报:通过 progress::DownloadProgressSink trait 把字节,瞬时速度,
//!    阶段信息透出给调用层,避免与 ncd-component 形成强耦合
//!
//! 不做的事:
//! - WebUI 客户端(127.0.0.1,不需要 mirror / race / 续传)
//! - 解压 / 校验 SHA256(解压在 ncd-host,校验在 caller)
//!
//! GitHub API 中转代理 / HMAC 签名:见 proxy 模块

pub mod chunked;
pub mod client;
pub mod download;
pub mod error;
pub mod mirror;
pub mod progress;
pub mod proxy;
pub mod proxy_constants;
pub mod race;
pub mod range;
pub mod retry;
pub mod speed;
pub mod verify;

pub use chunked::{CHUNKED_THRESHOLD, ChunkedConfig, DEFAULT_CHUNK_PARTS, download_smart};
pub use client::shared_client;
pub use download::{AggregatedProgress, DownloadConfig, download_with_resume};
pub use error::NetworkError;
pub use mirror::{DEFAULT_MIRROR_PREFIXES, build_mirror_urls};
pub use progress::{DownloadProgressSink, DownloadStage, NoopProgressSink, ProgressUpdate};
pub use proxy::{ProxySigner, ReleaseAlias, is_proxy_configured, proxy_release_url, proxy_signer};
pub use race::{MirrorRaceConfig, download_with_mirror_race};
pub use retry::{RetryPolicy, is_retryable, retry_with_backoff};
pub use speed::SpeedSampler;
