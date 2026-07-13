//! 远端 NapCat Native 会话：隧道 + 日志 tail + 启动规划

mod decision;
pub mod launch;
mod session;
mod tunnel_io;

pub use session::{RemoteNativeNapcatSession, RemoteNativeNapcatSessionRegistry, WebuiUnreachableHook};

#[cfg(test)]
mod tests;

