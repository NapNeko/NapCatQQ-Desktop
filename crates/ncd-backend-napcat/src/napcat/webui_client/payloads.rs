//! NapCat WebUI 请求/响应强类型 payload
//!
//! 字段级 serde rename 严格对齐 NapCat WebUI legacy JSON。

use serde::{Deserialize, Serialize};

/// POST /api/auth/login 请求体
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthLoginRequest {
    pub hash: String,
}

/// POST /api/auth/login 响应体(顶层)
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AuthLoginResponse {
    pub data: AuthLoginData,
}

/// POST /api/auth/login 响应体的 data 字段
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AuthLoginData {
    #[serde(rename = "Credential")]
    pub credential: String,
}

/// NapCat WebUI 通用信封。上游 sendError 一律 HTTP 200 + {code:-1,message} 且没有 data，
/// 所以先按信封解，再看 code；code 缺省按 0 兼容老版本 / 老测试
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct WebUiEnvelope<T> {
    #[serde(default)]
    pub code: i64,
    #[serde(default)]
    pub message: String,
    #[serde(default = "Option::default")]
    pub data: Option<T>,
}

/// /api/auth/login 成功或 2FA 时的 data；开了 2FA 且没带 totpCode 时没有 Credential
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub(crate) struct LoginEnvelopeData {
    #[serde(rename = "Credential", default)]
    pub credential: Option<String>,
    #[serde(rename = "require2FA", default)]
    pub require_2fa: bool,
}

/// POST /api/QQLogin/CheckLoginStatus 响应体(顶层)
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CheckLoginStatusResponse {
    pub data: CheckLoginStatusData,
}

/// POST /api/QQLogin/CheckLoginStatus 响应体的 data 字段
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct CheckLoginStatusData {
    #[serde(rename = "isLogin", default)]
    pub is_login: bool,
    #[serde(rename = "isOffline", default)]
    pub is_offline: Option<bool>,
    #[serde(rename = "qrcodeurl", default)]
    pub qrcode_url: String,
}

/// POST /api/QQLogin/GetQQLoginInfo 响应体(顶层)
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GetQQLoginInfoResponse {
    pub data: GetQQLoginInfoData,
}

/// POST /api/QQLogin/GetQQLoginInfo 响应体的 data 字段
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct GetQQLoginInfoData {
    /// 上游未初始化时可能缺失，表示未知，不能按 false 处理
    #[serde(default)]
    pub online: Option<bool>,
}
