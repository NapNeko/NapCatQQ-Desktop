//! 麦麦 WebUI 的 HTTP：只连本机回环（远端实例由编排层开好 SSH `-L`），不走代理、不跟跳转。
//!
//! 鉴权就是 Cookie `maibot_session=<token>`：上游每次请求拿它和 `data/webui.json` 的 token 比，
//! 不用先登录。上游 5 分钟内错 5 次会封 IP 10 分钟，所以 token 只用盘上读来的那份，不猜。
//! 上游出错时回 `{"detail": "..."}`；400 是它的校验没过，调用方按文档报成配置错误。

use std::time::Duration;

use ncd_domain::AppConfigIssue;
use ncd_traits::AppFrameworkError;
use reqwest::{Client, Method, StatusCode};
use serde_json::{Value, json};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const COOKIE_NAME: &str = "maibot_session";

#[derive(Debug)]
pub struct MaiBotWebUi {
    base: String,
    http: Client,
    cookie: String,
}

impl MaiBotWebUi {
    pub fn connect(port: u16, token: &str) -> Result<Self, AppFrameworkError> {
        if port == 0 {
            return Err(AppFrameworkError::DashboardUnreachable("WebUI 口无效".into()));
        }
        let token = token.trim();
        if token.is_empty() {
            return Err(AppFrameworkError::DashboardAuth(
                "没读到 WebUI token（data/webui.json 缺了或被清空）".into(),
            ));
        }
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|e| AppFrameworkError::DashboardUnreachable(e.to_string()))?;
        Ok(Self {
            base: format!("http://127.0.0.1:{port}"),
            http,
            cookie: format!("{COOKIE_NAME}={token}"),
        })
    }

    /// `doc` 是这次请求改的是哪份文档，上游 400 时报在它头上
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        doc: &str,
    ) -> Result<Value, AppFrameworkError> {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header(reqwest::header::COOKIE, &self.cookie);
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| AppFrameworkError::DashboardUnreachable(e.without_url().to_string()))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(value);
        }
        let detail = value
            .get("detail")
            .map(|d| d.as_str().map_or_else(|| d.to_string(), str::to_string))
            .unwrap_or_else(|| text.chars().take(300).collect());
        Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => AppFrameworkError::DashboardAuth(
                "麦麦 WebUI 不认这个 token，data/webui.json 里的 token 可能被改过".into(),
            ),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
                AppFrameworkError::ConfigInvalid(vec![AppConfigIssue::new(
                    doc,
                    format!("麦麦没收下：{detail}"),
                )])
            }
            _ => AppFrameworkError::Integration(format!("麦麦 WebUI 返回 {status}：{detail}")),
        })
    }

    /// 只带改了的键：上游在它自己的进程里合并进当前文件（保注释），和它别处的写入不打架。
    /// 删不掉键；有删除的改动走 [`Self::write_bot_config_raw`]
    pub async fn merge_bot_config(&self, partial: &Value) -> Result<(), AppFrameworkError> {
        self.send(Method::POST, "/api/webui/config/bot", Some(partial), "bot_config")
            .await
            .map(|_| ())
    }

    /// 整份原文：上游先按它的类校验，通过了原样写盘
    pub async fn write_bot_config_raw(&self, text: &str) -> Result<(), AppFrameworkError> {
        let body = json!({ "raw_content": text });
        self.send(Method::POST, "/api/webui/config/bot/raw", Some(&body), "bot_config")
            .await
            .map(|_| ())
    }

    /// 模型配置整份给：上游要求提供商和模型都不空，改名这类跨表的改动也得一次到位。写完它会自己重载
    pub async fn write_model_config(&self, full: &Value) -> Result<(), AppFrameworkError> {
        self.send(Method::POST, "/api/webui/config/model", Some(full), "model_config")
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_needs_a_port_and_a_token() {
        assert!(matches!(
            MaiBotWebUi::connect(0, "t").unwrap_err(),
            AppFrameworkError::DashboardUnreachable(_)
        ));
        assert!(matches!(
            MaiBotWebUi::connect(8001, "  ").unwrap_err(),
            AppFrameworkError::DashboardAuth(_)
        ));
        let c = MaiBotWebUi::connect(8001, " Ncd_x ").unwrap();
        assert_eq!(c.cookie, "maibot_session=Ncd_x");
        assert_eq!(c.base, "http://127.0.0.1:8001");
    }
}
