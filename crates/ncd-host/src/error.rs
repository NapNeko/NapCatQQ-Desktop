//! HostError:跨主机操作的统一错误枚举
//!
//! 设计原则:
//! - 错误必须能在 LocalHost / RemoteHost / 未来的 DockerHost 之间通用
//! - 错误不包含 SSH / Windows API 的低层细节,只暴露语义类别
//! - 每个 variant 提供足够上下文(路径,命令,退出码)用于诊断,但不泄漏密钥

use std::io;

use crate::path::HostPath;

/// HostError:[Host](crate::Host) trait 所有方法的统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// IO 错误(本地文件系统,SSH 通道字节流,SFTP 等)
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// 目标路径不存在或不可达
    #[error("path not found: {path}")]
    PathNotFound { path: HostPath },

    /// 路径已存在(opt-in 唯一性场景,如创建新目录失败)
    #[error("path already exists: {path}")]
    PathExists { path: HostPath },

    /// 路径权限不足(读 / 写 / 执行)
    #[error("permission denied: {path} ({operation})")]
    PermissionDenied {
        path: HostPath,
        /// 触发权限错误的具体操作(如 "read" / "write" / "extract")
        operation: &'static str,
    },

    /// 命令执行失败(非零退出码或无法 spawn)
    #[error("command failed: {program} (exit={exit_code:?}): {stderr}")]
    CommandFailed {
        /// 程序名(不含参数,避免日志泄露)
        program: String,
        /// 退出码,None 表示进程被信号杀死
        exit_code: Option<i32>,
        /// stderr 摘要(已脱敏 / 截断)
        stderr: String,
    },

    /// 提权失败(UAC 拒绝 / sudo 密码错 / 远端无 sudo 权限)
    #[error("elevation failed on {locality}: {reason}")]
    ElevationFailed {
        /// "local" / "remote" 字面量,用于上层文案
        locality: &'static str,
        reason: String,
    },

    /// 归档解压失败(SHA256 不匹配 / tar/zip 损坏 / 路径越界)
    #[error("extract archive failed: {archive}: {reason}")]
    ExtractFailed {
        archive: HostPath,
        reason: String,
    },

    /// 包管理器操作失败(apt / dnf / winget)
    #[error("package manager error ({manager}): {reason}")]
    PackageManagerError {
        /// "apt" / "dnf" / "winget" / "choco" 字面量
        manager: &'static str,
        reason: String,
    },

    /// 远端连接失败(SSH 握手 / 认证 / 隧道)
    /// 仅 RemoteHost 实装可能产生
    #[error("remote connection failed: {reason}")]
    RemoteConnection { reason: String },

    /// known_hosts 中没有这台主机的 host key上层需要让用户确认后再写入
    #[error("unknown ssh host key for {host}:{port} ({key_kind} {key_b64})")]
    HostKeyUnknown {
        host: String,
        port: u16,
        key_kind: String,
        key_b64: String,
    },

    /// known_hosts 中已有同主机条目但 key 不一致,必须阻断连接
    #[error("ssh host key mismatch for {host}:{port} ({key_kind} {key_b64})")]
    HostKeyMismatch {
        host: String,
        port: u16,
        key_kind: String,
        key_b64: String,
    },

    /// 远端会话被中断(网络抖动 / 主机重启 / 用户主动 close)
    #[error("remote session disconnected: {reason}")]
    RemoteDisconnected { reason: String },

    /// 当前 Host 不支持该操作(如 LocalWindowsHost 不支持 sudo)
    #[error("unsupported on this host: {operation}")]
    Unsupported { operation: &'static str },

    /// 调用方传入的参数不合法(空字符串 / 路径越界 / 错误的 archive kind)
    #[error("invalid argument: {reason}")]
    InvalidArgument { reason: String },

    /// 取消信号被触发(用户主动取消长任务)
    #[error("operation cancelled")]
    Cancelled,

    /// 操作超时(网络 IO / 命令执行超过 deadline)
    #[error("operation timed out: {operation}")]
    Timeout { operation: &'static str },
}

impl HostError {
    /// 把 SSH / 远端操作产生的 io::Error 标记为远端会话中断
    /// 各 RemoteHost 实装在 transport 层捕获后调用本辅助
    pub fn remote_disconnected(reason: impl Into<String>) -> Self {
        Self::RemoteDisconnected {
            reason: reason.into(),
        }
    }

    /// 该错误是否意味着远端连接 / 会话已不可用
    /// 远端 SFTP 复用会话时用它判断要不要丢弃缓存的 session 重开
    pub fn is_disconnect(&self) -> bool {
        matches!(
            self,
            Self::RemoteDisconnected { .. } | Self::RemoteConnection { .. }
        )
    }

    /// 命令失败的便捷构造器
    pub fn command_failed(
        program: impl Into<String>,
        exit_code: Option<i32>,
        stderr: impl Into<String>,
    ) -> Self {
        Self::CommandFailed {
            program: program.into(),
            exit_code,
            stderr: stderr.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_failed_renders_consistently() {
        let err = HostError::command_failed("git", Some(128), "fatal: not a repo");
        let text = err.to_string();
        assert!(text.contains("git"));
        assert!(text.contains("128"));
        assert!(text.contains("not a repo"));
    }

    #[test]
    fn from_io_error_works() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let host_err: HostError = io_err.into();
        assert!(matches!(host_err, HostError::Io(_)));
    }

    #[test]
    fn unsupported_keeps_static_str() {
        let err = HostError::Unsupported {
            operation: "winreg_query",
        };
        assert!(err.to_string().contains("winreg_query"));
    }
}
