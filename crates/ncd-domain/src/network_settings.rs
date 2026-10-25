//! 桌面端网络与公共下载线路设置。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ProxyMode {
    #[default]
    System,
    Direct,
    Manual,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DownloadSourceMode {
    #[default]
    Auto,
    Direct,
    Mirror,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NetworkSettings {
    pub proxy_mode: ProxyMode,
    pub proxy_url: String,
    pub proxy_username: String,
    pub proxy_bypass: String,
    pub github_download_mode: DownloadSourceMode,
    pub github_mirror: String,
    pub node_download_mode: DownloadSourceMode,
    pub node_mirror: String,
    pub pypi_index: String,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            proxy_mode: ProxyMode::System,
            proxy_url: String::new(),
            proxy_username: String::new(),
            proxy_bypass: "localhost,127.0.0.1,::1".to_string(),
            github_download_mode: DownloadSourceMode::Auto,
            github_mirror: "https://gh-proxy.com".to_string(),
            node_download_mode: DownloadSourceMode::Auto,
            node_mirror: "https://registry.npmmirror.com/-/binary/node".to_string(),
            pypi_index: String::new(),
        }
    }
}

impl NetworkSettings {
    pub fn normalize(&mut self) {
        self.proxy_url = self.proxy_url.trim().to_string();
        self.proxy_username = self.proxy_username.trim().to_string();
        self.proxy_bypass = self.proxy_bypass.trim().to_string();
        self.github_mirror = self.github_mirror.trim().to_string();
        self.node_mirror = self.node_mirror.trim().to_string();
        self.pypi_index = self.pypi_index.trim().to_string();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NetworkProbeResult {
    pub target: String,
    pub url: String,
    pub elapsed_ms: u32,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DownloadMirror {
    pub id: String,
    pub label: String,
    pub url: String,
    pub kind: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_keep_automatic_routes_and_system_proxy() {
        let settings: NetworkSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, NetworkSettings::default());
        let partial: NetworkSettings = serde_json::from_str(r#"{"proxyMode":"direct"}"#).unwrap();
        assert_eq!(partial.proxy_mode, ProxyMode::Direct);
        assert_eq!(partial.github_download_mode, DownloadSourceMode::Auto);
        assert_eq!(partial.node_download_mode, DownloadSourceMode::Auto);
        assert_eq!(partial.proxy_bypass, "localhost,127.0.0.1,::1");
        assert!(partial.pypi_index.is_empty());
    }

    #[test]
    fn manual_proxy_and_selected_sources_round_trip() {
        let canonical = r#"{"proxyMode":"manual","proxyUrl":"socks5h://127.0.0.1:1080","proxyUsername":"alice","proxyBypass":"localhost,192.168.0.0/16","githubDownloadMode":"mirror","githubMirror":"https://mirror.example","nodeDownloadMode":"direct","nodeMirror":"https://node.example/dist","pypiIndex":"https://pypi.org/simple"}"#;
        let settings: NetworkSettings = serde_json::from_str(canonical).unwrap();
        assert_eq!(settings.proxy_mode, ProxyMode::Manual);
        assert_eq!(settings.github_download_mode, DownloadSourceMode::Mirror);
        assert_eq!(settings.node_download_mode, DownloadSourceMode::Direct);
        assert_eq!(settings.pypi_index, "https://pypi.org/simple");
        assert_eq!(serde_json::to_string(&settings).unwrap(), canonical);
    }

    #[test]
    fn normalization_trims_transport_fields() {
        let mut settings = NetworkSettings {
            proxy_url: "  http://127.0.0.1:7890  ".into(),
            proxy_username: " alice ".into(),
            proxy_bypass: " localhost,127.0.0.1 ".into(),
            github_mirror: " https://mirror.example/ ".into(),
            node_mirror: " https://node.example/dist/ ".into(),
            pypi_index: " https://pypi.example/simple/ ".into(),
            ..NetworkSettings::default()
        };
        settings.normalize();
        assert_eq!(settings.proxy_url, "http://127.0.0.1:7890");
        assert_eq!(settings.proxy_username, "alice");
        assert_eq!(settings.proxy_bypass, "localhost,127.0.0.1");
        assert_eq!(settings.github_mirror, "https://mirror.example/");
        assert_eq!(settings.node_mirror, "https://node.example/dist/");
        assert_eq!(settings.pypi_index, "https://pypi.example/simple/");
    }
}
