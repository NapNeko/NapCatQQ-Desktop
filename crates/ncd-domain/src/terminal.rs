//! 内嵌终端的跨边界数据：开到哪、会话状态、推给前端的事件，以及文件栏和服务器状态条的返回值。
//!
//! 终端输出本身是原始字节，走单独的二进制通道，不在这里建模。

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 终端事件通道的信封版本（R14）
pub const TERMINAL_EVENT_VERSION: u32 = 1;

/// 终端会话 id，桌面端生成
// 同 AppInstanceId，不加 `serde(transparent)`：newtype 在 serde_json 下本就按裸字符串收发
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalSessionId(String);

impl TerminalSessionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TerminalSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 本机能开的 shell
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum LocalShellKind {
    /// PowerShell 7（pwsh.exe）
    Pwsh,
    /// 系统自带的 Windows PowerShell 5.1
    WindowsPowershell,
    /// 命令提示符
    Cmd,
    /// Git for Windows 带的 bash
    GitBash,
    /// 默认的 WSL 发行版
    Wsl,
}

/// 本机探到的一个 shell
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct LocalShellOption {
    pub kind: LocalShellKind,
    /// 菜单里显示的名字
    pub label: String,
    /// 可执行文件路径
    pub path: String,
}

/// 终端开到哪
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum TerminalTarget {
    /// 本机，从家目录开始
    Local,
    /// 远端主机的登录 shell，和直接 ssh 上去一样
    Server { server_id: String },
    /// 协议 Bot：原生部署进运行目录，Docker 部署进容器；`host_dir` 为真时开宿主机上的部署目录
    Bot {
        bot_id: String,
        #[serde(default)]
        host_dir: bool,
    },
    /// 应用端实例目录，接好 node / uv / .venv
    AppInstance { instance_id: String },
}

/// 开终端的请求
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalOpenRequest {
    pub target: TerminalTarget,
    pub cols: u16,
    pub rows: u16,
    /// 本机目标用哪个 shell；不给就按 PowerShell 7、Windows PowerShell、命令提示符的顺序挑
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub shell: Option<LocalShellKind>,
}

/// 会话状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum TerminalStatus {
    Starting,
    Running,
    /// 程序退出了；code 为空表示被信号杀掉或拿不到
    Exited {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        code: Option<i32>,
    },
    /// 连接断了（远端掉线）
    Disconnected { reason: String },
    /// 没开起来
    Failed { message: String },
}

impl TerminalStatus {
    /// 还能接着敲命令
    pub const fn is_live(&self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

/// 这个会话能用的附加功能
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalFeatures {
    /// 文件栏（容器里的会话没有）
    pub files: bool,
    /// 服务器状态条（远端主机）
    pub stats: bool,
    /// 这台主机存了提权密码，出现 sudo 提示时能代填
    pub sudo_fill: bool,
    /// 注入了命令标记和当前目录上报（bash / PowerShell）
    pub shell_integration: bool,
    /// 本机目标，能在系统终端里打开
    pub external: bool,
}

/// 一条常用命令，点了只填进输入行，不执行
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalSnippet {
    pub label: String,
    pub command: String,
}

impl TerminalSnippet {
    pub fn new(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            command: command.into(),
        }
    }
}

/// 主机的系统类型，前端据此拼路径、挑快捷键提示
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum TerminalHostOs {
    Windows,
    Linux,
}

/// 一个终端会话
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalSessionInfo {
    pub id: TerminalSessionId,
    pub target: TerminalTarget,
    /// 标签名，如「麦麦 · vps1」「本机 · PowerShell」
    pub title: String,
    /// `local` 或 `remote:<server_id>`
    pub host_id: String,
    /// 主机显示名
    pub host_label: String,
    pub host_os: TerminalHostOs,
    /// 起始目录，按主机本地风格写
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cwd: Option<String>,
    /// 本机会话用的 shell
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub shell: Option<LocalShellKind>,
    pub status: TerminalStatus,
    /// 创建时间（Unix 毫秒）
    #[ts(type = "number")]
    pub created_at_ms: i64,
    pub features: TerminalFeatures,
    /// 框架预置的常用命令
    pub snippets: Vec<TerminalSnippet>,
}

/// 事件通道推给前端的消息（终端输出另走二进制通道）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum TerminalEvent {
    /// 状态变了：退出、断线
    Status { status: TerminalStatus },
    /// 会话信息整个换了：同一个标签重新开了（按回车重开），标题、功能、状态都以这份为准
    Info { info: TerminalSessionInfo },
    /// 界面跟不上，中间跳过了一段输出
    Dropped {
        #[ts(type = "number")]
        bytes: u64,
    },
}

/// 带版本号的事件信封：`{"v":1,"kind":...}`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalEventEnvelope {
    pub v: u32,
    #[serde(flatten)]
    pub event: TerminalEvent,
}

impl TerminalEventEnvelope {
    pub fn new(event: TerminalEvent) -> Self {
        Self {
            v: TERMINAL_EVENT_VERSION,
            event,
        }
    }
}

/// 文件栏里的一项
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalFileEntry {
    pub name: String,
    /// 完整路径，按主机本地风格写
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    #[ts(type = "number")]
    pub size: u64,
    /// 修改时间（Unix 秒）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub modified: Option<i64>,
    /// 权限，如 `rwxr-xr-x`；本机 Windows 没有
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mode: Option<String>,
}

/// 文件栏列出的一个目录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalDirListing {
    pub path: String,
    /// 上一级；已经在根上为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<String>,
    pub entries: Vec<TerminalFileEntry>,
}

/// 文件栏里打开来改的小文本
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct TerminalTextFile {
    pub path: String,
    pub content: String,
    /// 行尾是不是 CRLF，保存时照原样写回
    pub crlf: bool,
}

/// 服务器状态条的一次读数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ServerStats {
    /// CPU 占用百分比；第一次读没有上一次的样本，为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cpu_percent: Option<f32>,
    pub cores: u32,
    #[ts(type = "number")]
    pub mem_used: u64,
    #[ts(type = "number")]
    pub mem_total: u64,
    #[ts(type = "number")]
    pub swap_used: u64,
    #[ts(type = "number")]
    pub swap_total: u64,
    /// 根分区
    #[ts(type = "number")]
    pub disk_used: u64,
    #[ts(type = "number")]
    pub disk_total: u64,
    /// 每秒收发字节；第一次读为空
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub net_rx_per_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub net_tx_per_sec: Option<u64>,
    pub load1: f32,
    pub load5: f32,
    pub load15: f32,
    #[ts(type = "number")]
    pub uptime_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_uses_kind_tag() {
        let json = serde_json::to_string(&TerminalTarget::Bot {
            bot_id: "10001".into(),
            host_dir: false,
        })
        .unwrap_or_default();
        assert_eq!(json, r#"{"kind":"bot","bot_id":"10001","host_dir":false}"#);
        let local = serde_json::to_string(&TerminalTarget::Local).unwrap_or_default();
        assert_eq!(local, r#"{"kind":"local"}"#);
        let parsed: TerminalTarget =
            serde_json::from_str(r#"{"kind":"bot","bot_id":"1"}"#).unwrap_or(TerminalTarget::Local);
        assert_eq!(
            parsed,
            TerminalTarget::Bot {
                bot_id: "1".into(),
                host_dir: false
            }
        );
    }

    #[test]
    fn envelope_is_flat_with_version() {
        let json = serde_json::to_string(&TerminalEventEnvelope::new(TerminalEvent::Status {
            status: TerminalStatus::Exited { code: Some(0) },
        }))
        .unwrap_or_default();
        assert_eq!(
            json,
            r#"{"v":1,"kind":"status","status":{"kind":"exited","code":0}}"#
        );
    }

    #[test]
    fn shell_kind_is_snake_case() {
        assert_eq!(
            serde_json::to_string(&LocalShellKind::WindowsPowershell).unwrap_or_default(),
            r#""windows_powershell""#
        );
    }
}
