//! Ubuntu/Debian 上 apt 与 unattended-upgrades 争用 dpkg 锁时的公共处理
//!
//! Desktop 侧 [PackageManagerLock] 只能串行本应用的 apt;无法阻止系统里的
//! unattended-upgr 占锁装完 Docker 后立刻装 noVNC 时常见此冲突

use std::time::Duration;

use crate::command::{CommandOutput, HostCommand};

/// 去掉脚本里的 CR。dash(/bin/sh) 会把 `set -e\r` 报成 `set: Illegal option -`。
/// Windows 源文件或 checkout 成 CRLF 时,include_str!/raw string 会把 \r 带进远端 sh -c。
pub fn normalize_posix_sh_script(script: &str) -> String {
    if script.contains('\r') {
        script.replace('\r', "")
    } else {
        script.to_string()
    }
}

/// 若 sh -c 脚本含 apt/apt-get,自动加上 dpkg 锁等待前导
pub fn host_command_wrap_dpkg_wait_for_apt(mut cmd: HostCommand) -> HostCommand {
    if cmd.program != "sh" || cmd.args.len() < 2 {
        return cmd;
    }
    let n = cmd.args.len();
    let script = normalize_posix_sh_script(&cmd.args[n - 1]);
    if !script.contains("apt-get") && !script.contains("apt ") {
        // 非 apt 也要去 CR:dnf/yum 阶段同样走 sh -c,CRLF 一样会炸 dash
        cmd.args[n - 1] = script;
        return cmd;
    }
    cmd.args[n - 1] = wrap_sh_script_with_dpkg_wait(&script);
    let need = Duration::from_secs(1200);
    if cmd.timeout.map(|t| t < need).unwrap_or(true) {
        cmd.timeout = Some(need);
    }
    cmd
}

/// 输出是否像「dpkg 前端锁被占用」(含 unattended-upgr)
pub fn output_indicates_dpkg_lock_hold(output: &CommandOutput) -> bool {
    let combined = format!("{}\n{}", output.stderr, output.stdout);
    let lower = combined.to_ascii_lowercase();
    lower.contains("lock-frontend")
        || lower.contains("could not get lock")
        || lower.contains("unable to acquire the dpkg")
        || lower.contains("is another process using it")
}

/// 在 POSIX sh 脚本开头插入:轮询等待 dpkg 锁释放(最长约 15 分钟)
/// 等待期间向 stdout 打 NCD: 前缀行,便于 UI/日志看到并非卡死
pub fn dpkg_lock_wait_preamble_sh() -> &'static str {
    r#"
wait_ncd_dpkg_lock() {
  n=0
  max=90
  while true; do
    held=0
    if command -v fuser >/dev/null 2>&1; then
      fuser /var/lib/dpkg/lock-frontend >/dev/null 2>&1 && held=1
      fuser /var/lib/dpkg/lock >/dev/null 2>&1 && held=1
    elif command -v lsof >/dev/null 2>&1; then
      lsof /var/lib/dpkg/lock-frontend >/dev/null 2>&1 && held=1
    else
      return 0
    fi
    if [ "$held" -eq 0 ]; then
      return 0
    fi
    n=$((n + 1))
    if [ "$n" -gt "$max" ]; then
      echo "E: 等待 dpkg 锁超时（常见占用进程: unattended-upgr）。请稍后在组件页重试，或登录远端执行 sudo systemctl stop unattended-upgrades 后再装。" >&2
      exit 1
    fi
    echo "NCD: 等待 apt/dpkg 锁释放 (${n}/${max})，可能被系统自动更新占用…"
    sleep 10
  done
}
wait_ncd_dpkg_lock
"#
}

/// 将内层 set -e 脚本包上锁等待前导
pub fn wrap_sh_script_with_dpkg_wait(inner: &str) -> String {
    // preamble 也可能来自 CRLF 的 .rs raw string,两边都去 CR
    let preamble = normalize_posix_sh_script(dpkg_lock_wait_preamble_sh());
    let inner = normalize_posix_sh_script(inner);
    let inner = inner.trim_start();
    format!("{preamble}\n{inner}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_lock_error() {
        let out = CommandOutput {
            exit_code: Some(100),
            stdout: String::new(),
            stderr: "E: Could not get lock /var/lib/dpkg/lock-frontend. It is held by process 3191 (unattended-upgr)".into(),
        };
        assert!(output_indicates_dpkg_lock_hold(&out));
    }

    #[test]
    fn ignores_unrelated_error() {
        let out = CommandOutput {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "E: Unable to locate package foo".into(),
        };
        assert!(!output_indicates_dpkg_lock_hold(&out));
    }

    #[test]
    fn normalize_strips_cr() {
        assert_eq!(
            normalize_posix_sh_script("set -e\r\napt-get update\r\n"),
            "set -e\napt-get update\n"
        );
        assert_eq!(normalize_posix_sh_script("set -e\n"), "set -e\n");
    }

    #[test]
    fn wrap_strips_crlf_so_set_e_is_dash_safe() {
        // 复现用户侧:sh: N: set: Illegal option -
        let inner = "set -e\r\nexport DEBIAN_FRONTEND=noninteractive\r\napt-get update\r\n";
        let wrapped = wrap_sh_script_with_dpkg_wait(inner);
        assert!(
            !wrapped.contains('\r'),
            "包装后不得残留 CR,否则 dash 会把 set -e\\r 当成非法选项"
        );
        assert!(
            wrapped.lines().any(|l| l == "set -e"),
            "set -e 必须是独立整行: {wrapped}"
        );
        assert!(wrapped.contains("wait_ncd_dpkg_lock"));
        assert!(wrapped.contains("apt-get update"));
    }

    #[test]
    fn host_command_normalizes_cr_for_non_apt_scripts() {
        let cmd = HostCommand::new("sh")
            .arg("-c")
            .arg("set -e\r\necho hi\r\n");
        let out = host_command_wrap_dpkg_wait_for_apt(cmd);
        let script = out.args.last().expect("sh -c script");
        assert!(!script.contains('\r'));
        assert_eq!(script, "set -e\necho hi\n");
    }

    #[test]
    fn host_command_wraps_apt_and_strips_cr() {
        let cmd = HostCommand::new("sh")
            .arg("-c")
            .arg("set -e\r\napt-get install -y curl\r\n");
        let out = host_command_wrap_dpkg_wait_for_apt(cmd);
        let script = out.args.last().expect("sh -c script");
        assert!(!script.contains('\r'));
        assert!(script.contains("wait_ncd_dpkg_lock"));
        assert!(script.contains("apt-get install -y curl"));
    }
}
