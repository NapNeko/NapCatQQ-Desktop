//! NapCatWebUiClient trait

use async_trait::async_trait;

use super::error::NapCatWebUiError;
use super::payloads::{CheckLoginStatusData, GetQQLoginInfoData};

/// NapCat WebUI HTTP 客户端 trait（object-safe，便于 mock）
#[async_trait]
pub trait NapCatWebUiClient: Send + Sync {
    async fn fetch_credential(&self, port: u16, token: &str) -> Result<String, NapCatWebUiError>;

    async fn check_login_status(
        &self,
        port: u16,
        auth: &str,
    ) -> Result<CheckLoginStatusData, NapCatWebUiError>;

    async fn check_online_status(
        &self,
        port: u16,
        auth: &str,
    ) -> Result<GetQQLoginInfoData, NapCatWebUiError>;

    /// 热推送 OneBot11 配置；QQ 未登录时返回 NotLogin
    async fn set_ob11_config(
        &self,
        port: u16,
        auth: &str,
        config_json: &str,
    ) -> Result<(), NapCatWebUiError>;
}
