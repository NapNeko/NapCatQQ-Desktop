//! 内嵌终端：会话表、回放、流控，以及「终端开到哪」的规划
//!
//! 会话活在桌面端后端，和网页无关：切页面、收起面板、轻量模式销毁网页都不断，前端回来重新
//! attach，先收一遍回放再接实时输出。关标签或退出桌面端才真正结束。
//! 本机走 ConPTY、远端走 SSH 的 pty 通道，都在 `ncd_host::Host::open_pty` 后面。

mod external;
mod files;
mod integration;
mod manager;
mod plan;
mod replay;
mod shells;
mod stats;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[cfg(windows)]
mod local_smoke;

pub use manager::{MAX_SESSIONS, TerminalManager, TerminalSink};
pub use plan::{DesktopTerminalPlanner, TerminalLaunchPlan, TerminalPlanner};
pub use shells::detect_local_shells;

#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("这个终端已经关了")]
    NotFound,
    #[error("终端里的程序已经结束，按回车重新打开")]
    NotRunning,
    #[error("终端开得太多了（最多 {0} 个），先关掉几个")]
    TooMany(usize),
    /// 目标解析不出来：Bot / 实例不存在、主机连不上
    #[error("{0}")]
    Plan(String),
    /// 主机上的操作失败
    #[error("{0}")]
    Host(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Invalid(String),
}
