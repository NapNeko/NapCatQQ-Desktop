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

/// 远端 sudo 提权能力探测结果。上层(ncd-deploy)据此决定要不要向用户要密码:
/// RootAlready / Passwordless 直接装,PasswordRequired 才弹密码框。
/// 不含任何凭证,可安全 Debug 打印。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SudoAccess {
    /// 当前账号就是 root(id -u == 0),根本不走 sudo。
    RootAlready,
    /// 非 root 但配了 NOPASSWD,`sudo -n` 直接过。
    Passwordless,
    /// sudo 需要密码,装之前得先拿到用户密码。
    PasswordRequired,
}

/// 远端 Linux 主机。
pub struct RemoteLinuxHost {
    id: String,
    shell: BashShell,
    /// SSH session 句柄(用 Mutex 串行化,避免多线程同时操作 channel)。
    /// 实际连接保留为 Arc 以便 spawn 出多个 channel 用同一 session。
    handle: Arc<Mutex<ClientHandle<ClientCallback>>>,
    /// 复用的 SFTP 会话。SFTP 子系统初始化(开 channel + request_subsystem +
    /// 协议版本协商)有好几个往返,原来每次 exists/read_file 都重开一条用完即弃,
    /// 是远端探测慢的大头。这里缓存一条 session 反复用;russh-sftp 的 SftpSession
    /// 内部按 request id 多路复用,支持并发请求。连接断了时 op 报错会清空缓存,
    /// 下次访问自动重开,实现断线自愈。
    sftp: Arc<Mutex<Option<Arc<russh_sftp::client::SftpSession>>>>,
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
            sftp: Arc::new(Mutex::new(None)),
            config,
        })
    }

    /// HostPath → 远端 POSIX 字符串
    fn to_remote(&self, path: &HostPath) -> String {
        path.render(PathStyle::Posix)
    }

    /// 取复用的 SFTP 会话；首次访问或缓存被清空时新建一条。
    async fn sftp_session(&self) -> Result<Arc<russh_sftp::client::SftpSession>, HostError> {
        {
            let guard = self.sftp.lock().await;
            if let Some(session) = guard.as_ref() {
                return Ok(Arc::clone(session));
            }
        }
        // 缓存未命中：开一条新 SFTP 会话并缓存。等锁期间可能已有别的请求建好，
        // 二次检查避免重复开。
        let mut guard = self.sftp.lock().await;
        if let Some(session) = guard.as_ref() {
            return Ok(Arc::clone(session));
        }
        let session = Arc::new(open_sftp(&self.handle).await?);
        *guard = Some(Arc::clone(&session));
        Ok(session)
    }

    /// 丢弃缓存的 SFTP 会话。某次 op 报错（多半连接断了）时调用，下次访问重开。
    async fn invalidate_sftp(&self) {
        *self.sftp.lock().await = None;
    }

    /// 主机 id 引用
    pub fn server_id(&self) -> &str {
        &self.id
    }
}

/// 探测某主机的 sudo 能力,供上层决定是否需要向用户索要密码。
/// 取 &dyn Host 而非 &RemoteLinuxHost:install 编排只持有 trait object,且这套
/// 探测对任何 Linux host 都成立(全走非提权命令,不消耗也不需要任何密码)。
pub async fn probe_sudo(host: &dyn Host) -> SudoAccess {
    // id -u == 0 说明已经是 root,后续装包根本不用 sudo。
    // 探测失败(连接抖动等)按"非 root"保守处理,继续看 sudo -n。
    if let Ok(out) = host.run_to_string(HostCommand::new("id").arg("-u")).await {
        if out.stdout.trim() == "0" {
            return SudoAccess::RootAlready;
        }
    }
    // sudo -n true:配了 NOPASSWD 时静默成功;否则因为需要密码而非零退出
    // (-n 保证它立刻失败而不是挂起等输入)。
    match host
        .run_to_string(HostCommand::new("sudo").arg("-n").arg("true"))
        .await
    {
        Ok(out) if out.success() => SudoAccess::Passwordless,
        _ => SudoAccess::PasswordRequired,
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
        let inner_line = build_remote_command_line(&self.shell, &cmd);
        // elevated 时把内层命令塞进 `sudo ... sh -c <inner>`,密码(带换行)走 stdin。
        // 非 elevated 路径行为不变:line 就是内层命令,stdin 原样透传。
        let (line, stdin) = if cmd.elevated {
            let pw = cmd.stdin.as_deref().map(ensure_trailing_newline);
            (
                wrap_with_sudo(&inner_line, &self.shell, pw.is_some()),
                pw,
            )
        } else {
            (inner_line, cmd.stdin)
        };
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
            if let Some(data) = stdin {
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
        let inner_line = build_remote_command_line(&self.shell, &cmd);
        let (line, stdin) = if cmd.elevated {
            let pw = cmd.stdin.as_deref().map(ensure_trailing_newline);
            (
                wrap_with_sudo(&inner_line, &self.shell, pw.is_some()),
                pw,
            )
        } else {
            (inner_line, cmd.stdin)
        };

        let session = self.handle.lock().await;
        let channel = session
            .channel_open_session()
            .await
            .map_err(|e| HostError::remote_disconnected(format!("open channel: {e}")))?;
        channel
            .exec(true, line.as_bytes())
            .await
            .map_err(|e| HostError::remote_disconnected(format!("exec: {e}")))?;

        if let Some(data) = stdin {
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
        let sftp = self.sftp_session().await?;
        let mut file = match sftp.open(&remote).await {
            Ok(f) => f,
            Err(e) => {
                let err = sftp_err_to_host(&remote, "read", e);
                if err.is_disconnect() {
                    self.invalidate_sftp().await;
                }
                return Err(err);
            }
        };
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

        let sftp = self.sftp_session().await?;
        let mut file = match sftp.create(&remote).await {
            Ok(f) => f,
            Err(e) => {
                let err = sftp_err_to_host(&remote, "write", e);
                if err.is_disconnect() {
                    self.invalidate_sftp().await;
                }
                return Err(err);
            }
        };
        file.write_all(bytes).await.map_err(HostError::Io)?;
        file.flush().await.map_err(HostError::Io)?;
        Ok(())
    }

    async fn list_dir(&self, path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        let remote = self.to_remote(path);
        let sftp = self.sftp_session().await?;
        let entries = match sftp.read_dir(&remote).await {
            Ok(e) => e,
            Err(e) => {
                let err = sftp_err_to_host(&remote, "list", e);
                if err.is_disconnect() {
                    self.invalidate_sftp().await;
                }
                return Err(err);
            }
        };
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
        let sftp = self.sftp_session().await?;
        match sftp.remove_file(&remote).await {
            Ok(()) => Ok(()),
            Err(e) => {
                let err = sftp_err_to_host(&remote, "remove_file", e);
                if err.is_disconnect() {
                    self.invalidate_sftp().await;
                }
                Err(err)
            }
        }
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
        let sftp = self.sftp_session().await?;
        match sftp.metadata(&remote).await {
            Ok(_) => Ok(true),
            Err(russh_sftp::client::error::Error::Status(s))
                if s.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
            {
                Ok(false)
            }
            Err(e) => {
                let err = sftp_err_to_host(&remote, "exists", e);
                if err.is_disconnect() {
                    self.invalidate_sftp().await;
                }
                Err(err)
            }
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
        let sftp = self.sftp_session().await?;
        let mut file = match sftp.create(&remote_str).await {
            Ok(f) => f,
            Err(e) => {
                let err = sftp_err_to_host(&remote_str, "upload", e);
                if err.is_disconnect() {
                    self.invalidate_sftp().await;
                }
                return Err(err);
            }
        };
        file.write_all(&bytes).await.map_err(HostError::Io)?;
        file.flush().await.map_err(HostError::Io)?;
        Ok(())
    }

    async fn download(&self, remote: &HostPath, local: &Path) -> Result<(), HostError> {
        let remote_str = self.to_remote(remote);
        let sftp = self.sftp_session().await?;
        let mut file = match sftp.open(&remote_str).await {
            Ok(f) => f,
            Err(e) => {
                let err = sftp_err_to_host(&remote_str, "download", e);
                if err.is_disconnect() {
                    self.invalidate_sftp().await;
                }
                return Err(err);
            }
        };
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

/// 把内层命令行包成 sudo 提权命令。inner_line 可能含 cd && 这类 shell 语法,
/// 所以整体丢给 `sh -c <inner>` 跑,而不是让 sudo 直接 exec 单个程序。
/// has_password 决定喂密码的方式:
///   true  → `sudo -S -p '' sh -c <inner>`,-S 从 stdin 读密码,-p '' 清空提示符
///           避免提示文字混进 stderr 干扰输出解析(密码字节另由调用方走 stdin 送)。
///   false → `sudo -n -p '' sh -c <inner>`,-n 非交互:免密(NOPASSWD/root)直接过,
///           真要密码时立刻失败而不是挂起等输入。
/// inner 用 shell.escape 转义成单个 token,内部空格 / 分号 / 引号都被安全包裹。
fn wrap_with_sudo(inner_line: &str, shell: &dyn HostShell, has_password: bool) -> String {
    let mode = if has_password { "-S" } else { "-n" };
    let inner = shell.escape(inner_line);
    format!("sudo {mode} -p '' sh -c {inner}")
}

/// sudo -S 读的密码必须以换行结尾。调用方已带 \n 就不重复加,否则补一个。
fn ensure_trailing_newline(pw: &[u8]) -> Vec<u8> {
    if pw.last() == Some(&b'\n') {
        pw.to_vec()
    } else {
        let mut out = Vec::with_capacity(pw.len() + 1);
        out.extend_from_slice(pw);
        out.push(b'\n');
        out
    }
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
        // 非 Status 错误（channel 关闭 / 流读写失败）= SFTP 会话本身坏了，标成
        // 远端中断，让复用层 is_disconnect() 命中后丢弃缓存会话、下次重开。
        other => HostError::remote_disconnected(format!("sftp {op} {path}: {other}")),
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

// ============================================================
// 单测:sudo 命令拼接是纯逻辑,从 async 方法里抽出来直接断言。
// 真正的 SSH 往返不在这测(需要活的远端),只锁定命令行字符串形状。
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_with_password_uses_dash_s_and_sh_c() {
        let line = wrap_with_sudo("apt-get install -y docker.io", &BashShell, true);
        assert!(line.contains("sudo -S -p ''"), "有密码必须用 -S 从 stdin 读: {line}");
        assert!(line.contains("sh -c"), "内层要丢给 sh -c 跑: {line}");
    }

    #[test]
    fn wrap_without_password_uses_dash_n() {
        let line = wrap_with_sudo("apt-get install -y docker.io", &BashShell, false);
        assert!(line.contains("sudo -n -p ''"), "免密走 -n 非交互: {line}");
        assert!(!line.contains("-S"), "免密不该出现 -S: {line}");
        assert!(line.contains("sh -c"), "内层要丢给 sh -c 跑: {line}");
    }

    #[test]
    fn wrap_escapes_inner_with_shell_metachars() {
        // 含 cd && 这类 shell 语法的内层命令必须被 escape 成单个单引号 token,
        // 否则空格 / && 会被外层 sudo 命令行拆散。
        let inner = "cd /opt/napcat && apt-get install -y docker";
        let line = wrap_with_sudo(inner, &BashShell, true);
        let escaped = BashShell.escape(inner);
        assert!(line.ends_with(&escaped), "内层应原样 escape 追加在末尾: {line}");
        assert!(escaped.starts_with('\''), "含空格的内层必须被单引号包裹: {escaped}");
    }

    #[test]
    fn wrap_escapes_inner_with_semicolon() {
        let inner = "echo hi; rm -rf /tmp/x";
        let line = wrap_with_sudo(inner, &BashShell, false);
        // 分号必须落在单引号内,不能泄漏成外层 sudo 命令行的语句分隔符。
        assert!(line.contains("'echo hi; rm -rf /tmp/x'"), "分号应被单引号包住: {line}");
    }

    #[test]
    fn ensure_newline_appends_when_missing() {
        assert_eq!(ensure_trailing_newline(b"hunter2"), b"hunter2\n");
    }

    #[test]
    fn ensure_newline_keeps_single_when_present() {
        assert_eq!(ensure_trailing_newline(b"hunter2\n"), b"hunter2\n");
    }
}
