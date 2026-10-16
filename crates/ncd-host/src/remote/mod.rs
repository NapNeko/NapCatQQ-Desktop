//! 远端主机实装(SSH / SFTP 抽象)
//!
//! 实装与预留矩阵:
//! - RemoteLinuxHost:基于 russh / russh-sftp 的中档实装
//! - RemoteWindowsHost:Windows OpenSSH / PowerShell / SFTP / Agent
//!
//! 中档能力清单:
//! - 密码 / ed25519 / RSA 私钥认证
//! - Host key 策略(Strict / Insecure;AcceptOnFirstUse 由上层 UI 实装)
//! - exec channel(短命令 + 长流式)
//! - SFTP read_file / write_file / list_dir / 目录管理
//! - 端口转发 / 隧道(open_tunnel)
//! - 连接复用 + Keepalive
//!
//! 暂未覆盖:跳板机 / Agent forwarding

mod authorized_keys;
pub mod connection;
pub mod credentials;
pub mod download_progress;
pub mod host_key;
pub mod linux;
mod raw_exec;
pub mod tunnel;
pub mod url_download;
pub mod windows;

pub use connection::ConnectionConfig;
pub use credentials::{SshCredentials, SshKey};
pub use download_progress::{CurlProgressParser, DownloadProgress, WgetProgressParser};
pub use host_key::{
    HostKeyCheck, HostKeyPolicy, KnownHostsLine, KnownHostsStore, known_hosts_host_matches,
    parse_known_hosts_line,
};
pub use linux::{RemoteLinuxHost, SudoAccess, probe_sudo};
pub use tunnel::{TunnelDirection, TunnelHandle, TunnelSpec};
pub use url_download::{
    REMOTE_URL_DOWNLOAD_TIMEOUT, curl_url_download_command, wget_url_download_command,
};
pub use windows::{RemoteWindowsHost, connect_remote_host, connect_remote_host_with_platform};
