//! 可热更新的外网客户端；可变配置由 runtime 提供，每次下载持有自己的快照。

use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use base64::Engine;
use hyper_util::client::proxy::matcher::Matcher;
use ncd_domain::{NetworkSettings, ProxyMode};
use reqwest::{Client, NoProxy, Proxy, Url};

use crate::error::NetworkError;
pub use crate::settings::validate_network_settings;

const DEFAULT_USER_AGENT: &str = concat!(
    "NapCatQQ-Desktop/",
    env!("CARGO_PKG_VERSION"),
    " (ncd-network)"
);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const LOOPBACK_BYPASS: &str = "localhost,127.0.0.0/8,::1";

static PROVIDER: OnceLock<Arc<dyn NetworkProvider>> = OnceLock::new();
static DEFAULT_CLIENT: OnceLock<Client> = OnceLock::new();

tokio::task_local! {
    static DOWNLOAD_SNAPSHOT: NetworkSnapshot;
}

#[derive(Clone)]
pub struct NetworkSnapshot {
    pub client: Client,
    pub settings: NetworkSettings,
}

pub trait NetworkProvider: Send + Sync {
    fn snapshot(&self) -> NetworkSnapshot;
}

pub fn install_network_provider(provider: Arc<dyn NetworkProvider>) -> Result<(), NetworkError> {
    PROVIDER.set(provider).map_err(|_| {
        NetworkError::InvalidArgument("network provider has already been installed".into())
    })
}

#[allow(clippy::expect_used, reason = "默认 HTTP 栈无法构造时无法继续使用网络")]
pub fn network_snapshot() -> NetworkSnapshot {
    if let Ok(snapshot) = DOWNLOAD_SNAPSHOT.try_with(Clone::clone) {
        return snapshot;
    }
    if let Some(provider) = PROVIDER.get() {
        return provider.snapshot();
    }
    NetworkSnapshot {
        client: DEFAULT_CLIENT
            .get_or_init(|| build_default_client().expect("ncd-network: HTTP 客户端构造失败"))
            .clone(),
        settings: NetworkSettings::default(),
    }
}

/// Client 克隆复用连接池；设置变更只影响之后取得的客户端。
pub fn shared_client() -> Client {
    network_snapshot().client
}

pub(crate) async fn with_network_snapshot<F: Future>(future: F) -> F::Output {
    with_snapshot(network_snapshot(), future).await
}

pub(crate) async fn with_snapshot<F: Future>(snapshot: NetworkSnapshot, future: F) -> F::Output {
    DOWNLOAD_SNAPSHOT.scope(snapshot, future).await
}

pub fn build_default_client() -> Result<Client, NetworkError> {
    build_configured_client(&NetworkSettings::default(), "")
}

pub fn build_configured_client(
    settings: &NetworkSettings,
    password: &str,
) -> Result<Client, NetworkError> {
    crate::settings::validate_network_settings(settings)?;
    crate::settings::validate_proxy_credentials(settings, password)?;
    let mut builder = Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_idle_timeout(POOL_IDLE_TIMEOUT)
        .gzip(true)
        .no_proxy();
    let bypass = Some(
        NoProxy::from_string(&format!(
            "{LOOPBACK_BYPASS},{}",
            settings.proxy_bypass.trim()
        ))
        .unwrap_or_default(),
    );
    match settings.proxy_mode {
        ProxyMode::Direct => {}
        ProxyMode::System => {
            let system = crate::system_proxy::discover()?;
            builder =
                builder.proxy(system_proxy(system.matcher, system.bypass_local).no_proxy(bypass));
        }
        ProxyMode::Manual => {
            let mut proxy = Proxy::all(settings.proxy_url.trim())
                .map_err(|_| NetworkError::InvalidArgument("代理地址无法使用".into()))?;
            if !settings.proxy_username.is_empty() {
                proxy = proxy.basic_auth(&settings.proxy_username, password);
            }
            builder = builder.proxy(proxy.no_proxy(bypass));
        }
    }
    builder
        .build()
        .map_err(|_| NetworkError::Http("无法创建网络客户端，请检查代理配置与系统证书".into()))
}

fn system_proxy(matcher: Matcher, bypass_local: bool) -> Proxy {
    Proxy::custom(move |destination| {
        if bypass_local && is_local_hostname(destination) {
            return None;
        }
        let uri = destination.as_str().parse().ok()?;
        let matched = matcher.intercept(&uri)?;
        let mut proxy = Url::parse(&matched.uri().to_string()).ok()?;
        if let Some((username, password)) = matched.raw_auth() {
            proxy.set_username(username).ok()?;
            proxy.set_password(Some(password)).ok()?;
        } else if let Some(header) = matched.basic_auth() {
            let encoded = header.to_str().ok()?.strip_prefix("Basic ")?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()?;
            let credentials = std::str::from_utf8(&decoded).ok()?;
            let (username, password) = credentials.split_once(':')?;
            proxy.set_username(username).ok()?;
            proxy.set_password(Some(password)).ok()?;
        }
        Some(proxy)
    })
}

fn is_local_hostname(destination: &Url) -> bool {
    destination.host_str().is_some_and(|host| {
        let host = host.trim_matches(['[', ']']);
        !host.contains('.') && host.parse::<std::net::IpAddr>().is_err()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn build_default_client_succeeds() {
        build_default_client().expect("默认 client 应当构造成功");
    }

    #[test]
    fn windows_local_bypass_only_matches_single_label_hostnames() {
        for url in ["http://localhost/", "http://intranet:8080/"] {
            assert!(is_local_hostname(&Url::parse(url).unwrap()), "{url}");
        }
        for url in [
            "https://[2606:4700:4700::1111]/",
            "http://[::1]/",
            "http://[::ffff:192.0.2.1]/",
            "http://192.0.2.1/",
            "https://www.example.com/",
        ] {
            assert!(!is_local_hostname(&Url::parse(url).unwrap()), "{url}");
        }
    }

    #[tokio::test]
    async fn manual_proxy_sends_auth_but_loopback_stays_direct() {
        let proxy = MockServer::start().await;
        let origin = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("proxy-authorization", "Basic dXNlcjpwYXNz"))
            .respond_with(ResponseTemplate::new(200).set_body_string("proxy"))
            .expect(1)
            .mount(&proxy)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("direct"))
            .expect(1)
            .mount(&origin)
            .await;
        let settings = NetworkSettings {
            proxy_mode: ProxyMode::Manual,
            proxy_url: proxy.uri(),
            proxy_username: "user".into(),
            proxy_bypass: String::new(),
            ..Default::default()
        };
        let client = build_configured_client(&settings, "pass").unwrap();
        let response = client
            .get("http://download.invalid/file")
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "proxy");
        let response = client.get(origin.uri()).send().await.unwrap();
        assert_eq!(response.text().await.unwrap(), "direct");
        let requests = origin.received_requests().await.unwrap();
        assert!(!requests[0].headers.contains_key("proxy-authorization"));
    }

    #[tokio::test]
    async fn system_matcher_preserves_proxy_credentials() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("proxy-authorization", "Basic dXNlcjpwOmFzcw=="))
            .respond_with(ResponseTemplate::new(200).set_body_string("system"))
            .expect(1)
            .mount(&server)
            .await;
        let proxy_url = server.uri().replacen("http://", "http://user:p%3Aass@", 1);
        let matcher = Matcher::builder().all(proxy_url).build();
        let client = Client::builder()
            .no_proxy()
            .proxy(system_proxy(matcher, false))
            .build()
            .unwrap();
        let response = client
            .get("http://download.invalid/file")
            .send()
            .await
            .unwrap();
        assert_eq!(response.text().await.unwrap(), "system");
    }

    #[tokio::test]
    async fn direct_mode_ignores_retained_manual_proxy() {
        let origin = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&origin)
            .await;
        let settings = NetworkSettings {
            proxy_mode: ProxyMode::Direct,
            proxy_url: "http://127.0.0.1:1".into(),
            ..Default::default()
        };
        let client = build_configured_client(&settings, "").unwrap();
        assert!(
            client
                .get(origin.uri())
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
        );
    }

    #[tokio::test]
    async fn snapshot_scope_retains_route_and_settings() {
        let settings = NetworkSettings {
            proxy_mode: ProxyMode::Direct,
            ..Default::default()
        };
        let client = build_configured_client(&settings, "").unwrap();
        with_snapshot(NetworkSnapshot { client, settings }, async {
            assert_eq!(network_snapshot().settings.proxy_mode, ProxyMode::Direct);
            with_network_snapshot(async {
                assert_eq!(network_snapshot().settings.proxy_mode, ProxyMode::Direct);
            })
            .await;
            let snapshot = network_snapshot();
            tokio::spawn(with_snapshot(snapshot, async {
                assert_eq!(network_snapshot().settings.proxy_mode, ProxyMode::Direct);
            }))
            .await
            .unwrap();
        })
        .await;
    }
}
