//! 命令标记和当前目录上报，照 VS Code 终端的 OSC 633 协议
//!
//! 前端据此知道当前目录（文件栏跟随、拖文件上传到哪）、每条命令从哪开始到哪结束、
//! 退出码是多少（成败标记、跳到上一条 / 下一条、最近命令）。只往提示符里加不可见的序列，
//! 不改提示符长相；不认这些序列的终端直接忽略。
//!
//! 序列：A 提示符开始，B 提示符结束（用户开始输入），E 命令原文，C 命令开始执行，
//! D;<退出码> 命令结束，P;Cwd=<目录> 当前目录。原文和目录里的 `\` `;` 和控制字符要转义。

/// bash：照登录 shell 的样子读用户自己的配置
///
/// 终端是带 `--rcfile` 的交互式非登录 shell（这样才能挂上我们的脚本），所以要自己把
/// 登录 shell 本来会读的几个文件读一遍；Debian / Ubuntu 的 ~/.profile 会再去读 ~/.bashrc
const BASH_PROFILE: &str = r#"if [ -r /etc/profile ]; then . /etc/profile; fi
if [ -r "$HOME/.bash_profile" ]; then . "$HOME/.bash_profile"
elif [ -r "$HOME/.bash_login" ]; then . "$HOME/.bash_login"
elif [ -r "$HOME/.profile" ]; then . "$HOME/.profile"
fi
"#;

/// bash：语言环境不是 UTF-8 时换成 C.UTF-8（系统里有才换），中文文件名和输出不乱码
const BASH_LOCALE: &str = r#"case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in
  *[Uu][Tt][Ff]-8*|*[Uu][Tt][Ff]8*) ;;
  *) if locale -a 2>/dev/null | grep -qiE '^c\.utf-?8$'; then export LC_CTYPE=C.UTF-8; fi ;;
esac
export COLORTERM=truecolor
"#;

/// bash：命令标记
///
/// PROMPT_COMMAND 首尾各挂一个：头一个存退出码（`$?` 只在最前面才是用户命令的），
/// 最后一个上报并给提示符补 A / B（starship 这类每次重算 PS1 的也能补上）。
/// DEBUG trap 报命令开始；用户已经挂了 DEBUG trap（bash-preexec 等）就不抢，只少了命令原文。
///
/// 命令原文取自历史：没进历史的（空格开头、和上一条重复、关了历史）报空，前端不记进最近命令。
/// 历史文件在 rc 跑完之后才读进来，所以起始的历史号等第一个提示符时再取
const BASH_MARKS: &str = r#"__ncd_esc() {
  local s=$1
  s=${s//\\/\\\\}
  s=${s//;/\\x3b}
  s=${s//$'\n'/\\x0a}
  s=${s//$'\r'/\\x0d}
  s=${s//$'\a'/\\x07}
  s=${s//$'\e'/\\x1b}
  printf '%s' "$s"
}
__ncd_in_cmd=0
__ncd_in_prompt=0
__ncd_last_hist=
__ncd_hist_ready=
__ncd_hist_line() {
  local line
  line=$(HISTTIMEFORMAT= builtin history 1)
  printf '%s' "${line#"${line%%[![:space:]]*}"}"
}
__ncd_save_ec() { __ncd_ec=$?; __ncd_in_prompt=1; }
__ncd_precmd() {
  if [ "$__ncd_in_cmd" = 1 ]; then printf '\033]633;D;%s\007' "$__ncd_ec"; fi
  __ncd_in_cmd=0
  if [ -z "$__ncd_hist_ready" ]; then
    __ncd_hist_ready=1
    __ncd_last_hist=$(__ncd_hist_line)
    __ncd_last_hist=${__ncd_last_hist%%[[:space:]]*}
  fi
  printf '\033]633;P;Cwd=%s\007' "$(__ncd_esc "$PWD")"
  case "$PS1" in *'633;A'*) ;; *) PS1='\[\033]633;A\007\]'"$PS1"'\[\033]633;B\007\]' ;; esac
  __ncd_in_prompt=0
}
__ncd_preexec() {
  [ "$__ncd_in_prompt" = 1 ] && return
  [ "$__ncd_in_cmd" = 1 ] && return
  [ -n "${COMP_LINE-}" ] && return
  [ "$BASH_COMMAND" = __ncd_save_ec ] && return
  __ncd_in_cmd=1
  local line num cmd
  line=$(__ncd_hist_line)
  num=${line%%[[:space:]]*}
  cmd=${line#"$num"}
  cmd=${cmd#"${cmd%%[![:space:]]*}"}
  # 历史号没变说明这条没进历史，原文报空
  if [ -z "$num" ] || [ "$num" = "$__ncd_last_hist" ]; then cmd=; else __ncd_last_hist=$num; fi
  printf '\033]633;E;%s\007\033]633;C\007' "$(__ncd_esc "$cmd")"
}
PROMPT_COMMAND="__ncd_save_ec;${PROMPT_COMMAND:+ $PROMPT_COMMAND;} __ncd_precmd"
if [ -z "$(trap -p DEBUG)" ]; then trap '__ncd_preexec' DEBUG; fi
"#;

/// PowerShell：包一层 prompt 函数，照样调用户原来的（oh-my-posh 之类照常显示）
///
/// 全程只用单引号：整段作为 `-Command` 的一个参数传进去，双引号在 Windows PowerShell 5.1
/// 的命令行解析里会被吃掉。执行策略只放宽这个进程：不然 npm / pnpm 装的 .ps1 小脚本和
/// .venv 的 Activate.ps1 在默认策略下都跑不了，报一屏红字
const POWERSHELL_MARKS: &str = r#"Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass -Force -ErrorAction SilentlyContinue
$global:__ncd = @{ LastId = -1; Prompt = $function:prompt; E = [string][char]27; B = [string][char]7 }
function global:__ncdEsc([string]$s) {
  if (-not $s) { return '' }
  $s = $s.Replace('\', '\\').Replace(';', '\x3b')
  return $s.Replace([string][char]10, '\x0a').Replace([string][char]13, '\x0d')
}
function global:prompt {
  $ok = $?
  $code = $global:LASTEXITCODE
  $e = $global:__ncd.E
  $b = $global:__ncd.B
  $out = ''
  $last = Get-History -Count 1
  if ($last -and $last.Id -ne $global:__ncd.LastId) {
    $global:__ncd.LastId = $last.Id
    if ($ok) { $ec = 0 } elseif ($code) { $ec = $code } else { $ec = 1 }
    $out += $e + ']633;E;' + (__ncdEsc $last.CommandLine) + $b + $e + ']633;D;' + $ec + $b
  }
  $loc = $executionContext.SessionState.Path.CurrentLocation
  if ($loc.Provider.Name -eq 'FileSystem') { $out += $e + ']633;P;Cwd=' + (__ncdEsc $loc.ProviderPath) + $b }
  $inner = & $global:__ncd.Prompt
  $global:LASTEXITCODE = $code
  return $out + $e + ']633;A' + $b + $inner + $e + ']633;B' + $b
}
"#;

/// cmd 的提示符：只能报当前目录和提示符起止，拿不到退出码
///
/// 目录走 Windows Terminal 认的 OSC 9;9（原样路径）：633 的 Cwd 要求转义反斜杠，cmd 做不到，
/// 原样塞进去的话 `C:\x64` 这种目录会被前端按 `\x64` 解码错
pub(crate) const CMD_PROMPT: &str = r"$E]9;9;$P$E\$E]633;A$E\$P$G$E]633;B$E\";

/// here-doc 结束标记，脚本里不会出现这一串
const RC_DELIMITER: &str = "NCD_RC_EOF_7F3A";

/// bash 单引号字面量
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// 读完用户配置之后接上的环境：PATH 前缀、变量、起始目录（给几个候选时进第一个存在的）
pub(crate) fn bash_env_section(
    path_prefix: &[String],
    env: &[(String, String)],
    cwd: &[String],
) -> String {
    let mut out = String::new();
    if !path_prefix.is_empty() {
        let joined: Vec<String> = path_prefix.iter().map(|p| sh_quote(p)).collect();
        out.push_str("export PATH=");
        out.push_str(&joined.join(":"));
        out.push_str(":\"$PATH\"\n");
    }
    for (key, value) in env.iter().filter(|(k, _)| is_env_name(k) && k != "PATH") {
        out.push_str("export ");
        out.push_str(key);
        out.push('=');
        out.push_str(&sh_quote(value));
        out.push('\n');
    }
    match cwd {
        [] => {}
        [dir] => {
            out.push_str("cd -- ");
            out.push_str(&sh_quote(dir));
            out.push_str(" 2>/dev/null || true\n");
        }
        dirs => {
            let list: Vec<String> = dirs.iter().map(|d| sh_quote(d)).collect();
            out.push_str("for __ncd_d in ");
            out.push_str(&list.join(" "));
            out.push_str("; do if [ -d \"$__ncd_d\" ]; then cd -- \"$__ncd_d\"; break; fi; done; unset __ncd_d\n");
        }
    }
    out
}

/// 完整的 bash rc：用户配置 → 语言环境 → 我们的环境 → 命令标记
pub(crate) fn bash_rc(env_section: &str) -> String {
    let mut rc = String::with_capacity(4096);
    rc.push_str(BASH_PROFILE);
    rc.push_str(BASH_LOCALE);
    rc.push_str(env_section);
    rc.push_str(BASH_MARKS);
    rc
}

/// 远端 exec 行：rc 走 fd 3 的 here-doc 交给 bash，不用往服务器上写临时文件。
/// 服务器上没有 bash（Alpine 之类）就进目录开登录 shell，没有标记但能用
pub(crate) fn remote_bash_exec_line(rc: &str, fallback_cwd: Option<&str>) -> String {
    let mut line = String::with_capacity(rc.len() + 256);
    line.push_str("if command -v bash >/dev/null 2>&1; then\n");
    line.push_str("exec bash --rcfile /dev/fd/3 -i 3<<'");
    line.push_str(RC_DELIMITER);
    line.push_str("'\n");
    line.push_str(rc);
    // rc 读完就关掉 fd 3，不留给用户的 shell
    line.push_str("exec 3<&-\n");
    line.push_str(RC_DELIMITER);
    line.push_str("\nelse\n");
    if let Some(dir) = fallback_cwd {
        line.push_str("cd -- ");
        line.push_str(&sh_quote(dir));
        line.push_str(" 2>/dev/null\n");
    }
    line.push_str("exec \"${SHELL:-/bin/sh}\" -l\nfi\n");
    line
}

/// 本机 Git Bash 用的 rc（环境变量和目录走进程本身，这里只有用户配置和标记）
pub(crate) fn git_bash_rc() -> String {
    bash_rc("")
}

/// PowerShell 的参数：不退出、先跑标记脚本
pub(crate) fn powershell_args() -> Vec<String> {
    vec![
        "-NoLogo".to_string(),
        "-NoExit".to_string(),
        "-Command".to_string(),
        POWERSHELL_MARKS.to_string(),
    ]
}

fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_quote_survives_single_quotes() {
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn env_section_prepends_path_and_changes_dir() {
        let section = bash_env_section(
            &["/srv/app/.venv/bin".to_string(), "/home/u/ncd/tools/uv".to_string()],
            &[
                ("VIRTUAL_ENV".to_string(), "/srv/app/.venv".to_string()),
                ("BAD NAME".to_string(), "x".to_string()),
                ("PATH".to_string(), "/ignored".to_string()),
            ],
            &["/srv/app".to_string()],
        );
        assert_eq!(
            section,
            "export PATH='/srv/app/.venv/bin':'/home/u/ncd/tools/uv':\"$PATH\"\n\
             export VIRTUAL_ENV='/srv/app/.venv'\n\
             cd -- '/srv/app' 2>/dev/null || true\n"
        );
    }

    #[test]
    fn several_dirs_enter_the_first_existing() {
        let section = bash_env_section(
            &[],
            &[],
            &["/h/.napcat-bots/slbot-1".to_string(), "/h/.napcat-bots/ncbot-1".to_string()],
        );
        assert_eq!(
            section,
            "for __ncd_d in '/h/.napcat-bots/slbot-1' '/h/.napcat-bots/ncbot-1'; do if [ -d \"$__ncd_d\" ]; then cd -- \"$__ncd_d\"; break; fi; done; unset __ncd_d\n"
        );
    }

    #[test]
    fn exec_line_feeds_rc_through_fd3_and_falls_back_without_bash() {
        let rc = bash_rc("cd -- '/srv/app' 2>/dev/null || true\n");
        let line = remote_bash_exec_line(&rc, Some("/srv/app"));
        assert!(line.starts_with("if command -v bash >/dev/null 2>&1; then\n"));
        assert!(line.contains("exec bash --rcfile /dev/fd/3 -i 3<<'NCD_RC_EOF_7F3A'\n"));
        assert!(line.contains("exec 3<&-\nNCD_RC_EOF_7F3A\nelse\n"));
        assert!(line.ends_with("cd -- '/srv/app' 2>/dev/null\nexec \"${SHELL:-/bin/sh}\" -l\nfi\n"));
        assert!(!rc.contains(RC_DELIMITER));
    }

    #[test]
    fn powershell_script_has_no_double_quotes() {
        assert!(!POWERSHELL_MARKS.contains('"'));
        let args = powershell_args();
        assert_eq!(&args[..3], &["-NoLogo", "-NoExit", "-Command"]);
    }
}
