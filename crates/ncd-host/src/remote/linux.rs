//! RemoteLinuxHost:基于 russh + russh-sftp 的远端 Linux 主机实装

//!
//! 中档能力:
//! - 密码 / 私钥认证(ed25519 + RSA)
//! - exec channel(短命令 + 长流式)
//! - SFTP 文件 IO
//! - 端口转发(direct-tcpip `-L` + tcpip-forward `-R`)
//! - 连接复用(单一 SSH session 复用)+ Keepalive
//!
//! 安全:
//! - 凭证(密码 / 私钥)由 [SshCredentials](super::credentials::SshCredentials) 持有,
//!   不在日志中打印
//! - Host key 校验由 [HostKeyPolicy](super::host_key::HostKeyPolicy) 控制
//! - SFTP 路径不做"路径越界"检查 —— 远端 Linux 上调用方有完整 POSIX 权限,
//!   越界由远端 OS 拒绝(权限不足 → HostError::PermissionDenied)

#[cfg(test)]
mod transport_tests;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use russh::Channel;
use russh::ChannelMsg;
use russh::Sig;
use russh::client::{self, Handle as ClientHandle, Handler, Msg};
use russh::keys::{PublicKeyBase64, key};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Notify, RwLock, Semaphore, SemaphorePermit};
use tracing::info;

use crate::command::{CommandOutput, DEFAULT_COMMAND_TIMEOUT, HostCommand, HostProcessWaitPolicy};
use crate::error::HostError;
use crate::host::{Arch, Host, Locality, Os, SshDialTarget};
use crate::linux_pkg::{
    EnsureCommandError, LinuxPackageManager, install_missing_command, plan_missing_command,
};
use crate::path::{ArchiveKind, DirEntry, HostPath, PathStyle};
use crate::process::{ExitStatus, HostProcess, ProcessId};
use crate::shell::{BashShell, HostShell};

use super::connection::ConnectionConfig;
use super::credentials::{SshCredentials, SshKey};
use super::host_key::{HostKeyCheck, HostKeyPolicy, KnownHostsStore};
use super::tunnel::{TunnelDirection, TunnelHandle, TunnelSpec};

mod pty;

const SFTP_SESSION_TIMEOUT: Duration = Duration::from_secs(10);
const SFTP_FILE_IO_TIMEOUT: Duration = Duration::from_secs(30);

/// `(远端听地址, 远端听口)` → `(本机目标 host, 本机目标口)`
type RemoteForwardTable = Arc<Mutex<HashMap<(String, u16), (String, u16)>>>;

/// russh client handler: host key 校验 + `-R` 进来的 forwarded-tcpip
struct ClientCallback {
    policy: HostKeyPolicy,
    host: String,
    port: u16,
    host_key_error: Arc<Mutex<Option<HostError>>>,
    remote_forwards: RemoteForwardTable,
    verified_host_key: Arc<std::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl Handler for ClientCallback {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let accepted = match &self.policy {
            HostKeyPolicy::Insecure => Ok(true),
            HostKeyPolicy::Strict { known_hosts_path }
            | HostKeyPolicy::AcceptOnFirstUse { known_hosts_path } => {
                let store = KnownHostsStore::new(known_hosts_path.clone());
                let kind = server_public_key.name().to_string();
                let b64 = server_public_key.public_key_base64();
                match store
                    .check(&self.host, self.port, &kind, &b64)
                    .await
                    .map_err(|_| russh::Error::Inconsistent)?
                {
                    HostKeyCheck::Match => Ok(true),
                    HostKeyCheck::Unknown => {
                        *self.host_key_error.lock().await = Some(HostError::HostKeyUnknown {
                            host: self.host.clone(),
                            port: self.port,
                            key_kind: kind,
                            key_b64: b64,
                        });
                        Ok(false)
                    }
                    HostKeyCheck::Mismatch => {
                        *self.host_key_error.lock().await = Some(HostError::HostKeyMismatch {
                            host: self.host.clone(),
                            port: self.port,
                            key_kind: kind,
                            key_b64: b64,
                        });
                        Ok(false)
                    }
                }
            }
        };
        if matches!(accepted, Ok(true)) && !matches!(self.policy, HostKeyPolicy::Insecure) {
            if let Ok(mut key) = self.verified_host_key.lock() {
                *key = Some(format!(
                    "{} {}",
                    server_public_key.name(),
                    server_public_key.public_key_base64()
                ));
            }
        }
        accepted
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let port = connected_port as u16;
        let target = {
            let table = self.remote_forwards.lock().await;
            table
                .get(&(connected_address.to_string(), port))
                .cloned()
                .or_else(|| {
                    table
                        .iter()
                        .find_map(|((_, p), dest)| (*p == port).then(|| dest.clone()))
                })
        };
        let Some((local_host, local_port)) = target else {
            tracing::warn!(
                target: "ncd_host::tunnel",
                connected_address,
                connected_port,
                "forwarded-tcpip 没有对应的 -R 登记"
            );
            return Ok(());
        };
        tokio::spawn(async move {
            let mut local = match TcpStream::connect((local_host.as_str(), local_port)).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        target: "ncd_host::tunnel",
                        local_host,
                        local_port,
                        "-R 连本机失败: {e}"
                    );
                    return;
                }
            };
            let mut remote = channel.into_stream();
            if let Err(e) = tokio::io::copy_bidirectional(&mut local, &mut remote).await {
                tracing::debug!(target: "ncd_host::tunnel", "remote-forward pump finished: {e}");
            }
        });
        Ok(())
    }
}

/// 远端 sudo 提权能力探测结果上层(ncd-deploy)据此决定要不要向用户要密码:
/// RootAlready / Passwordless 直接装,PasswordRequired 才弹密码框
/// 不含任何凭证,可安全 Debug 打印
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SudoAccess {
    /// 当前账号就是 root(id -u == 0),根本不走 sudo
    RootAlready,
    /// 非 root 但配了 NOPASSWD,sudo -n 直接过
    Passwordless,
    /// sudo 需要密码,装之前得先拿到用户密码
    PasswordRequired,
}

/// 远端 Linux 主机
pub struct RemoteLinuxHost {
    id: String,
    shell: BashShell,
    /// 普通通道共享读锁；仅反向转发的可变 API 与连接失效需要写锁。
    handle: Arc<RwLock<Option<ClientHandle<ClientCallback>>>>,
    // 保留 SFTP、长期日志和终端的会话余量；短命令占用直到通道收尾。
    short_exec: Semaphore,
    /// 复用的 SFTP 会话SFTP 子系统初始化(开 channel + request_subsystem +
    /// 协议版本协商)有好几个往返,原来每次 exists/read_file 都重开一条用完即弃,
    /// 是远端探测慢的大头这里缓存一条 session 反复用;russh-sftp 的 SftpSession
    /// 内部按 request id 多路复用,支持并发请求连接断了时 op 报错会清空缓存,
    /// 下次访问自动重开,实现断线自愈
    sftp: Arc<Mutex<Option<Arc<russh_sftp::client::SftpSession>>>>,
    /// 提权密码,这台主机所有 elevated 命令共用connect 后由 ServerManager 从
    /// keyring 注入(密码登录机器有登录密码,密钥登录机器可能有挪存的 sudo 密码),
    /// docker 弹框拿到新密码时覆盖有密码 → elevated 走 sudo -S 喂 stdin;None →
    /// 退回 sudo -n(root / 免密直接过,真要密码立刻失败而非挂起)
    /// 用 Mutex 包,因为 set_elevation_password 是 &self 异步方法(trait 约束),
    /// 且密码可能在连接生命周期内被 docker 弹框更新
    elevation_password: Arc<Mutex<Option<String>>>,
    /// 连接配置:拨号身份给跨机常驻隧道;`connect` 后由上层重连
    config: ConnectionConfig,
    remote_forwards: RemoteForwardTable,
    /// `which` 找到过的命令路径。跟着这个连接对象活：换了服务器档案会重建连接，自然作废
    which_found: std::sync::Mutex<HashMap<String, String>>,
    verified_host_key: Arc<std::sync::Mutex<Option<String>>>,
}

impl RemoteLinuxHost {
    /// 建立 SSH 连接并完成认证
    pub async fn connect(
        id: impl Into<String>,
        config: ConnectionConfig,
    ) -> Result<Self, HostError> {
        let host_key_error = Arc::new(Mutex::new(None));
        let remote_forwards = Arc::new(Mutex::new(HashMap::new()));
        let verified_host_key = Arc::new(std::sync::Mutex::new(None));
        let cb = ClientCallback {
            policy: config.host_key_policy.clone(),
            host: config.host.clone(),
            port: config.port,
            host_key_error: Arc::clone(&host_key_error),
            remote_forwards: Arc::clone(&remote_forwards),
            verified_host_key: Arc::clone(&verified_host_key),
        };
        // 长任务(apt 等锁,流式安装)需要更长的会话空闲上限;仅靠 keepalive 时
        // 默认 inactivity=2×keepalive 在部分 sshd/中间设备上仍可能被掐断
        let russh_cfg = client::Config {
            inactivity_timeout: match config.keepalive_interval {
                Some(d) if d <= Duration::from_secs(20) => Some(Duration::from_secs(900)),
                Some(d) => Some(d.saturating_mul(4)),
                None => None,
            },
            keepalive_interval: config.keepalive_interval,
            ..client::Config::default()
        };
        let russh_cfg = Arc::new(russh_cfg);

        let addr = format!("{}:{}", config.host, config.port);
        let connect_fut = client::connect(russh_cfg, addr, cb);
        let mut handle = match tokio::time::timeout(config.connect_timeout, connect_fut).await {
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                if matches!(&e, russh::Error::UnknownKey) {
                    if let Some(err) = host_key_error.lock().await.take() {
                        return Err(err);
                    }
                }
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
        let authentication = async {
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
            };
            Ok::<(), HostError>(())
        };
        tokio::time::timeout(config.connect_timeout, authentication)
            .await
            .map_err(|_| HostError::Timeout {
                operation: "ssh_authentication",
            })??;

        let id_str = id.into();
        info!(
            target: "ncd_host::remote",
            host_id = %id_str,
            host = %config.host,
            port = config.port,
            "ssh connect ok"
        );

        Ok(Self {
            id: id_str,
            shell: BashShell,
            handle: Arc::new(RwLock::new(Some(handle))),
            short_exec: Semaphore::new(4),
            sftp: Arc::new(Mutex::new(None)),
            elevation_password: Arc::new(Mutex::new(None)),
            config,
            remote_forwards,
            which_found: std::sync::Mutex::new(HashMap::new()),
            verified_host_key,
        })
    }

    /// HostPath → 远端 POSIX 字符串
    fn to_remote(&self, path: &HostPath) -> String {
        path.render(PathStyle::Posix)
    }

    /// 取复用的 SFTP 会话;首次访问或缓存被清空时新建一条
    pub(super) async fn sftp_session(
        &self,
    ) -> Result<Arc<russh_sftp::client::SftpSession>, HostError> {
        // 排队也消耗同一预算，初始化失败不能让等待者逐个再等完整握手超时。
        let deadline = tokio::time::Instant::now() + SFTP_SESSION_TIMEOUT;
        tokio::time::timeout_at(deadline, async {
            let mut guard = self.sftp.lock().await;
            if tokio::time::Instant::now() >= deadline {
                return Err(HostError::Timeout {
                    operation: "sftp_initialize",
                });
            }
            if let Some(session) = guard.as_ref() {
                return Ok(Arc::clone(session));
            }
            let session = Arc::new(open_sftp(self).await?);
            *guard = Some(Arc::clone(&session));
            Ok(session)
        })
        .await
        .map_err(|_| HostError::Timeout {
            operation: "sftp_initialize",
        })?
    }

    /// 丢弃缓存的 SFTP 会话某次 op 报错(多半连接断了)时调用,下次访问重开
    async fn invalidate_sftp(&self) {
        *self.sftp.lock().await = None;
    }

    async fn invalidate_sftp_session(&self, session: &Arc<russh_sftp::client::SftpSession>) {
        {
            let mut guard = self.sftp.lock().await;
            if guard
                .as_ref()
                .is_some_and(|cached| Arc::ptr_eq(cached, session))
            {
                *guard = None;
            }
        }
        // 唤醒同一坏子系统上的请求，保留其他 SSH channel 和已重建的 SFTP。
        let _ = session.close().await;
    }

    async fn sftp_failure(
        &self,
        session: &Arc<russh_sftp::client::SftpSession>,
        path: &str,
        operation: &'static str,
        error: russh_sftp::client::error::Error,
    ) -> HostError {
        if !matches!(&error, russh_sftp::client::error::Error::Status(_)) {
            self.invalidate_sftp_session(session).await;
        }
        sftp_err_to_host(path, operation, error)
    }

    async fn sftp_io<T>(
        &self,
        session: &Arc<russh_sftp::client::SftpSession>,
        budget: Duration,
        operation: &'static str,
        future: impl std::future::Future<Output = std::io::Result<T>>,
    ) -> Result<T, HostError> {
        match tokio::time::timeout(budget, future).await {
            Ok(Ok(value)) => Ok(value),
            result => {
                self.invalidate_sftp_session(session).await;
                match result {
                    Ok(Err(error)) => Err(HostError::Io(error)),
                    _ => Err(HostError::Timeout { operation }),
                }
            }
        }
    }

    /// 先移除主句柄，再限时通知 SSH 断开；已有 channel 的发送端不能延续旧连接。
    async fn invalidate_main_handle(&self) {
        let session = self.handle.write().await.take();
        if let Some(session) = session {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                session.disconnect(
                    russh::Disconnect::ByApplication,
                    "connection invalidated",
                    "en",
                ),
            )
            .await;
        }
    }

    /// 主机 id 引用
    pub fn server_id(&self) -> &str {
        &self.id
    }

    pub(super) async fn acquire_short_exec(&self) -> Result<SemaphorePermit<'_>, HostError> {
        self.short_exec
            .acquire()
            .await
            .map_err(|_| HostError::Io(std::io::Error::other("短命令并发队列已关闭")))
    }

    pub(super) async fn open_session_channel(&self) -> Result<Channel<Msg>, HostError> {
        for attempt in 0..3 {
            let result = {
                let guard = self.handle.read().await;
                let session = guard
                    .as_ref()
                    .ok_or_else(|| HostError::remote_disconnected("ssh session poisoned"))?;
                session.channel_open_session().await
            };
            match result {
                Ok(channel) => return Ok(channel),
                Err(russh::Error::ChannelOpenFailure(reason)) => {
                    if attempt < 2
                        && matches!(
                            reason,
                            russh::ChannelOpenFailure::ConnectFailed
                                | russh::ChannelOpenFailure::ResourceShortage
                        )
                    {
                        tokio::time::sleep(Duration::from_millis(if attempt == 0 {
                            100
                        } else {
                            250
                        }))
                        .await;
                        continue;
                    }
                    return Err(HostError::Io(std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        format!("会话通道被服务器拒绝：{reason:?}"),
                    )));
                }
                Err(error) => {
                    self.invalidate_connection().await;
                    return Err(HostError::remote_disconnected(format!(
                        "open channel: {error}"
                    )));
                }
            }
        }
        Err(HostError::Io(std::io::Error::other(
            "会话通道重试次数已用尽",
        )))
    }

    /// 只打开 SSH exec 通道；命令语法由目标平台的 Host 决定。
    pub(super) async fn open_raw_channel(
        &self,
        line: &str,
        stdin: Option<&[u8]>,
    ) -> Result<Channel<Msg>, HostError> {
        let mut channel = self.open_session_channel().await?;
        channel
            .exec(true, line.as_bytes())
            .await
            .map_err(|error| HostError::remote_disconnected(format!("exec: {error}")))?;
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Success) => break,
                Some(ChannelMsg::Failure | ChannelMsg::Close) | None => {
                    return Err(HostError::remote_disconnected("SSH exec was rejected"));
                }
                Some(ChannelMsg::Data { .. } | ChannelMsg::ExtendedData { .. }) => {
                    return Err(HostError::remote_disconnected(
                        "SSH exec output preceded acceptance",
                    ));
                }
                _ => {}
            }
        }
        if let Some(input) = stdin {
            channel
                .data(input)
                .await
                .map_err(|error| HostError::remote_disconnected(format!("stdin: {error}")))?;
            channel
                .eof()
                .await
                .map_err(|error| HostError::remote_disconnected(format!("stdin EOF: {error}")))?;
        }
        Ok(channel)
    }

    pub(super) async fn spawn_raw(
        &self,
        line: &str,
        cmd: &HostCommand,
    ) -> Result<Box<dyn HostProcess>, HostError> {
        let channel = self.open_raw_channel(line, cmd.stdin.as_deref()).await?;
        Ok(Box::new(RemoteHostProcess {
            channel: Some(channel),
            id: ProcessId {
                native: 0,
                origin: self.id.clone(),
            },
            timeout: cmd.timeout,
            wait_policy: cmd.wait_policy,
        }))
    }

    /// 确保解压工具(unzip / tar)在远端可用,缺了就用包管理器装一次
    ///
    /// 已装好的机器只多一次 command_exists;装不上时报带人话指引的 ExtractFailed
    /// (告诉用户手动 apt-get install <tool>),不把人留在 exit 127 现场
    async fn ensure_archive_tool(&self, tool: &str) -> Result<(), HostError> {
        let result = match plan_missing_command(self, tool).await {
            Ok(None) => return Ok(()),
            Ok(Some(pm)) => install_missing_command(self, pm, tool, tool).await,
            Err(err) => Err(err),
        };
        let reason = match result {
            Ok(()) => return Ok(()),
            Err(EnsureCommandError::NoPackageManager) => format!(
                "远端缺少 {tool} 且未识别到包管理器,请手动安装(如 {})后重试",
                LinuxPackageManager::Apt.install_hint(&[tool])
            ),
            Err(EnsureCommandError::NeedsElevation(pm)) => format!(
                "远端缺少 {tool},自动安装需要 root 权限但当前无免密 sudo 也未保存密码,请去远端页配置免密或手动执行 sudo {} 后重试",
                pm.install_hint(&[tool])
            ),
            Err(EnsureCommandError::StillMissing { pm, .. }) => format!(
                "已尝试自动安装 {tool} 但仍不可用,请登录远端手动执行 sudo {} 后重试",
                pm.install_hint(&[tool])
            ),
        };
        Err(HostError::ExtractFailed {
            archive: HostPath::from_posix(tool),
            reason,
        })
    }
}

/// 探测某主机的 sudo 能力,供上层决定是否需要向用户索要密码
/// 取 &dyn Host 而非 &RemoteLinuxHost:install 编排只持有 trait object,且这套
/// 探测对任何 Linux host 都成立(全走非提权命令,不消耗也不需要任何密码)
pub async fn probe_sudo(host: &dyn Host) -> SudoAccess {
    // id -u == 0 说明已经是 root,后续装包根本不用 sudo
    // 探测失败(连接抖动等)按"非 root"保守处理,继续看 sudo -n
    if let Ok(out) = host.run_to_string(HostCommand::new("id").arg("-u")).await {
        if out.stdout.trim() == "0" {
            return SudoAccess::RootAlready;
        }
    }
    // sudo -n true:配了 NOPASSWD 时静默成功;否则因为需要密码而非零退出
    // (-n 保证它立刻失败而不是挂起等输入)
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

fn ssh_dial_target_from_config(config: &ConnectionConfig) -> SshDialTarget {
    SshDialTarget {
        host: config.host.clone(),
        port: config.port,
        username: config.credentials.username().to_string(),
    }
}

// Host trait 实装

#[async_trait]
impl Host for RemoteLinuxHost {
    // ===== 身份 =====
    fn os(&self) -> Os {
        Os::Linux
    }

    fn arch(&self) -> Arch {
        // 远端架构应在 connect 时探测一次缓存,这里简化:默认 X86_64,
        // 实际部署逻辑会在 ncd-component 自己跑 uname -m 决策
        Arch::X86_64
    }

    fn locality(&self) -> Locality {
        Locality::Remote
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn ssh_dial_target(&self) -> Option<SshDialTarget> {
        Some(ssh_dial_target_from_config(&self.config))
    }

    fn shell(&self) -> &dyn HostShell {
        &self.shell
    }

    fn trusted_ssh_host_key(&self) -> Option<String> {
        self.verified_host_key
            .lock()
            .ok()
            .and_then(|key| key.clone())
    }

    async fn install_authorized_key(&self, line: &str, marker: &str) -> Result<(), HostError> {
        let command = super::authorized_keys::linux_key_command(Some(line), marker)?;
        let output = self.run_to_string(command).await?;
        super::authorized_keys::check_output(output, "install_authorized_key")
    }

    async fn remove_authorized_key(&self, marker: &str) -> Result<(), HostError> {
        let command = super::authorized_keys::linux_key_command(None, marker)?;
        let output = self.run_to_string(command).await?;
        super::authorized_keys::check_output(output, "remove_authorized_key")
    }

    async fn secure_private_key(&self, path: &HostPath) -> Result<(), HostError> {
        let output = self
            .run_to_string(HostCommand::new("chmod").arg("600").arg(path.as_posix()))
            .await?;
        super::authorized_keys::check_output(output, "secure_private_key")
    }

    // 只记找到的：没装的命令用户随时可能去装上，记住「没有」反而会一直报缺。
    // PATH 外兜底找到的也记：不记的话 PATH 里没有 bash 的机器每条短脚本都要再探一遍
    async fn which(&self, command: &str) -> Result<Option<String>, HostError> {
        if let Some(hit) = self
            .which_found
            .lock()
            .ok()
            .and_then(|found| found.get(command).cloned())
        {
            return Ok(Some(hit));
        }
        let out = self
            .run_to_string(crate::host::which_path_probe(Os::Linux, command))
            .await?;
        let path = crate::host::first_path_line(&out);
        if let (Some(path), Ok(mut found)) = (&path, self.which_found.lock()) {
            found.insert(command.to_string(), path.clone());
        }
        Ok(path)
    }

    // ===== 进程操作(基于 SSH exec channel)=====

    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        let inner_line = build_remote_command_line(&self.shell, &cmd);
        // elevated 时由 host 注入提权密码(set_elevation_password 存的那份):有密码走
        // sudo -S,把 密码\n 拼在真实 stdin 前面一起喂过去——sudo -S 读首行当密码,
        // 余下字节透传给内层命令(如 tee 收文件内容)没密码退回 sudo -n
        // 非 elevated 路径行为不变:line 是内层命令,stdin 原样透传
        let (line, stdin) = if cmd.elevated {
            let password = self.elevation_password.lock().await.clone();
            let has_pw = password.is_some();
            (
                wrap_with_sudo(&inner_line, &self.shell, has_pw),
                build_elevated_stdin(password.as_deref(), cmd.stdin),
            )
        } else {
            (inner_line, cmd.stdin)
        };
        let timeout = cmd.timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT);

        let self_for_invalidate = self; // 为了 best-effort invalidate 借用
        let exec_fut = async move {
            let _permit = self.acquire_short_exec().await?;
            let mut channel = self.open_session_channel().await?;

            if let Err(e) = channel.exec(true, line.as_bytes()).await {
                self_for_invalidate.invalidate_connection().await;
                return Err(HostError::remote_disconnected(format!("exec: {e}")));
            }

            // 写 stdin(若指定)
            if let Some(data) = stdin {
                if let Err(e) = channel.data(&data[..]).await {
                    self_for_invalidate.invalidate_connection().await;
                    return Err(HostError::remote_disconnected(format!("stdin write: {e}")));
                }
                if let Err(e) = channel.eof().await {
                    self_for_invalidate.invalidate_connection().await;
                    return Err(HostError::remote_disconnected(format!("stdin eof: {e}")));
                }
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
            let password = self.elevation_password.lock().await.clone();
            let has_pw = password.is_some();
            (
                wrap_with_sudo(&inner_line, &self.shell, has_pw),
                build_elevated_stdin(password.as_deref(), cmd.stdin),
            )
        } else {
            (inner_line, cmd.stdin)
        };

        let channel = self.open_session_channel().await?;
        if let Err(e) = channel.exec(true, line.as_bytes()).await {
            self.invalidate_connection().await;
            return Err(HostError::remote_disconnected(format!("exec: {e}")));
        }

        if let Some(data) = stdin {
            if let Err(e) = channel.data(&data[..]).await {
                self.invalidate_connection().await;
                return Err(HostError::remote_disconnected(format!("stdin write: {e}")));
            }
            if let Err(e) = channel.eof().await {
                self.invalidate_connection().await;
                return Err(HostError::remote_disconnected(format!("stdin eof: {e}")));
            }
        }

        Ok(Box::new(RemoteHostProcess {
            channel: Some(channel),
            id: ProcessId {
                native: 0, // SSH exec 不暴露真实 pid;调用方关心的是 channel
                origin: self.id.clone(),
            },
            timeout: cmd.timeout,
            wait_policy: cmd.wait_policy,
        }))
    }

    async fn set_elevation_password(&self, password: Option<String>) {
        *self.elevation_password.lock().await = password;
    }

    async fn has_elevation_password(&self) -> bool {
        self.elevation_password.lock().await.is_some()
    }

    async fn run_streaming(
        &self,
        cmd: HostCommand,
        mut on_line: Box<dyn FnMut(crate::host::StreamSource, String) + Send>,
    ) -> Result<CommandOutput, HostError> {
        use crate::host::StreamSource;

        let deadline = tokio::time::Instant::now() + cmd.timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT);
        let cancel = cmd.cancel.clone();
        let inner_line = build_remote_command_line(&self.shell, &cmd);
        let (line, stdin) = if cmd.elevated {
            let password = streaming_with_deadline(deadline, cancel.as_ref(), async {
                self.elevation_password.lock().await.clone()
            })
            .await?;
            let has_pw = password.is_some();
            (
                wrap_with_sudo(&inner_line, &self.shell, has_pw),
                build_elevated_stdin(password.as_deref(), cmd.stdin),
            )
        } else {
            (inner_line, cmd.stdin)
        };
        let mut channel =
            streaming_with_deadline(deadline, cancel.as_ref(), self.open_session_channel())
                .await??;

        // exec 可能已经送达；准备阶段中断也必须收掉远端进程，不能只丢弃 future。
        let prepare = async {
            if let Err(e) = channel.exec(true, line.as_bytes()).await {
                self.invalidate_connection().await;
                return Err(HostError::remote_disconnected(format!("exec: {e}")));
            }

            if let Some(data) = stdin {
                if let Err(e) = channel.data(&data[..]).await {
                    self.invalidate_connection().await;
                    return Err(HostError::remote_disconnected(format!("stdin write: {e}")));
                }
                if let Err(e) = channel.eof().await {
                    self.invalidate_connection().await;
                    return Err(HostError::remote_disconnected(format!("stdin eof: {e}")));
                }
            }
            Ok(())
        };
        match streaming_with_deadline(deadline, cancel.as_ref(), prepare).await {
            Ok(result) => result?,
            Err(error) => {
                force_kill_remote_channel(&channel).await;
                return Err(error);
            }
        }

        // russh 单路复用流:\n 与 \r 都切逻辑行,docker pull 的 \r 进度才能实时回调
        let mut stdout_buf = Vec::<u8>::new();
        let mut stderr_buf = Vec::<u8>::new();
        let mut stdout_lines = Vec::<String>::new();
        let mut stderr_lines = Vec::<String>::new();
        let mut exit_code: Option<i32> = None;

        loop {
            match streaming_with_deadline(deadline, cancel.as_ref(), channel.wait()).await {
                Ok(Some(msg)) => match msg {
                    ChannelMsg::Data { ref data } => {
                        crate::stream_chunk::feed_stream_chunk(&mut stdout_buf, data, |s| {
                            on_line(StreamSource::Stdout, s.clone());
                            stdout_lines.push(s);
                        });
                    }
                    ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                        crate::stream_chunk::feed_stream_chunk(&mut stderr_buf, data, |s| {
                            on_line(StreamSource::Stderr, s.clone());
                            stderr_lines.push(s);
                        });
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        exit_code = Some(exit_status as i32);
                    }
                    ChannelMsg::Eof => {}
                    ChannelMsg::Close => break,
                    _ => {}
                },
                Ok(None) => break,
                Err(error) => {
                    force_kill_remote_channel(&channel).await;
                    return Err(error);
                }
            }
        }

        crate::stream_chunk::flush_stream_remainder(&mut stdout_buf, |s| {
            on_line(StreamSource::Stdout, s.clone());
            stdout_lines.push(s);
        });
        crate::stream_chunk::flush_stream_remainder(&mut stderr_buf, |s| {
            on_line(StreamSource::Stderr, s.clone());
            stderr_lines.push(s);
        });

        Ok(CommandOutput {
            exit_code,
            stdout: stdout_lines.join("\n"),
            stderr: stderr_lines.join("\n"),
        })
    }

    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        RemoteLinuxHost::open_tunnel(self, spec).await
    }

    async fn open_pty(
        &self,
        req: crate::pty::PtyRequest,
    ) -> Result<crate::pty::PtySession, HostError> {
        self.open_pty_channel(req).await
    }

    async fn invalidate_connection(&self) {
        self.invalidate_main_handle().await;
        self.invalidate_sftp().await;
    }

    async fn is_healthy(&self) -> bool {
        // 不能再 exec `echo ok`：get_live_host / 后台探活每次都会走到这里，
        // 3s 超时的 exec 会跟正在跑的 SFTP/长命令抢同一把 session 锁。
        // 超时 drop channel 会让 russh 报 Packet for unknown recipient，
        // 随后 mark_unhealthy 把还活着的会话毒死，启动就变成 ssh session poisoned。
        match tokio::time::timeout(Duration::from_millis(200), self.handle.read()).await {
            Ok(guard) => session_lock_means_healthy(true, guard.is_some()),
            Err(_) => session_lock_means_healthy(false, false),
        }
    }

    // ===== 文件操作(基于 SFTP)=====

    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        let remote = self.to_remote(path);
        let sftp = self.sftp_session().await?;
        let mut file = match sftp.open(&remote).await {
            Ok(f) => f,
            Err(e) => {
                let err = self.sftp_failure(&sftp, &remote, "read", e).await;
                return Err(err);
            }
        };
        let mut buf = Vec::new();
        self.sftp_io(
            &sftp,
            SFTP_FILE_IO_TIMEOUT,
            "sftp_read",
            file.read_to_end(&mut buf),
        )
        .await?;
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
                let err = self.sftp_failure(&sftp, &remote, "write", e).await;
                return Err(err);
            }
        };
        self.sftp_io(&sftp, SFTP_FILE_IO_TIMEOUT, "sftp_write", async {
            file.write_all(bytes).await?;
            file.flush().await
        })
        .await
    }

    async fn list_dir(&self, path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        let remote = self.to_remote(path);
        let sftp = self.sftp_session().await?;
        let entries = match sftp.read_dir(&remote).await {
            Ok(e) => e,
            Err(e) => {
                let err = self.sftp_failure(&sftp, &remote, "list", e).await;
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
                modified: metadata.mtime.map(i64::from),
                is_symlink: metadata.is_symlink(),
                mode: metadata.permissions,
            });
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        // SFTP 协议没有原生 -p,需要逐级 mkdir简单做法:跑 mkdir -p
        // 走 SFTP 不可靠(权限错可能让 mkdir 半路成功),改 exec 更稳
        let remote = self.to_remote(path);
        let cmd = HostCommand::new("mkdir").arg("-p").arg(remote);
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
                let err = self.sftp_failure(&sftp, &remote, "remove_file", e).await;
                Err(err)
            }
        }
    }

    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), HostError> {
        // 同 create_dir_all,exec 比 SFTP rename 稳:SFTP v3 的 rename 目标已存在时直接失败,各服务端还不一致
        let cmd = HostCommand::new("mv")
            .arg("-f")
            .arg(self.to_remote(from))
            .arg(self.to_remote(to));
        let out = self.run_to_string(cmd).await?;
        if !out.success() {
            return Err(HostError::CommandFailed {
                program: "mv".into(),
                exit_code: out.exit_code,
                stderr: out.stderr,
            });
        }
        Ok(())
    }

    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        // 同 create_dir_all,走 rm -rf 更稳(SFTP 没有 -r remove)
        // 安全约束:测试时调用方应保证传入 path 在白名单内
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
                let err = self.sftp_failure(&sftp, &remote, "exists", e).await;
                Err(err)
            }
        }
    }

    async fn file_size(&self, path: &HostPath) -> Result<Option<u64>, HostError> {
        let remote = self.to_remote(path);
        let sftp = self.sftp_session().await?;
        // SFTP 的 STAT 跟随链接;LSTAT 才是链接本身
        match sftp.metadata(&remote).await {
            Ok(m) if m.is_dir() => Ok(None),
            Ok(m) => Ok(m.size),
            Err(russh_sftp::client::error::Error::Status(s))
                if s.status_code == russh_sftp::protocol::StatusCode::NoSuchFile =>
            {
                Ok(None)
            }
            Err(e) => {
                let err = self.sftp_failure(&sftp, &remote, "file_size", e).await;
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

        let mut source = tokio::fs::File::open(local).await.map_err(HostError::Io)?;
        let sftp = self.sftp_session().await?;
        let mut file = match sftp.create(&remote_str).await {
            Ok(f) => f,
            Err(e) => {
                let err = self.sftp_failure(&sftp, &remote_str, "upload", e).await;
                return Err(err);
            }
        };
        self.sftp_io(&sftp, DEFAULT_COMMAND_TIMEOUT, "sftp_upload", async {
            tokio::io::copy(&mut source, &mut file).await?;
            file.flush().await
        })
        .await
    }

    async fn download(&self, remote: &HostPath, local: &Path) -> Result<(), HostError> {
        let remote_str = self.to_remote(remote);
        let sftp = self.sftp_session().await?;
        let mut file = match sftp.open(&remote_str).await {
            Ok(f) => f,
            Err(e) => {
                let err = self.sftp_failure(&sftp, &remote_str, "download", e).await;
                return Err(err);
            }
        };
        if let Some(parent) = local.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        let mut destination = tokio::fs::File::create(local)
            .await
            .map_err(HostError::Io)?;
        self.sftp_io(&sftp, DEFAULT_COMMAND_TIMEOUT, "sftp_download", async {
            tokio::io::copy(&mut file, &mut destination).await?;
            destination.flush().await
        })
        .await
    }

    async fn extract_archive(
        &self,
        archive: &HostPath,
        dest: &HostPath,
        kind: ArchiveKind,
    ) -> Result<(), HostError> {
        // 远端解压走 shellzip 要 unzip(很多最小化镜像不自带),tar.* 要 tar
        // (几乎都自带)解压前先确保对应工具在,缺了就尝试用包管理器装一下,
        // 省得用户撞上 "unzip: command not found" 的 exit 127 一头雾水
        let archive_str = self.to_remote(archive);
        let dest_str = self.to_remote(dest);

        // 确保目标目录存在
        self.create_dir_all(dest).await?;

        let cmd = match kind {
            ArchiveKind::TarGz => {
                self.ensure_archive_tool("tar").await?;
                HostCommand::new("tar")
                    .arg("-xzf")
                    .arg(&archive_str)
                    .arg("-C")
                    .arg(&dest_str)
            }
            ArchiveKind::TarXz => {
                self.ensure_archive_tool("tar").await?;
                HostCommand::new("tar")
                    .arg("-xJf")
                    .arg(&archive_str)
                    .arg("-C")
                    .arg(&dest_str)
            }
            ArchiveKind::Zip => {
                self.ensure_archive_tool("unzip").await?;
                HostCommand::new("unzip")
                    .arg("-q")
                    .arg("-o")
                    .arg(&archive_str)
                    .arg("-d")
                    .arg(&dest_str)
            }
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

    async fn download_url(&self, url: &str, dest: &HostPath) -> Result<(), HostError> {
        let has_wget = self.command_exists("wget").await;
        let has_curl = !has_wget && self.command_exists("curl").await;

        if !has_wget && !has_curl {
            return Err(HostError::Unsupported {
                operation: "download_url (wget/curl not found)",
            });
        }

        let dest_str = self.to_remote(dest);
        let (program, cmd) = if has_wget {
            (
                "wget",
                super::url_download::wget_url_download_command(url, &dest_str),
            )
        } else {
            (
                "curl",
                super::url_download::curl_url_download_command(url, &dest_str),
            )
        };

        // 无进度回调场景用 run_to_string 即可；带 UI 进度由上层自行
        // run_streaming + 同款命令构造函数解析输出
        let out = self.run_to_string(cmd).await?;

        if !out.success() {
            return Err(HostError::CommandFailed {
                program: program.into(),
                exit_code: out.exit_code,
                stderr: out.stderr.lines().take(5).collect::<Vec<_>>().join("\n"),
            });
        }

        Ok(())
    }
}

// 辅助: 命令拼接 + SFTP 打开 + 错误映射

async fn streaming_with_deadline<T>(
    deadline: tokio::time::Instant,
    cancel: Option<&tokio_util::sync::CancellationToken>,
    future: impl std::future::Future<Output = T>,
) -> Result<T, HostError> {
    let cancelled = async {
        match cancel {
            Some(token) => token.cancelled().await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        biased;
        _ = cancelled => Err(HostError::Cancelled),
        _ = tokio::time::sleep_until(deadline) => Err(HostError::Timeout {
            operation: "remote_run_streaming",
        }),
        output = future => Ok(output),
    }
}

/// 远端 exec 强杀:SSH SIGKILL + 限时 close。
/// 只 close 时,部分 OpenSSH/docker pull 场景 channel.close 会一直等进程结束,
/// 任务队列「强制停止」就会表现为点了没反应。
pub(super) async fn force_kill_remote_channel(channel: &russh::Channel<russh::client::Msg>) {
    // 强制停止路径直接 KILL;信号失败也继续 close
    let _ = tokio::time::timeout(Duration::from_secs(2), channel.signal(Sig::KILL)).await;
    let close = channel.close();
    match tokio::time::timeout(Duration::from_secs(2), close).await {
        Ok(Ok(())) => {}
        Ok(Err(_)) | Err(_) => {
            // close 挂起或失败:放弃等待,让上层立刻返回 Cancelled/Timeout
            tracing::debug!(
                target: "ncd_host::remote",
                "remote channel force-kill: close timed out or failed"
            );
        }
    }
}

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

/// 把内层命令行包成 sudo 提权命令inner_line 可能含 cd && 这类 shell 语法,
/// 所以整体丢给 sh -c <inner> 跑,而不是让 sudo 直接 exec 单个程序
/// has_password 决定喂密码的方式:
///   true  → sudo -S -p '' sh -c <inner>,-S 从 stdin 读密码,-p '' 清空提示符
///           避免提示文字混进 stderr 干扰输出解析(密码字节另由调用方走 stdin 送)
///   false → sudo -n -p '' sh -c <inner>,-n 非交互:免密(NOPASSWD/root)直接过,
///           真要密码时立刻失败而不是挂起等输入
/// inner 用 shell.escape 转义成单个 token,内部空格 / 分号 / 引号都被安全包裹
fn wrap_with_sudo(inner_line: &str, shell: &dyn HostShell, has_password: bool) -> String {
    let mode = if has_password { "-S" } else { "-n" };
    let inner = shell.escape(inner_line);
    format!("sudo {mode} -p '' sh -c {inner}")
}

/// sudo -S 读的密码必须以换行结尾调用方已带 \n 就不重复加,否则补一个
/// `lock_acquired`: 是否在短超时内拿到 session 锁。
/// 拿不到锁说明有命令正在用这条 SSH，视为活着；拿到锁则看句柄是否还在。
fn session_lock_means_healthy(lock_acquired: bool, handle_present: bool) -> bool {
    if lock_acquired { handle_present } else { true }
}

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

/// 拼 elevated 命令喂给 SSH channel 的 stdin有提权密码时 密码\n 打头(sudo -S
/// 读首行当密码),后面接命令本身的 stdin(如 tee 收的文件内容)透传给内层命令;
/// 没密码时(免密/root,走 sudo -n)原样返回命令 stdin
fn build_elevated_stdin(password: Option<&str>, cmd_stdin: Option<Vec<u8>>) -> Option<Vec<u8>> {
    match password {
        Some(pw) => {
            let mut buf = ensure_trailing_newline(pw.as_bytes());
            if let Some(data) = cmd_stdin {
                buf.extend_from_slice(&data);
            }
            Some(buf)
        }
        None => cmd_stdin,
    }
}

async fn open_sftp(host: &RemoteLinuxHost) -> Result<russh_sftp::client::SftpSession, HostError> {
    let channel = host.open_session_channel().await?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| HostError::remote_disconnected(format!("sftp subsystem: {e}")))?;
    russh_sftp::client::SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| sftp_err_to_host("SFTP session", "initialize", e))
}

pub(super) fn sftp_err_to_host(
    path: &str,
    op: &'static str,
    e: russh_sftp::client::error::Error,
) -> HostError {
    use russh_sftp::client::error::Error as SftpErr;
    use russh_sftp::protocol::StatusCode;
    match e {
        // An unanswered SFTP request does not prove the multiplexed SSH transport died.
        SftpErr::Timeout => HostError::Timeout { operation: "sftp" },
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
        // 子系统失败不证明复用的 SSH 连接已断，文件操作只重建该 SFTP session。
        other => HostError::Io(std::io::Error::other(format!("sftp {op} {path}: {other}"))),
    }
}

// 收集 channel 输出(用于 run_to_string)

pub(super) async fn collect_channel_output(
    channel: &mut russh::Channel<russh::client::Msg>,
) -> Result<CommandOutput, HostError> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut exit_code: Option<i32> = None;

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
            ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                stderr.extend_from_slice(data);
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit_code = Some(exit_status as i32);
                // 不立即 break:等 server 主动 Close 或 Eof,让任何未收完的 Data 包都收到
            }
            ChannelMsg::Eof => {
                // server 写完 stdout/stderr,但 ExitStatus 可能还没到继续等
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

// RemoteHostProcess: russh::Channel 包装成 HostProcess

struct RemoteHostProcess {
    channel: Option<russh::Channel<russh::client::Msg>>,
    id: ProcessId,
    timeout: Option<Duration>,
    wait_policy: HostProcessWaitPolicy,
}

#[async_trait]
impl HostProcess for RemoteHostProcess {
    fn id(&self) -> ProcessId {
        self.id.clone()
    }

    async fn wait(mut self: Box<Self>) -> Result<CommandOutput, HostError> {
        let mut channel = self
            .channel
            .take()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "remote channel already consumed".into(),
            })?;
        match self.wait_policy.resolve_timeout(self.timeout) {
            Some(timeout) => {
                match tokio::time::timeout(timeout, collect_channel_output(&mut channel)).await {
                    Ok(out) => out,
                    Err(_) => Err(HostError::Timeout {
                        operation: "remote_process_wait",
                    }),
                }
            }
            None => collect_channel_output(&mut channel).await,
        }
    }

    async fn try_wait(&mut self) -> Result<ExitStatus, HostError> {
        // SSH exec 没有"快速 try_wait":没有快速 fd polling
        // 简化:用 poll 试一次 channel.wait() with 0 timeout(tokio 的 timeout)
        let channel = self
            .channel
            .as_mut()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "remote channel already consumed".into(),
            })?;
        match tokio::time::timeout(Duration::from_millis(1), channel.wait()).await {
            Ok(Some(ChannelMsg::ExitStatus { exit_status })) => {
                Ok(ExitStatus::Exited(exit_status as i32))
            }
            Ok(Some(ChannelMsg::Close)) => Ok(ExitStatus::Killed),
            Ok(Some(_)) => Ok(ExitStatus::Running),
            Ok(None) => Ok(ExitStatus::Running),
            Err(_) => Ok(ExitStatus::Running),
        }
    }

    async fn kill(&mut self) -> Result<(), HostError> {
        let channel = self
            .channel
            .as_mut()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "remote channel already consumed".into(),
            })?;
        // signal + 限时 close;裸 close 对 docker pull 等可能挂死
        force_kill_remote_channel(channel).await;
        Ok(())
    }

    async fn write_stdin(&mut self, data: &[u8]) -> Result<(), HostError> {
        let channel = self
            .channel
            .as_mut()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "remote channel already consumed".into(),
            })?;
        channel
            .data(data)
            .await
            .map_err(|e| HostError::remote_disconnected(format!("stdin write: {e}")))?;
        Ok(())
    }

    async fn close_stdin(&mut self) -> Result<(), HostError> {
        let channel = self
            .channel
            .as_mut()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "remote channel already consumed".into(),
            })?;
        channel
            .eof()
            .await
            .map_err(|e| HostError::remote_disconnected(format!("stdin eof: {e}")))?;
        Ok(())
    }
}

// Tunnel (端口转发, direct-tcpip)

impl RemoteLinuxHost {
    /// 打开端口转发：`-L` 本机听、`-R` 远端听
    pub async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        match spec.direction {
            TunnelDirection::LocalToRemote => self.open_local_to_remote(spec).await,
            TunnelDirection::RemoteToLocal => self.open_remote_to_local(spec).await,
        }
    }

    async fn open_local_to_remote(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
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
                                let session_guard = h.read().await;
                                let session = match session_guard.as_ref() {
                                    Some(s) => s,
                                    None => {
                                        tracing::warn!(target: "ncd_host::tunnel", "tunnel open skipped: session poisoned");
                                        return;
                                    }
                                };
                                session
                                    .channel_open_direct_tcpip(
                                        remote_host_for_conn.clone(),
                                        remote_port as u32,
                                        originator_for_conn,
                                        peer.port() as u32,
                                    )
                                    .await
                            };
                            let channel = match channel {
                                Ok(c) => c,
                                Err(e) => {
                                    tracing::warn!(
                                        target: "ncd_host::tunnel",
                                        remote_host = %remote_host_for_conn,
                                        remote_port,
                                        "open direct-tcpip: {e}"
                                    );
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
            remote_listen_port: 0,
            shutdown,
            closed: Default::default(),
            _task: task,
        })
    }

    async fn open_remote_to_local(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        if spec.local_port == 0 {
            return Err(HostError::InvalidArgument {
                reason: "remote-forward 本机目标端口无效".into(),
            });
        }
        let requested = spec.remote_port as u32;
        let replied = {
            let mut guard = self.handle.write().await;
            let session = guard.as_mut().ok_or_else(|| HostError::RemoteConnection {
                reason: "ssh session poisoned".into(),
            })?;
            session
                .tcpip_forward(spec.remote_host.clone(), requested)
                .await
                .map_err(|e| HostError::RemoteConnection {
                    reason: format!("tcpip-forward（检查 sshd AllowTcpForwarding）: {e}"),
                })?
        };
        let remote_listen = if requested == 0 {
            replied as u16
        } else {
            spec.remote_port
        };
        if remote_listen == 0 {
            return Err(HostError::RemoteConnection {
                reason: "sshd 未分配远端转发端口（检查 AllowTcpForwarding）".into(),
            });
        }
        self.remote_forwards.lock().await.insert(
            (spec.remote_host.clone(), remote_listen),
            (spec.local_host.clone(), spec.local_port),
        );

        let shutdown = Arc::new(Notify::new());
        let shutdown_for_task = shutdown.clone();
        let handle = self.handle.clone();
        let table = self.remote_forwards.clone();
        let addr = spec.remote_host.clone();
        let task = tokio::spawn(async move {
            shutdown_for_task.notified().await;
            table.lock().await.remove(&(addr.clone(), remote_listen));
            let guard = handle.read().await;
            if let Some(session) = guard.as_ref() {
                if let Err(e) = session
                    .cancel_tcpip_forward(addr, remote_listen as u32)
                    .await
                {
                    tracing::debug!(target: "ncd_host::tunnel", "cancel tcpip-forward: {e}");
                }
            }
        });

        Ok(TunnelHandle {
            local_port: spec.local_port,
            remote_listen_port: remote_listen,
            shutdown,
            closed: Default::default(),
            _task: task,
        })
    }
}

// 单测: sudo 命令拼接是纯逻辑,从 async 方法里抽出来直接断言
// 真正的 SSH 往返不在这测(需要活的远端),只锁定命令行字符串形状

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_with_password_uses_dash_s_and_sh_c() {
        let line = wrap_with_sudo("apt-get install -y docker.io", &BashShell, true);
        assert!(
            line.contains("sudo -S -p ''"),
            "有密码必须用 -S 从 stdin 读: {line}"
        );
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
        // 否则空格 / && 会被外层 sudo 命令行拆散
        let inner = "cd /opt/napcat && apt-get install -y docker";
        let line = wrap_with_sudo(inner, &BashShell, true);
        let escaped = BashShell.escape(inner);
        assert!(
            line.ends_with(&escaped),
            "内层应原样 escape 追加在末尾: {line}"
        );
        assert!(
            escaped.starts_with('\''),
            "含空格的内层必须被单引号包裹: {escaped}"
        );
    }

    #[test]
    fn wrap_escapes_inner_with_semicolon() {
        let inner = "echo hi; rm -rf /tmp/x";
        let line = wrap_with_sudo(inner, &BashShell, false);
        // 分号必须落在单引号内,不能泄漏成外层 sudo 命令行的语句分隔符
        assert!(
            line.contains("'echo hi; rm -rf /tmp/x'"),
            "分号应被单引号包住: {line}"
        );
    }

    #[test]
    fn ensure_newline_appends_when_missing() {
        assert_eq!(ensure_trailing_newline(b"hunter2"), b"hunter2\n");
    }

    #[test]
    fn ensure_newline_keeps_single_when_present() {
        assert_eq!(ensure_trailing_newline(b"hunter2\n"), b"hunter2\n");
    }

    #[test]
    fn elevated_stdin_prefixes_password_then_passes_through_inner_stdin() {
        // 有密码 + 命令本身有 stdin(如 tee 收文件内容):密码\n 打头,后接文件内容
        // sudo -S 吃掉首行密码,余下字节正好透传给内层 tee
        let out = build_elevated_stdin(Some("hunter2"), Some(b"file body".to_vec()));
        assert_eq!(out.unwrap(), b"hunter2\nfile body");
    }

    #[test]
    fn elevated_stdin_password_only_when_no_inner_stdin() {
        // 有密码但命令没有 stdin(如 apt-get install):只喂 密码\n
        let out = build_elevated_stdin(Some("hunter2"), None);
        assert_eq!(out.unwrap(), b"hunter2\n");
    }

    #[test]
    fn elevated_stdin_passes_through_unchanged_without_password() {
        // 没密码(免密/root,走 sudo -n):命令 stdin 原样透传,不掺密码
        assert_eq!(
            build_elevated_stdin(None, Some(b"file body".to_vec())).unwrap(),
            b"file body"
        );
        assert!(build_elevated_stdin(None, None).is_none());
    }

    #[test]
    fn health_busy_session_is_alive() {
        assert!(session_lock_means_healthy(false, false));
        assert!(session_lock_means_healthy(true, true));
        assert!(!session_lock_means_healthy(true, false));
    }

    #[test]
    fn ssh_dial_target_exposes_endpoint_not_secrets() {
        let cfg = ConnectionConfig::new(
            "bot.example",
            2222,
            SshCredentials::password("alice", "secret"),
            HostKeyPolicy::Insecure,
        );
        let dial = ssh_dial_target_from_config(&cfg);
        assert_eq!(dial.host, "bot.example");
        assert_eq!(dial.port, 2222);
        assert_eq!(dial.username, "alice");
        let dbg = format!("{dial:?}");
        assert!(!dbg.contains("secret"), "{dbg}");
    }
}
