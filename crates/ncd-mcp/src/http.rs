//! HTTP 层：MCP Streamable HTTP 的最小实现——单一 `POST /mcp` 端点，应答一律 JSON；
//! 纯通知回 202 空身；GET / 其它方法 / 其它路径分别由路由落成 405 / 404（不开 SSE：
//! 事件走 read_events 轮询，客户端规范允许服务端不提供事件流）。

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::Value;

use crate::rpc;
use crate::server::ServerCtx;

pub(crate) fn router(ctx: Arc<ServerCtx>) -> Router {
    Router::new().route("/mcp", post(handle_post)).with_state(ctx)
}

async fn handle_post(State(ctx): State<Arc<ServerCtx>>, headers: HeaderMap, body: Bytes) -> Response {
    if !origin_allowed(&headers) {
        return (StatusCode::FORBIDDEN, "Origin 不在本机域内").into_response();
    }
    if !authorized(&headers, &ctx.token) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "需要 Bearer token",
        )
            .into_response();
    }
    if !content_type_is_json(&headers) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Content-Type 必须是 application/json",
        )
            .into_response();
    }
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return (StatusCode::BAD_REQUEST, axum::Json(rpc::parse_error_frame())).into_response();
    };
    match rpc::handle_body(&ctx, &value).await {
        Some(answer) => axum::Json(answer).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// 常量时间比较，别让匹配时长透出 token 的前缀长度
fn tokens_equal(provided: &str, expected: &str) -> bool {
    let (a, b) = (provided.as_bytes(), expected.as_bytes());
    if a.is_empty() || a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    // scheme 大小写不敏感（RFC 7235），token 本身严格相等
    let (scheme, provided) = value.split_once(' ').unwrap_or(("", ""));
    scheme.eq_ignore_ascii_case("bearer") && tokens_equal(provided, expected)
}

/// 挡 DNS 重绑：浏览器站点的跨域 POST 会自带 Origin，只放明确本机的；
/// 原生客户端（agent）一般不带 Origin，由 Bearer token 兜着
fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let after_scheme = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or(origin);
    let authority = after_scheme.split('/').next().unwrap_or("");
    let host = authority.split(':').next().unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

fn content_type_is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::tests_support::test_server_ctx;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    fn post(path: &str, token: Option<&str>, body: &str) -> axum::http::Request<Body> {
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        request.body(Body::from(body.to_owned())).unwrap()
    }

    async fn body_json(response: Response) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn auth_required_and_wrong_paths() {
        let app = router(test_server_ctx());

        let no_token = app
            .clone()
            .oneshot(post("/mcp", None, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
            .await
            .unwrap();
        assert_eq!(no_token.status(), StatusCode::UNAUTHORIZED);

        let wrong_token = app
            .clone()
            .oneshot(post("/mcp", Some("nope"), r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
            .await
            .unwrap();
        assert_eq!(wrong_token.status(), StatusCode::UNAUTHORIZED);

        let wrong_path = app
            .clone()
            .oneshot(post("/nope", Some("test-token"), r#"{}"#))
            .await
            .unwrap();
        assert_eq!(wrong_path.status(), StatusCode::NOT_FOUND);

        let get = app
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/mcp")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn ping_pong_and_notification_accepted() {
        let app = router(test_server_ctx());

        let ping = app
            .clone()
            .oneshot(post(
                "/mcp",
                Some("test-token"),
                r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(ping.status(), StatusCode::OK);
        let answer = body_json(ping).await;
        assert_eq!(answer["result"], json!({}));

        let notification = app
            .clone()
            .oneshot(post(
                "/mcp",
                Some("test-token"),
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(notification.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn bad_content_type_and_bad_json() {
        let app = router(test_server_ctx());

        let wrong_type = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(Body::from("hi"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(wrong_type.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let bad_json = app
            .oneshot(post("/mcp", Some("test-token"), "{nope"))
            .await
            .unwrap();
        assert_eq!(bad_json.status(), StatusCode::BAD_REQUEST);
        let answer = body_json(bad_json).await;
        assert_eq!(answer["error"]["code"], -32700);
    }

    #[tokio::test]
    async fn foreign_origin_rejected() {
        let app = router(test_server_ctx());
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ORIGIN, "https://evil.example.com")
                    .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn helpers_compare_tokens_and_origins() {
        assert!(tokens_equal("abc", "abc"));
        assert!(!tokens_equal("abc", "abd"));
        assert!(!tokens_equal("abc", "abcd"));
        assert!(!tokens_equal("", ""));

        let mut headers = HeaderMap::new();
        assert!(origin_allowed(&headers));
        headers.insert(header::ORIGIN, "http://localhost:3210".parse().unwrap());
        assert!(origin_allowed(&headers));
        headers.insert(header::ORIGIN, "https://127.0.0.1".parse().unwrap());
        assert!(origin_allowed(&headers));
        headers.insert(header::ORIGIN, "https://example.com".parse().unwrap());
        assert!(!origin_allowed(&headers));
    }
}
