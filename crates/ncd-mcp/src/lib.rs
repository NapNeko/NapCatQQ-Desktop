//! OneBot 调试台的 MCP 服务：把 `DebugManager` 的能力按 MCP（Streamable HTTP）
//! 开给本机的 agent。只绑 127.0.0.1，Bearer token 鉴权，token 存 SecretStore。
//!
//! 权限闸按工具分级：只读放行；有副作用发一次性确认令牌（挑战-重放）；
//! 危险级默认拒绝（设置 `mcp.allowDangerous` 打开后降级为要确认）。
//! 服务自身只读 Bot 配置、往调试台发调用，不改 Bot 的任何东西。
//!
//! 不用 rmcp：MCP 这边只需要 initialize / ping / tools/list / tools/call 四个方法，
//! 手写 JSON-RPC 比引整条 SDK 的依赖面小，行为也好控。

mod gate;
mod http;
mod rpc;
mod server;
mod tools;
#[cfg(test)]
mod tests_support;

pub use server::{McpApplyOutcome, McpServer};
