//! `HostProcess`:跨平台进程句柄抽象。
//!
//! 设计要点:
//! - 本地 Host 实装包 `tokio::process::Child`
//! - 远端 Host 实装包 SSH channel(由 `russh::Channel` 间接持有)
//! - 不直接暴露平台原生 PID(因为远端 Linux 的 PID 跟本地 Windows 不在同一空间),
//!   通过 `ProcessId` newtype 表达,内含原生数值 + 来源主机标识
//! - `wait()` 一次性消费句柄返回退出结果;长流式读由 Host 实装在 spawn 时通过回调注入

use std::fmt;

use tokio::io::AsyncRead;

use crate::command::CommandOutput;
use crate::error::HostError;

/// 进程 ID(平台原生数值 + 来源主机标识)。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessId {
    /// 原生进程 ID(Windows / Linux 都是 u32 / pid_t,这里统一用 u32)
    pub native: u32,
    /// 来源主机标识(local / remote-<server-id>),用于跨主机区分
    pub origin: String,
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.origin, self.native)
    }
}

/// 退出状态(简化版,不暴露信号细节)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// 正常退出,带退出码
    Exited(i32),
    /// 被信号杀死(Linux),Windows 上对应被 TerminateProcess
    Killed,
    /// 进程仍在运行(`HostProcess::try_wait` 返回值)
    Running,
}

impl ExitStatus {
    pub fn success(&self) -> bool {
        matches!(self, Self::Exited(0))
    }

    pub fn exit_code(&self) -> Option<i32> {
        match self {
            Self::Exited(code) => Some(*code),
            _ => None,
        }
    }
}

/// 跨平台进程句柄。
///
/// 由 [`Host::spawn`](crate::Host::spawn) 返回。具体实装:
/// - `LocalWindowsHost::spawn` → 包 `tokio::process::Child`
/// - `RemoteLinuxHost::spawn` → 包 `russh::Channel`
#[async_trait::async_trait]
pub trait HostProcess: Send + Sync {
    /// 进程 ID。
    fn id(&self) -> ProcessId;

    /// 等待进程结束,消费句柄,返回完整 CommandOutput。
    /// stdout/stderr 在等待期间被全量收集。
    async fn wait(self: Box<Self>) -> Result<CommandOutput, HostError>;

    /// 立即查询是否已退出,不消费句柄。
    async fn try_wait(&mut self) -> Result<ExitStatus, HostError>;

    /// 强制终止进程。
    /// - 本地:对应 `Child::kill`(Windows TerminateProcess / Linux SIGKILL)
    /// - 远端:对应 SSH channel signal SIGKILL,如果不支持回退 close channel
    async fn kill(&mut self) -> Result<(), HostError>;

    /// 写 stdin(当 spawn 时指定了 stdin pipe)。
    /// 若进程未开 stdin pipe,返回 `HostError::Unsupported`。
    async fn write_stdin(&mut self, data: &[u8]) -> Result<(), HostError>;

    /// 关闭 stdin(发送 EOF)。
    async fn close_stdin(&mut self) -> Result<(), HostError>;

    /// 取走 stdout 流式句柄。返回的 `AsyncRead` 给调用方做"边运行边读"的
    /// 长流处理（NapCat / SnowLuma 这类需要从 stdout 解析 WebUI URL 或者
    /// 实时输出日志的场景必备）。
    ///
    /// 语义：
    /// - 调用一次后流被消费，再次调用返回 `None`
    /// - spawn 时若 stdout 没有 piped（例如 elevated 模式 stdio 被重定向），
    ///   实装可返回 `None`
    /// - 远端 SSH 实装暂不支持时返回 `None`，调用方应回退到 `wait()` 路径
    ///
    /// 默认实装返回 `None`，让现有 stub / 远端实装保持向后兼容。NativeDeployment
    /// 调用 take_stdout 拿到 None 时会回退到一次性 wait(child) 路径。
    fn take_stdout(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>> {
        None
    }

    /// 取走 stderr 流式句柄，语义同 [`Self::take_stdout`]。
    fn take_stderr(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_id_display_format() {
        let pid = ProcessId {
            native: 12345,
            origin: "local".to_string(),
        };
        assert_eq!(pid.to_string(), "local#12345");
    }

    #[test]
    fn exit_status_success_only_for_zero() {
        assert!(ExitStatus::Exited(0).success());
        assert!(!ExitStatus::Exited(1).success());
        assert!(!ExitStatus::Killed.success());
        assert!(!ExitStatus::Running.success());
    }

    #[test]
    fn exit_code_only_for_exited() {
        assert_eq!(ExitStatus::Exited(42).exit_code(), Some(42));
        assert_eq!(ExitStatus::Killed.exit_code(), None);
        assert_eq!(ExitStatus::Running.exit_code(), None);
    }
}
