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
