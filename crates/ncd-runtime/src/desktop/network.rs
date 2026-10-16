//! 桌面端网络配置生效、代理凭据与连接测试。

use std::sync::RwLock;
use std::time::{Duration, Instant};

use futures_util::{StreamExt, future::BoxFuture, stream};
use ncd_domain::{NetworkProbeResult, NetworkSettings, SecretError};
use ncd_network::client::{NetworkProvider, NetworkSnapshot, build_configured_client};
use ncd_network::mirror::mirror_urls_for_settings;
use ncd_traits::SecretStore;

pub const PROXY_PASSWORD_SECRET_KEY: &str = "app:proxy_password";
const PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const GITHUB_API_PROBE: &str = "https://api.github.com/rate_limit";
const GITHUB_DOWNLOAD_PROBE: &str =
    "https://raw.githubusercontent.com/NapNeko/NapCatQQ/main/README.md";
const NODE_DOWNLOAD_PROBE: &str = "https://nodejs.org/dist/index.json";

pub struct NetworkManager {
    active: RwLock<NetworkSnapshot>,
}

impl NetworkManager {
    pub fn new(settings: &NetworkSettings, password: &str) -> Result<Self, String> {
        Ok(Self {
            active: RwLock::new(Self::prepare(settings, password)?),
        })
    }

    /// 启动配置损坏时保持应用可修复，并阻止外网请求绕过用户的代理设置。
    pub fn blocked(settings: &NetworkSettings) -> Result<Self, String> {
        let proxy = reqwest::Proxy::all("http://127.0.0.1:0")
            .map_err(|_| "无法创建网络隔离代理".to_string())?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .proxy(proxy)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_secs(1))
            .build()
            .map_err(|_| "无法创建网络隔离客户端".to_string())?;
        Ok(Self {
            active: RwLock::new(NetworkSnapshot {
                client,
                settings: settings.clone(),
            }),
        })
    }

    pub fn prepare(settings: &NetworkSettings, password: &str) -> Result<NetworkSnapshot, String> {
        let mut settings = settings.clone();
        settings.normalize();
        let client = build_configured_client(&settings, password).map_err(|e| e.to_string())?;
        Ok(NetworkSnapshot { client, settings })
    }

    /// 调用方先持久化设置和凭据，再一次替换；在途任务继续使用自己取得的快照。
    pub fn apply(&self, snapshot: NetworkSnapshot) {
        *self.active.write().unwrap_or_else(|e| e.into_inner()) = snapshot;
    }
}

impl NetworkProvider for NetworkManager {
    fn snapshot(&self) -> NetworkSnapshot {
        self.active
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

pub fn read_proxy_password(store: &dyn SecretStore) -> Result<Option<String>, String> {
    store
        .get(PROXY_PASSWORD_SECRET_KEY)
        .map_err(|_| "读取代理密码失败，请检查系统凭据存储".to_string())
}

pub fn save_proxy_password(store: &dyn SecretStore, password: &str) -> Result<(), String> {
    if password.is_empty() {
        restore_proxy_password(store, None)
    } else {
        store
            .put(PROXY_PASSWORD_SECRET_KEY, password)
            .map_err(|_| "保存代理密码失败，请检查系统凭据存储".to_string())
    }
}

pub fn restore_proxy_password(
    store: &dyn SecretStore,
    previous: Option<&str>,
) -> Result<(), String> {
    if let Some(password) = previous {
        return store
            .put(PROXY_PASSWORD_SECRET_KEY, password)
            .map_err(|_| "恢复代理密码失败，请重新保存网络设置".to_string());
    }
    let existed = read_proxy_password(store)?.is_some();
    match store.delete(PROXY_PASSWORD_SECRET_KEY) {
        Ok(()) => {}
        Err(SecretError::NotFound) if !existed => {}
        Err(_) => return Err("清除代理密码失败，请检查系统凭据存储".to_string()),
    }
    if read_proxy_password(store)?.is_some() {
        return Err("清除代理密码失败，凭据仍存在".to_string());
    }
    Ok(())
}

pub async fn probe_network_settings(
    settings: &NetworkSettings,
    password: &str,
) -> Result<Vec<NetworkProbeResult>, String> {
    let snapshot = NetworkManager::prepare(settings, password)?;
    let mut probes: Vec<BoxFuture<'static, NetworkProbeResult>> = vec![Box::pin(probe_url(
        snapshot.client.clone(),
        "GitHub API".to_string(),
        GITHUB_API_PROBE.to_string(),
    ))];
    for (target, original) in [
        ("GitHub 下载", GITHUB_DOWNLOAD_PROBE),
        ("Node.js 下载", NODE_DOWNLOAD_PROBE),
    ] {
        for url in mirror_urls_for_settings(original, &snapshot.settings) {
            probes.push(Box::pin(probe_url(
                snapshot.client.clone(),
                target.to_string(),
                url,
            )));
        }
    }
    Ok(stream::iter(probes).buffered(4).collect().await)
}

async fn probe_url(client: reqwest::Client, target: String, url: String) -> NetworkProbeResult {
    let started = Instant::now();
    let response = client
        .get(&url)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .timeout(PROBE_TIMEOUT)
        .send()
        .await;
    // 只验证连接和响应头，服务器即使忽略 Range 也不继续消费整个文件。
    let (success, message) = match response {
        Ok(response) => {
            let status = response.status();
            let success = status.is_success();
            let message = match status {
                reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED => {
                    "代理认证失败，请检查用户名和密码".to_string()
                }
                reqwest::StatusCode::TOO_MANY_REQUESTS => "请求受限，请稍后再试".to_string(),
                _ if success => format!("连接成功（HTTP {}）", status.as_u16()),
                _ => format!("服务器返回 HTTP {}", status.as_u16()),
            };
            (success, message)
        }
        Err(error) => {
            let message = if error.is_timeout() {
                "连接超时，请检查代理和下载线路"
            } else if error.is_connect() {
                "连接失败，请检查网络、代理地址和认证信息"
            } else if error.is_redirect() {
                "重定向失败，请检查下载线路"
            } else {
                "请求失败，请检查网络设置"
            };
            (false, message.to_string())
        }
    };
    NetworkProbeResult {
        target,
        url,
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32,
        success,
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use ncd_domain::{DownloadSourceMode, ProxyMode};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[derive(Default)]
    struct MemorySecrets {
        value: Mutex<Option<String>>,
        fail_write: bool,
    }

    impl SecretStore for MemorySecrets {
        fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
            assert_eq!(key, PROXY_PASSWORD_SECRET_KEY);
            Ok(self.value.lock().unwrap().clone())
        }

        fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
            assert_eq!(key, PROXY_PASSWORD_SECRET_KEY);
            if self.fail_write {
                return Err(SecretError::Unavailable);
            }
            *self.value.lock().unwrap() = Some(value.to_string());
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), SecretError> {
            assert_eq!(key, PROXY_PASSWORD_SECRET_KEY);
            if self.fail_write {
                return Err(SecretError::Unavailable);
            }
            self.value
                .lock()
                .unwrap()
                .take()
                .map(|_| ())
                .ok_or(SecretError::NotFound)
        }
    }

    #[test]
    fn preparing_a_draft_does_not_replace_active_settings() {
        let current = NetworkSettings {
            proxy_mode: ProxyMode::Direct,
            ..NetworkSettings::default()
        };
        let manager = NetworkManager::new(&current, "").unwrap();
        let mut draft = current.clone();
        draft.github_download_mode = DownloadSourceMode::Direct;
        let prepared = NetworkManager::prepare(&draft, "").unwrap();
        assert_eq!(manager.snapshot().settings, current);
        manager.apply(prepared);
        assert_eq!(manager.snapshot().settings, draft);

        let mut invalid = current.clone();
        invalid.proxy_mode = ProxyMode::Manual;
        invalid.proxy_url = "not a proxy URL".into();
        assert!(NetworkManager::prepare(&invalid, "secret").is_err());
        assert_eq!(manager.snapshot().settings, draft);
    }

    #[test]
    fn proxy_password_keeps_whitespace_and_can_be_restored() {
        let store = MemorySecrets::default();
        save_proxy_password(&store, "  password  ").unwrap();
        let previous = read_proxy_password(&store).unwrap();
        assert_eq!(previous.as_deref(), Some("  password  "));
        save_proxy_password(&store, "replacement").unwrap();
        restore_proxy_password(&store, previous.as_deref()).unwrap();
        assert_eq!(read_proxy_password(&store).unwrap(), previous);
        save_proxy_password(&store, "").unwrap();
        assert_eq!(read_proxy_password(&store).unwrap(), None);
        save_proxy_password(&store, "").unwrap();
    }

    #[test]
    fn credential_write_failures_are_reported() {
        let store = MemorySecrets {
            fail_write: true,
            ..MemorySecrets::default()
        };
        let error = save_proxy_password(&store, "private-password").unwrap_err();
        assert!(!error.contains("private-password"));
        assert!(save_proxy_password(&store, "").is_err());
        assert!(restore_proxy_password(&store, Some("old-password")).is_err());
    }

    #[tokio::test]
    async fn probes_use_bounded_range_and_report_http_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file"))
            .and(header("range", "bytes=0-0"))
            .respond_with(ResponseTemplate::new(206).set_body_string("x"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/auth"))
            .respond_with(ResponseTemplate::new(407))
            .mount(&server)
            .await;
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let success = probe_url(
            client.clone(),
            "test".to_string(),
            format!("{}/file", server.uri()),
        )
        .await;
        assert!(success.success);
        let failure = probe_url(client, "test".to_string(), format!("{}/auth", server.uri())).await;
        assert!(!failure.success);
        assert!(failure.message.contains("代理认证失败"));
    }

    #[tokio::test]
    async fn blocked_client_never_reaches_a_destination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let manager = NetworkManager::blocked(&NetworkSettings::default()).unwrap();
        let result = probe_url(
            manager.snapshot().client,
            "blocked".to_string(),
            server.uri(),
        )
        .await;
        assert!(!result.success);
    }
}
