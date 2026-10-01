//! 调用与收事件用的协议客户端：HTTP 动作调用、WebSocket（按 echo 配对回包并转发事件）、
//! SSE 增量解析，以及它们共用的回包信封处理。
//!
//! 这一层只管「把一个动作发出去、把回包原样拿回来」，不认识 Bot、通道选择和重连策略 ——
//! 那些由上层运行时决定。

pub mod body;
pub mod envelope;
pub mod http;
pub mod sse;
pub mod ws;

use serde_json::Value;

pub use body::{BodyError, MAX_BODY_BYTES, read_body_limited, read_text_limited};
pub use envelope::{
    Ob11Reply, RAW_PREVIEW_LIMIT, RESPONSE_INLINE_LIMIT, outcome_from, parse_ob11_reply,
};
pub use http::HttpActionClient;
pub use sse::{SseFrame, SseParser, SseTooLarge};
pub use ws::{WsClient, WsStreamCall, connect_ws};

/// 客户端层的失败。都是「没拿到回包」的情形；拿到了回包但 retcode 非 0 不算错误
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("超时")]
    Timeout,
    #[error("连接失败：{0}")]
    Connect(String),
    /// 上游用 401 / 403 拒绝了令牌；数字是原始状态码
    #[error("鉴权失败（{0}）")]
    Unauthorized(u16),
    #[error("HTTP {status}")]
    Status { status: u16, body: String },
    #[error("协议错误：{0}")]
    Protocol(String),
    /// 等回包期间连接断了。请求已经交给了套接字，上游可能执行过
    #[error("连接已关闭")]
    Closed,
    /// 请求还没写出去连接就关了：上游根本没收到，换一条连接重发是安全的
    #[error("连接在请求发出前已关闭")]
    NotSent,
}

/// 一次调用拿到的原始回包：文本用来算体积、做截断，解析后的值给上层继续用
#[derive(Debug, Clone, PartialEq)]
pub struct RawCall {
    pub text: String,
    pub value: Value,
}
