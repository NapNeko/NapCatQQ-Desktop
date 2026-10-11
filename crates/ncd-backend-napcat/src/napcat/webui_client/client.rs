//! Reqwest 实现：仅访问 127.0.0.1

use std::time::Duration;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use super::error::NapCatWebUiError;
use super::payloads::{
    AuthLoginRequest, CheckLoginStatusData, GetQQLoginInfoData, LoginEnvelopeData, WebUiEnvelope,
};
use super::trait_::NapCatWebUiClient;

/// NapCat WebUI 登录用的 hash：sha256_hex(token + ".napcat")。
/// 调试客户端也要登录，共用这一份以免两处算法漂移。
pub(crate) fn login_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.update(b".napcat");
    hex::encode(hasher.finalize())
}

/// 上游 auth 中间件拒绝时的 message（HTTP 仍是 200）
fn is_unauthorized_message(message: &str) -> bool {
    let m = message.trim();
    m.eq_ignore_ascii_case("Unauthorized") || m.eq_ignore_ascii_case("Server token not initialized")
}

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

    /// 先读字节再解信封：reqwest 的 json() 失败只会给 "error decoding response body"，
    /// 把上游真实 message 吞掉。非 JSON 时只报 content-type 和长度，不回显 body
    async fn read_envelope<T: DeserializeOwned>(
        resp: reqwest::Response,
    ) -> Result<WebUiEnvelope<T>, NapCatWebUiError> {
        if let Some(err) = Self::handle_unauth(resp.status()) {
            return Err(err);
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-")
            .to_string();
        let bytes = resp.bytes().await?;
        serde_json::from_slice::<WebUiEnvelope<T>>(&bytes).map_err(|e| {
            NapCatWebUiError::Decode(format!(
                "响应不是预期 JSON（content-type: {content_type}，{} 字节）: {e}",
                bytes.len()
            ))
        })
    }

    /// 带鉴权接口：信封 Unauthorized 归成 Unauthorized(200) 让 poller 重新换凭据
    fn authed_data<T>(envelope: WebUiEnvelope<T>) -> Result<T, NapCatWebUiError> {
        if envelope.code != 0 {
            if is_unauthorized_message(&envelope.message) {
                return Err(NapCatWebUiError::Unauthorized(200));
            }
            return Err(NapCatWebUiError::BusinessCode {
                code: envelope.code,
                message: envelope.message,
            });
        }
        envelope
            .data
            .ok_or_else(|| NapCatWebUiError::Decode("响应缺少 data".into()))
    }

    /// /api/auth/login 失败形态见上游 api/Auth.ts LoginHandler
    pub(crate) fn credential_from_login(
        envelope: WebUiEnvelope<LoginEnvelopeData>,
    ) -> Result<String, NapCatWebUiError> {
        if envelope.code != 0 {
            let message = envelope.message.trim();
            if message.eq_ignore_ascii_case("login rate limit") {
                return Err(NapCatWebUiError::RateLimited);
            }
            if message.eq_ignore_ascii_case("token is invalid")
                || message.eq_ignore_ascii_case("token is empty")
                || message.eq_ignore_ascii_case("Server token not initialized")
                || message.eq_ignore_ascii_case("Unauthorized")
            {
                return Err(NapCatWebUiError::Rejected(message.to_string()));
            }
            return Err(NapCatWebUiError::BusinessCode {
                code: envelope.code,
                message: message.to_string(),
            });
        }
        let data = envelope.data.unwrap_or_default();
        if let Some(credential) = data.credential.filter(|c| !c.is_empty()) {
            return Ok(credential);
        }
        if data.require_2fa {
            return Err(NapCatWebUiError::TwoFactorRequired);
        }
        Err(NapCatWebUiError::Decode("登录响应缺少 Credential".into()))
    }
}

#[async_trait]
impl NapCatWebUiClient for ReqwestNapCatWebUiClient {
    async fn fetch_credential(&self, port: u16, token: &str) -> Result<String, NapCatWebUiError> {
        let hash = login_hash(token);
        let resp = self
            .client
            .post(Self::webui_url(port, "/api/auth/login"))
            .json(&AuthLoginRequest { hash })
            .send()
            .await?;
        let envelope = Self::read_envelope::<LoginEnvelopeData>(resp).await?;
        Self::credential_from_login(envelope)
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
        Self::authed_data(Self::read_envelope::<CheckLoginStatusData>(resp).await?)
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
        Self::authed_data(Self::read_envelope::<GetQQLoginInfoData>(resp).await?)
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
        // 成功时 data 是 null，只关心 code / message
        let envelope = Self::read_envelope::<serde::de::IgnoredAny>(resp).await?;
        if envelope.code == 0 {
            return Ok(());
        }
        let message = envelope.message.trim();
        if message.eq_ignore_ascii_case("Not Login") {
            return Err(NapCatWebUiError::NotLogin);
        }
        if is_unauthorized_message(message) {
            return Err(NapCatWebUiError::Unauthorized(200));
        }
        Err(NapCatWebUiError::BusinessCode {
            code: envelope.code,
            message: envelope.message,
        })
    }
}
