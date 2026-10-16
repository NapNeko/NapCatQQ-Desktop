//! 系统固定代理发现；Windows 分协议映射与环境变量合并，不执行 PAC。

use hyper_util::client::proxy::matcher::Matcher;

use crate::NetworkError;

pub(crate) struct SystemProxy {
    pub matcher: Matcher,
    pub bypass_local: bool,
}

#[cfg(not(windows))]
pub(crate) fn discover() -> Result<SystemProxy, NetworkError> {
    Ok(SystemProxy {
        matcher: Matcher::from_system(),
        bypass_local: false,
    })
}

#[cfg(windows)]
pub(crate) fn discover() -> Result<SystemProxy, NetworkError> {
    let env = EnvironmentProxy {
        http: read_env("HTTP_PROXY", "http_proxy")?,
        https: read_env("HTTPS_PROXY", "https_proxy")?,
        all: read_env("ALL_PROXY", "all_proxy")?,
        no: read_env("NO_PROXY", "no_proxy")?,
    };
    let covered = (nonempty(&env.http).is_some() || nonempty(&env.all).is_some())
        && (nonempty(&env.https).is_some() || nonempty(&env.all).is_some());
    let system = if covered { None } else { read_windows_proxy()? };
    merge(env, system)
}

#[cfg(any(windows, test))]
#[derive(Default)]
struct EnvironmentProxy {
    http: Option<String>,
    https: Option<String>,
    all: Option<String>,
    no: Option<String>,
}

#[cfg(any(windows, test))]
#[derive(Default)]
struct ProxyEndpoints {
    http: Option<String>,
    https: Option<String>,
    socks: Option<String>,
}

#[cfg(any(windows, test))]
struct WindowsProxy {
    endpoints: ProxyEndpoints,
    bypass: String,
}

#[cfg(any(windows, test))]
fn nonempty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(any(windows, test))]
fn merge(env: EnvironmentProxy, system: Option<WindowsProxy>) -> Result<SystemProxy, NetworkError> {
    let endpoints = system.as_ref().map(|system| &system.endpoints);
    let http = nonempty(&env.http)
        .or_else(|| nonempty(&env.all))
        .or_else(|| endpoints.and_then(|value| value.http.as_deref().or(value.socks.as_deref())));
    let https = nonempty(&env.https)
        .or_else(|| nonempty(&env.all))
        .or_else(|| endpoints.and_then(|value| value.https.as_deref().or(value.socks.as_deref())));
    let mut builder = Matcher::builder();
    if let Some(http) = http {
        builder = builder.http(normalize_proxy(http, "http")?);
    }
    if let Some(https) = https {
        builder = builder.https(normalize_proxy(https, "http")?);
    }
    let (bypass, bypass_local) = match env.no {
        Some(value) => (value, false),
        None => parse_bypass(system.as_ref().map_or("", |system| &system.bypass)),
    };
    Ok(SystemProxy {
        matcher: builder.no(bypass).build(),
        bypass_local,
    })
}

#[cfg(windows)]
fn read_env(upper: &str, lower: &str) -> Result<Option<String>, NetworkError> {
    for name in [upper, lower] {
        match std::env::var(name) {
            Ok(value) => return Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => {}
            Err(_) => return Err(configuration_error("代理环境变量编码无效")),
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn read_windows_proxy() -> Result<Option<WindowsProxy>, NetworkError> {
    use std::io::ErrorKind;
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};

    let key = match RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
    {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(configuration_error("无法读取 Windows 系统代理设置")),
    };
    match key.get_value::<u32, _>("ProxyEnable") {
        Ok(0) => return Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(configuration_error("无法读取 Windows 系统代理启用状态")),
        Ok(_) => {}
    }
    let server: String = key
        .get_value("ProxyServer")
        .map_err(|_| configuration_error("Windows 已启用固定代理，但代理地址无法读取"))?;
    let bypass: String = match key.get_value("ProxyOverride") {
        Ok(value) => value,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(_) => return Err(configuration_error("无法读取 Windows 系统代理排除列表")),
    };
    Ok(Some(WindowsProxy {
        endpoints: parse_proxy_server(&server)?,
        bypass,
    }))
}

#[cfg(any(windows, test))]
fn parse_proxy_server(raw: &str) -> Result<ProxyEndpoints, NetworkError> {
    let raw = raw.trim();
    if !raw.contains('=') {
        let proxy = normalize_proxy(raw, "http")?;
        return Ok(ProxyEndpoints {
            http: Some(proxy.clone()),
            https: Some(proxy),
            socks: None,
        });
    }
    let mut endpoints = ProxyEndpoints::default();
    for entry in raw
        .split(';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let (scheme, address) = entry
            .split_once('=')
            .ok_or_else(|| configuration_error("Windows 系统代理的分协议格式无效"))?;
        // Windows 的 Secure 栏表示目标 HTTPS，代理连接本身仍默认使用 HTTP。
        match scheme.trim().to_ascii_lowercase().as_str() {
            "http" => endpoints.http = Some(normalize_proxy(address, "http")?),
            "https" => endpoints.https = Some(normalize_proxy(address, "http")?),
            "socks" => endpoints.socks = Some(normalize_proxy(address, "socks4")?),
            "ftp" => {}
            _ => {
                return Err(configuration_error(
                    "Windows 系统代理包含无法识别的协议映射",
                ));
            }
        }
    }
    Ok(endpoints)
}

#[cfg(any(windows, test))]
fn normalize_proxy(raw: &str, default_scheme: &str) -> Result<String, NetworkError> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw
            .chars()
            .any(|value| value.is_whitespace() || value.is_control())
    {
        return Err(configuration_error("系统代理地址为空或包含无效字符"));
    }
    let address = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("{default_scheme}://{raw}")
    };
    let mut url =
        reqwest::Url::parse(&address).map_err(|_| configuration_error("系统代理地址格式无效"))?;
    let is_socks = matches!(url.scheme(), "socks4" | "socks4a" | "socks5" | "socks5h");
    if !matches!(url.scheme(), "http" | "https") && !is_socks
        || url.host_str().is_none()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port() == Some(0)
    {
        return Err(configuration_error(
            "系统代理地址必须使用 HTTP(S) 或 SOCKS，且不能包含路径或查询参数",
        ));
    }
    if is_socks && url.port().is_none() {
        url.set_port(Some(1080))
            .map_err(|_| configuration_error("系统 SOCKS 代理端口无效"))?;
    }
    Ok(url.to_string())
}

#[cfg(any(windows, test))]
fn parse_bypass(raw: &str) -> (String, bool) {
    let mut local = false;
    let domains = raw
        .split(';')
        .map(str::trim)
        .filter_map(|entry| {
            if entry.eq_ignore_ascii_case("<local>") {
                local = true;
                None
            } else if entry.is_empty() {
                None
            } else {
                Some(
                    ipv4_wildcard_cidr(entry)
                        .unwrap_or_else(|| entry.strip_prefix("*.").unwrap_or(entry).to_string()),
                )
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    (domains, local)
}

#[cfg(any(windows, test))]
fn ipv4_wildcard_cidr(entry: &str) -> Option<String> {
    let prefix = entry.strip_suffix(".*")?;
    let octets: Vec<_> = prefix.split('.').collect();
    if !(1..=3).contains(&octets.len()) {
        return None;
    }
    let mut address = [0u8; 4];
    for (index, octet) in octets.iter().enumerate() {
        if octet.is_empty() || !octet.bytes().all(|value| value.is_ascii_digit()) {
            return None;
        }
        address[index] = octet.parse().ok()?;
    }
    Some(format!(
        "{}/{}",
        std::net::Ipv4Addr::from(address),
        octets.len() * 8
    ))
}

#[cfg(any(windows, test))]
fn configuration_error(message: &str) -> NetworkError {
    NetworkError::InvalidArgument(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(raw: &str) -> WindowsProxy {
        WindowsProxy {
            endpoints: parse_proxy_server(raw).unwrap(),
            bypass: "*.internal;<local>".into(),
        }
    }

    fn selected(proxy: &SystemProxy, url: &str) -> Option<String> {
        proxy
            .matcher
            .intercept(&url.parse().unwrap())
            .map(|value| value.uri().to_string())
    }

    #[test]
    fn unified_and_protocol_specific_windows_addresses_are_supported() {
        let unified = merge(EnvironmentProxy::default(), Some(system("127.0.0.1:7890"))).unwrap();
        assert_eq!(
            selected(&unified, "http://example.com"),
            Some("http://127.0.0.1:7890/".into())
        );
        assert_eq!(
            selected(&unified, "https://example.com"),
            Some("http://127.0.0.1:7890/".into())
        );
        let split = merge(
            EnvironmentProxy::default(),
            Some(system(
                "http=plain:80;https=secure:8080;socks=socks-host:1080;ftp=ignored:21",
            )),
        )
        .unwrap();
        assert_eq!(
            selected(&split, "http://example.com"),
            Some("http://plain/".into())
        );
        assert_eq!(
            selected(&split, "https://example.com"),
            Some("http://secure:8080/".into())
        );
        assert!(split.bypass_local);
        assert!(selected(&split, "https://host.internal").is_none());
    }

    #[test]
    fn socks_fills_only_missing_protocol_and_preserves_explicit_scheme() {
        let socks = merge(
            EnvironmentProxy::default(),
            Some(system("http=web:8080;socks=other:1080")),
        )
        .unwrap();
        assert_eq!(
            selected(&socks, "https://example.com"),
            Some("socks4://other:1080/".into())
        );
        assert_eq!(
            selected(&socks, "http://example.com"),
            Some("http://web:8080/".into())
        );
        let explicit = parse_proxy_server("socks=socks5h://other:1080").unwrap();
        assert_eq!(explicit.socks.as_deref(), Some("socks5h://other:1080"));
    }

    #[test]
    fn environment_specific_then_all_precede_windows_routes() {
        let env = EnvironmentProxy {
            http: Some("http://env-http:8080".into()),
            all: Some("socks5h://env-all:1080".into()),
            no: Some("".into()),
            ..Default::default()
        };
        let merged = merge(env, Some(system("http=sys-http:80;https=sys-https:80"))).unwrap();
        assert_eq!(
            selected(&merged, "http://example.com"),
            Some("http://env-http:8080/".into())
        );
        assert_eq!(
            selected(&merged, "https://example.com"),
            Some("socks5h://env-all:1080/".into())
        );
        assert!(!merged.bypass_local);
        assert!(selected(&merged, "https://host.internal").is_some());
    }

    #[test]
    fn invalid_active_proxy_does_not_silently_become_direct_or_expose_credentials() {
        for raw in [
            "",
            "http=",
            "http=ftp://host:21",
            "http=host:80;broken",
            "htp=host:80",
            "http=http://user:secret@host:80/path",
        ] {
            let error = parse_proxy_server(raw)
                .err()
                .expect("invalid mapping must fail");
            assert!(!error.to_string().contains("secret"));
        }
        assert!(
            merge(
                EnvironmentProxy {
                    all: Some("broken/path".into()),
                    ..Default::default()
                },
                None
            )
            .is_err()
        );
    }

    #[test]
    fn windows_ipv4_wildcards_keep_private_networks_direct() {
        let mut config = system("proxy.example:7890");
        config.bypass = "10.*;192.168.*;172.16.20.*;*.internal;<local>".into();
        let merged = merge(EnvironmentProxy::default(), Some(config)).unwrap();
        for url in [
            "https://10.42.1.5/",
            "http://192.168.1.2/",
            "https://172.16.20.255/",
            "https://host.internal/",
        ] {
            assert!(selected(&merged, url).is_none(), "{url}");
        }
        for url in [
            "https://172.16.21.1/",
            "https://192.169.1.2/",
            "https://8.8.8.8/",
        ] {
            assert!(selected(&merged, url).is_some(), "{url}");
        }
        assert!(merged.bypass_local);
    }

    #[test]
    fn ipv4_wildcards_require_one_to_three_valid_octets() {
        assert_eq!(ipv4_wildcard_cidr("10.*").as_deref(), Some("10.0.0.0/8"));
        assert_eq!(
            ipv4_wildcard_cidr("192.168.*").as_deref(),
            Some("192.168.0.0/16")
        );
        assert_eq!(
            ipv4_wildcard_cidr("172.16.20.*").as_deref(),
            Some("172.16.20.0/24")
        );
        for entry in [
            "*",
            "*.example.com",
            "256.*",
            "192..*",
            "1.2.3.4.*",
            "1.*.2",
            "+1.*",
        ] {
            assert!(ipv4_wildcard_cidr(entry).is_none(), "{entry}");
        }
    }
}
