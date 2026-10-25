//! 助手沿用全局 MCP 的来源校验，但只路由当前会话的工具。

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::to_bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::Value;

use super::{SessionCtx, rpc};
use crate::http as common;

const BODY_LIMIT: usize = 256 * 1024;
const BODY_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(110);

pub(super) fn router(ctx: Arc<SessionCtx>) -> Router {
    Router::new()
        .route("/mcp", post(handle_post))
        .layer(middleware::from_fn_with_state(ctx.clone(), guard))
        .with_state(ctx)
}

async fn guard(State(ctx): State<Arc<SessionCtx>>, request: Request, next: Next) -> Response {
    let mut response = guarded(&ctx, request, next).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert(header::CONNECTION, HeaderValue::from_static("close"));
    response
}

async fn guarded(ctx: &SessionCtx, request: Request, next: Next) -> Response {
    let port = ctx.server.bound_port;
    let headers = request.headers();
    if !common::host_allowed(headers, port)
        || !common::origin_allowed(headers, port)
        || request
            .uri()
            .authority()
            .is_some_and(|value| !common::authority_allowed(value.as_str(), port))
    {
        return (StatusCode::FORBIDDEN, "仅允许当前诊断工具的回环地址").into_response();
    }
    if !common::authorized(headers, &ctx.server.token) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "诊断工具需要有效会话凭据",
        )
            .into_response();
    }
    if ctx.server.cancel.is_cancelled() {
        return (StatusCode::GONE, "诊断会话已结束").into_response();
    }
    if headers.contains_key("mcp-protocol-version")
        && !common::single_header(headers, "mcp-protocol-version")
            .is_some_and(crate::rpc::protocol_version_supported)
    {
        return (StatusCode::BAD_REQUEST, "不支持的 MCP-Protocol-Version").into_response();
    }
    let Ok(_permit) = ctx.server.requests.try_acquire() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
            "诊断工具正在处理其他请求",
        )
            .into_response();
    };
    let epoch = ctx.server.tools.gate.epoch();
    tokio::select! {
        biased;
        _ = ctx.server.cancel.cancelled() => revoked(),
        _ = epoch.cancelled() => revoked(),
        result = tokio::time::timeout(REQUEST_TIMEOUT, next.run(request)) => {
            match result {
                Ok(response) if !ctx.server.cancel.is_cancelled() && !epoch.is_cancelled() => response,
                Ok(_) => revoked(),
                Err(_) => (StatusCode::GATEWAY_TIMEOUT,
                    "诊断工具超时。已提交的操作可能已执行，请先核实结果，勿自动重试。")
                    .into_response(),
            }
        }
    }
}

fn revoked() -> Response {
    (
        StatusCode::GONE,
        "诊断会话已结束。已提交的操作可能已执行，请先核实结果，勿自动重试。",
    )
        .into_response()
}

async fn handle_post(State(ctx): State<Arc<SessionCtx>>, request: Request) -> Response {
    if !common::content_type_is_json(request.headers()) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Content-Type 必须是 application/json",
        )
            .into_response();
    }
    if request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|value| value > BODY_LIMIT)
    {
        return (StatusCode::PAYLOAD_TOO_LARGE, "诊断工具请求体过大").into_response();
    }
    let body =
        match tokio::time::timeout(BODY_TIMEOUT, to_bytes(request.into_body(), BODY_LIMIT)).await {
            Ok(Ok(body)) => body,
            Ok(Err(_)) => {
                return (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "诊断工具请求体过大或无法读取",
                )
                    .into_response();
            }
            Err(_) => return (StatusCode::REQUEST_TIMEOUT, "读取诊断请求体超时").into_response(),
        };
    let value = match serde_json::from_slice::<Value>(&body) {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(crate::rpc::parse_error_frame()),
            )
                .into_response();
        }
    };
    match rpc::handle_body(&ctx, &value).await {
        Some(answer) if answer["error"]["code"] == -32600 => {
            (StatusCode::BAD_REQUEST, axum::Json(answer)).into_response()
        }
        Some(answer) => axum::Json(answer).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}
