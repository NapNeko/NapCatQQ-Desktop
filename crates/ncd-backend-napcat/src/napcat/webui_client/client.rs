//! Reqwest 实现：仅访问 127.0.0.1

use std::time::Duration;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use super::error::NapCatWebUiError;
use super::payloads::{
    AuthLoginRequest, AuthLoginResponse, CheckLoginStatusData, CheckLoginStatusResponse,
    GetQQLoginInfoData, GetQQLoginInfoResponse,
};
use super::trait_::NapCatWebUiClient;

/// [NapCatWebUiClient] 的默认实现（reqwest + rustls，仅 loopback）
pub struct ReqwestNapCatWebUiClient {
    // pub(crate): 单测可注入自定义 reqwest::Client（wiremock）
    pub(crate) client: reqwest::Client,
}

impl ReqwestNapCatWebUiClient {
    pub fn new() -> Result<Self, NapCatWebUiError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(30))
            .no_proxy()
            .build()
            .map_err(NapCatWebUiError::from)?;
        Ok(Self { client })
    }

    /// 仅拼接 http://127.0.0.1:{port}{path}
    pub(crate) fn webui_url(port: u16, path: &str) -> String {
        format!("http://127.0.0.1:{port}{path}")
    }

    pub(crate) fn handle_unauth(status: reqwest::StatusCode) -> Option<NapCatWebUiError> {
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

    async fn set_ob11_config(
        &self,
        port: u16,
        auth: &str,
        config_json: &str,
    ) -> Result<(), NapCatWebUiError> {
        let body = serde_json::json!({ "config": config_json });
        let resp = self
            .client
            .post(Self::webui_url(port, "/api/OB11Config/SetConfig"))
            .bearer_auth(auth)
            .json(&body)
            .send()
            .await?;
        if let Some(err) = Self::handle_unauth(resp.status()) {
            return Err(err);
        }
        let status = resp.status();
        if !status.is_success() {
            return Err(NapCatWebUiError::Status(status.as_u16()));
        }
        let payload: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| NapCatWebUiError::Decode(e.to_string()))?;
        let code = payload.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        if code == 0 {
            return Ok(());
        }
        let message = payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if message.eq_ignore_ascii_case("Not Login") {
            return Err(NapCatWebUiError::NotLogin);
        }
        Err(NapCatWebUiError::BusinessCode { code, message })
    }
}
