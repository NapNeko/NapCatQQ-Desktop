//! Desktop 网络设置的只读目录与草稿连通性测试。

use ncd_domain::{DownloadMirror, NetworkProbeResult, NetworkSettings};

#[tauri::command]
pub fn list_download_mirrors() -> Vec<DownloadMirror> {
    ncd_network::mirror::download_mirror_catalog()
}

#[tauri::command]
pub async fn test_network_settings(
    settings: NetworkSettings,
    proxy_password: String,
) -> Result<Vec<NetworkProbeResult>, String> {
    ncd_runtime::desktop::network::probe_network_settings(&settings, &proxy_password).await
}
