//! 远端主机实装(SSH / SFTP 抽象)。
//!
//! 蓝图 §6.3 / §6.4 实装与预留矩阵:
//! - **M3.3**(本节):`RemoteLinuxHost`(基于 russh / russh-sftp,中档实装)
//! - **M3.4**:`RemoteWindowsHost` stub(接口预留 unimplemented!)
//!
//! ## M3.3 范围(中档)
//!
//! - ✅ 密码 / ed25519 / RSA 私钥认证
//! - ✅ Host key 策略(Strict / Insecure;AcceptOnFirstUse 由上层 UI 实装)
//! - ✅ exec channel(短命令 + 长流式)
//! - ✅ SFTP read_file / write_file / list_dir / 目录管理
//! - ✅ 端口转发 / 隧道(`open_tunnel`)
//! - ✅ 连接复用 + Keepalive
//! - ❌ 跳板机 / Agent forwarding(留 M5+)

pub mod connection;
pub mod credentials;
pub mod host_key;
pub mod linux;
pub mod tunnel;
pub mod windows_stub;

pub use connection::ConnectionConfig;
pub use credentials::{SshCredentials, SshKey};
pub use host_key::{HostKeyPolicy, KnownHostsStore};
pub use linux::RemoteLinuxHost;
pub use tunnel::{TunnelHandle, TunnelSpec};
pub use windows_stub::RemoteWindowsHost;
