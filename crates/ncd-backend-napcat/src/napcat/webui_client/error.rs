//! NapCat WebUI HTTP 错误枚举

use thiserror::Error;

/// NapCat WebUI HTTP 客户端的统一错误枚举
///
/// 每个 variant 对应 login_poller 状态机的一个分支:
/// - Unauthorized: caller 必须触发 auth refresh(受 5s 节流)
/// - Status: 其它非 2xx,仅记日志
/// - Throttled: 刷新节流命中(仅 fetch_credential 路径)
/// - Timeout / Http / Decode: 网络与解析
/// - NotLogin / BusinessCode: set_ob11_config 业务码
#[derive(Debug, Error)]
pub enum NapCatWebUiError {
    #[error("napcat webui auth invalid (status {0})")]
    Unauthorized(u16),
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
