//! 麦麦 WebUI 的 HTTP：只连本机回环（远端实例由编排层开好 SSH `-L`），不走代理、不跟跳转。
//!
//! 鉴权就是 Cookie `maibot_session=<token>`：上游每次请求拿它和 `data/webui.json` 的 token 比，
//! 不用先登录。上游 5 分钟内错 5 次会封 IP 10 分钟，所以 token 只用盘上读来的那份，不猜。
//! 上游出错时回 `{"detail": "..."}`；400 是它的校验没过，调用方按文档报成配置错误。

use std::time::Duration;

use ncd_domain::AppConfigIssue;
use ncd_traits::AppFrameworkError;
use reqwest::{Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::runtime::{
    MaiBotStatsSummary, UpstreamMcpStatus, UpstreamMcpTest, UpstreamModelList, UpstreamProviderCheck,
    UpstreamSessions, UpstreamStatus,
};
use super::schema::MaiBotAPIProvider;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const SLOW_TIMEOUT: Duration = Duration::from_secs(45);
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const COOKIE_NAME: &str = "maibot_session";

#[derive(Debug)]
pub struct MaiBotWebUi {
    base: String,
    http: Client,
    cookie: String,
}

/// 所有实例共用一个连接池。远端实例走 SSH `-L`，每新开一条 TCP 就要开一个 direct-tcpip 通道，
/// 每次调用都新建 Client 等于每次都重新打洞；空闲时限放短，麦麦重启后旧连接早点丢掉
fn shared_http() -> Result<Client, AppFrameworkError> {
    static HTTP: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    if let Some(http) = HTTP.get() {
        return Ok(http.clone());
    }
    let http = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .pool_idle_timeout(POOL_IDLE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|e| AppFrameworkError::DashboardUnreachable(e.to_string()))?;
    Ok(HTTP.get_or_init(|| http).clone())
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
        Ok(Self {
            base: format!("http://127.0.0.1:{port}"),
            http: shared_http()?,
            cookie: format!("{COOKIE_NAME}={token}"),
        })
    }

    /// `doc` 是这次请求改的是哪份文档，上游 400 时报成它的配置错误；运行期查询不带 doc，400 照原话报
    pub(crate) async fn send(&self, req: Request<'_>) -> Result<Value, AppFrameworkError> {
        let mut builder = self
            .http
            .request(req.method, format!("{}{}", self.base, req.path))
            .header(reqwest::header::COOKIE, &self.cookie)
            .query(req.query);
        if let Some(body) = req.body {
            builder = builder.json(body);
        }
        if let Some(timeout) = req.timeout {
            builder = builder.timeout(timeout);
        }
        let resp = builder.send().await.map_err(transport_err)?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(value);
        }
        Err(failure(status, &value, &text, req.doc))
    }

    /// 图片这类二进制（表情包缩略图、原图）。上游缩略图还在生成时回 202，文件被清理过回 404，
    /// 这两种都不算错，交给调用方决定等一会儿再要还是显示「图没了」
    pub(crate) async fn fetch_bytes(&self, path: &str, query: &[(&str, &str)]) -> Result<Fetched, AppFrameworkError> {
        let resp = self
            .http
            .get(format!("{}{path}", self.base))
            .header(reqwest::header::COOKIE, &self.cookie)
            .query(query)
            .send()
            .await
            .map_err(transport_err)?;
        let status = resp.status();
        if status == StatusCode::ACCEPTED {
            return Ok(Fetched::Pending);
        }
        if status == StatusCode::NOT_FOUND {
            return Ok(Fetched::Missing);
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            return Err(failure(status, &value, &text, None));
        }
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
            .unwrap_or_default();
        let bytes = resp.bytes().await.map_err(transport_err)?;
        Ok(Fetched::Ready { mime, bytes: bytes.to_vec() })
    }

    /// multipart 表单（上传表情包）。上游处理图片要一会儿，按慢请求等
    pub(crate) async fn send_form(&self, path: &str, form: reqwest::multipart::Form) -> Result<Value, AppFrameworkError> {
        let resp = self
            .http
            .post(format!("{}{path}", self.base))
            .header(reqwest::header::COOKIE, &self.cookie)
            .timeout(SLOW_TIMEOUT)
            .multipart(form)
            .send()
            .await
            .map_err(transport_err)?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if status.is_success() {
            return Ok(value);
        }
        Err(failure(status, &value, &text, None))
    }

    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, AppFrameworkError> {
        let value = self.send(Request::new(Method::GET, path).query(query)).await?;
        parse(value, path)
    }

    /// 发出去并按 `T` 收；认不出时报的是这条路径
    pub(crate) async fn call<T: DeserializeOwned>(&self, req: Request<'_>) -> Result<T, AppFrameworkError> {
        let path = req.path;
        parse(self.send(req).await?, path)
    }

    /// 只带改了的键：上游在它自己的进程里合并进当前文件（保注释），和它别处的写入不打架。
    /// 删不掉键；有删除的改动走 [`Self::write_bot_config_raw`]
    pub async fn merge_bot_config(&self, partial: &Value) -> Result<(), AppFrameworkError> {
        let req = Request::new(Method::POST, "/api/webui/config/bot").body(partial).doc("bot_config");
        self.send(req).await.map(|_| ())
    }

    /// 整份原文：上游先按它的类校验，通过了原样写盘
    pub async fn write_bot_config_raw(&self, text: &str) -> Result<(), AppFrameworkError> {
        let body = json!({ "raw_content": text });
        let req = Request::new(Method::POST, "/api/webui/config/bot/raw").body(&body).doc("bot_config");
        self.send(req).await.map(|_| ())
    }

    /// 模型配置整份给：上游要求提供商和模型都不空，改名这类跨表的改动也得一次到位。写完它会自己重载
    pub async fn write_model_config(&self, full: &Value) -> Result<(), AppFrameworkError> {
        let req = Request::new(Method::POST, "/api/webui/config/model").body(full).doc("model_config");
        self.send(req).await.map(|_| ())
    }

    pub(crate) async fn status(&self) -> Result<UpstreamStatus, AppFrameworkError> {
        self.get("/api/webui/system/status", &[]).await
    }

    /// 上游 0.5 秒后让工作进程以 42 退出，bot.py 的外层看到 42 就重新拉起，进程号不变
    pub async fn restart(&self) -> Result<(), AppFrameworkError> {
        self.send(Request::new(Method::POST, "/api/webui/system/restart")).await.map(|_| ())
    }

    pub async fn stats_summary(&self, hours: u32) -> Result<MaiBotStatsSummary, AppFrameworkError> {
        let hours = hours.to_string();
        self.get("/api/webui/statistics/summary", &[("hours", hours.as_str())]).await
    }

    /// 本地聊天这组接口不在 /api/webui 底下
    pub(crate) async fn chat_sessions(&self) -> Result<UpstreamSessions, AppFrameworkError> {
        self.get("/api/chat/sessions", &[("limit", "500")]).await
    }

    /// 盘上已有的提供商按名字查：上游用它自己读到的配置，回环 / 内网地址（本机 Ollama）也放行
    pub(crate) async fn provider_models_by_name(
        &self,
        name: &str,
        gemini: bool,
    ) -> Result<UpstreamModelList, AppFrameworkError> {
        let parser = if gemini { "gemini" } else { "openai" };
        let q = [("provider_name", name), ("parser", parser)];
        let value = self.send(Request::new(Method::GET, "/api/webui/models/list").query(&q).slow()).await?;
        parse(value, "/models/list")
    }

    /// 表单里还没保存的草稿按地址查。上游这条只放行公网地址
    pub(crate) async fn provider_models_by_url(
        &self,
        p: &MaiBotAPIProvider,
    ) -> Result<UpstreamModelList, AppFrameworkError> {
        let parser = if p.client_type == "gemini" { "gemini" } else { "openai" };
        let q = [
            ("base_url", p.base_url.as_str()),
            ("api_key", p.api_key.as_str()),
            ("parser", parser),
            ("endpoint", p.model_list_endpoint.as_str()),
            ("client_type", p.client_type.as_str()),
            ("auth_type", p.auth_type.as_str()),
            ("auth_header_name", p.auth_header_name.as_str()),
            ("auth_header_prefix", p.auth_header_prefix.as_str()),
            ("auth_query_name", p.auth_query_name.as_str()),
        ];
        let req = Request::new(Method::GET, "/api/webui/models/list-by-url").query(&q).slow();
        parse(self.send(req).await?, "/models/list-by-url")
    }

    pub(crate) async fn test_provider_by_name(&self, name: &str) -> Result<UpstreamProviderCheck, AppFrameworkError> {
        let q = [("provider_name", name)];
        let req = Request::new(Method::POST, "/api/webui/models/test-connection-by-name").query(&q).slow();
        parse(self.send(req).await?, "/models/test-connection-by-name")
    }

    pub(crate) async fn test_provider_by_url(
        &self,
        p: &MaiBotAPIProvider,
    ) -> Result<UpstreamProviderCheck, AppFrameworkError> {
        let mut q = vec![("base_url", p.base_url.as_str()), ("client_type", p.client_type.as_str())];
        if !p.api_key.trim().is_empty() {
            q.push(("api_key", p.api_key.as_str()));
        }
        let req = Request::new(Method::GET, "/api/webui/models/test-connection").query(&q).slow();
        parse(self.send(req).await?, "/models/test-connection")
    }

    pub(crate) async fn mcp_status(&self) -> Result<UpstreamMcpStatus, AppFrameworkError> {
        self.get("/api/webui/mcp/status", &[]).await
    }

    pub(crate) async fn mcp_test(&self, server: &Value) -> Result<UpstreamMcpTest, AppFrameworkError> {
        let req = Request::new(Method::POST, "/api/webui/mcp/test").body(server).slow();
        parse(self.send(req).await?, "/mcp/test")
    }
}

/// 一次请求。默认 20 秒超时；探测服务商、试连 MCP 这类上游自己就要等三十秒的用 `slow`
pub(crate) struct Request<'a> {
    method: Method,
    path: &'a str,
    query: &'a [(&'a str, &'a str)],
    body: Option<&'a Value>,
    doc: Option<&'a str>,
    timeout: Option<Duration>,
}

impl<'a> Request<'a> {
    pub(crate) fn new(method: Method, path: &'a str) -> Self {
        Self { method, path, query: &[], body: None, doc: None, timeout: None }
    }

    pub(crate) fn query(mut self, query: &'a [(&'a str, &'a str)]) -> Self {
        self.query = query;
        self
    }

    pub(crate) fn body(mut self, body: &'a Value) -> Self {
        self.body = Some(body);
        self
    }

    pub(crate) fn doc(mut self, doc: &'a str) -> Self {
        self.doc = Some(doc);
        self
    }

    pub(crate) fn slow(mut self) -> Self {
        self.timeout = Some(SLOW_TIMEOUT);
        self
    }

    pub(crate) fn path(&self) -> &'a str {
        self.path
    }
}

/// 二进制请求的结果：缩略图生成中（202）、文件没了（404）和拿到了分开
#[derive(Debug)]
pub(crate) enum Fetched {
    Ready { mime: String, bytes: Vec<u8> },
    Pending,
    Missing,
}

fn transport_err(e: reqwest::Error) -> AppFrameworkError {
    AppFrameworkError::DashboardUnreachable(e.without_url().to_string())
}

/// 上游出错时回 `{"detail": "..."}`；`doc` 是这次改的是哪份配置，400 时报成它的配置错误
fn failure(status: StatusCode, value: &Value, text: &str, doc: Option<&str>) -> AppFrameworkError {
    let detail = value
        .get("detail")
        .map(|d| d.as_str().map_or_else(|| d.to_string(), str::to_string))
        .unwrap_or_else(|| text.chars().take(300).collect());
    match (status, doc) {
        (StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN, _) => AppFrameworkError::DashboardAuth(
            "麦麦 WebUI 不认这个 token，data/webui.json 里的 token 可能被改过".into(),
        ),
        (StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY, Some(doc)) => {
            AppFrameworkError::ConfigInvalid(vec![AppConfigIssue::new(doc, format!("麦麦没收下：{detail}"))])
        }
        // 上游把服务商那边的 401 / 403 换成 502 回来（免得前端当成 WebUI 登录失效），原话就够清楚
        (StatusCode::BAD_REQUEST | StatusCode::BAD_GATEWAY | StatusCode::GATEWAY_TIMEOUT, None) => {
            AppFrameworkError::Integration(detail)
        }
        _ => AppFrameworkError::Integration(format!("麦麦 WebUI 返回 {status}：{detail}")),
    }
}

pub(crate) fn parse<T: DeserializeOwned>(value: Value, path: &str) -> Result<T, AppFrameworkError> {
    serde_json::from_value(value)
        .map_err(|e| AppFrameworkError::Integration(format!("麦麦 WebUI {path} 回的内容认不出：{e}")))
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
