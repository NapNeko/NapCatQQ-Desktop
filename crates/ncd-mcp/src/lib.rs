//! 本机 MCP 运维与 OneBot 调试入口；桌面管理权限，密钥保存在 SecretStore。

mod assistant;
mod gate;
mod http;
mod inspection;
mod rpc;
mod security;
mod server;
#[cfg(test)]
mod tests_support;
mod tools;

pub use assistant::AssistantMcpHost;
pub use inspection::McpInspection;
pub use server::McpServer;
