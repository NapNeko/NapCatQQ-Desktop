//! 本机走 ncd-network；远端走 Host::download_url。不给 ncd-host 加 HTTP 依赖。

use ncd_host::{Host, HostPath, Locality};
use ncd_network::shared_client;
use ncd_traits::AppFrameworkError;

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
            let resp = shared_client()
                .get(url)
                .send()
                .await
                .map_err(|e| AppFrameworkError::Integration(format!("下载失败: {e}")))?;
            if !resp.status().is_success() {
                return Err(AppFrameworkError::Integration(format!(
                    "下载失败: HTTP {}",
                    resp.status()
                )));
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| AppFrameworkError::Integration(format!("读取下载内容失败: {e}")))?;
            host.write_file(dest, &bytes)
                .await
                .map_err(|e| AppFrameworkError::Host(e.to_string()))
        }
        Locality::Remote => host
            .download_url(url, dest)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string())),
    }
}
