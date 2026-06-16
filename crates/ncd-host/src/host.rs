//! `Host` trait:跨主机操作的统一接口。
//!
//! 把"一台机器"抽象成统一接口,上层 Component / Deploy / Backend 通过它完成
//! 所有"跑命令、传文件、装组件"操作。
//!
//! 实装矩阵:
//! - `LocalWindowsHost`:本地 Windows 实装(基于 std::fs + tokio::process)
//! - `RemoteLinuxHost`:远端 Linux 实装(基于 russh + russh-sftp)
//! - `RemoteWindowsHost`:接口 stub,所有方法返回 `HostError::Unsupported`
//! - 未来 `LocalLinuxHost` / `LocalMacOsHost` / `DockerHost` / `AgentHost`

use async_trait::async_trait;
use bytes::Bytes;
use std::path::Path;

use crate::command::{CommandOutput, HostCommand};
use crate::error::HostError;
use crate::package_manager::PackageManager;
use crate::path::{ArchiveKind, DirEntry, HostPath};
use crate::process::HostProcess;
use crate::shell::HostShell;

/// 主机所在操作系统。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum Os {
    Windows,
    Linux,
    MacOs,
}

/// 主机 CPU 架构。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum Arch {
    X86_64,
    Aarch64,
    X86,
    Armv7,
}

/// 本地或远端。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum Locality {
    Local,
    Remote,
}

/// 流式命令输出的来源通道。`run_streaming` 回调每行时带上，调用方据此区分
/// stdout / stderr（docker pull 进度走 stdout，compose 日志走 stderr）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSource {
    Stdout,
    Stderr,
}

/// 跨平台主机统一接口。
///
/// 调用方使用模式:
/// ```ignore
/// async fn install_napcat(host: &dyn Host) -> Result<(), HostError> {
///     match host.os() {
///         Os::Windows => host.spawn(HostCommand::new("powershell")
///             .arg("-Command")
///             .arg("Expand-Archive napcat.zip -DestinationPath 'C:/NapCat'")).await?.wait().await?,
///         Os::Linux => host.spawn(HostCommand::new("tar")
///             .arg("-xzf").arg("napcat.tar.gz")
///             .arg("-C").arg("/opt/napcat")).await?.wait().await?,
///         Os::MacOs => return Err(HostError::Unsupported { operation: "napcat-install-macos" }),
///     };
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait Host: Send + Sync {
    // ===== 身份信息(实装时探测一次缓存) =====

    /// 主机所在 OS。
    fn os(&self) -> Os;

    /// 主机 CPU 架构。
    fn arch(&self) -> Arch;

    /// 本地或远端。
    fn locality(&self) -> Locality;

    /// 主机标识(local / remote-<server-id>),用于跨主机区分日志、进程 ID。
    fn id(&self) -> &str;

    /// 拿到 shell 抽象(用于命令拼接 / SSH 远端)。
    fn shell(&self) -> &dyn HostShell;

    /// 拿到包管理器(若该主机配置了)。
    /// 调用方根据返回值是否 None 决定走包管理器路径还是手动下载路径。
    fn pkg_manager(&self) -> Option<&dyn PackageManager>;

    // ===== 文件操作 =====

    /// 读文件全部内容到内存。
    /// 调用方应保证文件 < 64 MB,大文件请用 [`Self::download`]。
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError>;

    /// 写文件(覆盖或新建)。
    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError>;

    /// 列目录。
    async fn list_dir(&self, path: &HostPath) -> Result<Vec<DirEntry>, HostError>;

    /// 创建目录(含父目录)。
    async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError>;

    /// 删除文件。
    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError>;

    /// 递归删除目录。
    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), HostError>;

    /// 检查路径是否存在。
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError>;

    /// 上传本地文件到主机(本地 Host 等同于 copy)。
    async fn upload(&self, local: &Path, remote: &HostPath) -> Result<(), HostError>;

    /// 从主机下载文件到本地(本地 Host 等同于 copy)。
    async fn download(&self, remote: &HostPath, local: &Path) -> Result<(), HostError>;

    /// 在主机上从 URL 下载文件到指定路径。
    ///
    /// - 本地主机：委托到本地文件系统下载
    /// - 远程 Linux：优先使用 wget/curl 直接下载
    /// - 远程 Windows Stub：返回 Unsupported
    ///
    /// 默认实现返回 `Unsupported`，让调用方 fallback 到"本地下载→upload"。
    async fn download_url(
        &self,
        _url: &str,
        _dest: &HostPath,
    ) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "download_url",
        })
    }

    /// 解压归档(zip / tar.gz / tar.xz / msi)。
    async fn extract_archive(
        &self,
        archive: &HostPath,
        dest: &HostPath,
        kind: ArchiveKind,
    ) -> Result<(), HostError>;

    // ===== 进程操作 =====

    /// 启动进程,返回 [`HostProcess`] 句柄。
    /// 句柄被消费即等待退出;调用方可保留句柄做 streaming I/O。
    async fn spawn(&self, cmd: HostCommand) -> Result<Box<dyn HostProcess>, HostError>;

    /// 启动进程并等待结束,返回完整 [`CommandOutput`]。
    /// 适用于短命令 + 全量 stdout 收集场景。
    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError>;

    /// 注入提权密码,作为这台主机后续所有 `HostCommand::elevated` 命令的固有能力。
    ///
    /// 远端 Linux 上,有密码就让 elevated 命令走 `sudo -S`(密码喂 stdin),没有就
    /// 退回 `sudo -n`(免密 / root 直接过,需要密码时立刻失败而非挂起)。调用方
    /// (ServerManager)在连接建立后从 keyring 注入一次,docker 弹框拿到新密码时
    /// 再覆盖。这样装 unzip、写 /opt/QQ、apt 装包等所有提权操作共用同一份密码,
    /// 不必每条命令各自塞。
    ///
    /// 本机 Windows / stub 默认忽略:本机提权走 UAC,没有密码字符串这一说。
    async fn set_elevation_password(&self, _password: Option<String>) {}

    /// 这台主机当前是否已注入提权密码。
    ///
    /// 调用方据此判断「sudo -n true 失败」时是真能用 sudo -S,还是只能放弃。
    /// 没注入过密码的实现（本机 Windows / stub）恒返回 false。
    async fn has_elevation_password(&self) -> bool {
        false
    }

    /// 探测某个外部命令在主机上是否可用(在 PATH 里)。Linux/macOS 走
    /// `command -v`,Windows 走 `where`。探测本身失败(连接抖动等)按"不存在"
    /// 保守返回 false,让调用方走"装一下"或报错路径,而不是把探测错误当致命。
    async fn command_exists(&self, command: &str) -> bool {
        let probe = match self.os() {
            Os::Windows => HostCommand::new("where").arg(command),
            _ => HostCommand::new("sh").arg("-c").arg(format!("command -v {command}")),
        };
        matches!(self.run_to_string(probe).await, Ok(out) if out.success())
    }

    /// 运行命令并把 stdout / stderr 逐行流式回调，适合 docker pull / compose up
    /// 这类「跑得久、要实时进度」的命令。`on_line(source, line)` 每收到完整一行
    /// （已去掉行尾换行）就被调用一次，调用方在回调里做解析 / 转进度事件。命令
    /// 结束后返回 [`CommandOutput`]，其中 stdout/stderr 是回调过的全部行重新拼回
    /// （调用方通常只看 exit_code，行内容已经在回调里处理过）。
    ///
    /// 回调收的是 owned `String` 而非 `&str`：trait 走 `#[async_trait]`，`&str`
    /// 在 `Box<dyn FnMut(..., &str)>` 里会被固定一个生命周期，编译器会认为 box 的
    /// 析构可能用到它，逼着每行的借用活到函数尾——owned String 没有借用，彻底绕开。
    /// 行很小，这点 alloc 可忽略。
    ///
    /// 默认实装回退到 [`Self::run_to_string`]：一次性跑完，再把 stdout/stderr 按
    /// 行补发一遍回调。这样没实现流式的 Host（stub / 未来主机）行为正确，只是
    /// 进度变成「跑完一次性出」。真正的流式由 LocalWindowsHost / RemoteLinuxHost
    /// override。回调是 `FnMut + Send`，因为调用方常在闭包里改可变状态（layer 表）。
    async fn run_streaming(
        &self,
        cmd: HostCommand,
        mut on_line: Box<dyn FnMut(StreamSource, String) + Send>,
    ) -> Result<CommandOutput, HostError> {
        let out = self.run_to_string(cmd).await?;
        for line in out.stdout.lines() {
            on_line(StreamSource::Stdout, line.to_string());
        }
        for line in out.stderr.lines() {
            on_line(StreamSource::Stderr, line.to_string());
        }
        Ok(out)
    }

    /// 把远端 loopback 端口转发到本机。仅远端 Linux SSH 实装；其它 Host 返回 Unsupported。
    async fn open_tunnel(
        &self,
        _spec: crate::remote::TunnelSpec,
    ) -> Result<crate::remote::TunnelHandle, HostError> {
        Err(HostError::Unsupported {
            operation: "open_tunnel",
        })
    }

    /// 是否支持连接刷新/失效语义。本地/stub 返回 false，远端返回 true。
    fn supports_refresh(&self) -> bool {
        false
    }

    /// 请求该 host 主动失效底层连接（使后续操作快速失败，便于上层观测）。
    /// 本地/stub 为 no-op；RemoteLinuxHost 实现为 poison 主句柄 + 清 SFTP。
    async fn invalidate_connection(&self) {}

    /// 廉价活性探测（非权威健康，仅用于自愈触发）。
    /// 成功返回 true；任何错误/超时返回 false。默认实现对本地恒 true。
    /// 实现方必须 bounded（建议 2~3s 超时），不得复用长命令超时。
    async fn is_healthy(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_serialization_uses_snake_case() {
        assert_eq!(serde_json::to_string(&Os::Windows).unwrap(), "\"windows\"");
        assert_eq!(serde_json::to_string(&Os::MacOs).unwrap(), "\"mac_os\"");
        assert_eq!(serde_json::to_string(&Os::Linux).unwrap(), "\"linux\"");
    }

    #[test]
    fn arch_serialization_x86_64() {
        // 严格 snake_case,与 Tauri 端口约定一致
        let s = serde_json::to_string(&Arch::X86_64).unwrap();
        assert_eq!(s, "\"x86_64\"");
    }

    #[test]
    fn locality_round_trip() {
        let local = serde_json::to_string(&Locality::Local).unwrap();
        let remote = serde_json::to_string(&Locality::Remote).unwrap();
        assert_eq!(local, "\"local\"");
        assert_eq!(remote, "\"remote\"");
        let back: Locality = serde_json::from_str(&local).unwrap();
        assert_eq!(back, Locality::Local);
    }
}
