//! HostShell:shell escape 与命令拼接抽象
//!
//! 设计要点:
//! - 本地 Host 把 [HostCommand](crate::HostCommand) 直接传给 tokio::process::Command,
//!   不需要 shell;但远端 Host 走 SSH 后必须把 program + args 拼成 shell 字符串发送,
//!   此时 shell escape 就是关键(防止 args 中的空格 / 引号 / $ 注入)
//! - 三种 shell:BashShell(Linux/macOS 远端默认)/ PowerShellShell(Windows 优先)/
//!   CmdShell(legacy Windows bat 兼容)
//! - 各 shell 的 escape 规则不同,本 trait 抽象单一接口
//!
//! HostShell::escape 输出必须是单个 shell token,调用方拼接
//! program + " " + args.join(" ") 时直接安全

use crate::command::HostCommand;

/// Shell 种类,用于 PackageManager / Host 决策时分发
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Bash,
    PowerShell,
    Cmd,
}

/// HostShell:shell 能力抽象
pub trait HostShell: Send + Sync {
    fn kind(&self) -> ShellKind;

    /// 把单个参数 escape 成安全的 shell token
    /// - Bash:用单引号包裹,内部 ' 替换成 '\''
    /// - PowerShell:用单引号包裹,内部 ' 替换成 ''
    /// - Cmd:用 " 包裹,内部 " 替换成 "",部分元字符 ^ 转义
    fn escape(&self, arg: &str) -> String;

    /// 行分隔符
    fn line_separator(&self) -> &'static str;

    /// 把 [HostCommand] 拼成完整的 shell 命令字符串(供 SSH 通道直接执行)
    fn build_command_line(&self, cmd: &HostCommand) -> String {
        let mut parts = Vec::with_capacity(cmd.args.len() + 1);

        // 环境变量前缀(Bash / PowerShell 都支持 inline env,但语法不同)
        // Bash: K1=v1 K2=v2 program args...
        // PowerShell: 必须用 $env:K1='v1'; ...; & program args
        // Cmd: set K1=v1 && program args
        // 这里默认 Bash 风格,PowerShellShell / CmdShell 各自重写本方法
        for (k, v) in &cmd.environment {
            parts.push(format!("{}={}", k, self.escape(v)));
        }

        parts.push(self.escape(&cmd.program));
        for arg in &cmd.args {
            parts.push(self.escape(arg));
        }

        parts.join(" ")
    }
}

// ============================================================
// BashShell
// ============================================================

#[derive(Debug, Default, Clone, Copy)]
pub struct BashShell;

impl HostShell for BashShell {
    fn kind(&self) -> ShellKind {
        ShellKind::Bash
    }

    fn escape(&self, arg: &str) -> String {
        if arg.is_empty() {
            return "''".to_string();
        }
        // 简单 ASCII 字母 / 数字 / 部分安全符号无需 escape
        let safe = arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '='));
        if safe {
            arg.to_string()
        } else {
            // 单引号包裹,内部 ' 替换为 '\''
            let escaped = arg.replace('\'', "'\\''");
            format!("'{escaped}'")
        }
    }

    fn line_separator(&self) -> &'static str {
        "\n"
    }
}

// ============================================================
// PowerShellShell
// ============================================================

#[derive(Debug, Default, Clone, Copy)]
pub struct PowerShellShell;

impl HostShell for PowerShellShell {
    fn kind(&self) -> ShellKind {
        ShellKind::PowerShell
    }

    fn escape(&self, arg: &str) -> String {
        if arg.is_empty() {
            return "''".to_string();
        }
        let safe = arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '='));
        if safe {
            arg.to_string()
        } else {
            // PowerShell 单引号字面量,内部 ' 双写
            let escaped = arg.replace('\'', "''");
            format!("'{escaped}'")
        }
    }

    fn line_separator(&self) -> &'static str {
        "\r\n"
    }

    fn build_command_line(&self, cmd: &HostCommand) -> String {
        // PowerShell:$env:K='v'; & 'program' 'arg1' 'arg2'
        let mut parts = Vec::new();
        for (k, v) in &cmd.environment {
            parts.push(format!("$env:{}={};", k, self.escape(v)));
        }
        parts.push(format!("& {}", self.escape(&cmd.program)));
        for arg in &cmd.args {
            parts.push(self.escape(arg));
        }
        parts.join(" ")
    }
}

// ============================================================
// CmdShell(legacy Windows bat / cmd 兼容)
// ============================================================

#[derive(Debug, Default, Clone, Copy)]
pub struct CmdShell;

impl HostShell for CmdShell {
    fn kind(&self) -> ShellKind {
        ShellKind::Cmd
    }

    fn escape(&self, arg: &str) -> String {
        if arg.is_empty() {
            return "\"\"".to_string();
        }
        // cmd 转义最复杂,最稳的做法:用 " 包裹,内部 " → "",
        // ^ & | < > ( ) 等元字符在双引号内已经被禁用所以不用单独转
        if arg.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '=')) {
            arg.to_string()
        } else {
            let escaped = arg.replace('"', "\"\"");
            format!("\"{escaped}\"")
        }
    }

    fn line_separator(&self) -> &'static str {
        "\r\n"
    }

    fn build_command_line(&self, cmd: &HostCommand) -> String {
        // cmd:set K=v && program arg1 arg2
        let mut parts = Vec::new();
        for (k, v) in &cmd.environment {
            parts.push(format!("set {}={} &&", k, v));
        }
        parts.push(self.escape(&cmd.program));
        for arg in &cmd.args {
            parts.push(self.escape(arg));
        }
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_escapes_spaces_and_quotes() {
        let sh = BashShell;
        assert_eq!(sh.escape("hello"), "hello");
        assert_eq!(sh.escape("hello world"), "'hello world'");
        assert_eq!(sh.escape("it's"), r"'it'\''s'");
    }

    #[test]
    fn bash_keeps_safe_chars_raw() {
        let sh = BashShell;
        assert_eq!(sh.escape("/etc/napcat/runtime.json"), "/etc/napcat/runtime.json");
        assert_eq!(sh.escape("--depth=1"), "--depth=1");
    }

    #[test]
    fn bash_command_line_includes_env() {
        let sh = BashShell;
        let cmd = HostCommand::new("git").arg("status").env("GIT_TERMINAL_PROMPT", "0");
        let line = sh.build_command_line(&cmd);
        assert!(line.contains("GIT_TERMINAL_PROMPT=0"));
        assert!(line.contains("git status"));
    }

    #[test]
    fn powershell_escapes_quotes_doubly() {
        let sh = PowerShellShell;
        assert_eq!(sh.escape("it's"), "'it''s'");
        assert_eq!(sh.escape("plain"), "plain");
    }

    #[test]
    fn powershell_command_line_uses_ampersand() {
        let sh = PowerShellShell;
        let cmd = HostCommand::new("Get-Process").arg("-Name").arg("napcat");
        let line = sh.build_command_line(&cmd);
        assert!(line.starts_with("& "));
        assert!(line.contains("Get-Process"));
    }

    #[test]
    fn cmd_escapes_quotes_with_double_quote() {
        let sh = CmdShell;
        assert_eq!(sh.escape(r#"hello "world""#), r#""hello ""world""""#);
    }

    #[test]
    fn cmd_keeps_safe_paths_unquoted() {
        let sh = CmdShell;
        assert_eq!(sh.escape("C:/Users/foo"), "C:/Users/foo");
    }

    #[test]
    fn empty_arg_is_quoted_in_all_shells() {
        assert_eq!(BashShell.escape(""), "''");
        assert_eq!(PowerShellShell.escape(""), "''");
        assert_eq!(CmdShell.escape(""), "\"\"");
    }

    #[test]
    fn line_separators_match_platform_convention() {
        assert_eq!(BashShell.line_separator(), "\n");
        assert_eq!(PowerShellShell.line_separator(), "\r\n");
        assert_eq!(CmdShell.line_separator(), "\r\n");
    }
}
