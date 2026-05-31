//! 远端主机实装(SSH / SFTP 抽象)。
//!
//! 实装与预留矩阵:
//! - `RemoteLinuxHost`:基于 russh / russh-sftp 的中档实装
//! - `RemoteWindowsHost`:接口预留 stub,所有方法返回 `HostError::Unsupported`
//!
//! 中档能力清单:
//! - 密码 / ed25519 / RSA 私钥认证
//! - Host key 策略(Strict / Insecure;AcceptOnFirstUse 由上层 UI 实装)
//! - exec channel(短命令 + 长流式)
//! - SFTP read_file / write_file / list_dir / 目录管理
//! - 端口转发 / 隧道(`open_tunnel`)
//! - 连接复用 + Keepalive
//!
//! 暂未覆盖:跳板机 / Agent forwarding。

pub mod connection;
pub mod credentials;
pub mod host_key;
pub mod linux;
pub mod tunnel;
pub mod windows_stub;

pub use connection::ConnectionConfig;
pub use credentials::{SshCredentials, SshKey};
pub use host_key::{HostKeyPolicy, KnownHostsStore};
pub use linux::{probe_sudo, RemoteLinuxHost, SudoAccess};
pub use tunnel::{TunnelHandle, TunnelSpec};
pub use windows_stub::RemoteWindowsHost;
