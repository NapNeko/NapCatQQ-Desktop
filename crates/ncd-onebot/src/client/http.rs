//! OneBot HTTP 服务端的动作调用：`POST {base}/{action}`，请求体是参数对象，回包是 OB11 信封。

use std::time::Duration;

use reqwest::{Client, StatusCode, Url};
use serde_json::Value;

use super::body::{BodyError, MAX_BODY_BYTES, read_text_limited};
use super::envelope::truncate_at_char_boundary;
use super::{ClientError, RawCall};

/// 非 2xx 时带回的响应体上限。只用于报错展示，不必完整
const ERROR_BODY_LIMIT: usize = 4 * 1024;

/// 对一个 OneBot HTTP 服务端调用动作。可以随便 `Clone` 共享（内部是连接池）
#[derive(Clone)]
pub struct HttpActionClient {
    client: Client,
    base: Url,
    token: Option<String>,
}

/// 手写 Debug 而不是 derive：令牌不能出现在任何日志 / panic 信息里，
/// 基地址里若带了 `user:pass@` 也一并去掉
impl std::fmt::Debug for HttpActionClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut base = self.base.clone();
        // 这两个 setter 只在「不能作为基地址」的 URL 上失败，而 `new` 已经排除了
        let _ = base.set_username("");
        let _ = base.set_password(None);
        f.debug_struct("HttpActionClient")
            .field("base_url", &base.as_str().trim_end_matches('/'))
            .field(
                "token",
                &if self.token.is_some() {
                    "<set>"
                } else {
                    "<none>"
                },
            )
            .finish_non_exhaustive()
    }
}

impl HttpActionClient {
    /// `base_url` 形如 `http://127.0.0.1:3000`，末尾的 `/` 会被忽略；令牌走 `Authorization: Bearer`。
    ///
    /// 显式 `no_proxy`：目标基本是本机端口或 SSH 隧道映射出来的本机端口，
    /// 系统代理（尤其是全局代理软件）会把它们绕到外面去。
    pub fn new(base_url: &str, token: Option<String>) -> Result<Self, ClientError> {
        let base = Url::parse(base_url.trim())
            .map_err(|e| ClientError::Connect(format!("地址无效：{e}")))?;
        if !matches!(base.scheme(), "http" | "https") || base.host_str().is_none() {
            return Err(ClientError::Connect(
                "地址无效：需要 http:// 或 https:// 开头的完整地址".to_owned(),
            ));
        }
        let client = Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| ClientError::Connect(e.without_url().to_string()))?;
        Ok(Self {
            client,
            base,
            token: token.filter(|t| !t.is_empty()),
        })
    }

    /// 调用一个动作。`timeout` 覆盖整个请求（连接、等待、读完响应体）。
    ///
    /// 401 / 403 → [`ClientError::Unauthorized`]；其它非 2xx → [`ClientError::Status`]；
    /// 2xx 但响应体不是 JSON 对象 → [`ClientError::Protocol`]。
    /// OB11 自己的失败（retcode 非 0）仍是 2xx，照常作为 `RawCall` 返回，由调用方判断。
    pub async fn call(
        &self,
        action: &str,
        params: &Value,
        timeout: Duration,
    ) -> Result<RawCall, ClientError> {
        if action.is_empty() {
            return Err(ClientError::Protocol("动作名不能为空".to_owned()));
        }
        let url = self.action_url(action)?;
        let mut request = self.client.post(url).json(params).timeout(timeout);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await.map_err(map_transport_error)?;
        let status = response.status();
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            return Err(ClientError::Unauthorized(status.as_u16()));
        }
        // 读响应体也受 `timeout` 约束；失败一并归到传输层错误。超过上限的不读完
        let text = read_text_limited(response, MAX_BODY_BYTES)
            .await
            .map_err(|err| match err {
                BodyError::Read(err) => map_transport_error(err),
                too_large @ BodyError::TooLarge { .. } => {
                    ClientError::Protocol(too_large.to_string())
                }
            })?;
        if !status.is_success() {
            return Err(ClientError::Status {
                status: status.as_u16(),
                body: truncate_at_char_boundary(&text, ERROR_BODY_LIMIT).to_owned(),
            });
        }

        let value: Value = serde_json::from_str(&text)
            .map_err(|e| ClientError::Protocol(format!("响应不是合法 JSON：{e}")))?;
        if !value.is_object() {
            return Err(ClientError::Protocol("响应不是 JSON 对象".to_owned()));
        }
        Ok(RawCall { text, value })
    }

    /// 把动作名作为一个路径段追加到基地址后面：动作名里若混进 `/`、`?`、`#` 会被转义，
    /// 不会悄悄改变请求路径
    fn action_url(&self, action: &str) -> Result<Url, ClientError> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|()| ClientError::Connect("地址无效：不能作为基地址".to_owned()))?
            .pop_if_empty()
            .push(action);
        Ok(url)
    }
}

/// reqwest 的错误信息里会带完整 URL；这里去掉，避免哪天基地址里混进凭据后被原样展示
fn map_transport_error(err: reqwest::Error) -> ClientError {
    if err.is_timeout() {
        ClientError::Timeout
    } else {
        ClientError::Connect(err.without_url().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(base: &str) -> HttpActionClient {
        HttpActionClient::new(base, None).unwrap()
    }

    #[test]
    fn action_url_ignores_trailing_slash_and_keeps_prefix() {
        for base in ["http://127.0.0.1:3000", "http://127.0.0.1:3000/"] {
            let url = client(base).action_url("get_login_info").unwrap();
            assert_eq!(url.as_str(), "http://127.0.0.1:3000/get_login_info");
        }
        let url = client("http://h:1/onebot/").action_url("send_msg").unwrap();
        assert_eq!(url.as_str(), "http://h:1/onebot/send_msg");
    }

    #[test]
    fn action_name_cannot_change_the_path() {
        let url = client("http://h:1").action_url("a/b?c#d").unwrap();
        assert_eq!(url.path(), "/a%2Fb%3Fc%23d");
        assert_eq!(url.query(), None);
    }

    #[test]
    fn debug_output_never_shows_the_token_or_url_credentials() {
        let client = HttpActionClient::new(
            "http://user:hunter2@127.0.0.1:3000/",
            Some("s3cret-token".into()),
        )
        .unwrap();
        let shown = format!("{client:?}");
        assert!(shown.contains("http://127.0.0.1:3000"), "{shown}");
        assert!(shown.contains("<set>"), "{shown}");
        for secret in ["s3cret-token", "hunter2", "user"] {
            assert!(
                !shown.contains(secret),
                "Debug 输出泄露了 {secret}：{shown}"
            );
        }
        let anonymous = format!("{:?}", HttpActionClient::new("http://h:1", None).unwrap());
        assert!(anonymous.contains("<none>"), "{anonymous}");
    }

    #[test]
    fn invalid_base_urls_are_rejected() {
        for bad in [
            "",
            "not a url",
            "ftp://h/",
            "mailto:a@b.c",
            "127.0.0.1:3000",
        ] {
            assert!(
                matches!(
                    HttpActionClient::new(bad, None),
                    Err(ClientError::Connect(_))
                ),
                "{bad:?} 应被拒绝"
            );
        }
    }
}
