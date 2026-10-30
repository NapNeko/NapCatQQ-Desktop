//! 本机目标在系统终端里打开
//!
//! 直接起一个新控制台窗口，而不是去调 wt.exe：已经开着的 Windows Terminal 会用它自己的环境开
//! 新标签，我们接好的 PATH / .venv 就丢了。Windows 11 默认终端设成 Windows Terminal 时，
//! 新控制台窗口会自动交给它托管，环境照样是我们给的这份。
//! 这里不挂命令标记：系统终端不认，cmd 在老控制台里还会把序列当乱码打出来。

use std::collections::BTreeMap;

use ncd_domain::{LocalShellKind, TerminalOpenRequest};
use ncd_host::{PathStyle, PtyProgram};

use super::TerminalError;
use super::manager::TerminalManager;

/// 系统终端里的启动参数：PowerShell 放宽本进程的执行策略（和内嵌终端一样），其余普通交互
pub(crate) fn external_args(kind: Option<LocalShellKind>) -> Vec<String> {
    match kind {
        Some(LocalShellKind::Pwsh | LocalShellKind::WindowsPowershell) => vec![
            "-NoLogo".to_string(),
            "-NoExit".to_string(),
            "-Command".to_string(),
            "Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass -Force -ErrorAction SilentlyContinue"
                .to_string(),
        ],
        Some(LocalShellKind::GitBash | LocalShellKind::Bash | LocalShellKind::Zsh) => {
            vec!["--login".to_string(), "-i".to_string()]
        }
        Some(LocalShellKind::Fish | LocalShellKind::Sh) => vec!["-i".to_string()],
        Some(LocalShellKind::Cmd | LocalShellKind::Wsl) | None => Vec::new(),
    }
}

/// 内嵌终端专用的变量去掉（cmd 的标记提示符）
pub(crate) fn external_env(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("PROMPT"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

impl TerminalManager {
    pub async fn open_external(&self, request: &TerminalOpenRequest) -> Result<(), TerminalError> {
        let plan = self.plan_for_external(request).await?;
        if !plan.features.external {
            return Err(TerminalError::Unsupported(
                "只有本机的终端能在系统终端里打开".into(),
            ));
        }
        let PtyProgram::Program { program, .. } = &plan.program else {
            return Err(TerminalError::Unsupported(
                "这个终端没法在系统终端里打开".into(),
            ));
        };
        let cwd = plan
            .cwd
            .as_ref()
            .map(|p| p.render(PathStyle::for_os(plan.host.os())));
        spawn_console(
            program,
            &external_args(plan.shell),
            cwd.as_deref(),
            &external_env(&plan.env),
        )
    }
}

#[cfg(windows)]
fn spawn_console(
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    env: &BTreeMap<String, String>,
) -> Result<(), TerminalError> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let mut cmd = std::process::Command::new(program);
    cmd.args(args).envs(env).creation_flags(CREATE_NEW_CONSOLE);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| TerminalError::Host(format!("打开系统终端失败：{e}")))
}

#[cfg(target_os = "linux")]
fn spawn_console(
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    env: &BTreeMap<String, String>,
) -> Result<(), TerminalError> {
    use ncd_host::shell_single_quote;
    use std::os::unix::fs::PermissionsExt;

    let path = std::env::var_os("PATH").unwrap_or_default();
    let terminal = [
        ("x-terminal-emulator", "-e"),
        ("gnome-terminal", "--"),
        ("konsole", "-e"),
        ("xfce4-terminal", "-x"),
        ("xterm", "-e"),
    ]
    .into_iter()
    .find_map(|(name, flag)| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|p| {
                p.metadata()
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
            .map(|p| (p, flag))
    })
    .ok_or_else(|| {
        TerminalError::Unsupported("未找到系统终端，请安装 xterm 或桌面环境的终端程序".into())
    })?;

    // GNOME/KDE 可能复用已运行的终端进程，工作目录和环境在新 shell 内显式设置。
    let mut script = String::new();
    if let Some(dir) = cwd {
        script.push_str(&format!("cd {} && ", shell_single_quote(dir)));
    }
    script.push_str("exec env ");
    for (key, value) in env {
        script.push_str(&shell_single_quote(&format!("{key}={value}")));
        script.push(' ');
    }
    script.push_str(&shell_single_quote(program));
    for arg in args {
        script.push(' ');
        script.push_str(&shell_single_quote(arg));
    }
    std::process::Command::new(terminal.0)
        .args([terminal.1, "sh", "-c", &script])
        .spawn()
        .map(|_| ())
        .map_err(|e| TerminalError::Host(format!("打开系统终端失败：{e}")))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn spawn_console(
    _program: &str,
    _args: &[String],
    _cwd: Option<&str>,
    _env: &BTreeMap<String, String>,
) -> Result<(), TerminalError> {
    Err(TerminalError::Unsupported(
        "当前系统不支持打开系统终端".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powershell_keeps_the_policy_relaxation_only() {
        let args = external_args(Some(LocalShellKind::Pwsh));
        assert_eq!(&args[..3], &["-NoLogo", "-NoExit", "-Command"]);
        assert!(args[3].starts_with("Set-ExecutionPolicy -Scope Process"));
        assert!(external_args(Some(LocalShellKind::Cmd)).is_empty());
    }

    #[test]
    fn marking_prompt_is_dropped() {
        let mut env = BTreeMap::new();
        env.insert("PROMPT".to_string(), "$E]633;A".to_string());
        env.insert("PATH".to_string(), r"C:\x".to_string());
        let cleaned = external_env(&env);
        assert!(!cleaned.contains_key("PROMPT"));
        assert_eq!(cleaned.get("PATH").map(String::as_str), Some(r"C:\x"));
    }
}
