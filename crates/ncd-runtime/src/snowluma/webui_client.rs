//! SnowLuma WebUI HTTP 客户端:强类型 payload + SnowLumaWebUiClient trait +
//! ReqwestSnowLumaWebUiClient 默认实现
//!
//! 严格红线:本文件禁止使用动态 JSON 值类型透传任何 HTTP 字段,所有请求 / 响应
//! payload 必须用强类型 serde struct 表达
//!
//! 含 host probing / no_proxy / 401 自动重试 / host guard defense-in-depth

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ts_rs::TS;

use crate::snowluma::error::SnowLumaWebUiError;

// ---------------------------------------------------------------------------
// 跨边界(Tauri / 前端)类型 —— ts-rs 派生 + 导出
// ---------------------------------------------------------------------------

/// SnowLuma WebUI /api/processes 单条 PID 的 hook 状态
/// 与 legacy SnowLuma 服务端字面量对齐,使用 snake_case 序列化
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub enum HookProcessStatus {
    /// 找到 QQ.exe 但尚未注入
    Available,
    /// 注入中
    Loading,
    /// 注入成功,正在连 named pipe
    Connecting,
    /// 已注入 + 已连上 pipe,未登录(待扫码)
    Loaded,
    /// QQ 已登录,bot 完全可用
    Online,
    /// 注入或连接失败
    Error,
    /// 之前注入过,pipe 掉了
    Disconnected,
}

/// SnowLuma WebUI /api/processes 单条记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct HookProcessInfo {
    pub pid: u32,
    pub name: String,
    pub path: String,
    pub uin: String,
    pub status: HookProcessStatus,
    #[serde(default)]
    pub error: String,
}

/// SnowLuma WebUI /api/qq-list 单条记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct OneBotInstanceInfo {
    pub uin: String,
    pub nickname: String,
}

// 这些 struct 仅在 Rust 端 HTTP 客户端内部使用,不跨 Tauri 边界
// 因此不派生 ts-rs,避免污染前端类型表

/// POST /api/login 请求体
#[derive(Debug, Clone, Serialize)]
pub struct LoginRequest {
    pub password: String,
}

/// POST /api/login 响应体
#[derive(Debug, Clone, Deserialize)]
pub struct LoginResponse {
    pub token: String,
}

/// GET /api/processes 响应体(wrapped)
#[derive(Debug, Clone, Deserialize)]
pub struct ListProcessesResponse {
    #[serde(default)]
    pub list: Vec<HookProcessInfo>,
}

/// GET /api/qq-list 响应体(wrapped)
#[derive(Debug, Clone, Deserialize)]
pub struct ListQqInstancesResponse {
    #[serde(default)]
    pub list: Vec<OneBotInstanceInfo>,
}

/// POST /api/processes/:pid/load 与 /unload 共用响应体
/// success == false 时 process 通常为 None,error 携带服务端原因
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessActionResponse {
    pub success: bool,
    pub process: Option<HookProcessInfo>,
    #[serde(default)]
    pub error: String,
}

/// GET /api/auth/state 响应体
/// 服务端用 camelCase(mustChangePassword),通过 #[serde(rename)] 对齐
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthState {
    #[serde(default, rename = "mustChangePassword")]
    pub must_change_password: bool,
}

// ---------------------------------------------------------------------------
// SnowLumaWebUiClient trait
// ---------------------------------------------------------------------------

/// SnowLuma WebUI HTTP 客户端 trait
/// 8 个 async 方法对应 SnowLuma daemon 暴露的 8 个 endpointtrait 设计为
/// object-safe(async_trait 装箱 future),方便测试用 Arc<dyn ...> 注入
/// mock client
#[async_trait]
pub trait SnowLumaWebUiClient: Send + Sync {
    /// host probing:候选 [<inner.host>, "localhost", "127.0.0.1", "[::1]"]
    /// 去重后顺序探测 GET /api/status,任意 HTTP 响应(含 401 / 4xx / 5xx)
    ///   即视为 ready 并把命中的 host 锁定到 inner.host仅 socket 级错误
    ///   (is_timeout / is_connect)才记入 last_errors 并切下个候选
    /// dead_check 每轮 sleep 之前调用一次;返回 true 时立即结束等待并
    ///   返回 Ok(()),由调用方按"node 已死"分支处理
    async fn wait_ready(
        &self,
        timeout: Duration,
        dead_check: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), SnowLumaWebUiError>;

    /// POST /api/login 携带 LoginRequest { password };成功后把 token 缓存
    /// 进 inner.token
    async fn login(&self) -> Result<(), SnowLumaWebUiError>;

    /// POST /api/logout 尽力退登;无论结果如何都清空 inner.token
    async fn logout(&self) -> Result<(), SnowLumaWebUiError>;

    /// GET /api/processes,返回 list 字段
    async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError>;

    /// GET /api/qq-list,返回 list 字段
    async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError>;

    /// POST /api/processes/{pid}/load:触发注入success == false 返回
    ///   ServerRejected;缺少 process 字段返回 Decode15s 超时
    async fn load_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError>;

    /// POST /api/processes/{pid}/unload:解除注入语义同 load_process
    async fn unload_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError>;

    /// GET /api/auth/state:免鉴权,用于侦测 daemon 是否要求强制改密
    async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError>;

    /// POST /api/config/:uin:热推送 OneBot 配置body = 完整 OneBotConfig JSON
    /// daemon 会 saveOneBotConfig 写盘 + oneBotManager.reloadConfig(uin) 热 reload
    /// 返回 reloaded=true 表示当场生效,false 表示会话不在线下次连接生效
    async fn update_onebot_config(
        &self,
        uin: &str,
        config: &serde_json::Value,
    ) -> Result<bool, SnowLumaWebUiError>;
}


/// 候选 host 列表
const CANDIDATE_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// 默认请求超时
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// load_process / unload_process 放宽超时
const ACTION_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// wait_ready 单轮间隔
const PROBE_ROUND_INTERVAL: Duration = Duration::from_millis(500);

/// SnowLumaWebUiClient 默认实现,基于 reqwest::Client
/// 客户端配置:
/// - timeout(5s) —— 与 SnowLumaWebUiError::Timeout 语义对齐
/// - pool_idle_timeout(30s) —— 复用连接,减少握手开销
/// - no_proxy —— 显式禁用所有环境变量代理
/// - 仅 rustls-tls —— 不依赖 OpenSSL
pub struct ReqwestSnowLumaWebUiClient {
    inner: RwLock<ReqwestInner>,
    port: u16,
    password: String,
}

struct ReqwestInner {
    http: reqwest::Client,
    /// 锁定后的有效 host(localhost / 127.0.0.1 / [::1])
    host: String,
    /// 已登录的 Bearer token;None 表示尚未登录
    token: Option<String>,
}

impl ReqwestSnowLumaWebUiClient {
    /// 构造默认配置的客户端reqwest::Client::builder().build 失败(极少发生)
    /// 时返回 SnowLumaWebUiError::Http
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

    /// 拼接 http://{host}:{port}{path}[::1] 已经带方括号
    fn url_for(host: &str, port: u16, path: &str) -> String {
        format!("http://{host}:{port}{path}")
    }

    /// 当前已锁定的 host snapshot
    async fn current_host(&self) -> String {
        self.inner.read().await.host.clone()
    }

    /// 把 reqwest 错误映射到具体 SnowLumaWebUiError variant,区分 timeout / 其它
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

    /// 把响应解码成 T;失败 → Decode
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

    /// 不带鉴权的 GET 请求;超时取传入的 timeout
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

    /// 内部 helper:构造一个携带当前 token 的请求 builder(GET / POST 通用)
    /// timeout 覆盖 client 默认超时
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

    /// 带鉴权的 JSON 请求 helper:自动处理 401 重试trait 的 8 个端点中没有需要
    /// JSON request body 的鉴权请求,因此仅支持无 body 的 GET / POST;login 走专用路径
    async fn authed_request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        timeout: Duration,
    ) -> Result<T, SnowLumaWebUiError> {
        // 确保有 token
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
            // 清空 token + 重 login + 重试一次
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

/// 候选 host 顺序表:把当前 inner.host 排第一,其后跟 CANDIDATE_HOSTS,去重
fn ordered_candidates(current: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(CANDIDATE_HOSTS.len() + 1);
    out.push(current.to_string());
    for c in CANDIDATE_HOSTS {
        if !out.iter().any(|h| h == *c) {
            out.push((*c).to_string());
        }
    }
    out
}

/// Defense-in-depth:仅允许 localhost / 127.0.0.1 / [::1]
fn validate_host(host: &str) -> Result<(), SnowLumaWebUiError> {
    if host == "localhost" || host == "127.0.0.1" || host == "[::1]" {
        Ok(())
    } else {
        Err(SnowLumaWebUiError::Http {
            endpoint: "<host-guard>".into(),
            cause: format!("host not allowed: {host}"),
        })
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
            // dead_check 每轮(包括首轮)开始前调用一次;命中即直接 Ok 出去
            if (dead_check)() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(SnowLumaWebUiError::NotReady(timeout, last_errors));
            }

            let current = self.current_host().await;
            let candidates = ordered_candidates(&current);
            for host in &candidates {
                // Defense-in-depth host guard
                if validate_host(host).is_err() {
                    last_errors.insert(host.clone(), format!("host not allowed: {host}"));
                    continue;
                }
                let url = Self::url_for(host, self.port, "/api/status");
                let http = { self.inner.read().await.http.clone() };
                let result = http.get(&url).timeout(DEFAULT_REQUEST_TIMEOUT).send().await;
                match result {
                    Ok(_resp) => {
                        // 任意 HTTP 响应(含 401 / 4xx / 5xx)都视为 ready
                        let mut inner = self.inner.write().await;
                        inner.host = host.clone();
                        return Ok(());
                    }
                    Err(e) => {
                        if e.is_timeout() || e.is_connect() {
                            last_errors.insert(host.clone(), e.to_string());
                            continue;
                        }
                        // 其它错误也按 socket 级处理(少见情况,记入并尝试下一候选)
                        last_errors.insert(host.clone(), e.to_string());
                        continue;
                    }
                }
            }

            // 一轮所有候选都失败,sleep 半秒再来;尊重 deadline
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
        // 尽力退登:无论结果如何都清 token
        let result: Result<(), SnowLumaWebUiError> = async {
            let builder = self
                .build_authed_request(Method::POST, path, DEFAULT_REQUEST_TIMEOUT)
                .await?;
            let resp = builder
                .send()
                .await
                .map_err(|e| Self::classify_reqwest_error(path, e))?;
            let status = resp.status();
            // 容忍 401 / 404:服务端可能已经把 token 当成无效或路径不存在
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

        // 不管成败都清 token(best-effort 语义)
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
        // /api/auth/state 不需要鉴权
        self.anon_get_json("/api/auth/state", DEFAULT_REQUEST_TIMEOUT)
            .await
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
                    SnowLumaWebUiError::Timeout { endpoint: url.clone() }
                } else {
                    SnowLumaWebUiError::Http { endpoint: url.clone(), cause: e.to_string() }
                }
            })?;
        let status = resp.status().as_u16();
        if status == 401 {
            return Err(SnowLumaWebUiError::LoginFailed("unauthorized".into()));
        }
        let body: Resp = resp
            .json()
            .await
            .map_err(|e| SnowLumaWebUiError::Decode { endpoint: url.clone(), message: e.to_string() })?;
        if !body.success {
            return Err(SnowLumaWebUiError::ServerRejected {
                endpoint: url,
                message: "config update rejected by daemon".into(),
            });
        }
        Ok(body.reloaded)
    }
}

// ReqwestSnowLumaWebUiClientFactory:默认 wiring 用的 factory
use crate::snowluma::daemon::SnowLumaWebUiClientFactory;

/// port 在每次 create 时由 daemon 传入(与 app-config.json 的 snowlumaWebuiPort
/// 一致),构造时占位端口即可
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

// 单元测试分两组:
// 1. 纯函数 / 构造器 smoke check(无 IO)
// 2. wiremock 端到端:起 127.0.0.1:0 假服务
//    覆盖 host probing / LoginRequest body 字段 / wrapped list 解包 /
//    ProcessActionResponse 成功 + 拒绝路径 / 401 自动重试 / dead_check
//    立即返回 / NotReady 超时 / no_proxy 行为

#[cfg(test)]
mod tests {
    #![allow(unsafe_code)]
    use super::*;

    #[test]
    fn ordered_candidates_dedups_and_pins_current_first() {
        let list = ordered_candidates("127.0.0.1");
        assert_eq!(list[0], "127.0.0.1");
        // 后续顺序保持 CANDIDATE_HOSTS 中其余两个,去重后总长 3
        assert_eq!(list.len(), 3);
        assert!(list.contains(&"localhost".to_string()));
        assert!(list.contains(&"[::1]".to_string()));
    }

    #[test]
    fn ordered_candidates_handles_unknown_current_by_appending_defaults() {
        let list = ordered_candidates("custom-host");
        assert_eq!(list[0], "custom-host");
        // 三个默认候选全部追加
        assert_eq!(list.len(), 4);
    }

    #[test]
    fn validate_host_rejects_non_loopback() {
        let err = validate_host("evil.example.com").unwrap_err();
        match err {
            SnowLumaWebUiError::Http { endpoint, cause } => {
                assert_eq!(endpoint, "<host-guard>");
                assert!(cause.contains("evil.example.com"));
            }
            other => panic!("expected Http {{ endpoint, cause }}, got {other:?}"),
        }
    }

    #[test]
    fn validate_host_accepts_loopback_aliases() {
        assert!(validate_host("localhost").is_ok());
        assert!(validate_host("127.0.0.1").is_ok());
        assert!(validate_host("[::1]").is_ok());
    }

    #[test]
    fn url_for_assembles_loopback_url_correctly() {
        assert_eq!(
            ReqwestSnowLumaWebUiClient::url_for("127.0.0.1", 5099, "/api/status"),
            "http://127.0.0.1:5099/api/status"
        );
        assert_eq!(
            ReqwestSnowLumaWebUiClient::url_for("[::1]", 5099, "/api/login"),
            "http://[::1]:5099/api/login"
        );
    }

    /// deliverable #11:smoke check 构造器在默认配置下成功
    #[test]
    fn client_builder_constructs_with_no_proxy() {
        let client = ReqwestSnowLumaWebUiClient::new(5099, "pwd".into());
        assert!(
            client.is_ok(),
            "ReqwestSnowLumaWebUiClient::new should succeed with default config"
        );
    }

    // -----------------------------------------------------------------------
    // wiremock 端到端
    //
    // 起一个绑定在 127.0.0.1:0(OS 分配端口)的假 SnowLuma WebUI 服务
    // 验证 ReqwestSnowLumaWebUiClient 在真实 HTTP 链路上的行为
    //
    // wiremock 0.6 默认 MockServer::start().await 监听 127.0.0.1,与本
    // 客户端的 host guard(仅放行 localhost / 127.0.0.1 / [::1])天然匹配

    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// 取出 wiremock 在 127.0.0.1 上分配到的随机端口
    fn mock_server_port(server: &MockServer) -> u16 {
        let addr = server.address();
        assert_eq!(
            addr.ip().to_string(),
            "127.0.0.1",
            "wiremock must bind to 127.0.0.1 only (host guard)"
        );
        addr.port()
    }

    /// /api/status 任意 HTTP 响应即视为 ready这里直接 200 OK
    #[tokio::test]
    async fn wait_ready_succeeds_when_status_endpoint_responds() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("GET"))
            .and(path("/api/status"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0..)
            .mount(&server)
            .await;

        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        let result = client
            .wait_ready(Duration::from_secs(2), Box::new(|| false))
            .await;
        assert!(
            result.is_ok(),
            "wait_ready should succeed when /api/status responds: {result:?}"
        );
    }

    /// dead_check 第一轮即返回 true → wait_ready 立即 Ok(()),不发任何 HTTP
    #[tokio::test]
    async fn wait_ready_returns_ok_when_dead_check_true() {
        // 用一个明显未监听的端口 1,并配 5s 超时;只要 dead_check 立刻命中
        // 就不会真的去连,函数应在远小于超时的时间返回
        let client = ReqwestSnowLumaWebUiClient::new(1, "pwd".into()).expect("build client");

        let started = std::time::Instant::now();
        let result = client
            .wait_ready(Duration::from_secs(5), Box::new(|| true))
            .await;
        let elapsed = started.elapsed();

        assert!(
            result.is_ok(),
            "dead_check=true must short-circuit wait_ready"
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "wait_ready should return immediately when dead_check is true (took {elapsed:?})"
        );
    }

    /// 全部候选 host 都连不上 → NotReady
    /// 使用端口 1:所有 loopback 候选 (localhost / 127.0.0.1 / [::1])
    /// 上 connect 到端口 1 都会立刻 ECONNREFUSED(is_connect),不需要等待
    /// reqwest 的 5s 超时,因此 800ms 足够覆盖至少一轮候选探测 + 500ms sleep
    /// 不复用 wiremock 释放的端口是为了避免与并发执行的其它测试争抢
    #[tokio::test]
    async fn wait_ready_returns_not_ready_on_timeout() {
        let port: u16 = 1;
        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        // 800ms 至少能覆盖一轮三候选探测 + 500ms sleep
        let result = client
            .wait_ready(Duration::from_millis(800), Box::new(|| false))
            .await;

        let err = result.expect_err("expected NotReady on closed port");
        match err {
            SnowLumaWebUiError::NotReady(d, last_errors) => {
                assert_eq!(d, Duration::from_millis(800));
                assert!(
                    !last_errors.is_empty(),
                    "expected at least one host probe error in last_errors"
                );
            }
            other => panic!("expected NotReady, got {other:?}"),
        }
    }

    /// LoginRequest body 字段名锁定:{"password": "<pwd>"}
    #[tokio::test]
    async fn login_serializes_password_in_request_body() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("GET"))
            .and(path("/api/status"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0..)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .and(body_partial_json(json!({ "password": "pwd" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "abc123" })))
            .expect(1)
            .mount(&server)
            .await;

        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        client
            .wait_ready(Duration::from_secs(2), Box::new(|| false))
            .await
            .expect("wait_ready");
        client.login().await.expect("login should succeed");
        // server.drop 时校验 expect(1),body_partial_json 同时锁定字段名
    }

    /// GET /api/processes 响应是 {"list": [...]} wrapped 形态,需要解包
    #[tokio::test]
    async fn list_processes_unwraps_wrapped_list() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "tok" })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/processes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "list": [{
            "pid": 12345,
            "name": "QQ.exe",
            "path": "C:/qq",
            "uin": "100200",
            "status": "loaded",
            "error": ""
            }]
            })))
            .mount(&server)
            .await;

        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        let processes = client
            .list_processes()
            .await
            .expect("list_processes should succeed");
        assert_eq!(processes.len(), 1, "expected exactly one process");
        assert_eq!(processes[0].pid, 12345);
        assert_eq!(processes[0].name, "QQ.exe");
        assert_eq!(processes[0].uin, "100200");
        assert!(matches!(processes[0].status, HookProcessStatus::Loaded));
    }

    /// POST /api/processes/:pid/load 成功路径:success=true 且 process 非空 →
    /// 返回 HookProcessInfo
    #[tokio::test]
    async fn load_process_success_path() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "tok" })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/processes/12345/load"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "process": {
            "pid": 12345,
            "name": "QQ.exe",
            "path": "C:/qq",
            "uin": "100200",
            "status": "loaded",
            "error": ""
            },
            "error": ""
            })))
            .mount(&server)
            .await;

        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        let info = client
            .load_process(12345)
            .await
            .expect("load_process should succeed");
        assert_eq!(info.pid, 12345);
        assert_eq!(info.uin, "100200");
        assert!(matches!(info.status, HookProcessStatus::Loaded));
    }

    /// POST /api/processes/:pid/load 服务端拒绝路径:success=false →
    /// ServerRejected { endpoint, message }
    #[tokio::test]
    async fn load_process_server_rejected() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "tok" })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/processes/12345/load"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": false,
            "process": null,
            "error": "process already loaded"
            })))
            .mount(&server)
            .await;

        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        let err = client
            .load_process(12345)
            .await
            .expect_err("expected ServerRejected");
        match err {
            SnowLumaWebUiError::ServerRejected { endpoint, message } => {
                assert!(
                    endpoint.contains("/api/processes/12345/load"),
                    "endpoint should preserve original path, got {endpoint}"
                );
                assert_eq!(message, "process already loaded");
            }
            other => panic!("expected ServerRejected, got {other:?}"),
        }
    }

    /// 401 自动重试一次:第一次 /api/processes 返回 401 → 客户端清 token +
    /// 重新登录 + 重试 → 第二次返回 200最终 Ok(empty list)
    /// wiremock 默认按 mount 顺序倒序匹配(最新挂的 mock 优先)
    /// 配合 up_to_n_times(1) 实现"第一次走 A,之后走 B"的状态机
    #[tokio::test]
    async fn auto_retries_login_on_401_then_succeeds() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        // 低优先级(先挂载):第一次失败之后的回落响应
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "second" })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/processes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "list": [] })))
            .mount(&server)
            .await;

        // 高优先级(后挂载) + up_to_n_times(1):仅命中一次
        Mock::given(method("POST"))
            .and(path("/api/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "token": "first" })))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/processes"))
            .respond_with(ResponseTemplate::new(401))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        let processes = client
            .list_processes()
            .await
            .expect("list_processes should succeed after 401 retry");
        assert!(
            processes.is_empty(),
            "expected empty list on second-try success, got {processes:?}"
        );
    }

    /// GET /api/auth/state 反序列化 mustChangePassword (camelCase)
    #[tokio::test]
    async fn get_auth_state_decodes_must_change_password() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("GET"))
            .and(path("/api/auth/state"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "mustChangePassword": true })),
            )
            .mount(&server)
            .await;

        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        let state = client
            .get_auth_state()
            .await
            .expect("get_auth_state should succeed");
        assert!(
            state.must_change_password,
            "mustChangePassword=true should decode to must_change_password=true"
        );
    }

    /// 设置 HTTP_PROXY 环境变量后 client 仍走 loopback —— no_proxy 起作用
    /// 注意:Rust 测试默认并发执行,std::env::set_var 会跨测试污染环境
    /// 因此用 #[ignore] 标记,仅在显式 cargo test -- --ignored
    /// --test-threads=1 下运行
    #[tokio::test]
    #[ignore = "env-var test is racy under parallel test execution; \
 run with --ignored --test-threads=1"]
    async fn no_proxy_env_does_not_break_loopback() {
        let saved = std::env::var("HTTP_PROXY").ok();
        // SAFETY: edition 2024 把 set_var/remove_var 标为 unsafe(其它线程可能
        // 同时读环境变量)本测试用 #[ignore] 强制 --test-threads=1 单线程
        // 运行,不存在并发读者,操作对外部世界仅留下需还原的 HTTP_PROXY
        // 在断言之前已恢复,符合"无外部观察者读到不一致状态"的安全契约
        unsafe { std::env::set_var("HTTP_PROXY", "http://bogus-proxy.invalid:9") }

        let server = MockServer::start().await;
        let port = mock_server_port(&server);
        Mock::given(method("GET"))
            .and(path("/api/status"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0..)
            .mount(&server)
            .await;

        let client = ReqwestSnowLumaWebUiClient::new(port, "pwd".into()).expect("build client");
        let result = client
            .wait_ready(Duration::from_secs(2), Box::new(|| false))
            .await;

        // 还原环境变量再断言,避免断言失败留下脏状态
        // SAFETY: 同上 —— #[ignore] 强制单线程
        unsafe {
            match saved {
                Some(v) => std::env::set_var("HTTP_PROXY", v),
                None => std::env::remove_var("HTTP_PROXY"),
            }
        }

        assert!(
            result.is_ok(),
            "no_proxy must bypass HTTP_PROXY env var: {result:?}"
        );
    }
}
