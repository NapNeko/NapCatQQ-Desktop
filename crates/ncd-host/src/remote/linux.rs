//! `RemoteLinuxHost`:基于 russh + russh-sftp 的远端 Linux 主机实装。
//!
//! 中档能力:
//! - 密码 / 私钥认证(ed25519 + RSA)
//! - exec channel(短命令 + 长流式)
//! - SFTP 文件 IO
//! - 端口转发(direct-tcpip)
//! - 连接复用(单一 SSH session 复用)+ Keepalive
//!
//! 安全:
//! - 凭证(密码 / 私钥)由 [`SshCredentials`](super::credentials::SshCredentials) 持有,
//!   不在日志中打印
//! - Host key 校验由 [`HostKeyPolicy`](super::host_key::HostKeyPolicy) 控制
//! - SFTP 路径不做"路径越界"检查 —— 远端 Linux 上调用方有完整 POSIX 权限,
//!   越界由远端 OS 拒绝(权限不足 → `HostError::PermissionDenied`)

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use russh::client::{self, Handle as ClientHandle, Handler};
use russh::keys::{key, PublicKeyBase64};
use russh::ChannelMsg;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Notify};

use crate::command::{CommandOutput, HostCommand};
use crate::error::HostError;
use crate::host::{Arch, Host, Locality, Os};
use crate::package_manager::PackageManager;
use crate::path::{ArchiveKind, DirEntry, HostPath, PathStyle};
use crate::process::{ExitStatus, HostProcess, ProcessId};
use crate::shell::{BashShell, HostShell};

use super::connection::ConnectionConfig;
use super::credentials::{SshCredentials, SshKey};
use super::host_key::{HostKeyPolicy, KnownHostsStore};
use super::tunnel::{TunnelHandle, TunnelSpec};

/// russh client handler:用于 host key 校验。
struct ClientCallback {
    policy: HostKeyPolicy,
    host: String,
    port: u16,
}

#[async_trait]
impl Handler for ClientCallback {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        match &self.policy {
            HostKeyPolicy::Insecure => Ok(true),
            HostKeyPolicy::Strict { known_hosts_path } => {
                let store = KnownHostsStore::new(known_hosts_path.clone());
                let kind = server_public_key.name().to_string();
                let b64 = server_public_key
                    .public_key_base64();
                store
                    .matches(&self.host, self.port, &kind, &b64)
                    .await
                    .map_err(|_| russh::Error::Inconsistent)
            }
            HostKeyPolicy::AcceptOnFirstUse { .. } => {
                tracing::warn!(
                    target: "ncd_host::remote",
                    "AcceptOnFirstUse policy is stub, accepting unconditionally (TODO: implement)"
                );
                Ok(true)
            }
        }
    }
}

/// 远端 Linux 主机。
pub struct RemoteLinuxHost {
    id: String,
    shell: BashShell,
    /// SSH session 句柄(用 Mutex 串行化,避免多线程同时操作 channel)。
    /// 实际连接保留为 Arc 以便 spawn 出多个 channel 用同一 session。
    handle: Arc<Mutex<ClientHandle<ClientCallback>>>,
    /// 连接配置。当前断线后由上层 ServerManager 重新 connect,本结构体内部
    /// 还没用到它重连,先留着等断线自愈实装。
    #[allow(dead_code)]
    config: ConnectionConfig,
}

impl RemoteLinuxHost {
    /// 建立 SSH 连接并完成认证。
    pub async fn connect(id: impl Into<String>, config: ConnectionConfig) -> Result<Self, HostError> {
        let cb = ClientCallback {
            policy: config.host_key_policy.clone(),
            host: config.host.clone(),
            port: config.port,
        };
        let mut russh_cfg = client::Config::default();
        russh_cfg.inactivity_timeout = config.keepalive_interval.map(|d| d * 2);
        russh_cfg.keepalive_interval = config.keepalive_interval;
        let russh_cfg = Arc::new(russh_cfg);

        let addr = format!("{}:{}", config.host, config.port);
        let connect_fut = client::connect(russh_cfg, addr, cb);
        let mut handle = match tokio::time::timeout(config.connect_timeout, connect_fut).await {
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                return Err(HostError::RemoteConnection {
                    reason: format!("ssh connect failed: {e}"),
                });
            }
            Err(_) => {
                return Err(HostError::Timeout {
                    operation: "ssh_connect",
                });
            }
        };

        // 认证
        match &config.credentials {
            SshCredentials::Password { username, password } => {
                let ok = handle
                    .authenticate_password(username, password)
                    .await
                    .map_err(|e| HostError::RemoteConnection {
                        reason: format!("password auth: {e}"),
                    })?;
                if !ok {
                    return Err(HostError::RemoteConnection {
                        reason: "password rejected".into(),
                    });
                }
            }
            SshCredentials::Key { username, key } => {
                let key_pair = load_key(key).await?;
                let ok = handle
                    .authenticate_publickey(username, Arc::new(key_pair))
                    .await
                    .map_err(|e| HostError::RemoteConnection {
                        reason: format!("publickey auth: {e}"),
                    })?;
                if !ok {
                    return Err(HostError::RemoteConnection {
                        reason: "publickey rejected".into(),
                    });
                }
            }
        }

        Ok(Self {
            id: id.into(),
            shell: BashShell,
            handle: Arc::new(Mutex::new(handle)),
            config,
        })
    }

    /// HostPath → 远端 POSIX 字符串
    fn to_remote(&self, path: &HostPath) -> String {
        path.render(PathStyle::Posix)
    }

    /// 主机 id 引用
    pub fn server_id(&self) -> &str {
        &self.id
    }
}

async fn load_key(key: &SshKey) -> Result<key::KeyPair, HostError> {
    match key {
        SshKey::Path { path, passphrase } => {
            let path_clone = path.clone();
            let passphrase = passphrase.clone();
            tokio::task::spawn_blocking(move || {
                russh::keys::load_secret_key(&path_clone, passphrase.as_deref())
            })
            .await
            .map_err(|e| HostError::InvalidArgument {
                reason: format!("ssh key blocking task: {e}"),
            })?
            .map_err(|e| HostError::InvalidArgument {
                reason: format!("ssh key load: {e}"),
            })
        }
        SshKey::Pem { bytes, passphrase } => {
            let s = std::str::from_utf8(bytes).map_err(|_| HostError::InvalidArgument {
                reason: "ssh key bytes not utf-8".into(),
            })?;
            russh::keys::decode_secret_key(s, passphrase.as_deref()).map_err(|e| {
                HostError::InvalidArgument {
                    reason: format!("ssh key decode: {e}"),
                }
            })
        }
    }
}


// ============================================================
// Host trait 实装
// ============================================================

#[async_trait]
impl Host for RemoteLinuxHost {
    // ===== 身份 =====
    fn os(&self) -> Os {
        Os::Linux
    }

    fn arch(&self) -> Arch {
        // 远端架构应在 connect 时探测一次缓存,这里简化:默认 X86_64,
        // 实际部署逻辑会在 ncd-component 自己跑 `uname -m` 决策。
        Arch::X86_64
    }

    fn locality(&self) -> Locality {
        Locality::Remote
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn shell(&self) -> &dyn HostShell {
        &self.shell
    }

    fn pkg_manager(&self) -> Option<&dyn PackageManager> {
        // 暂不实装 apt PackageManager,Component 直接走 spawn(`apt-get install ...`)。
        // 后续可统一加 AptPackageManager / DnfPackageManager。
        None
    }

    // ===== 进程操作(基于 SSH exec channel)=====

    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        if cmd.elevated {
            return Err(HostError::ElevationFailed {
                locality: "remote",
                reason: "remote elevation via sudo NOPASSWD must be configured by user".into(),
            });
        }
        let line = build_remote_command_line(&self.shell, &cmd);
        let timeout = cmd.timeout.unwrap_or(Duration::from_secs(300));

        let handle = self.handle.clone();
        let exec_fut = async move {
            let session = handle.lock().await;
            let mut channel = session
                .channel_open_session()
                .await
                .map_err(|e| HostError::remote_disconnected(format!("open channel: {e}")))?;

            channel
                .exec(true, line.as_bytes())
                .await
                .map_err(|e| HostError::remote_disconnected(format!("exec: {e}")))?;

            // 写 stdin(若指定)
            if let Some(data) = cmd.stdin {
                channel
                    .data(&data[..])
                    .await
                    .map_err(|e| HostError::remote_disconnected(format!("stdin write: {e}")))?;
                channel
                    .eof()
                    .await
                    .map_err(|e| HostError::remote_disconnected(format!("stdin eof: {e}")))?;
            }

            collect_channel_output(&mut channel).await
        };

        match tokio::time::timeout(timeout, exec_fut).await {
            Ok(out) => out,
            Err(_) => Err(HostError::Timeout {
                operation: "remote_run_to_string",
            }),
        }
    }

    async fn spawn(&self, cmd: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        // 中档实装:spawn 也走 exec channel,但返回流式 HostProcess 让调用方逐步读
        // 暂不支持流式 stdin(开 channel 之后再 push stdin,语义复杂),只在 spawn 时一次性写
        if cmd.elevated {
            return Err(HostError::ElevationFailed {
                locality: "remote",
                reason: "remote elevation via sudo NOPASSWD must be configured by user".into(),
            });
        }
        let line = build_remote_command_line(&self.shell, &cmd);

        let session = self.handle.lock().await;
        let channel = session
            .channel_open_session()
            .await
            .map_err(|e| HostError::remote_disconnected(format!("open channel: {e}")))?;
        channel
            .exec(true, line.as_bytes())
            .await
            .map_err(|e| HostError::remote_disconnected(format!("exec: {e}")))?;

        if let Some(data) = cmd.stdin {
            channel
                .data(&data[..])
                .await
                .map_err(|e| HostError::remote_disconnected(format!("stdin write: {e}")))?;
            channel
                .eof()
                .await
                .map_err(|e| HostError::remote_disconnected(format!("stdin eof: {e}")))?;
        }

        Ok(Box::new(RemoteHostProcess {
            channel: Some(channel),
            id: ProcessId {
                native: 0, // SSH exec 不暴露真实 pid;调用方关心的是 channel
                origin: self.id.clone(),
            },
            timeout: cmd.timeout,
        }))
    }


    // ===== 文件操作(基于 SFTP)=====

    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        let remote = self.to_remote(path);
        let sftp = open_sftp(&self.handle).await?;
        let mut file = sftp
            .open(&remote)
            .await
            .map_err(|e| sftp_err_to_host(&remote, "read", e))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(HostError::Io)?;
        Ok(Bytes::from(buf))
    }

    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        let remote = self.to_remote(path);

        // 自动建父目录
        if let Some(parent) = path.parent() {
            let parent_str = parent.as_posix();
            if parent_str != "/" && !parent_str.is_empty() {
                self.create_dir_all(&parent).await?;
            }
        }

        let sftp = open_sftp(&self.handle).await?;
        let mut file = sftp
            .create(&remote)
            .await
            .map_err(|e| sftp_err_to_host(&remote, "write", e))?;
        file.write_all(bytes).await.map_err(HostError::Io)?;
        file.flush().await.map_err(HostError::Io)?;
        Ok(())
    }

    async fn list_dir(&self, path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        let remote = self.to_remote(path);
        let sftp = open_sftp(&self.handle).await?;
        let entries = sftp
            .read_dir(&remote)
            .await
            .map_err(|e| sftp_err_to_host(&remote, "list", e))?;
        let mut result = Vec::new();
        for entry in entries {
            let name = entry.file_name();
            // 跳过 . 与 ..
            if name == "." || name == ".." {
                continue;
            }
            let metadata = entry.metadata();
            result.push(DirEntry {
                name,
                is_dir: metadata.is_dir(),
                size: metadata.size.unwrap_or(0),
            });
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        // SFTP 协议没有原生 -p,需要逐级 mkdir。简单做法:跑 `mkdir -p`
        // 走 SFTP 不可靠(权限错可能让 mkdir 半路成功),改 exec 更稳。
        let remote = self.to_remote(path);
        let escaped = self.shell.escape(&remote);
        let cmd = HostCommand::new("mkdir").arg("-p").arg(remote);
        let _ = escaped; // 防止 unused warning,真正 escape 在 build_remote_command_line 内
        let out = self.run_to_string(cmd).await?;
        if !out.success() {
            return Err(HostError::CommandFailed {
                program: "mkdir".into(),
                exit_code: out.exit_code,
                stderr: out.stderr,
            });
        }
        Ok(())
    }

    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
        let remote = self.to_remote(path);
        let sftp = open_sftp(&self.handle).await?;
        sftp.remove_file(&remote)
            .await
            .map_err(|e| sftp_err_to_host(&remote, "remove_file", e))
    }

    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        // 同 create_dir_all,走 `rm -rf` 更稳(SFTP 没有 -r remove)。
        // 安全约束:测试时调用方应保证传入 path 在白名单内。
        let remote = self.to_remote(path);
        // 防御:绝不递归删 / 与 /home / /root
        let dangerous: &[&str] = &["/", "/home", "/root", "/etc", "/usr", "/var", "/tmp"];
        let normalized = remote.trim_end_matches('/');
        if dangerous.contains(&normalized) {
            return Err(HostError::InvalidArgument {
                reason: format!("refuse to remove protected path: {remote}"),
            });
        }
        let cmd = HostCommand::new("rm").arg("-rf").arg(remote);
        let out = self.run_to_string(cmd).await?;
        if !out.success() {
            return Err(HostError::CommandFailed {
                program: "rm".into(),
                exit_code: out.exit_code,
                stderr: out.stderr,
            });
        }
        Ok(())
    }

    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        let remote = self.to_remote(path);
        let sftp = open_sftp(&self.handle).await?;
        match sftp.metadata(&remote).await {
            Ok(_) => Ok(true),
            Err(russh_sftp::client::error::Error::Status(s))
                if s.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
            {
                Ok(false)
            }
            Err(e) => Err(sftp_err_to_host(&remote, "exists", e)),
        }
    }

    async fn upload(&self, local: &Path, remote: &HostPath) -> Result<(), HostError> {
        let remote_str = self.to_remote(remote);

        // 自动建父目录
        if let Some(parent) = remote.parent() {
            let parent_str = parent.as_posix();
            if parent_str != "/" && !parent_str.is_empty() {
                self.create_dir_all(&parent).await?;
            }
        }

        let bytes = tokio::fs::read(local).await.map_err(HostError::Io)?;
        let sftp = open_sftp(&self.handle).await?;
        let mut file = sftp
            .create(&remote_str)
            .await
            .map_err(|e| sftp_err_to_host(&remote_str, "upload", e))?;
        file.write_all(&bytes).await.map_err(HostError::Io)?;
        file.flush().await.map_err(HostError::Io)?;
        Ok(())
    }

    async fn download(&self, remote: &HostPath, local: &Path) -> Result<(), HostError> {
        let remote_str = self.to_remote(remote);
        let sftp = open_sftp(&self.handle).await?;
        let mut file = sftp
            .open(&remote_str)
            .await
            .map_err(|e| sftp_err_to_host(&remote_str, "download", e))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(HostError::Io)?;
        if let Some(parent) = local.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        tokio::fs::write(local, buf).await.map_err(HostError::Io)?;
        Ok(())
    }

    async fn extract_archive(
        &self,
        archive: &HostPath,
        dest: &HostPath,
        kind: ArchiveKind,
    ) -> Result<(), HostError> {
        // 远端解压走 shell:tar / unzip 都是 Linux 标配
        let archive_str = self.to_remote(archive);
        let dest_str = self.to_remote(dest);

        // 确保目标目录存在
        self.create_dir_all(dest).await?;

        let cmd = match kind {
            ArchiveKind::TarGz => HostCommand::new("tar")
                .arg("-xzf")
                .arg(&archive_str)
                .arg("-C")
                .arg(&dest_str),
            ArchiveKind::TarXz => HostCommand::new("tar")
                .arg("-xJf")
                .arg(&archive_str)
                .arg("-C")
                .arg(&dest_str),
            ArchiveKind::Zip => HostCommand::new("unzip")
                .arg("-q")
                .arg("-o")
                .arg(&archive_str)
                .arg("-d")
                .arg(&dest_str),
            ArchiveKind::Msi => {
                return Err(HostError::Unsupported {
                    operation: "extract_msi_on_linux",
                });
            }
        };

        let out = self.run_to_string(cmd).await?;
        if !out.success() {
            return Err(HostError::ExtractFailed {
                archive: archive.clone(),
                reason: format!("exit={:?}: {}", out.exit_code, out.stderr.trim()),
            });
        }
        Ok(())
    }
}


// ============================================================
// 辅助:命令拼接 + SFTP 打开 + 错误映射
// ============================================================

fn build_remote_command_line(shell: &dyn HostShell, cmd: &HostCommand) -> String {
    // 远端不复用 BashShell::build_command_line 完全的能力(它没处理 cwd),这里手动加 cd
    let mut prefix = String::new();
    if let Some(wd) = &cmd.working_dir {
        prefix.push_str("cd ");
        prefix.push_str(&shell.escape(wd.as_posix()));
        prefix.push_str(" && ");
    }
    let body = shell.build_command_line(cmd);
    format!("{prefix}{body}")
}

async fn open_sftp(
    handle: &Arc<Mutex<ClientHandle<ClientCallback>>>,
) -> Result<russh_sftp::client::SftpSession, HostError> {
    let session = handle.lock().await;
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| HostError::remote_disconnected(format!("sftp open channel: {e}")))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| HostError::remote_disconnected(format!("sftp subsystem: {e}")))?;
    russh_sftp::client::SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| HostError::remote_disconnected(format!("sftp session: {e}")))
}

fn sftp_err_to_host(
    path: &str,
    op: &'static str,
    e: russh_sftp::client::error::Error,
) -> HostError {
    use russh_sftp::client::error::Error as SftpErr;
    use russh_sftp::protocol::StatusCode;
    match e {
        SftpErr::Status(s) => match s.status_code {
            StatusCode::NoSuchFile => HostError::PathNotFound {
                path: HostPath::from_posix(path),
            },
            StatusCode::PermissionDenied => HostError::PermissionDenied {
                path: HostPath::from_posix(path),
                operation: op,
            },
            _ => HostError::Io(std::io::Error::other(format!(
                "sftp {op} {path}: {:?}",
                s.status_code
            ))),
        },
        other => HostError::Io(std::io::Error::other(format!(
            "sftp {op} {path}: {other}"
        ))),
    }
}

// ============================================================
// 收集 channel 输出(用于 run_to_string)
// ============================================================

async fn collect_channel_output(
    channel: &mut russh::Channel<russh::client::Msg>,
) -> Result<CommandOutput, HostError> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_code: Option<i32> = None;

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
            ChannelMsg::ExtendedData { ref data, ext } if ext == 1 => {
                stderr.extend_from_slice(data);
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit_code = Some(exit_status as i32);
                // 不立即 break:等 server 主动 Close 或 Eof,让任何未收完的 Data 包都收到
            }
            ChannelMsg::Eof => {
                // server 写完 stdout/stderr,但 ExitStatus 可能还没到。继续等
            }
            ChannelMsg::Close => break,
            _ => {}
        }
    }

    Ok(CommandOutput {
        exit_code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

// ============================================================
// RemoteHostProcess:russh::Channel 包装成 HostProcess
// ============================================================

struct RemoteHostProcess {
    channel: Option<russh::Channel<russh::client::Msg>>,
    id: ProcessId,
    timeout: Option<Duration>,
}

#[async_trait]
impl HostProcess for RemoteHostProcess {
    fn id(&self) -> ProcessId {
        self.id.clone()
    }

    async fn wait(mut self: Box<Self>) -> Result<CommandOutput, HostError> {
        let mut channel = self.channel.take().ok_or_else(|| HostError::InvalidArgument {
            reason: "remote channel already consumed".into(),
        })?;
        let timeout = self.timeout.unwrap_or(Duration::from_secs(300));
        match tokio::time::timeout(timeout, collect_channel_output(&mut channel)).await {
            Ok(out) => out,
            Err(_) => Err(HostError::Timeout {
                operation: "remote_process_wait",
            }),
        }
    }

    async fn try_wait(&mut self) -> Result<ExitStatus, HostError> {
        // SSH exec 没有"快速 try_wait":没有快速 fd polling。
        // 简化:用 poll 试一次 channel.wait() with 0 timeout(tokio 的 timeout)
        let channel = self.channel.as_mut().ok_or_else(|| HostError::InvalidArgument {
            reason: "remote channel already consumed".into(),
        })?;
        match tokio::time::timeout(Duration::from_millis(1), channel.wait()).await {
            Ok(Some(ChannelMsg::ExitStatus { exit_status })) => Ok(ExitStatus::Exited(exit_status as i32)),
            Ok(Some(ChannelMsg::Close)) => Ok(ExitStatus::Killed),
            Ok(Some(_)) => Ok(ExitStatus::Running),
            Ok(None) => Ok(ExitStatus::Running),
            Err(_) => Ok(ExitStatus::Running),
        }
    }

    async fn kill(&mut self) -> Result<(), HostError> {
        let channel = self.channel.as_mut().ok_or_else(|| HostError::InvalidArgument {
            reason: "remote channel already consumed".into(),
        })?;
        // SSH 没有标准 SIGKILL 跨服务器请求,尝试 close channel
        channel
            .close()
            .await
            .map_err(|e| HostError::remote_disconnected(format!("close channel: {e}")))?;
        Ok(())
    }

    async fn write_stdin(&mut self, data: &[u8]) -> Result<(), HostError> {
        let channel = self.channel.as_mut().ok_or_else(|| HostError::InvalidArgument {
            reason: "remote channel already consumed".into(),
        })?;
        channel
            .data(data)
            .await
            .map_err(|e| HostError::remote_disconnected(format!("stdin write: {e}")))?;
        Ok(())
    }

    async fn close_stdin(&mut self) -> Result<(), HostError> {
        let channel = self.channel.as_mut().ok_or_else(|| HostError::InvalidArgument {
            reason: "remote channel already consumed".into(),
        })?;
        channel
            .eof()
            .await
            .map_err(|e| HostError::remote_disconnected(format!("stdin eof: {e}")))?;
        Ok(())
    }
}

// ============================================================
// Tunnel(端口转发,direct-tcpip)
// ============================================================

impl RemoteLinuxHost {
    /// 打开端口转发隧道:本地 spec.local_port → 远端 spec.remote_host:spec.remote_port
    pub async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        let local_addr = format!("{}:{}", spec.local_host, spec.local_port);
        let listener = TcpListener::bind(&local_addr)
            .await
            .map_err(HostError::Io)?;
        let actual_port = listener.local_addr().map_err(HostError::Io)?.port();

        let shutdown = Arc::new(Notify::new());
        let shutdown_for_task = shutdown.clone();
        let handle = self.handle.clone();
        let originator = "127.0.0.1".to_string();
        let remote_host = spec.remote_host.clone();
        let remote_port = spec.remote_port;

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_for_task.notified() => {
                        tracing::debug!(target: "ncd_host::tunnel", "tunnel shutdown signaled");
                        break;
                    }
                    accept = listener.accept() => {
                        let (local_stream, peer) = match accept {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!(target: "ncd_host::tunnel", "accept failed: {e}");
                                continue;
                            }
                        };
                        let h = handle.clone();
                        let remote_host_for_conn = remote_host.clone();
                        let originator_for_conn = originator.clone();
                        tokio::spawn(async move {
                            // 开 direct-tcpip channel
                            let channel = {
                                let session = h.lock().await;
                                session
                                    .channel_open_direct_tcpip(
                                        remote_host_for_conn,
                                        remote_port as u32,
                                        originator_for_conn,
                                        peer.port() as u32,
                                    )
                                    .await
                            };
                            let channel = match channel {
                                Ok(c) => c,
                                Err(e) => {
                                    tracing::warn!(target: "ncd_host::tunnel", "open direct-tcpip: {e}");
                                    return;
                                }
                            };
                            // channel.into_stream() 转成 AsyncRead + AsyncWrite,直接 copy_bidirectional
                            let mut local_stream = local_stream;
                            let mut channel_stream = channel.into_stream();
                            if let Err(e) = tokio::io::copy_bidirectional(&mut local_stream, &mut channel_stream).await {
                                tracing::debug!(target: "ncd_host::tunnel", "tunnel pump finished: {e}");
                            }
                        });
                    }
                }
            }
        });

        Ok(TunnelHandle {
            local_port: actual_port,
            shutdown,
            _task: task,
        })
    }
}
