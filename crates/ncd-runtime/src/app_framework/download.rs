//! 本机走 ncd-network；远端走 Host::download_url。不给 ncd-host 加 HTTP 依赖。

use std::path::PathBuf;
use std::sync::Arc;

use ncd_host::{Host, HostPath, Locality};
use ncd_network::{ChunkedConfig, NoopProgressSink, build_mirror_urls, download_smart};
use ncd_traits::AppFrameworkError;
use tokio_util::sync::CancellationToken;

pub async fn download_url_to_host(
    host: &dyn Host,
    url: &str,
    dest: &HostPath,
) -> Result<(), AppFrameworkError> {
    if let Some(parent) = dest.parent() {
        host.create_dir_all(&parent)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    }
    match host.locality() {
        Locality::Local => {
            let mirrors = build_mirror_urls(url, None);
            let local_dest = PathBuf::from(dest.render_for(host.os()));
            download_smart(
                &mirrors,
                &local_dest,
                Arc::new(NoopProgressSink),
                CancellationToken::new(),
                ChunkedConfig::default(),
            )
            .await
            .map(|_| ())
            .map_err(|e| match e {
                ncd_network::NetworkError::Io(e) => AppFrameworkError::Host(e.to_string()),
                other => AppFrameworkError::Integration(format!("下载失败: {other}")),
            })
        }
        Locality::Remote => host
            .download_url(url, dest)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string())),
    }
}
