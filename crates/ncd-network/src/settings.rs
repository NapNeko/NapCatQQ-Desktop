//! 网络设置校验；错误消息不包含代理地址或认证内容。

use std::net::IpAddr;

use ncd_domain::{DownloadSourceMode, NetworkSettings, ProxyMode};
use reqwest::Url;

use crate::NetworkError;

pub fn validate_network_settings(settings: &NetworkSettings) -> Result<(), NetworkError> {
    let proxy_url = settings.proxy_url.trim();
    if settings.proxy_mode == ProxyMode::Manual || !proxy_url.is_empty() {
        validate_proxy_url(proxy_url)?;
    }
    if settings.proxy_username.len() > 255
        || settings
            .proxy_username
            .chars()
            .any(|c| c.is_control() || c == ':')
    {
        return invalid("代理用户名不能包含控制字符或冒号，且不能超过 255 字节");
    }
    validate_bypass(&settings.proxy_bypass)?;
    for (mode, raw, label) in [
        (
            settings.github_download_mode,
            settings.github_mirror.as_str(),
            "GitHub 镜像地址",
        ),
        (
            settings.node_download_mode,
            settings.node_mirror.as_str(),
            "Node.js 镜像地址",
        ),
    ] {
        if mode == DownloadSourceMode::Mirror || !raw.trim().is_empty() {
            validate_mirror_url(raw).map_err(|_| {
                NetworkError::InvalidArgument(format!(
                    "{label}必须是无用户名、密码、查询参数和片段的 HTTP(S) 地址"
                ))
            })?;
        }
    }
    Ok(())
}

pub(crate) fn validate_proxy_credentials(
    settings: &NetworkSettings,
    password: &str,
) -> Result<(), NetworkError> {
    if settings.proxy_mode == ProxyMode::Manual {
        if settings.proxy_username.is_empty() && !password.is_empty() {
            return invalid("使用代理密码时请同时填写用户名");
        }
        let is_socks = Url::parse(settings.proxy_url.trim())
            .is_ok_and(|url| matches!(url.scheme(), "socks5" | "socks5h"));
        if is_socks && password.len() > 255 {
            return invalid("SOCKS5 代理密码不能超过 255 字节");
        }
    }
    Ok(())
}

fn validate_proxy_url(raw: &str) -> Result<(), NetworkError> {
    let message = "代理地址须包含协议和主机，支持 HTTP、HTTPS、SOCKS5、SOCKS5H；SOCKS5 须填写端口，账号密码请单独填写";
    if raw.is_empty() || raw.len() > 2048 || raw.chars().any(char::is_whitespace) {
        return invalid(message);
    }
    let parsed = Url::parse(raw).map_err(|_| NetworkError::InvalidArgument(message.into()))?;
    if !matches!(parsed.scheme(), "http" | "https" | "socks5" | "socks5h")
        || parsed.host_str().is_none()
        || !parsed.port_or_known_default().is_some_and(|port| port > 0)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return invalid(message);
    }
    Ok(())
}

pub(crate) fn validate_mirror_url(raw: &str) -> Result<Url, NetworkError> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 2048 || raw.chars().any(char::is_whitespace) {
        return invalid("无效的镜像地址");
    }
    let url =
        Url::parse(raw).map_err(|_| NetworkError::InvalidArgument("无效的镜像地址".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return invalid("无效的镜像地址");
    }
    Ok(url)
}

fn validate_bypass(raw: &str) -> Result<(), NetworkError> {
    if raw.len() > 4096 {
        return invalid("代理排除列表不能超过 4096 字节");
    }
    for entry in raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if entry == "*" || entry.parse::<IpAddr>().is_ok() {
            continue;
        }
        if let Some((address, prefix)) = entry.split_once('/') {
            let valid = address
                .parse::<IpAddr>()
                .ok()
                .zip(prefix.parse::<u8>().ok())
                .is_some_and(|(address, prefix)| {
                    prefix <= if address.is_ipv4() { 32 } else { 128 }
                });
            if valid {
                continue;
            }
            return invalid("代理排除列表的 CIDR 地址无效");
        }
        if entry
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || ":/@?#*\\".contains(c))
        {
            return invalid("代理排除列表请用逗号分隔域名、IP 或 CIDR，不要填写 URL 或端口");
        }
        let domain = entry.trim_start_matches('.');
        if domain.is_empty() || Url::parse(&format!("http://{domain}/")).is_err() {
            return invalid("代理排除列表包含无效的域名");
        }
    }
    Ok(())
}

fn invalid<T>(message: &str) -> Result<T, NetworkError> {
    Err(NetworkError::InvalidArgument(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_url_requires_clean_authority_and_known_port() {
        for url in [
            "http://localhost:7890",
            "https://proxy.example:443/",
            "socks5://[::1]:1080",
            "socks5h://proxy.example:1080",
            "http://proxy.example:80",
            "https://proxy.example",
        ] {
            assert!(validate_proxy_url(url).is_ok(), "{url}");
        }
        for url in [
            "",
            "localhost:7890",
            "socks5://localhost",
            "http://localhost:0",
            "file:///proxy",
            "http://user:secret@proxy.example:80",
            "http://proxy.example:80/path",
            "http://proxy.example:80?token=secret",
            "http://proxy.example:80#secret",
            "http://localhost:65536",
            "http://local host:7890",
        ] {
            let error = validate_proxy_url(url).unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
    }

    #[test]
    fn inactive_routes_can_be_empty_but_selected_routes_are_required() {
        let mut settings = NetworkSettings {
            proxy_mode: ProxyMode::Direct,
            github_mirror: String::new(),
            node_mirror: String::new(),
            ..Default::default()
        };
        assert!(validate_network_settings(&settings).is_ok());
        settings.github_download_mode = DownloadSourceMode::Mirror;
        assert!(validate_network_settings(&settings).is_err());
        settings.github_download_mode = DownloadSourceMode::Auto;
        settings.proxy_mode = ProxyMode::Manual;
        assert!(validate_network_settings(&settings).is_err());
    }

    #[test]
    fn bypass_accepts_no_proxy_syntax_and_rejects_ambiguous_urls() {
        assert!(validate_bypass("localhost,.example.com,10.0.0.0/8,::1,2001:db8::/32,*").is_ok());
        for value in [
            "https://example.com",
            "host:8080",
            "*.example.com",
            "10.0.0.1/33",
            "::1/129",
            "a b",
        ] {
            assert!(validate_bypass(value).is_err(), "{value}");
        }
    }

    #[test]
    fn inactive_urls_cannot_retain_embedded_credentials() {
        let mut settings = NetworkSettings {
            proxy_mode: ProxyMode::Direct,
            ..Default::default()
        };
        settings.proxy_url = "http://user:secret@proxy.example:7890".into();
        assert!(validate_network_settings(&settings).is_err());
        settings.proxy_url.clear();
        settings.github_mirror = "https://mirror.example/?token=secret".into();
        assert!(validate_network_settings(&settings).is_err());
    }
}
