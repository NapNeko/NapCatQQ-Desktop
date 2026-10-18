//! 统一 HTTP 下载、镜像测速、并行分片、散列校验与进度上报。

pub mod chunked;
pub mod client;
pub mod download;
pub mod error;
pub mod mirror;
pub mod progress;
pub mod proxy;
pub mod proxy_constants {
    include!(concat!(env!("OUT_DIR"), "/proxy_constants.rs"));
}
pub mod race;
pub mod range;
pub mod retry;
mod segmented;
pub mod settings;
pub mod speed;
mod staging;
mod system_proxy;
pub mod verify;

pub use chunked::{
    ChunkedConfig, download_smart, download_url,
};
pub use client::{
    NetworkProvider, NetworkSnapshot, build_configured_client, install_network_provider,
    shared_client,
};
pub use download::{DownloadConfig, download_with_resume};
pub use error::NetworkError;
pub use mirror::{
    DEFAULT_MIRROR_PREFIXES, build_mirror_urls, download_mirror_catalog, mirror_urls_for_settings,
};
pub use progress::{DownloadProgressSink, DownloadStage, NoopProgressSink, ProgressUpdate};
pub use proxy::{ProxySigner, ReleaseAlias, is_proxy_configured, proxy_release_url, proxy_signer};
pub use race::{MirrorRaceConfig, download_with_mirror_race};
pub use retry::{RetryPolicy, retry_with_backoff};
pub use settings::validate_network_settings;
pub use speed::{LowSpeedConfig};
