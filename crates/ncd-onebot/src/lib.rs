//! OneBot 11 调试台的协议层：动作目录（NapCat / SnowLuma 两种文档格式的统一模型），
//! 以及调用与收事件用的协议客户端（HTTP / WebSocket / SSE）、重连退避和事件环形缓冲。
//!
//! 只依赖 `ncd-domain` 的数据类型，不认识 Bot、通道或 Tauri —— 谁来调用、连到哪
//! 由上层的运行时决定。

pub mod backoff;
pub mod catalog;
pub mod client;
pub mod ring;

/// `client::sse` 的别名，上层习惯直接写 `ncd_onebot::sse::SseParser`
pub use client::sse;
