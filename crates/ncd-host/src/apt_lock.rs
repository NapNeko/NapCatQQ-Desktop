//! Ubuntu/Debian 上 apt 与 unattended-upgrades 争用 dpkg 锁时的公共处理
//!
//! Desktop 侧 [PackageManagerLock] 只能串行本应用的 apt;无法阻止系统里的
//! unattended-upgr 占锁装完 Docker 后立刻装 noVNC 时常见此冲突

use std::time::Duration;

use crate::command::{CommandOutput, HostCommand};

/// 若 sh -c 脚本含 apt/apt-get,自动加上 dpkg 锁等待前导
pub fn host_command_wrap_dpkg_wait_for_apt(mut cmd: HostCommand) -> HostCommand {
    if cmd.program != "sh" || cmd.args.len() < 2 {
        return cmd;
    }
    let script = cmd.args.last().map(String::as_str).unwrap_or("");
    if !script.contains("apt-get") && !script.contains("apt ") {
        return cmd;
    }
    let wrapped = wrap_sh_script_with_dpkg_wait(script);
    let n = cmd.args.len();
    cmd.args[n - 1] = wrapped;
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
    let inner = inner.trim_start();
    format!("{}\n{}", dpkg_lock_wait_preamble_sh(), inner)
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
}