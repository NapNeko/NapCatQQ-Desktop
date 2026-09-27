//! 试聊：桌面端自己的聊天窗口连麦麦 WebUI 的统一 WebSocket（`/api/webui/ws`）。
//! 连接由页面直接开，这里只用 Cookie 换一张一次性握手票（上游 60 秒内有效、用一次就废），
//! WebUI 的会话 token 不进页面；远端实例给的是编排层开的隧道在本机的口。

use ncd_traits::AppFrameworkError;
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{MaiBotResourceDone, UpstreamMessage};
use crate::maibot::webui_client::{MaiBotWebUi, Request};

/// 桌面端在麦麦那边的身份。上游加前缀存成 `webui_user_ncd_desktop` 的 WebUI 私聊，
/// 同一台麦麦上每次打开都接着同一段对话，清空记录也按它
const CHAT_USER_ID: &str = "ncd_desktop";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotChatTicket {
    /// `ws://127.0.0.1:<口>/api/webui/ws?token=…`，拿到就连，过一分钟就作废
    pub url: String,
    /// 开会话时带上
    pub user_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamWsToken {
    success: Option<bool>,
    token: Option<String>,
    message: Option<String>,
}

pub(crate) async fn ticket(c: &MaiBotWebUi, port: u16) -> Result<MaiBotChatTicket, AppFrameworkError> {
    // token 不对时上游回 200 + success=false（怕前端见 401 就刷新），要自己拆
    let up: UpstreamWsToken = c.get("/api/webui/ws-token", &[]).await?;
    let token = up
        .token
        .filter(|t| up.success == Some(true) && !t.is_empty())
        .ok_or_else(|| {
            AppFrameworkError::DashboardAuth(format!(
                "麦麦没给聊天连接的票：{}",
                up.message.filter(|m| !m.is_empty()).unwrap_or_else(|| "没说原因".into())
            ))
        })?;
    let url = Url::parse_with_params(&format!("ws://127.0.0.1:{port}/api/webui/ws"), &[("token", token.as_str())])
        .map_err(|e| AppFrameworkError::Integration(format!("拼聊天连接地址失败：{e}")))?;
    Ok(MaiBotChatTicket { url: url.into(), user_id: CHAT_USER_ID.into() })
}

/// 清掉桌面端这段私聊的记录。麦麦对这个人的印象、学到的东西不跟着清
pub(crate) async fn clear_history(c: &MaiBotWebUi) -> Result<MaiBotResourceDone, AppFrameworkError> {
    let req = Request::new(Method::DELETE, "/api/chat/history").query(&[("user_id", CHAT_USER_ID)]);
    let up: UpstreamMessage = c.call(req).await?;
    Ok(MaiBotResourceDone {
        affected: up.deleted_count.unwrap_or(0),
        message: up.message.filter(|m| !m.is_empty()).unwrap_or_else(|| "记录清空了".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> MaiBotWebUi {
        MaiBotWebUi::connect(server.address().port(), "tok").unwrap()
    }

    #[tokio::test]
    async fn ticket_points_at_the_same_port_with_a_one_time_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/webui/ws-token"))
            .and(header("cookie", "maibot_session=tok"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"success":true,"token":"Ab-c_9","expires_in":60}"#,
            ))
            .mount(&server)
            .await;
        let t = ticket(&client(&server), 18001).await.unwrap();
        assert_eq!(t.url, "ws://127.0.0.1:18001/api/webui/ws?token=Ab-c_9");
        assert_eq!(t.user_id, CHAT_USER_ID);
    }

    #[tokio::test]
    async fn refused_ticket_is_an_auth_error_with_upstream_words() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/webui/ws-token"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"success":false,"message":"认证已过期，请重新登录","token":null,"expires_in":0}"#,
            ))
            .mount(&server)
            .await;
        let err = ticket(&client(&server), 18001).await.unwrap_err();
        assert!(matches!(&err, AppFrameworkError::DashboardAuth(m) if m.contains("认证已过期")), "{err}");
    }

    #[tokio::test]
    async fn clear_targets_the_desktop_user_only() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/chat/history"))
            .and(query_param("user_id", CHAT_USER_ID))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"success":true,"message":"已清空 12 条聊天记录"}"#))
            .expect(1)
            .mount(&server)
            .await;
        let done = clear_history(&client(&server)).await.unwrap();
        assert_eq!(done.message, "已清空 12 条聊天记录");
    }
}
