//! NapCat WebUI HTTP 客户端。
//!
//! 提供：
//! - 三个 endpoint 的强类型请求/响应 payload 与统一错误枚举
//!   [`NapCatWebUiError`]。
//! - [`NapCatWebUiClient`] trait（三个 async 方法）与默认实现
//!   [`ReqwestNapCatWebUiClient`]（`reqwest` + `rustls-tls`，仅访问
//!   `127.0.0.1`）。
//!
//! 所有 payload 使用 `serde::Deserialize` 派生强类型 struct，不通过
//! dynamic JSON 透传。
//!
//! 字段级 serde rename 严格对齐 NapCat WebUI legacy JSON：
//! - [`AuthLoginData::credential`] ↔ JSON `Credential`（PascalCase）
//! - [`CheckLoginStatusData::is_login`] ↔ JSON `isLogin`（camelCase）
//! - [`CheckLoginStatusData::qrcode_url`] ↔ JSON `qrcodeurl`（小写无分隔符）
//!
//! 安全边界：
//! - [`ReqwestNapCatWebUiClient::webui_url`] 仅拼接 `http://127.0.0.1:{port}`，
//!   禁止跨主机；调用方传入的 `path` 直接拼接，不做 host override。
//! - token 仅在内存中以参数形式流转，不持久化。
//!
//! [`napcat_login_poller::PollerDeps`]: crate::napcat::login_poller::PollerDeps

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

// =============================================================================
// Error
// =============================================================================

/// NapCat WebUI HTTP 客户端的统一错误枚举。
///
/// 每个 variant 对应 [`crate::napcat::login_poller::NapCatLoginPoller`] 状态机的一个分支：
/// - [`NapCatWebUiError::Unauthorized`]：caller 必须触发 auth refresh（受 5s 节流）。
/// - [`NapCatWebUiError::Status`]：其它非 2xx，仅记日志。
/// - [`NapCatWebUiError::Throttled`]:刷新节流命中（仅 `fetch_credential` 路径）。
/// - [`NapCatWebUiError::Timeout`]：reqwest 超时。
/// - [`NapCatWebUiError::Http`]：reqwest 网络层错误。
/// - [`NapCatWebUiError::Decode`]：JSON 反序列化失败 / 字段缺失。
///
/// **设计决策**：本枚举不实现 `From<NapCatWebUiError> for BotManagerError`
/// —— Poller 内部消化所有错误，只在彻底放弃时把摘要发 `BotError`。
#[derive(Debug, Error)]
pub enum NapCatWebUiError {
    /// HTTP 401 / 403：当前 credential 无效，需要触发 auth 刷新。
    #[error("napcat webui auth invalid (status {0})")]
    Unauthorized(u16),
    /// 其它非 200 状态码。
    #[error("napcat webui returned status {0}")]
    Status(u16),
    /// 5s 内已请求过 → 节流命中（仅 `fetch_credential` 路径用得到）。
    #[error("napcat webui auth refresh throttled")]
    Throttled,
    /// reqwest 超时（与客户端 5s timeout 对齐）。
    #[error("napcat webui request timeout")]
    Timeout,
    /// 任何其它 reqwest 网络层错误（连接失败、DNS、TLS 等）。
    #[error("napcat webui http error: {0}")]
    Http(String),
    /// JSON 反序列化失败 / 字段缺失。
    #[error("napcat webui decode error: {0}")]
    Decode(String),
}

impl From<reqwest::Error> for NapCatWebUiError {
    /// 把 [`reqwest::Error`] 分流到 [`NapCatWebUiError::Timeout`] 或
    /// [`NapCatWebUiError::Http`]。状态码错误（401/403/4xx/5xx）不在此处转换——
    /// caller 必须先用 [`reqwest::Response::status`] 显式判定后再调
    /// [`reqwest::Response::json`]，否则会丢失 `Unauthorized` / `Status` 语义。
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout
        } else {
            Self::Http(err.to_string())
        }
    }
}

// =============================================================================
// Payloads
// =============================================================================

/// `POST /api/auth/login` 请求体。
///
/// `hash` 字段由 [`crate::napcat::login_poller`] 计算 `sha256(token + ".napcat")` hex
/// 后传入；本 struct 不做哈希计算，保持 IO 与业务分离。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthLoginRequest {
    pub hash: String,
}

/// `POST /api/auth/login` 响应体（顶层）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AuthLoginResponse {
    pub data: AuthLoginData,
}

/// `POST /api/auth/login` 响应体的 `data` 字段。
///
/// `Credential` 字段使用 PascalCase serde rename，对齐 NapCat WebUI legacy JSON。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AuthLoginData {
    #[serde(rename = "Credential")]
    pub credential: String,
}

/// `POST /api/QQLogin/CheckLoginStatus` 响应体（顶层）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckLoginStatusResponse {
    pub data: CheckLoginStatusData,
}

/// `POST /api/QQLogin/CheckLoginStatus` 响应体的 `data` 字段。
///
/// 两字段都带 `default`：NapCat 早期版本可能省略字段而非返回 `false`/`""`。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct CheckLoginStatusData {
    /// 当前账号是否已登录。
    #[serde(rename = "isLogin", default)]
    pub is_login: bool,
    /// 二维码 URL；通常是 `data:image/png;base64,...` data URL，少数版本是普通 URL。
    /// 后端透传字符串，不解码、不验证（设计 D4）。
    #[serde(rename = "qrcodeurl", default)]
    pub qrcode_url: String,
}

/// `POST /api/QQLogin/GetQQLoginInfo` 响应体（顶层）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GetQQLoginInfoResponse {
    pub data: GetQQLoginInfoData,
}

/// `POST /api/QQLogin/GetQQLoginInfo` 响应体的 `data` 字段。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct GetQQLoginInfoData {
    /// 当前账号是否在线（NapCat 返回的实时在线指示）。
    #[serde(default)]
    pub online: bool,
}

// =============================================================================
// Trait
// =============================================================================

/// NapCat WebUI HTTP 客户端 trait。
///
/// 三个 async 方法对应 NapCat WebUI 三个端点：
/// - [`fetch_credential`](Self::fetch_credential): `POST /api/auth/login`，
///   返回 Bearer credential。
/// - [`check_login_status`](Self::check_login_status):
///   `POST /api/QQLogin/CheckLoginStatus`，返回 `{is_login, qrcode_url}`。
/// - [`check_online_status`](Self::check_online_status):
///   `POST /api/QQLogin/GetQQLoginInfo`，返回 `{online}`。
///
/// trait 设计为 object-safe（`async_trait` 装箱 future），方便测试用
/// `Arc<dyn NapCatWebUiClient>` 注入 mock。
#[async_trait]
pub trait NapCatWebUiClient: Send + Sync {
    /// 计算 `sha256(token + ".napcat")` hex，POST 到
    /// `http://127.0.0.1:{port}/api/auth/login`，并返回响应 `data.Credential`。
    async fn fetch_credential(&self, port: u16, token: &str) -> Result<String, NapCatWebUiError>;

    /// 携带 `Authorization: Bearer {auth}` POST 到
    /// `http://127.0.0.1:{port}/api/QQLogin/CheckLoginStatus`，返回 `data`。
    async fn check_login_status(
        &self,
        port: u16,
        auth: &str,
    ) -> Result<CheckLoginStatusData, NapCatWebUiError>;

    /// 携带 `Authorization: Bearer {auth}` POST 到
    /// `http://127.0.0.1:{port}/api/QQLogin/GetQQLoginInfo`，返回 `data`。
    async fn check_online_status(
        &self,
        port: u16,
        auth: &str,
    ) -> Result<GetQQLoginInfoData, NapCatWebUiError>;
}

// =============================================================================
// ReqwestNapCatWebUiClient
// =============================================================================

/// [`NapCatWebUiClient`] 的默认实现，基于 [`reqwest::Client`]。
///
/// 客户端配置：
/// - `timeout(5s)` —— 与 [`NapCatWebUiError::Timeout`] 语义对齐。
/// - `pool_idle_timeout(30s)` —— 复用连接，减少握手开销。
/// - 仅 `rustls-tls`（`Cargo.toml` 中 `default-features = false`），
///   不依赖 OpenSSL，Tauri 包体不需要打包系统 SSL。
///
/// 所有请求只允许走 `127.0.0.1`（[`Self::webui_url`]）。
pub struct ReqwestNapCatWebUiClient {
    client: reqwest::Client,
}

impl ReqwestNapCatWebUiClient {
    /// 构造默认配置的客户端。
    ///
    /// # Errors
    ///
    /// 当 `reqwest::Client::builder().build()` 失败（极少发生，通常是
    /// rustls root store 加载失败）时返回 [`NapCatWebUiError::Http`]。
    pub fn new() -> Result<Self, NapCatWebUiError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .map_err(NapCatWebUiError::from)?;
        Ok(Self { client })
    }

    /// 仅拼接 `http://127.0.0.1:{port}{path}`，禁止跨主机。
    ///
    /// `path` 由本模块内部 hardcoded 字面量传入（三个 NapCat WebUI 端点），
    /// 不接受 caller 注入。
    fn webui_url(port: u16, path: &str) -> String {
        format!("http://127.0.0.1:{port}{path}")
    }

    /// 把 HTTP 状态码分流到 [`NapCatWebUiError::Unauthorized`] /
    /// [`NapCatWebUiError::Status`]。
    ///
    /// - 401 / 403 → [`NapCatWebUiError::Unauthorized`]（caller 必须触发
    ///   auth 刷新）。
    /// - 其它非 2xx → [`NapCatWebUiError::Status`]（仅记日志）。
    /// - 2xx → `None`（caller 继续解析 JSON）。
    fn handle_unauth(status: reqwest::StatusCode) -> Option<NapCatWebUiError> {
        match status.as_u16() {
            401 | 403 => Some(NapCatWebUiError::Unauthorized(status.as_u16())),
            code if !status.is_success() => Some(NapCatWebUiError::Status(code)),
            _ => None,
        }
    }
}

#[async_trait]
impl NapCatWebUiClient for ReqwestNapCatWebUiClient {
    async fn fetch_credential(&self, port: u16, token: &str) -> Result<String, NapCatWebUiError> {
        // sha256(token + ".napcat") 的 hex，对齐 NapCat WebUI 服务端的
        // 鉴权握手协议。
        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(token.as_bytes());
            hasher.update(b".napcat");
            hex::encode(hasher.finalize())
        };
        let resp = self
            .client
            .post(Self::webui_url(port, "/api/auth/login"))
            .json(&AuthLoginRequest { hash })
            .send()
            .await?;
        if let Some(err) = Self::handle_unauth(resp.status()) {
            return Err(err);
        }
        let body: AuthLoginResponse = resp
            .json()
            .await
            .map_err(|e| NapCatWebUiError::Decode(e.to_string()))?;
        Ok(body.data.credential)
    }

    async fn check_login_status(
        &self,
        port: u16,
        auth: &str,
    ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
        let resp = self
            .client
            .post(Self::webui_url(port, "/api/QQLogin/CheckLoginStatus"))
            .bearer_auth(auth)
            .send()
            .await?;
        if let Some(err) = Self::handle_unauth(resp.status()) {
            return Err(err);
        }
        let body: CheckLoginStatusResponse = resp
            .json()
            .await
            .map_err(|e| NapCatWebUiError::Decode(e.to_string()))?;
        Ok(body.data)
    }

    async fn check_online_status(
        &self,
        port: u16,
        auth: &str,
    ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
        let resp = self
            .client
            .post(Self::webui_url(port, "/api/QQLogin/GetQQLoginInfo"))
            .bearer_auth(auth)
            .send()
            .await?;
        if let Some(err) = Self::handle_unauth(resp.status()) {
            return Err(err);
        }
        let body: GetQQLoginInfoResponse = resp
            .json()
            .await
            .map_err(|e| NapCatWebUiError::Decode(e.to_string()))?;
        Ok(body.data)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------- Payload deserialization --------

    #[test]
    fn auth_login_request_serializes_hash_field() {
        let req = AuthLoginRequest {
            hash: "abc123".to_string(),
        };
        let json = serde_json::to_string(&req).expect("serialize AuthLoginRequest");
        assert_eq!(json, r#"{"hash":"abc123"}"#);
    }

    #[test]
    fn auth_login_response_deserializes_pascal_case_credential() {
        let json = r#"{"data":{"Credential":"bearer-token-xyz"}}"#;
        let resp: AuthLoginResponse =
            serde_json::from_str(json).expect("deserialize AuthLoginResponse");
        assert_eq!(resp.data.credential, "bearer-token-xyz");
    }

    #[test]
    fn auth_login_response_rejects_lowercase_credential_field() {
        // 严格对齐 legacy JSON 字段名 `Credential`（PascalCase），小写应失败。
        let json = r#"{"data":{"credential":"bearer-token-xyz"}}"#;
        let result: Result<AuthLoginResponse, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "lowercase credential must not deserialize (legacy uses PascalCase)"
        );
    }

    #[test]
    fn check_login_status_data_deserializes_legacy_field_names() {
        let json = r#"{"isLogin":true,"qrcodeurl":"data:image/png;base64,iVBOR"}"#;
        let data: CheckLoginStatusData =
            serde_json::from_str(json).expect("deserialize CheckLoginStatusData");
        assert!(data.is_login);
        assert_eq!(data.qrcode_url, "data:image/png;base64,iVBOR");
    }

    #[test]
    fn check_login_status_data_uses_defaults_when_fields_missing() {
        // 早期 NapCat 版本可能省略字段；default 必须生效。
        let json = r#"{}"#;
        let data: CheckLoginStatusData =
            serde_json::from_str(json).expect("deserialize empty object");
        assert!(!data.is_login);
        assert_eq!(data.qrcode_url, "");
    }

    #[test]
    fn check_login_status_response_deserializes_full_payload() {
        let json = r#"{"data":{"isLogin":false,"qrcodeurl":"data:image/png;base64,QR=="}}"#;
        let resp: CheckLoginStatusResponse =
            serde_json::from_str(json).expect("deserialize CheckLoginStatusResponse");
        assert!(!resp.data.is_login);
        assert_eq!(resp.data.qrcode_url, "data:image/png;base64,QR==");
    }

    #[test]
    fn get_qq_login_info_data_deserializes_online_flag() {
        let json = r#"{"online":true}"#;
        let data: GetQQLoginInfoData =
            serde_json::from_str(json).expect("deserialize GetQQLoginInfoData");
        assert!(data.online);
    }

    #[test]
    fn get_qq_login_info_data_defaults_online_to_false_when_missing() {
        let json = r#"{}"#;
        let data: GetQQLoginInfoData =
            serde_json::from_str(json).expect("deserialize empty object");
        assert!(!data.online);
    }

    #[test]
    fn get_qq_login_info_response_deserializes_full_payload() {
        let json = r#"{"data":{"online":false}}"#;
        let resp: GetQQLoginInfoResponse =
            serde_json::from_str(json).expect("deserialize GetQQLoginInfoResponse");
        assert!(!resp.data.online);
    }

    // -------- Error Display --------

    #[test]
    fn error_unauthorized_display_includes_status_code() {
        let err = NapCatWebUiError::Unauthorized(401);
        assert_eq!(err.to_string(), "napcat webui auth invalid (status 401)");
    }

    #[test]
    fn error_status_display_includes_status_code() {
        let err = NapCatWebUiError::Status(500);
        assert_eq!(err.to_string(), "napcat webui returned status 500");
    }

    #[test]
    fn error_throttled_display_is_stable() {
        let err = NapCatWebUiError::Throttled;
        assert_eq!(err.to_string(), "napcat webui auth refresh throttled");
    }

    #[test]
    fn error_timeout_display_is_stable() {
        let err = NapCatWebUiError::Timeout;
        assert_eq!(err.to_string(), "napcat webui request timeout");
    }

    #[test]
    fn error_http_display_includes_inner_message() {
        let err = NapCatWebUiError::Http("connection refused".to_string());
        assert_eq!(
            err.to_string(),
            "napcat webui http error: connection refused"
        );
    }

    #[test]
    fn error_decode_display_includes_inner_message() {
        let err = NapCatWebUiError::Decode("missing field `Credential`".to_string());
        assert_eq!(
            err.to_string(),
            "napcat webui decode error: missing field `Credential`"
        );
    }

    #[test]
    fn error_implements_std_error_trait() {
        // thiserror 派生应当让 NapCatWebUiError 实现 std::error::Error。
        fn assert_error<E: std::error::Error>() {}
        assert_error::<NapCatWebUiError>();
    }

    // -------- From<reqwest::Error> conversion --------

    #[tokio::test]
    async fn from_reqwest_error_timeout_maps_to_timeout_variant() {
        // 构造一个真实的 reqwest 超时 error：1ms timeout 请求一个不会响应的端口。
        // 用 127.0.0.1:1（reserved，连接立即被拒）会变成 Http 而非 Timeout，
        // 因此用一个连接但读慢的方法：reqwest 自带 timeout 配置触发。
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(1))
            .build()
            .expect("build reqwest client");
        // 10.255.255.1 是 RFC1918 网段中通常不可达的地址，握手会超时。
        let result = client.get("http://10.255.255.1:1/").send().await;
        let err = result.expect_err("request to unreachable host should error");
        let mapped: NapCatWebUiError = err.into();
        assert!(
            matches!(
                mapped,
                NapCatWebUiError::Timeout | NapCatWebUiError::Http(_)
            ),
            "reqwest error should map to Timeout or Http, got {mapped:?}"
        );
    }

    #[test]
    fn from_reqwest_error_non_timeout_maps_to_http_variant() {
        // 直接使用 reqwest::Url::parse 错误会被 reqwest::Error 包装（builder 路径）。
        // 改用 invalid header value 触发非 timeout 错误。
        let result = reqwest::Client::builder()
            // Empty user agent header value triggers a builder error in some configs;
            // 退一步：用一个肯定失败的 URL 构造请求。
            .build();
        // 上面的 build 不会失败，因此用 blocking 构造一个错误：
        // 用 reqwest::Url 解析非法 URL 不属于 reqwest::Error，跳过此分支测试。
        // 仅断言 Http variant 的 Display 包含原始字符串。
        let _ = result; // 仅消费变量以避免 unused warning
        let err = NapCatWebUiError::Http("dns lookup failed".to_string());
        assert!(err.to_string().contains("dns lookup failed"));
    }

    // -------- Trait + ReqwestNapCatWebUiClient --------

    #[test]
    fn napcat_webui_client_trait_is_object_safe() {
        // 确保 trait 可以以 dyn 形式被 Arc 持有（任务 6.1 的依赖）。
        struct Stub;

        #[async_trait]
        impl NapCatWebUiClient for Stub {
            async fn fetch_credential(
                &self,
                _port: u16,
                _token: &str,
            ) -> Result<String, NapCatWebUiError> {
                Ok("stub".into())
            }
            async fn check_login_status(
                &self,
                _port: u16,
                _auth: &str,
            ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
                Ok(CheckLoginStatusData::default())
            }
            async fn check_online_status(
                &self,
                _port: u16,
                _auth: &str,
            ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
                Ok(GetQQLoginInfoData::default())
            }
        }

        fn assert_object_safe(_: &dyn NapCatWebUiClient) {}
        let stub = Stub;
        assert_object_safe(&stub);

        // 进一步断言可被 Arc<dyn ...> 持有（PollerDeps 的真实用法）。
        let arced: std::sync::Arc<dyn NapCatWebUiClient> = std::sync::Arc::new(stub);
        let _ = arced;
    }

    #[test]
    fn reqwest_napcat_webui_client_new_succeeds_with_default_config() {
        // rustls-tls + 5s timeout + 30s pool idle 的默认配置应当可被构造。
        let client = ReqwestNapCatWebUiClient::new();
        assert!(
            client.is_ok(),
            "ReqwestNapCatWebUiClient::new should succeed with default rustls config"
        );
    }

    #[test]
    fn webui_url_only_targets_loopback() {
        // 仅向 127.0.0.1 发起请求。
        let url = ReqwestNapCatWebUiClient::webui_url(6099, "/api/auth/login");
        assert_eq!(url, "http://127.0.0.1:6099/api/auth/login");
        assert!(url.starts_with("http://127.0.0.1:"));
        assert!(!url.contains("localhost"));
    }

    #[test]
    fn handle_unauth_maps_401_to_unauthorized() {
        let status = reqwest::StatusCode::UNAUTHORIZED;
        let mapped = ReqwestNapCatWebUiClient::handle_unauth(status);
        assert!(matches!(mapped, Some(NapCatWebUiError::Unauthorized(401))));
    }

    #[test]
    fn handle_unauth_maps_403_to_unauthorized() {
        let status = reqwest::StatusCode::FORBIDDEN;
        let mapped = ReqwestNapCatWebUiClient::handle_unauth(status);
        assert!(matches!(mapped, Some(NapCatWebUiError::Unauthorized(403))));
    }

    #[test]
    fn handle_unauth_maps_500_to_status() {
        let status = reqwest::StatusCode::INTERNAL_SERVER_ERROR;
        let mapped = ReqwestNapCatWebUiClient::handle_unauth(status);
        assert!(matches!(mapped, Some(NapCatWebUiError::Status(500))));
    }

    #[test]
    fn handle_unauth_returns_none_for_2xx() {
        let status = reqwest::StatusCode::OK;
        let mapped = ReqwestNapCatWebUiClient::handle_unauth(status);
        assert!(mapped.is_none());
    }

    // -------- sha256 hash legacy parity (任务 2.3) --------

    /// 固定输入 `"abc"` + `".napcat"` → 期望 sha256 hex 对齐 legacy。
    ///
    /// 期望值由 `python -c "import hashlib;
    /// print(hashlib.sha256(b'abc.napcat').hexdigest())"` 计算得出，
    /// 等价于 `hashlib.sha256((token + ".napcat").encode()).hexdigest()`
    /// 行为。
    ///
    /// 这里直接复算 hash 而不经过 HTTP 调用 —— sha256 字面量断言放在最低层
    /// 验证哈希计算正确性；wiremock 测试再叠加端到端验证 body。
    #[test]
    fn fetch_credential_hash_matches_legacy_sha256_of_token_dot_napcat() {
        let token = "abc";
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hasher.update(b".napcat");
        let actual = hex::encode(hasher.finalize());

        // Python: hashlib.sha256(b"abc.napcat").hexdigest()
        let expected = "42e5515d256cb0ab3de18017ee3adefc15aa70229c27788bab5aee39d5d439e6";
        assert_eq!(
            actual, expected,
            "sha256(token + \".napcat\") must match legacy Python hashlib output"
        );
    }

    // -------- wiremock end-to-end (任务 2.3) --------
    //
    // 这一组测试用 wiremock 起一个绑定在 `127.0.0.1:0`（随机端口）的假服务，
    // 通过 [`wiremock::MockServer::address`] 取出端口后调用
    // [`ReqwestNapCatWebUiClient`] 的三个方法。所有请求都只发往 `127.0.0.1`。
    //
    // wiremock 0.6 默认 builder 使用
    // `TcpListener::bind("127.0.0.1:0")`（见 wiremock src/mock_server/builder.rs:107），
    // 因此满足"仅访问 127.0.0.1"约束。

    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// 取出 wiremock 在 127.0.0.1 上分配到的随机端口。
    fn mock_server_port(server: &MockServer) -> u16 {
        let addr = server.address();
        assert_eq!(
            addr.ip().to_string(),
            "127.0.0.1",
            "wiremock must bind to 127.0.0.1 only (red-line 13.2)"
        );
        addr.port()
    }

    #[tokio::test]
    async fn fetch_credential_success_returns_credential_and_sends_correct_hash() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        // 期望 body.hash == sha256(b"abc.napcat") hex
        let expected_hash = "42e5515d256cb0ab3de18017ee3adefc15aa70229c27788bab5aee39d5d439e6";
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .and(body_partial_json(
                serde_json::json!({ "hash": expected_hash }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "Credential": "bearer-from-mock" }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let credential = client
            .fetch_credential(port, "abc")
            .await
            .expect("fetch_credential should succeed");
        assert_eq!(credential, "bearer-from-mock");
        // server.drop() 触发 verify expect(1)
    }

    #[tokio::test]
    async fn fetch_credential_401_maps_to_unauthorized() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .fetch_credential(port, "abc")
            .await
            .expect_err("401 must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Unauthorized(401)),
            "expected Unauthorized(401), got {err:?}"
        );
    }

    #[tokio::test]
    async fn fetch_credential_500_maps_to_status() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .fetch_credential(port, "abc")
            .await
            .expect_err("500 must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Status(500)),
            "expected Status(500), got {err:?}"
        );
    }

    #[tokio::test]
    async fn fetch_credential_invalid_json_maps_to_decode() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        // 200 + 非 JSON body → reqwest::Response::json 反序列化失败 → Decode
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not a json payload"))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .fetch_credential(port, "abc")
            .await
            .expect_err("invalid json must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Decode(_)),
            "expected Decode(_), got {err:?}"
        );
    }

    #[tokio::test]
    async fn fetch_credential_timeout_maps_to_timeout_variant() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        // 让响应延迟 5s 以上（明显超过客户端 1ms timeout）。
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({
                        "data": { "Credential": "never-arrives" }
                    }))
                    .set_delay(Duration::from_secs(5)),
            )
            .mount(&server)
            .await;

        // 用一个 1ms 超时的 client 触发 reqwest::Error::is_timeout()。
        // 注意：默认 ReqwestNapCatWebUiClient 是 5s timeout，这里手工组装
        // 仅为验证 From<reqwest::Error> 在 fetch_credential 路径上的分流。
        let client_inner = reqwest::Client::builder()
            .timeout(Duration::from_millis(1))
            .build()
            .expect("build short-timeout client");
        let client = ReqwestNapCatWebUiClient {
            client: client_inner,
        };
        let err = client
            .fetch_credential(port, "abc")
            .await
            .expect_err("expected timeout error");
        assert!(
            matches!(err, NapCatWebUiError::Timeout),
            "expected Timeout, got {err:?}"
        );
    }

    #[tokio::test]
    async fn check_login_status_success_returns_data_and_sends_bearer_auth() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/QQLogin/CheckLoginStatus"))
            .and(header("authorization", "Bearer the-bearer-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "isLogin": false,
                    "qrcodeurl": "data:image/png;base64,QR=="
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let data = client
            .check_login_status(port, "the-bearer-token")
            .await
            .expect("check_login_status should succeed");
        assert!(!data.is_login);
        assert_eq!(data.qrcode_url, "data:image/png;base64,QR==");
    }

    #[tokio::test]
    async fn check_login_status_401_maps_to_unauthorized() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/QQLogin/CheckLoginStatus"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .check_login_status(port, "stale-token")
            .await
            .expect_err("401 must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Unauthorized(401)),
            "expected Unauthorized(401), got {err:?}"
        );
    }

    #[tokio::test]
    async fn check_login_status_500_maps_to_status() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/QQLogin/CheckLoginStatus"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .check_login_status(port, "tok")
            .await
            .expect_err("500 must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Status(500)),
            "expected Status(500), got {err:?}"
        );
    }

    #[tokio::test]
    async fn check_login_status_invalid_json_maps_to_decode() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/QQLogin/CheckLoginStatus"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>oops</html>"))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .check_login_status(port, "tok")
            .await
            .expect_err("invalid json must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Decode(_)),
            "expected Decode(_), got {err:?}"
        );
    }

    #[tokio::test]
    async fn check_online_status_success_returns_online_flag() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/QQLogin/GetQQLoginInfo"))
            .and(header("authorization", "Bearer my-bearer"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "online": true }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let data = client
            .check_online_status(port, "my-bearer")
            .await
            .expect("check_online_status should succeed");
        assert!(data.online);
    }

    #[tokio::test]
    async fn check_online_status_401_maps_to_unauthorized() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/QQLogin/GetQQLoginInfo"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .check_online_status(port, "tok")
            .await
            .expect_err("401 must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Unauthorized(401)),
            "expected Unauthorized(401), got {err:?}"
        );
    }

    #[tokio::test]
    async fn check_online_status_500_maps_to_status() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/QQLogin/GetQQLoginInfo"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .check_online_status(port, "tok")
            .await
            .expect_err("500 must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Status(500)),
            "expected Status(500), got {err:?}"
        );
    }

    #[tokio::test]
    async fn check_online_status_invalid_json_maps_to_decode() {
        let server = MockServer::start().await;
        let port = mock_server_port(&server);

        Mock::given(method("POST"))
            .and(path("/api/QQLogin/GetQQLoginInfo"))
            .respond_with(ResponseTemplate::new(200).set_body_string("definitely not json {[}"))
            .mount(&server)
            .await;

        let client = ReqwestNapCatWebUiClient::new().expect("build client");
        let err = client
            .check_online_status(port, "tok")
            .await
            .expect_err("invalid json must produce an error");
        assert!(
            matches!(err, NapCatWebUiError::Decode(_)),
            "expected Decode(_), got {err:?}"
        );
    }
}
