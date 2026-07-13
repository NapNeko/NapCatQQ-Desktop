//! ReqwestSnowLumaWebUiClient 默认实现

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::Method;
use serde::de::DeserializeOwned;
use tokio::sync::RwLock;

use super::trait_::SnowLumaWebUiClient;
use super::types::{
    AgreementsPayload, AuthState, HookProcessInfo, ListProcessesResponse, ListQqInstancesResponse,
    LoginRequest, LoginResponse, OneBotInstanceInfo, ProbeProcessLoginResponse,
    ProcessActionResponse, QqPortLoginInfo, RecordConsentRequest, RecordConsentResponse,
};
use crate::snowluma::daemon::SnowLumaWebUiClientFactory;
use crate::snowluma::error::SnowLumaWebUiError;

const CANDIDATE_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]"];
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const ACTION_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const PROBE_ROUND_INTERVAL: Duration = Duration::from_millis(500);

pub struct ReqwestSnowLumaWebUiClient {
    inner: RwLock<ReqwestInner>,
    port: u16,
    password: String,
}

struct ReqwestInner {
    http: reqwest::Client,
    host: String,
    token: Option<String>,
}

impl ReqwestSnowLumaWebUiClient {
    pub fn new(port: u16, password: String) -> Result<Self, SnowLumaWebUiError> {
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_REQUEST_TIMEOUT)
            .pool_idle_timeout(Duration::from_secs(30))
            .no_proxy()
            .build()
            .map_err(|e| SnowLumaWebUiError::Http {
                endpoint: "<builder>".into(),
                cause: e.to_string(),
            })?;
        Ok(Self {
            inner: RwLock::new(ReqwestInner {
                http,
                host: "localhost".into(),
                token: None,
            }),
            port,
            password,
        })
    }

    pub(crate) fn url_for(host: &str, port: u16, path: &str) -> String {
        // 测试与内部 helper 共用；对外不 re-export
        format!("http://{host}:{port}{path}")
    }

    async fn current_host(&self) -> String {
        self.inner.read().await.host.clone()
    }

    fn classify_reqwest_error(endpoint: &str, err: reqwest::Error) -> SnowLumaWebUiError {
        if err.is_timeout() {
            SnowLumaWebUiError::Timeout {
                endpoint: endpoint.into(),
            }
        } else {
            SnowLumaWebUiError::Http {
                endpoint: endpoint.into(),
                cause: err.to_string(),
            }
        }
    }

    async fn decode_json<T: DeserializeOwned>(
        endpoint: &str,
        resp: reqwest::Response,
    ) -> Result<T, SnowLumaWebUiError> {
        resp.json::<T>()
            .await
            .map_err(|e| SnowLumaWebUiError::Decode {
                endpoint: endpoint.into(),
                message: e.to_string(),
            })
    }

    async fn anon_get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        timeout: Duration,
    ) -> Result<T, SnowLumaWebUiError> {
        let host = self.current_host().await;
        validate_host(&host)?;
        let url = Self::url_for(&host, self.port, path);
        let http = { self.inner.read().await.http.clone() };
        let resp = http
            .get(&url)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| Self::classify_reqwest_error(path, e))?;
        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(SnowLumaWebUiError::Status {
                endpoint: path.into(),
                status: status.as_u16(),
                message,
            });
        }
        Self::decode_json(path, resp).await
    }

    async fn build_authed_request(
        &self,
        method: Method,
        path: &str,
        timeout: Duration,
    ) -> Result<reqwest::RequestBuilder, SnowLumaWebUiError> {
        let (http, host, token) = {
            let inner = self.inner.read().await;
            (inner.http.clone(), inner.host.clone(), inner.token.clone())
        };
        validate_host(&host)?;
        let url = Self::url_for(&host, self.port, path);
        let mut builder = http.request(method, &url).timeout(timeout);
        if let Some(token) = token {
            builder = builder.bearer_auth(token);
        }
        Ok(builder)
    }

    async fn authed_request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        timeout: Duration,
    ) -> Result<T, SnowLumaWebUiError> {
        {
            let has_token = self.inner.read().await.token.is_some();
            if !has_token {
                self.login().await?;
            }
        }

        let resp = {
            let builder = self
                .build_authed_request(method.clone(), path, timeout)
                .await?;
            builder
                .send()
                .await
                .map_err(|e| Self::classify_reqwest_error(path, e))?
        };
        let status = resp.status();
        if status.as_u16() == 401 {
            {
                let mut inner = self.inner.write().await;
                inner.token = None;
            }
            self.login().await?;
            let resp2 = {
                let builder = self.build_authed_request(method, path, timeout).await?;
                builder
                    .send()
                    .await
                    .map_err(|e| Self::classify_reqwest_error(path, e))?
            };
            let status2 = resp2.status();
            if !status2.is_success() {
                let message = resp2.text().await.unwrap_or_default();
                return Err(SnowLumaWebUiError::Status {
                    endpoint: path.into(),
                    status: status2.as_u16(),
                    message,
                });
            }
            return Self::decode_json(path, resp2).await;
        }
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(SnowLumaWebUiError::Status {
                endpoint: path.into(),
                status: status.as_u16(),
                message,
            });
        }
        Self::decode_json(path, resp).await
    }
}

pub(crate) fn ordered_candidates(current: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(CANDIDATE_HOSTS.len() + 1);
    out.push(current.to_string());
    for c in CANDIDATE_HOSTS {
        if !out.iter().any(|h| h == *c) {
            out.push((*c).to_string());
        }
    }
    out
}

pub(crate) fn validate_host(host: &str) -> Result<(), SnowLumaWebUiError> {
    if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
        Ok(())
    } else {
        Err(SnowLumaWebUiError::Http {
            endpoint: "<host-guard>".into(),
            cause: format!("host not allowed: {host}"),
        })
    }
}

async fn decode_record_consent_response(
    path: &str,
    resp: reqwest::Response,
) -> Result<(), SnowLumaWebUiError> {
    let status = resp.status();
    if !status.is_success() {
        let message = resp.text().await.unwrap_or_default();
        return Err(SnowLumaWebUiError::Status {
            endpoint: path.into(),
            status: status.as_u16(),
            message,
        });
    }
    let body: RecordConsentResponse = ReqwestSnowLumaWebUiClient::decode_json(path, resp).await?;
    if body.success {
        Ok(())
    } else {
        let suffix = body
            .current_version
            .as_deref()
            .map(|v| format!(" currentVersion={v}"))
            .unwrap_or_default();
        Err(SnowLumaWebUiError::ServerRejected {
            endpoint: path.into(),
            message: format!("{}{}", body.message, suffix),
        })
    }
}

pub fn snowluma_error_requires_consent(err: &SnowLumaWebUiError) -> bool {
    match err {
        SnowLumaWebUiError::Status {
            status, message, ..
        } => *status == 403 && message.contains("\"consentRequired\":true"),
        _ => false,
    }
}

#[async_trait]
impl SnowLumaWebUiClient for ReqwestSnowLumaWebUiClient {
    async fn wait_ready(
        &self,
        timeout: Duration,
        dead_check: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), SnowLumaWebUiError> {
        let deadline = Instant::now() + timeout;
        let mut last_errors: BTreeMap<String, String> = BTreeMap::new();

        loop {
            if (dead_check)() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(SnowLumaWebUiError::NotReady(timeout, last_errors));
            }

            let current = self.current_host().await;
            let candidates = ordered_candidates(&current);
            for host in &candidates {
                if validate_host(host).is_err() {
                    last_errors.insert(host.clone(), format!("host not allowed: {host}"));
                    continue;
                }
                let url = Self::url_for(host, self.port, "/api/status");
                let http = { self.inner.read().await.http.clone() };
                let result = http.get(&url).timeout(DEFAULT_REQUEST_TIMEOUT).send().await;
                match result {
                    Ok(_resp) => {
                        let mut inner = self.inner.write().await;
                        inner.host = host.clone();
                        return Ok(());
                    }
                    Err(e) => {
                        last_errors.insert(host.clone(), e.to_string());
                        continue;
                    }
                }
            }

            let now = Instant::now();
            if now >= deadline {
                return Err(SnowLumaWebUiError::NotReady(timeout, last_errors));
            }
            let sleep_for = std::cmp::min(PROBE_ROUND_INTERVAL, deadline - now);
            tokio::time::sleep(sleep_for).await;
        }
    }

    async fn login(&self) -> Result<(), SnowLumaWebUiError> {
        let path = "/api/login";
        let host = self.current_host().await;
        validate_host(&host)?;
        let url = Self::url_for(&host, self.port, path);
        let http = { self.inner.read().await.http.clone() };
        let body = LoginRequest {
            password: self.password.clone(),
        };
        let resp = http
            .post(&url)
            .timeout(DEFAULT_REQUEST_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| Self::classify_reqwest_error(path, e))?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            let message = resp.text().await.unwrap_or_default();
            return Err(SnowLumaWebUiError::LoginFailed(format!(
                "status {} {}",
                status.as_u16(),
                message
            )));
        }
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(SnowLumaWebUiError::LoginFailed(format!(
                "status {} {}",
                status.as_u16(),
                message
            )));
        }
        let body: LoginResponse = Self::decode_json(path, resp).await?;
        let mut inner = self.inner.write().await;
        inner.token = Some(body.token);
        Ok(())
    }

    async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
        let path = "/api/logout";
        let result: Result<(), SnowLumaWebUiError> = async {
            let builder = self
                .build_authed_request(Method::POST, path, DEFAULT_REQUEST_TIMEOUT)
                .await?;
            let resp = builder
                .send()
                .await
                .map_err(|e| Self::classify_reqwest_error(path, e))?;
            let status = resp.status();
            if status.is_success() || status.as_u16() == 401 || status.as_u16() == 404 {
                return Ok(());
            }
            let message = resp.text().await.unwrap_or_default();
            Err(SnowLumaWebUiError::Status {
                endpoint: path.into(),
                status: status.as_u16(),
                message,
            })
        }
        .await;

        {
            let mut inner = self.inner.write().await;
            inner.token = None;
        }
        result
    }

    async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
        let resp: ListProcessesResponse = self
            .authed_request_json(Method::GET, "/api/processes", DEFAULT_REQUEST_TIMEOUT)
            .await?;
        Ok(resp.list)
    }

    async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
        let resp: ListQqInstancesResponse = self
            .authed_request_json(Method::GET, "/api/qq-list", DEFAULT_REQUEST_TIMEOUT)
            .await?;
        Ok(resp.list)
    }

    async fn probe_process_login_info(
        &self,
        pid: u32,
    ) -> Result<Option<QqPortLoginInfo>, SnowLumaWebUiError> {
        let path = format!("/api/processes/{pid}/probe-login");
        let resp: ProbeProcessLoginResponse = self
            .authed_request_json(Method::GET, &path, DEFAULT_REQUEST_TIMEOUT)
            .await?;
        Ok(resp.info)
    }

    async fn load_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        let path = format!("/api/processes/{pid}/load");
        let resp: ProcessActionResponse = self
            .authed_request_json(Method::POST, &path, ACTION_REQUEST_TIMEOUT)
            .await?;
        if !resp.success {
            return Err(SnowLumaWebUiError::ServerRejected {
                endpoint: path,
                message: resp.error,
            });
        }
        resp.process.ok_or_else(|| SnowLumaWebUiError::Decode {
            endpoint: path,
            message: "load 响应缺少 process 字段".into(),
        })
    }

    async fn unload_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        let path = format!("/api/processes/{pid}/unload");
        let resp: ProcessActionResponse = self
            .authed_request_json(Method::POST, &path, ACTION_REQUEST_TIMEOUT)
            .await?;
        if !resp.success {
            return Err(SnowLumaWebUiError::ServerRejected {
                endpoint: path,
                message: resp.error,
            });
        }
        resp.process.ok_or_else(|| SnowLumaWebUiError::Decode {
            endpoint: path,
            message: "unload 响应缺少 process 字段".into(),
        })
    }

    async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError> {
        self.anon_get_json("/api/auth/state", DEFAULT_REQUEST_TIMEOUT)
            .await
    }

    async fn get_agreements(&self) -> Result<AgreementsPayload, SnowLumaWebUiError> {
        self.authed_request_json(Method::GET, "/api/agreements", DEFAULT_REQUEST_TIMEOUT)
            .await
    }

    async fn record_agreement_consent(&self, version: &str) -> Result<(), SnowLumaWebUiError> {
        let path = "/api/agreements/record-consent";
        {
            let has_token = self.inner.read().await.token.is_some();
            if !has_token {
                self.login().await?;
            }
        }

        let body = RecordConsentRequest {
            version: version.to_string(),
        };
        let builder = self
            .build_authed_request(Method::POST, path, DEFAULT_REQUEST_TIMEOUT)
            .await?
            .json(&body);
        let resp = builder
            .send()
            .await
            .map_err(|e| Self::classify_reqwest_error(path, e))?;
        let status = resp.status();
        if status.as_u16() == 401 {
            {
                let mut inner = self.inner.write().await;
                inner.token = None;
            }
            self.login().await?;
            let builder = self
                .build_authed_request(Method::POST, path, DEFAULT_REQUEST_TIMEOUT)
                .await?
                .json(&body);
            let resp2 = builder
                .send()
                .await
                .map_err(|e| Self::classify_reqwest_error(path, e))?;
            return decode_record_consent_response(path, resp2).await;
        }
        decode_record_consent_response(path, resp).await
    }

    async fn update_onebot_config(
        &self,
        uin: &str,
        config: &serde_json::Value,
    ) -> Result<bool, SnowLumaWebUiError> {
        #[derive(serde::Deserialize)]
        struct Resp {
            #[serde(default)]
            success: bool,
            #[serde(default)]
            reloaded: bool,
        }
        let inner = self.inner.read().await;
        let url = Self::url_for(&inner.host, self.port, &format!("/api/config/{uin}"));
        let token = inner.token.clone().unwrap_or_default();
        let http = inner.http.clone();
        drop(inner);

        let resp = http
            .post(&url)
            .bearer_auth(&token)
            .json(config)
            .timeout(DEFAULT_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    SnowLumaWebUiError::Timeout {
                        endpoint: url.clone(),
                    }
                } else {
                    SnowLumaWebUiError::Http {
                        endpoint: url.clone(),
                        cause: e.to_string(),
                    }
                }
            })?;
        let status = resp.status().as_u16();
        if status == 401 {
            return Err(SnowLumaWebUiError::LoginFailed("unauthorized".into()));
        }
        let body: Resp = resp.json().await.map_err(|e| SnowLumaWebUiError::Decode {
            endpoint: url.clone(),
            message: e.to_string(),
        })?;
        if !body.success {
            return Err(SnowLumaWebUiError::ServerRejected {
                endpoint: url,
                message: "config update rejected by daemon".into(),
            });
        }
        Ok(body.reloaded)
    }
}

pub struct ReqwestSnowLumaWebUiClientFactory;

impl Default for ReqwestSnowLumaWebUiClientFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ReqwestSnowLumaWebUiClientFactory {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SnowLumaWebUiClientFactory for ReqwestSnowLumaWebUiClientFactory {
    async fn create(
        &self,
        password: String,
        port: u16,
    ) -> Result<std::sync::Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError> {
        let client = ReqwestSnowLumaWebUiClient::new(port, password)?;
        Ok(std::sync::Arc::new(client))
    }
}
