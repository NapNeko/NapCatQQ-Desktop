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
use russh::ChannelMsg;
use russh::client::{self, Handle as ClientHandle, Handler};
use russh::keys::{PublicKeyBase64, key};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Notify};
use tracing::info;

use crate::command::{CommandOutput, DEFAULT_COMMAND_TIMEOUT, HostCommand, HostProcessWaitPolicy};
use crate::error::HostError;
use crate::host::{Arch, Host, Locality, Os};
use crate::package_manager::PackageManager;
use crate::path::{ArchiveKind, DirEntry, HostPath, PathStyle};
use crate::process::{ExitStatus, HostProcess, ProcessId};
use crate::shell::{BashShell, HostShell};

use super::connection::ConnectionConfig;
use super::credentials::{SshCredentials, SshKey};
use super::host_key::{HostKeyCheck, HostKeyPolicy, KnownHostsStore};
use super::tunnel::{TunnelHandle, TunnelSpec};

/// russh client handler:用于 host key 校验。
struct ClientCallback {
    policy: HostKeyPolicy,
    host: String,
    port: u16,
    host_key_error: Arc<Mutex<Option<HostError>>>,
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
    /// 提权密码,这台主机所有 elevated 命令共用。connect 后由 ServerManager 从
    /// keyring 注入(密码登录机器有登录密码,密钥登录机器可能有挪存的 sudo 密码),
    /// docker 弹框拿到新密码时覆盖。有密码 → elevated 走 sudo -S 喂 stdin;None →
    /// 退回 sudo -n(root / 免密直接过,真要密码立刻失败而非挂起)。
    /// 用 Mutex 包,因为 set_elevation_password 是 &self 异步方法(trait 约束),
    /// 且密码可能在连接生命周期内被 docker 弹框更新。
    elevation_password: Arc<Mutex<Option<String>>>,
    /// 连接配置。当前断线后由上层 ServerManager 重新 connect,本结构体内部
    /// 还没用到它重连,先留着等断线自愈实装。
    #[allow(dead_code)]
    config: ConnectionConfig,
}

impl RemoteLinuxHost {
    /// 建立 SSH 连接并完成认证。
    pub async fn connect(
        id: impl Into<String>,
        config: ConnectionConfig,
    ) -> Result<Self, HostError> {
        let host_key_error = Arc::new(Mutex::new(None));
        let cb = ClientCallback {
            policy: config.host_key_policy.clone(),
            host: config.host.clone(),
            port: config.port,
            host_key_error: Arc::clone(&host_key_error),
        };
        let mut russh_cfg = client::Config::default();
        // 长任务（apt 等锁、流式安装）需要更长的会话空闲上限；仅靠 keepalive 时
        // 默认 inactivity=2×keepalive 在部分 sshd/中间设备上仍可能被掐断。
        russh_cfg.inactivity_timeout = match config.keepalive_interval {
            Some(d) if d <= Duration::from_secs(20) => Some(Duration::from_secs(900)),
            Some(d) => Some(d.saturating_mul(4)),
            None => None,
        };
        russh_cfg.keepalive_interval = config.keepalive_interval;
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
            handle: Arc::new(Mutex::new(handle)),
            sftp: Arc::new(Mutex::new(None)),
            elevation_password: Arc::new(Mutex::new(None)),
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

    /// 确保解压工具(unzip / tar)在远端可用,缺了就用包管理器装一次。
    ///
    /// 流程:先 command_exists 探,有就直接过(快路径,装好的机器零开销)。没有
    /// 就探包管理器(apt-get / dnf / yum / apk),用 install_archive_tool 提权装,
    /// 装完再探一次确认。探不到包管理器、或装完仍不在,就报带人话指引的 ExtractFailed
    /// (告诉用户手动 apt-get install <tool>),不把人留在 exit 127 现场。
    async fn ensure_archive_tool(&self, tool: &str) -> Result<(), HostError> {
        if self.command_exists(tool).await {
            return Ok(());
        }

        let Some(pm) = self.detect_package_manager().await else {
            return Err(HostError::ExtractFailed {
                archive: HostPath::from_posix(tool),
                reason: format!(
                    "远端缺少 {tool} 且未识别到包管理器,请手动安装(如 apt-get install {tool})后重试"
                ),
            });
        };

        // 装包要 root。判定能不能提权:root / 免密 sudo 直接行;否则看 host 有没有
        // 注入提权密码(ServerManager 从 keyring 注入的登录/ sudo 密码),有就能走
        // sudo -S 装。两者都没有才退回让用户手动装的人话提示,而不是静默挂起。
        let access = probe_sudo(self).await;
        let has_password = self.elevation_password.lock().await.is_some();
        let elevation_ok =
            matches!(access, SudoAccess::RootAlready | SudoAccess::Passwordless) || has_password;
        if !elevation_ok {
            return Err(HostError::ExtractFailed {
                archive: HostPath::from_posix(tool),
                reason: format!(
                    "远端缺少 {tool},自动安装需要 root 权限但当前无免密 sudo 也未保存密码,请去远端页配置免密或手动执行 sudo {} 后重试",
                    pm.install_hint(tool)
                ),
            });
        }

        let install_line = pm.install_command(tool);
        let cmd = HostCommand::new("sh")
            .arg("-c")
            .arg(&install_line)
            .elevated()
            .timeout(Duration::from_secs(180));
        let _ = self.run_to_string(cmd).await;

        if self.command_exists(tool).await {
            Ok(())
        } else {
            Err(HostError::ExtractFailed {
                archive: HostPath::from_posix(tool),
                reason: format!(
                    "已尝试自动安装 {tool} 但仍不可用,请登录远端手动执行 sudo {} 后重试",
                    pm.install_hint(tool)
                ),
            })
        }
    }

    /// 探测远端用哪个包管理器。按常见度顺序探,探到第一个就用。
    async fn detect_package_manager(&self) -> Option<PackageManagerKindLite> {
        for pm in PackageManagerKindLite::ALL {
            if self.command_exists(pm.binary()).await {
                return Some(*pm);
            }
        }
        None
    }
}

/// extract 自动装依赖用的轻量包管理器枚举。不复用 PackageManager trait:那套是
/// 给"装业务包"的完整抽象,这里只需要"探在不在 + 拼一条非交互装包命令"两件事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManagerKindLite {
    Apt,
    Dnf,
    Yum,
    Apk,
    Pacman,
}

impl PackageManagerKindLite {
    const ALL: &'static [PackageManagerKindLite] =
        &[Self::Apt, Self::Dnf, Self::Yum, Self::Apk, Self::Pacman];

    fn binary(self) -> &'static str {
        match self {
            Self::Apt => "apt-get",
            Self::Dnf => "dnf",
            Self::Yum => "yum",
            Self::Apk => "apk",
            Self::Pacman => "pacman",
        }
    }

    /// 非交互安装命令(已含自动确认参数)。apt 先 update 一把,否则全新机器
    /// 可能因没有包索引而找不到包。
    fn install_command(self, pkg: &str) -> String {
        match self {
            Self::Apt => format!("apt-get update && apt-get install -y {pkg}"),
            Self::Dnf => format!("dnf install -y {pkg}"),
            Self::Yum => format!("yum install -y {pkg}"),
            Self::Apk => format!("apk add --no-cache {pkg}"),
            Self::Pacman => format!("pacman -Sy --noconfirm {pkg}"),
        }
    }

    /// 给用户看的手动安装提示(去掉 update 前缀,精简)。
    fn install_hint(self, pkg: &str) -> String {
        match self {
            Self::Apt => format!("apt-get install -y {pkg}"),
            Self::Dnf => format!("dnf install -y {pkg}"),
            Self::Yum => format!("yum install -y {pkg}"),
            Self::Apk => format!("apk add {pkg}"),
            Self::Pacman => format!("pacman -S {pkg}"),
        }
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
        // elevated 时由 host 注入提权密码(set_elevation_password 存的那份):有密码走
        // sudo -S,把 `密码\n` 拼在真实 stdin 前面一起喂过去——sudo -S 读首行当密码,
        // 余下字节透传给内层命令(如 tee 收文件内容)。没密码退回 sudo -n。
        // 非 elevated 路径行为不变:line 是内层命令,stdin 原样透传。
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
            let password = self.elevation_password.lock().await.clone();
            let has_pw = password.is_some();
            (
                wrap_with_sudo(&inner_line, &self.shell, has_pw),
                build_elevated_stdin(password.as_deref(), cmd.stdin),
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
        let timeout = cmd.timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT);

        // channel 开启和 stdin 写入在 timeout 外做（通常很快），主循环套 timeout。
        let session = self.handle.lock().await;
        let mut channel = session
            .channel_open_session()
            .await
            .map_err(|e| HostError::remote_disconnected(format!("open channel: {e}")))?;
        drop(session);

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

        // russh 单路复用流：\n 与 \r 都切逻辑行，docker pull 的 \r 进度才能实时回调。
        let mut stdout_buf = Vec::<u8>::new();
        let mut stderr_buf = Vec::<u8>::new();
        let mut stdout_lines = Vec::<String>::new();
        let mut stderr_lines = Vec::<String>::new();
        let mut exit_code: Option<i32> = None;

        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            match tokio::time::timeout_at(deadline, channel.wait()).await {
                Ok(Some(msg)) => match msg {
                    ChannelMsg::Data { ref data } => {
                        crate::stream_chunk::feed_stream_chunk(
                            &mut stdout_buf,
                            data,
                            |s| {
                                on_line(StreamSource::Stdout, s.clone());
                                stdout_lines.push(s);
                            },
                        );
                    }
                    ChannelMsg::ExtendedData { ref data, ext } if ext == 1 => {
                        crate::stream_chunk::feed_stream_chunk(
                            &mut stderr_buf,
                            data,
                            |s| {
                                on_line(StreamSource::Stderr, s.clone());
                                stderr_lines.push(s);
                            },
                        );
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        exit_code = Some(exit_status as i32);
                    }
                    ChannelMsg::Eof => {}
                    ChannelMsg::Close => break,
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => {
                    return Err(HostError::Timeout {
                        operation: "remote_run_streaming",
                    });
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
        // 远端解压走 shell。zip 要 unzip(很多最小化镜像不自带),tar.* 要 tar
        // (几乎都自带)。解压前先确保对应工具在,缺了就尝试用包管理器装一下,
        // 省得用户撞上 "unzip: command not found" 的 exit 127 一头雾水。
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
        // 检测 wget 或 curl
        let has_wget = self.command_exists("wget").await;
        let has_curl = !has_wget && self.command_exists("curl").await;

        if !has_wget && !has_curl {
            return Err(HostError::Unsupported {
                operation: "download_url (wget/curl not found)",
            });
        }

        let dest_str = self.to_remote(dest);

        // 构建下载命令
        let cmd = if has_wget {
            HostCommand::new("wget")
                .arg("--progress=dot:mega")
                .arg("-O")
                .arg(&dest_str)
                .arg(url)
        } else {
            HostCommand::new("curl")
                .arg("--progress-bar")
                .arg("-fL") // -f: fail on HTTP errors, -L: follow redirects
                .arg("-o")
                .arg(&dest_str)
                .arg(url)
        };

        // 执行下载（暂不解析进度，先实现基础功能）
        let out = self.run_to_string(cmd).await?;

        if !out.success() {
            return Err(HostError::CommandFailed {
                program: if has_wget { "wget".into() } else { "curl".into() },
                exit_code: out.exit_code,
                stderr: out.stderr.lines().take(5).collect::<Vec<_>>().join("\n"),
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

/// 拼 elevated 命令喂给 SSH channel 的 stdin。有提权密码时 `密码\n` 打头(sudo -S
/// 读首行当密码),后面接命令本身的 stdin(如 tee 收的文件内容)透传给内层命令;
/// 没密码时(免密/root,走 sudo -n)原样返回命令 stdin。
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
        // SSH exec 没有"快速 try_wait":没有快速 fd polling。
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
        // SSH 没有标准 SIGKILL 跨服务器请求,尝试 close channel
        channel
            .close()
            .await
            .map_err(|e| HostError::remote_disconnected(format!("close channel: {e}")))?;
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
        // 否则空格 / && 会被外层 sudo 命令行拆散。
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
        // 分号必须落在单引号内,不能泄漏成外层 sudo 命令行的语句分隔符。
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
        // 有密码 + 命令本身有 stdin(如 tee 收文件内容):密码\n 打头,后接文件内容。
        // sudo -S 吃掉首行密码,余下字节正好透传给内层 tee。
        let out = build_elevated_stdin(Some("hunter2"), Some(b"file body".to_vec()));
        assert_eq!(out.unwrap(), b"hunter2\nfile body");
    }

    #[test]
    fn elevated_stdin_password_only_when_no_inner_stdin() {
        // 有密码但命令没有 stdin(如 apt-get install):只喂 `密码\n`。
        let out = build_elevated_stdin(Some("hunter2"), None);
        assert_eq!(out.unwrap(), b"hunter2\n");
    }

    #[test]
    fn elevated_stdin_passes_through_unchanged_without_password() {
        // 没密码(免密/root,走 sudo -n):命令 stdin 原样透传,不掺密码。
        assert_eq!(
            build_elevated_stdin(None, Some(b"file body".to_vec())).unwrap(),
            b"file body"
        );
        assert!(build_elevated_stdin(None, None).is_none());
    }

    #[test]
    fn pkg_install_command_is_noninteractive() {
        // 自动装依赖必须非交互(带 -y/--noconfirm),否则在 SSH 非交互会话里会挂起等输入。
        assert_eq!(
            PackageManagerKindLite::Apt.install_command("unzip"),
            "apt-get update && apt-get install -y unzip"
        );
        assert_eq!(
            PackageManagerKindLite::Dnf.install_command("tar"),
            "dnf install -y tar"
        );
        assert_eq!(
            PackageManagerKindLite::Apk.install_command("unzip"),
            "apk add --no-cache unzip"
        );
        assert_eq!(
            PackageManagerKindLite::Pacman.install_command("tar"),
            "pacman -Sy --noconfirm tar"
        );
    }

    #[test]
    fn pkg_detection_order_prefers_apt() {
        // 探测顺序按常见度,apt 在最前(Debian/Ubuntu 占多数)。
        assert_eq!(PackageManagerKindLite::ALL[0], PackageManagerKindLite::Apt);
        assert_eq!(PackageManagerKindLite::Apt.binary(), "apt-get");
        assert_eq!(PackageManagerKindLite::Apk.binary(), "apk");
    }
}
