//! NapCat WebUI HTTP 错误枚举

use thiserror::Error;

/// NapCat WebUI HTTP 客户端的统一错误枚举
///
/// 上游失败多数是 HTTP 200 + {code:-1,message}，这里按 message 归类，
/// login_poller 据此决定退避和徽章:
/// - Unauthorized: 401/403 或信封 Unauthorized(记作 200)，caller 重新换凭据
/// - Rejected / RateLimited / TwoFactorRequired: /api/auth/login 明确拒绝
/// - Status: 其它非 2xx
/// - Throttled: 刷新节流命中(仅 fetch_credential 路径)
/// - Timeout / Http / Decode: 网络与解析
/// - NotLogin / BusinessCode: 其余业务码
#[derive(Debug, Error)]
pub enum NapCatWebUiError {
    #[error("napcat webui auth invalid (status {0})")]
    Unauthorized(u16),
    #[error("napcat webui login rejected: {0}")]
    Rejected(String),
    #[error("napcat webui login rate limited")]
    RateLimited,
    #[error("napcat webui requires two-factor code")]
    TwoFactorRequired,
    #[error("napcat webui returned status {0}")]
    Status(u16),
    #[error("napcat webui auth refresh throttled")]
    Throttled,
    #[error("napcat webui request timeout")]
    Timeout,
    #[error("napcat webui http error: {0}")]
    Http(String),
    #[error("napcat webui decode error: {0}")]
    Decode(String),
    #[error("napcat webui rejected: QQ not login")]
    NotLogin,
    #[error("napcat webui business error (code {code}): {message}")]
    BusinessCode { code: i64, message: String },
}

impl From<reqwest::Error> for NapCatWebUiError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout
        } else {
            Self::Http(err.to_string())
        }
    }
}
