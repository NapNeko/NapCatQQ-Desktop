//! MCP HTTP 入口：先验证来源与凭据，再受限读取请求体。

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::to_bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header, uri::Authority};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::Value;
use subtle::ConstantTimeEq;

use crate::rpc;
use crate::server::ServerCtx;

pub(crate) const MAX_CONCURRENT_REQUESTS: usize = 16;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const BODY_READ_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn router(ctx: Arc<ServerCtx>) -> Router {
    Router::new()
        .route("/mcp", post(handle_post))
        .layer(middleware::from_fn_with_state(ctx.clone(), guard))
        .with_state(ctx)
}

async fn guard(State(ctx): State<Arc<ServerCtx>>, request: Request, next: Next) -> Response {
    let mut response = guarded_request(&ctx, request, next).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // 单次连接只承载一次调用，避免后续调用落入前一连接的截止时间。
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("close"));
    response
}

async fn guarded_request(ctx: &ServerCtx, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if !host_allowed(headers, ctx.bound_port)
        || request
            .uri()
            .authority()
            .is_some_and(|authority| !authority_allowed(authority.as_str(), ctx.bound_port))
        || !origin_allowed(headers, ctx.bound_port)
    {
        return (StatusCode::FORBIDDEN, "仅允许当前 MCP 回环地址").into_response();
    }
    if !authorized(headers, &ctx.token) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "需要 Bearer token",
        )
            .into_response();
    }
    if ctx.cancel.is_cancelled() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "MCP 服务已停止，请重新连接",
        )
            .into_response();
    }
    if headers.contains_key("mcp-protocol-version")
        && !single_header(headers, "mcp-protocol-version")
            .is_some_and(rpc::protocol_version_supported)
    {
        return (StatusCode::BAD_REQUEST, "不支持的 MCP-Protocol-Version").into_response();
    }
    let Ok(_permit) = ctx.requests.try_acquire() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            "MCP 正在处理其他请求，请稍后重试",
        )
            .into_response();
    };
    let epoch = ctx.tools.gate.epoch();
    tokio::select! {
        biased;
        _ = ctx.cancel.cancelled() => revoked_response(),
        _ = epoch.cancelled() => revoked_response(),
        response = next.run(request) => {
            if ctx.cancel.is_cancelled() || epoch.is_cancelled() {
                revoked_response()
            } else {
                response
            }
        }
    }
}

fn revoked_response() -> Response {
    (
        StatusCode::CONFLICT,
        "MCP 服务、权限或凭据已改变。已提交的操作可能已经执行；请先核实结果，勿自动重试。",
    )
        .into_response()
}

async fn handle_post(State(ctx): State<Arc<ServerCtx>>, request: Request) -> Response {
    let headers = request.headers();
    if !content_type_is_json(headers) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Content-Type 必须是 application/json",
        )
            .into_response();
    }
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > MAX_BODY_BYTES as u64)
    {
        return (StatusCode::PAYLOAD_TOO_LARGE, "MCP 请求体不得超过 1 MiB").into_response();
    }
    let body = tokio::select! {
        _ = ctx.cancel.cancelled() => {
            return (StatusCode::SERVICE_UNAVAILABLE, "MCP 服务已停止").into_response();
        }
        result = tokio::time::timeout(
            BODY_READ_TIMEOUT,
            to_bytes(request.into_body(), MAX_BODY_BYTES),
        ) => match result {
            Ok(Ok(body)) => body,
            Ok(Err(_)) => {
                return (StatusCode::PAYLOAD_TOO_LARGE, "请求体过大或无法读取").into_response();
            }
            Err(_) => return (StatusCode::REQUEST_TIMEOUT, "读取 MCP 请求体超时").into_response(),
        }
    };
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(rpc::parse_error_frame()),
        )
            .into_response();
    };
    match rpc::handle_body(&ctx, &value).await {
        Some(answer)
            if answer["error"]["code"] == -32600
                || (answer["id"].is_null() && answer["error"]["code"] == -32602) =>
        {
            (StatusCode::BAD_REQUEST, axum::Json(answer)).into_response()
        }
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
    bool::from(a.ct_eq(b))
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(first)
}

fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    let Some(value) = single_header(headers, header::AUTHORIZATION.as_str()) else {
        return false;
    };
    // scheme 大小写不敏感（RFC 7235），token 本身严格相等
    let (scheme, provided) = value.split_once(' ').unwrap_or(("", ""));
    scheme.eq_ignore_ascii_case("bearer") && tokens_equal(provided, expected)
}

fn authority_allowed(value: &str, port: u16) -> bool {
    let Ok(authority) = value.parse::<Authority>() else {
        return false;
    };
    let requested_port = match authority.port() {
        Some(value) => value.as_str().parse::<u16>().ok(),
        None => Some(80),
    };
    !value.contains('@')
        && !value.ends_with(':')
        && (authority.host().eq_ignore_ascii_case("localhost") || authority.host() == "127.0.0.1")
        && requested_port == Some(port)
}

fn host_allowed(headers: &HeaderMap, port: u16) -> bool {
    single_header(headers, header::HOST.as_str()).is_some_and(|host| authority_allowed(host, port))
}

/// 无 Origin 的原生客户端仍须通过 Host 和 Bearer；有 Origin 时只接受同端口回环源。
fn origin_allowed(headers: &HeaderMap, port: u16) -> bool {
    if !headers.contains_key(header::ORIGIN) {
        return true;
    }
    let Some(origin) = single_header(headers, header::ORIGIN.as_str()) else {
        return false;
    };
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    uri.scheme_str() == Some("http")
        && uri.authority().is_some_and(|authority| {
            authority_allowed(authority.as_str(), port) && origin == format!("http://{authority}")
        })
}

fn content_type_is_json(headers: &HeaderMap) -> bool {
    single_header(headers, header::CONTENT_TYPE.as_str())
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
            .header(header::HOST, "127.0.0.1:3999")
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
            .oneshot(post(
                "/mcp",
                None,
                r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(no_token.status(), StatusCode::UNAUTHORIZED);

        let wrong_token = app
            .clone()
            .oneshot(post(
                "/mcp",
                Some("nope"),
                r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            ))
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
                    .header(header::HOST, "127.0.0.1:3999")
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
                    .header(header::HOST, "127.0.0.1:3999")
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
                    .header(header::HOST, "127.0.0.1:3999")
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
        assert!(origin_allowed(&headers, 3999));
        for origin in ["http://localhost:3999", "http://127.0.0.1:3999"] {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
            assert!(origin_allowed(&headers, 3999));
        }
        for origin in [
            "http://localhost:3210",
            "https://example.com",
            "null",
            "localhost:3999",
            "http://localhost:3999/",
            "http://localhost:3999@evil.example",
            "http://localhost:3999?x=1",
            "http://[::1]:3999",
            "https://127.0.0.1:3999",
        ] {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
            assert!(!origin_allowed(&headers, 3999), "{origin}");
        }
        headers.insert(header::ORIGIN, "http://localhost:3999".parse().unwrap());
        headers.append(header::ORIGIN, "http://localhost:3999".parse().unwrap());
        assert!(!origin_allowed(&headers, 3999));
        headers.remove(header::ORIGIN);
        headers.insert(header::ORIGIN, HeaderValue::from_bytes(&[0xff]).unwrap());
        assert!(!origin_allowed(&headers, 3999));
    }

    #[tokio::test]
    async fn host_and_auth_headers_are_unambiguous() {
        let app = router(test_server_ctx());
        for host in [
            None,
            Some("evil.example:3999"),
            Some("127.0.0.1:4000"),
            Some("localhost:3999@evil.example"),
        ] {
            let mut request = post(
                "/mcp",
                Some("test-token"),
                r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            );
            request.headers_mut().remove(header::HOST);
            if let Some(host) = host {
                request
                    .headers_mut()
                    .insert(header::HOST, host.parse().unwrap());
            }
            assert_eq!(
                app.clone().oneshot(request).await.unwrap().status(),
                StatusCode::FORBIDDEN
            );
        }
        let mut request = post("/mcp", Some("test-token"), "{}");
        request
            .headers_mut()
            .append(header::AUTHORIZATION, "Bearer test-token".parse().unwrap());
        assert_eq!(
            app.oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn rejects_oversized_bodies_after_authentication() {
        let app = router(test_server_ctx());
        let large = " ".repeat(MAX_BODY_BYTES + 1);
        let unauthorized = app
            .clone()
            .oneshot(post("/mcp", None, &large))
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .oneshot(post("/mcp", Some("test-token"), &large))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }

    #[tokio::test]
    async fn overload_and_shutdown_fail_closed() {
        let ctx = test_server_ctx();
        let permit = ctx
            .requests
            .acquire_many(MAX_CONCURRENT_REQUESTS as u32)
            .await
            .unwrap();
        let app = router(ctx.clone());
        let response = app
            .clone()
            .oneshot(post("/mcp", Some("test-token"), "{}"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
        drop(permit);
        ctx.cancel.cancel();
        let response = app
            .oneshot(post("/mcp", Some("test-token"), "{}"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn revocation_drops_already_authenticated_work_and_releases_capacity() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::sync::Notify;

        struct Dropped(Arc<AtomicBool>);
        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        for revoke_service in [false, true] {
            let ctx = test_server_ctx();
            let entered = Arc::new(Notify::new());
            let dropped = Arc::new(AtomicBool::new(false));
            let app = Router::new()
                .route(
                    "/mcp",
                    axum::routing::post({
                        let entered = entered.clone();
                        let dropped = dropped.clone();
                        move || {
                            let entered = entered.clone();
                            let dropped = dropped.clone();
                            async move {
                                let _drop = Dropped(dropped);
                                entered.notify_one();
                                std::future::pending::<StatusCode>().await
                            }
                        }
                    }),
                )
                .layer(middleware::from_fn_with_state(ctx.clone(), guard));
            let request = tokio::spawn(app.oneshot(post("/mcp", Some("test-token"), "{}")));
            tokio::time::timeout(Duration::from_secs(1), entered.notified())
                .await
                .unwrap();
            assert_eq!(
                ctx.requests.available_permits(),
                MAX_CONCURRENT_REQUESTS - 1
            );
            if revoke_service {
                ctx.cancel.cancel();
            } else {
                ctx.tools.gate.set_policy(false, false);
            }
            let response = tokio::time::timeout(Duration::from_secs(1), request)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert_eq!(response.headers()[header::CONNECTION], "close");
            assert!(dropped.load(Ordering::SeqCst));
            assert_eq!(ctx.requests.available_permits(), MAX_CONCURRENT_REQUESTS);
            let text = response.into_body().collect().await.unwrap().to_bytes();
            assert!(String::from_utf8_lossy(&text).contains("勿自动重试"));
        }
    }

    #[tokio::test]
    async fn absolute_target_and_ambiguous_host_cannot_bypass_origin_checks() {
        let app = router(test_server_ctx());
        for target in ["http://evil.example:3999/mcp", "http://127.0.0.1:4000/mcp"] {
            let response = app
                .clone()
                .oneshot(post(target, Some("test-token"), "{}"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        let mut request = post("/mcp", Some("test-token"), "{}");
        request
            .headers_mut()
            .append(header::HOST, "localhost:3999".parse().unwrap());
        assert_eq!(
            app.oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        for host in [
            "localhost.:3999",
            "127.1:3999",
            "2130706433:3999",
            "127.0.0.1:3999,evil.example",
            "127.0.0.1:",
        ] {
            assert!(!authority_allowed(host, 3999), "{host}");
        }
    }

    #[tokio::test]
    async fn validates_protocol_and_rejects_batches() {
        let app = router(test_server_ctx());
        let mut request = post("/mcp", Some("test-token"), "{}");
        request
            .headers_mut()
            .insert("mcp-protocol-version", "not-a-version".parse().unwrap());
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        let request = post(
            "/mcp",
            Some("test-token"),
            r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#,
        );
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_json(response).await["error"]["code"], -32600);
    }
}
