//! 交互终端（伪终端）契约
//!
//! 本机走 ConPTY、远端走 SSH 的 pty 通道，上层只见这一套：往里写按键、改窗口大小、
//! 读输出字节、等它结束。两边的实现都拿 [pty_channel_pair] 生成的后端半边去驱动，
//! 测试替身也一样，所以会话管理层不用关心下面是哪种主机。
//!
//! 输出通道是有界的：消费方读慢了，本机读线程卡在发送上，子进程写满管道后自己停，
//! `cat` 一个大文件不会把桌面端刷死；远端那头 russh 自己缓存，至少界面不会卡住。

use std::collections::BTreeMap;

use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

use crate::error::HostError;
use crate::path::HostPath;

/// 输出通道能压几块。一块最多一次读取的大小，合起来约 1 MiB 在路上
pub const PTY_OUTPUT_CHUNKS: usize = 64;

/// 单次读取的缓冲大小
pub const PTY_READ_BUF: usize = 16 * 1024;

/// 终端的列数和行数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
}

impl PtySize {
    /// 夹到合理范围：0 列会让 ConPTY 创建失败，超大值多半是前端量错了
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: cols.clamp(2, 1000),
            rows: rows.clamp(1, 500),
        }
    }
}

impl Default for PtySize {
    fn default() -> Self {
        Self { cols: 120, rows: 30 }
    }
}

/// 终端里跑什么
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyProgram {
    /// 远端用户的登录 shell，和直接 ssh 上去一样；目录和环境变量不生效
    LoginShell,
    /// 指定程序和参数；远端会拼成一行交给登录 shell 执行
    Program { program: String, args: Vec<String> },
    /// 远端交给登录 shell 执行的一段脚本；本机不支持
    Script(String),
}

/// 开终端的请求
#[derive(Debug, Clone)]
pub struct PtyRequest {
    pub program: PtyProgram,
    /// 起始目录；None 用主机默认（远端家目录、本机当前目录）
    pub cwd: Option<HostPath>,
    /// 在当前环境之上追加或覆盖的变量
    pub env: BTreeMap<String, String>,
    pub size: PtySize,
}

impl PtyRequest {
    pub fn new(program: PtyProgram, size: PtySize) -> Self {
        Self {
            program,
            cwd: None,
            env: BTreeMap::new(),
            size,
        }
    }
}

/// 终端怎么结束的
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PtyExit {
    /// 程序自己退出；None 表示被信号杀掉或拿不到退出码
    Exited(Option<i32>),
    /// 连接断了（远端掉线、SSH 会话没了、服务器拒绝分配终端）
    Disconnected(String),
}

/// 送进终端后端的指令
#[derive(Debug)]
pub enum PtyInput {
    Write(Bytes),
    Resize(PtySize),
    Close,
}

/// 往终端里写、改大小、关掉。能克隆，多处同时持有
#[derive(Debug, Clone)]
pub struct PtyControl {
    tx: mpsc::UnboundedSender<PtyInput>,
}

impl PtyControl {
    pub fn write(&self, data: impl Into<Bytes>) -> Result<(), HostError> {
        self.tx
            .send(PtyInput::Write(data.into()))
            .map_err(|_| closed_error())
    }

    pub fn resize(&self, size: PtySize) -> Result<(), HostError> {
        self.tx
            .send(PtyInput::Resize(size))
            .map_err(|_| closed_error())
    }

    /// 请求结束；已经结束了也不报错
    pub fn close(&self) {
        let _ = self.tx.send(PtyInput::Close);
    }

    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

fn closed_error() -> HostError {
    HostError::remote_disconnected("terminal already closed")
}

/// 一个开着的终端（上层持有的那一半）
#[derive(Debug)]
pub struct PtySession {
    /// 终端输出的原始字节（带颜色码和光标控制），通道关了就是没有更多输出
    pub output: mpsc::Receiver<Bytes>,
    pub control: PtyControl,
    /// 结束时收到一次；发送方没了（后端崩了）按断线处理
    pub exit: oneshot::Receiver<PtyExit>,
}

/// 驱动终端的那一半：本机 ConPTY 线程、远端通道任务、测试替身都拿它
#[derive(Debug)]
pub struct PtyBackend {
    pub input: mpsc::UnboundedReceiver<PtyInput>,
    pub output: mpsc::Sender<Bytes>,
    pub exit: oneshot::Sender<PtyExit>,
}

/// 生成一对连着的终端两端
pub fn pty_channel_pair() -> (PtySession, PtyBackend) {
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    let (output_tx, output_rx) = mpsc::channel(PTY_OUTPUT_CHUNKS);
    let (exit_tx, exit_rx) = oneshot::channel();
    (
        PtySession {
            output: output_rx,
            control: PtyControl { tx: input_tx },
            exit: exit_rx,
        },
        PtyBackend {
            input: input_rx,
            output: output_tx,
            exit: exit_tx,
        },
    )
}

/// Windows 命令行按 CommandLineToArgvW 的规矩给单个参数加引号
///
/// 反斜杠只有挨着双引号时才要翻倍，其余原样；空参数也要一对引号占位
pub fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.chars().any(|c| matches!(c, ' ' | '\t' | '\n' | '\u{b}' | '"')) {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

/// 程序加参数拼成一整行 Windows 命令行
pub fn windows_command_line(program: &str, args: &[String]) -> String {
    let mut line = quote_windows_arg(program);
    for arg in args {
        line.push(' ');
        line.push_str(&quote_windows_arg(arg));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_clamped_to_something_conpty_accepts() {
        assert_eq!(PtySize::new(0, 0), PtySize { cols: 2, rows: 1 });
        assert_eq!(PtySize::new(80, 24), PtySize { cols: 80, rows: 24 });
        assert_eq!(PtySize::new(5000, 5000), PtySize { cols: 1000, rows: 500 });
    }

    #[test]
    fn windows_args_follow_argv_quoting_rules() {
        assert_eq!(quote_windows_arg("plain"), "plain");
        assert_eq!(quote_windows_arg(""), "\"\"");
        assert_eq!(quote_windows_arg("C:\\Program Files\\x"), "\"C:\\Program Files\\x\"");
        assert_eq!(quote_windows_arg("say \"hi\""), "\"say \\\"hi\\\"\"");
        // 结尾的反斜杠挨着收尾引号，必须翻倍，不然引号被吃掉
        assert_eq!(quote_windows_arg("C:\\dir with space\\"), "\"C:\\dir with space\\\\\"");
        assert_eq!(quote_windows_arg("a\\b"), "a\\b");
    }

    #[test]
    fn command_line_joins_quoted_parts() {
        let line = windows_command_line(
            "C:\\Program Files\\PowerShell\\7\\pwsh.exe",
            &["-NoLogo".to_string(), "-Command".to_string(), "Write-Host 'a b'".to_string()],
        );
        assert_eq!(
            line,
            "\"C:\\Program Files\\PowerShell\\7\\pwsh.exe\" -NoLogo -Command \"Write-Host 'a b'\""
        );
    }

    #[tokio::test]
    async fn control_reports_closed_after_backend_drops() {
        let (session, backend) = pty_channel_pair();
        assert!(session.control.write(Bytes::from_static(b"x")).is_ok());
        drop(backend);
        assert!(session.control.is_closed());
        assert!(session.control.write(Bytes::from_static(b"x")).is_err());
    }
}
