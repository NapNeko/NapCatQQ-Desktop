//! 本机能开的 shell：探测、默认挑哪个、怎么起（带上命令标记）

use ncd_domain::{LocalShellKind, LocalShellOption};

use super::integration::{CMD_PROMPT, powershell_args};

/// 本机一个 shell 的启动方式
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalShellLaunch {
    pub program: String,
    pub args: Vec<String>,
    /// 这个 shell 自己要的环境变量（cmd 的提示符、Git Bash 别跳回家目录）
    pub env: Vec<(String, String)>,
    /// 挂上了命令标记和目录上报
    pub integration: bool,
}

/// 按默认顺序挑：指定了且有就用指定的，不然 PowerShell 7 → Windows PowerShell → 命令提示符
pub(crate) fn pick_shell(
    options: &[LocalShellOption],
    wanted: Option<LocalShellKind>,
) -> Option<LocalShellOption> {
    if let Some(kind) = wanted {
        if let Some(found) = options.iter().find(|o| o.kind == kind) {
            return Some(found.clone());
        }
    }
    [
        LocalShellKind::Pwsh,
        LocalShellKind::WindowsPowershell,
        LocalShellKind::Cmd,
    ]
    .iter()
    .find_map(|kind| options.iter().find(|o| o.kind == *kind).cloned())
}

/// `git_bash_rc`：Git Bash 用的 rc 文件（写在数据根的缓存目录里）；没写成功就按普通登录 shell 起
pub(crate) fn launch_for(shell: &LocalShellOption, git_bash_rc: Option<&str>) -> LocalShellLaunch {
    let program = shell.path.clone();
    match shell.kind {
        LocalShellKind::Pwsh | LocalShellKind::WindowsPowershell => LocalShellLaunch {
            program,
            args: powershell_args(),
            env: Vec::new(),
            integration: true,
        },
        LocalShellKind::Cmd => LocalShellLaunch {
            program,
            args: Vec::new(),
            env: vec![("PROMPT".to_string(), CMD_PROMPT.to_string())],
            integration: true,
        },
        LocalShellKind::GitBash => {
            // Git 的 /etc/profile 没见到 CHERE_INVOKING 会 cd 回家目录
            let env = vec![("CHERE_INVOKING".to_string(), "1".to_string())];
            match git_bash_rc {
                Some(rc) => LocalShellLaunch {
                    program,
                    args: vec!["--rcfile".to_string(), rc.to_string(), "-i".to_string()],
                    env,
                    integration: true,
                },
                None => LocalShellLaunch {
                    program,
                    args: vec!["--login".to_string(), "-i".to_string()],
                    env,
                    integration: false,
                },
            }
        }
        LocalShellKind::Wsl => LocalShellLaunch {
            program,
            args: Vec::new(),
            env: Vec::new(),
            integration: false,
        },
    }
}

/// 本机探到的 shell，按默认偏好排好
pub fn detect_local_shells() -> Vec<LocalShellOption> {
    #[cfg(windows)]
    {
        windows_impl::detect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// 新开终端用的 PATH：注册表里现在的系统 + 用户 PATH，再补上桌面端进程里有而注册表里没有的。
/// 桌面端开着的时候装了 git / node，新开的终端就能找到，不用重启桌面端
pub(crate) fn fresh_local_path() -> String {
    let process = std::env::var("PATH").unwrap_or_default();
    #[cfg(windows)]
    {
        match windows_impl::registry_path() {
            Some(fresh) => merge_path_lists(&fresh, &process),
            None => process,
        }
    }
    #[cfg(not(windows))]
    {
        process
    }
}

/// 两份 `;` 分隔的 PATH 合一份：前一份在前，后一份只补前面没有的（不分大小写、忽略尾部 `\`）
pub(crate) fn merge_path_lists(first: &str, second: &str) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<&str> = Vec::new();
    for entry in first.split(';').chain(second.split(';')) {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.trim_end_matches('\\').to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(trimmed);
    }
    out.join(";")
}

/// 把 `%VAR%` 换成当前环境里的值；不认识的原样留着
pub(crate) fn expand_percent_vars(raw: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(windows)]
mod windows_impl {
    use std::env;
    use std::path::{Path, PathBuf};

    use ncd_domain::{LocalShellKind, LocalShellOption};

    fn option(kind: LocalShellKind, label: &str, path: &Path) -> LocalShellOption {
        LocalShellOption {
            kind,
            label: label.to_string(),
            path: path.to_string_lossy().into_owned(),
        }
    }

    /// 应用商店装的 pwsh 在 WindowsApps 里是个「应用执行别名」，跟随链接取元数据会失败，
    /// 所以只看它本身在不在
    fn present(path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn env_dir(name: &str) -> Option<PathBuf> {
        env::var_os(name).map(PathBuf::from)
    }

    fn find_on_path(exe: &str) -> Option<PathBuf> {
        let path = env::var_os("PATH")?;
        env::split_paths(&path)
            .map(|dir| dir.join(exe))
            .find(|candidate| present(candidate))
    }

    fn find_pwsh() -> Option<PathBuf> {
        ["ProgramFiles", "ProgramW6432"]
            .iter()
            .filter_map(|v| env_dir(v))
            .flat_map(|root| {
                [
                    root.join("PowerShell").join("7").join("pwsh.exe"),
                    root.join("PowerShell").join("7-preview").join("pwsh.exe"),
                ]
            })
            .find(|p| present(p))
            .or_else(|| find_on_path("pwsh.exe"))
    }

    fn find_git_bash() -> Option<PathBuf> {
        let installed = ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
            .iter()
            .filter_map(|v| env_dir(v))
            .map(|root| root.join("Git"))
            .chain(env_dir("LOCALAPPDATA").map(|d| d.join("Programs").join("Git")))
            .map(|git| git.join("bin").join("bash.exe"))
            .find(|p| present(p));
        if installed.is_some() {
            return installed;
        }
        // 装在别处的 Git：PATH 上的 git.exe 通常在 <Git>\cmd 或 <Git>\mingw64\bin 下
        let git = find_on_path("git.exe")?;
        git.ancestors()
            .skip(1)
            .take(3)
            .map(|dir| dir.join("bin").join("bash.exe"))
            .find(|p| present(p) && !p.to_string_lossy().to_lowercase().contains("system32"))
    }

    /// 装了至少一个 WSL 发行版（光有 wsl.exe 不算，没装发行版时它只会提示去装）
    fn wsl_has_distro() -> bool {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Lxss")
            .map(|key| key.enum_keys().next().is_some())
            .unwrap_or(false)
    }

    pub(super) fn detect() -> Vec<LocalShellOption> {
        let mut out = Vec::new();
        if let Some(path) = find_pwsh() {
            out.push(option(LocalShellKind::Pwsh, "PowerShell 7", &path));
        }
        let system_root = env_dir("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let system32 = system_root.join("System32");
        let ps51 = system32
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        if present(&ps51) {
            out.push(option(
                LocalShellKind::WindowsPowershell,
                "Windows PowerShell",
                &ps51,
            ));
        }
        let cmd = env_dir("ComSpec")
            .filter(|p| present(p))
            .unwrap_or_else(|| system32.join("cmd.exe"));
        if present(&cmd) {
            out.push(option(LocalShellKind::Cmd, "命令提示符", &cmd));
        }
        if let Some(path) = find_git_bash() {
            out.push(option(LocalShellKind::GitBash, "Git Bash", &path));
        }
        let wsl = system32.join("wsl.exe");
        if present(&wsl) && wsl_has_distro() {
            out.push(option(LocalShellKind::Wsl, "WSL", &wsl));
        }
        out
    }

    pub(super) fn registry_path() -> Option<String> {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        let machine: Option<String> = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
            .and_then(|key| key.get_value("Path"))
            .ok();
        let user: Option<String> = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Environment")
            .and_then(|key| key.get_value("Path"))
            .ok();
        let joined: Vec<String> = [machine, user]
            .into_iter()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .collect();
        if joined.is_empty() {
            return None;
        }
        Some(super::expand_percent_vars(&joined.join(";"), |name| {
            env::var(name).ok()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(kind: LocalShellKind) -> LocalShellOption {
        LocalShellOption {
            kind,
            label: String::new(),
            path: format!("{kind:?}.exe"),
        }
    }

    #[test]
    fn picks_wanted_then_default_order() {
        let all = vec![
            opt(LocalShellKind::Cmd),
            opt(LocalShellKind::WindowsPowershell),
            opt(LocalShellKind::GitBash),
        ];
        assert_eq!(
            pick_shell(&all, None).map(|o| o.kind),
            Some(LocalShellKind::WindowsPowershell)
        );
        assert_eq!(
            pick_shell(&all, Some(LocalShellKind::GitBash)).map(|o| o.kind),
            Some(LocalShellKind::GitBash)
        );
        // 要的没有，退回默认
        assert_eq!(
            pick_shell(&all, Some(LocalShellKind::Pwsh)).map(|o| o.kind),
            Some(LocalShellKind::WindowsPowershell)
        );
        assert_eq!(pick_shell(&[], None), None);
    }

    #[test]
    fn merge_keeps_first_order_and_drops_duplicates() {
        let merged = merge_path_lists(r"C:\A;C:\B\;;c:\a", r"c:\b;C:\Extra");
        assert_eq!(merged, r"C:\A;C:\B\;C:\Extra");
    }

    #[test]
    fn percent_vars_expand_known_names_only() {
        let lookup = |name: &str| (name == "SystemRoot").then(|| r"C:\Windows".to_string());
        assert_eq!(
            expand_percent_vars(r"%SystemRoot%\system32;%NOPE%\x;50%", lookup),
            r"C:\Windows\system32;%NOPE%\x;50%"
        );
    }

    #[test]
    fn git_bash_without_rc_falls_back_to_login_shell() {
        let shell = opt(LocalShellKind::GitBash);
        let with_rc = launch_for(&shell, Some("/c/ProgramData/x/rc.sh"));
        assert_eq!(
            with_rc.args,
            vec!["--rcfile", "/c/ProgramData/x/rc.sh", "-i"]
        );
        assert!(with_rc.integration);
        let plain = launch_for(&shell, None);
        assert_eq!(plain.args, vec!["--login", "-i"]);
        assert!(!plain.integration);
        assert!(
            plain
                .env
                .iter()
                .any(|(k, v)| k == "CHERE_INVOKING" && v == "1")
        );
    }

    #[test]
    fn cmd_gets_the_marking_prompt() {
        let launch = launch_for(&opt(LocalShellKind::Cmd), None);
        assert!(
            launch
                .env
                .iter()
                .any(|(k, v)| k == "PROMPT" && v.contains("]9;9;$P"))
        );
    }
}
