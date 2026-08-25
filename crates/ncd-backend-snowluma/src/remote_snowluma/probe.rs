//! 远端 SnowLuma 是否在跑、WebUI 实际端口：看进程和套接字，不写死 5099。
//!
//! 栈就绪：Desktop pid 文件，或 node 的 cwd/cmdline 指向本安装的 index.mjs。
//! WebUI 端口：daemon 日志（SL 换口会打 using N instead / listening）→
//! node 的 LISTEN 套接字 → runtime.json webuiPort → 默认 5099。

use std::time::Duration;

use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::runtime_backend::BotBackendError;

use super::helpers::read_remote_log_tail_lines;
use super::layout::{DEFAULT_WEBUI_PORT, SnowLumaRemotePaths, shell_single_quote};
use crate::snowluma::session::parse_bound_webui_port_from_logs;

/// 生成栈就绪探测脚本（无 WebUI 端口字面量，避免导入安装换口后误判）。
pub(crate) fn stack_ready_script(paths: &SnowLumaRemotePaths) -> String {
    let pid_daemon = shell_single_quote(&paths.pid_daemon);
    let pid_node = shell_single_quote(&paths.pid_node_path());
    let sl = shell_single_quote(&paths.snowluma_dir);
    format!(
        r#"PID_DAEMON={pid_daemon}
PID_NODE={pid_node}
SL={sl}
alive() {{
  pid="$1"
  [ -n "$pid" ] && [ "$pid" != "0" ] && kill -0 "$pid" 2>/dev/null
}}
if [ -f "$PID_DAEMON" ]; then
  pid=$(cat "$PID_DAEMON" 2>/dev/null || echo "")
  if alive "$pid"; then exit 0; fi
fi
if [ -f "$PID_NODE" ]; then
  pid=$(cat "$PID_NODE" 2>/dev/null || echo "")
  if alive "$pid"; then exit 0; fi
fi
for proc in /proc/[0-9]*; do
  [ -d "$proc" ] || continue
  pid=${{proc#/proc/}}
  comm=$(cat "$proc/comm" 2>/dev/null || true)
  case "$comm" in node|nodejs) ;; *) continue ;; esac
  alive "$pid" || continue
  cwd=$(readlink -f "$proc/cwd" 2>/dev/null || true)
  if [ -n "$cwd" ] && [ "$cwd" = "$SL" ]; then exit 0; fi
  cmd=$(tr '\0' ' ' < "$proc/cmdline" 2>/dev/null || true)
  case "$cmd" in
    *"index.mjs"*)
      case "$cmd" in
        *"$SL"*) exit 0 ;;
      esac
      ;;
  esac
done
exit 1
"#
    )
}

/// daemon / node 是否已在远端就绪（进程，不是某个固定 TCP 口）
pub async fn is_stack_ready(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
) -> Result<bool, BotBackendError> {
    let script = stack_ready_script(paths);
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(out.success())
}

pub fn parse_webui_port_from_runtime_json(bytes: &[u8]) -> Option<u16> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let n = v.get("webuiPort")?;
    let port = if let Some(u) = n.as_u64() {
        u
    } else if let Some(s) = n.as_str() {
        s.trim().parse().ok()?
    } else {
        return None;
    };
    let port = u16::try_from(port).ok()?;
    (port > 0).then_some(port)
}

pub fn parse_port_lines(out: &str) -> Vec<u16> {
    let mut ports = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let Ok(p) = t.parse::<u16>() else {
            continue;
        };
        if p > 0 && !ports.contains(&p) {
            ports.push(p);
        }
    }
    ports
}

/// 日志里的实际绑定口优先（SL 会在占用时换口且不回写 runtime.json）。
pub fn pick_remote_webui_port(
    log_port: Option<u16>,
    socket_ports: &[u16],
    runtime_port: Option<u16>,
) -> u16 {
    if let Some(p) = log_port {
        return p;
    }
    if let Some(rt) = runtime_port {
        if socket_ports.contains(&rt) {
            return rt;
        }
    }
    if let Some(&p) = socket_ports.first() {
        return p;
    }
    runtime_port.unwrap_or(DEFAULT_WEBUI_PORT as u16)
}

fn node_listen_ports_script(paths: &SnowLumaRemotePaths) -> String {
    let pid_daemon = shell_single_quote(&paths.pid_daemon);
    let pid_node = shell_single_quote(&paths.pid_node_path());
    let sl = shell_single_quote(&paths.snowluma_dir);
    format!(
        r#"PID_DAEMON={pid_daemon}
PID_NODE={pid_node}
SL={sl}
alive() {{
  pid="$1"
  [ -n "$pid" ] && [ "$pid" != "0" ] && kill -0 "$pid" 2>/dev/null
}}
find_node_pid() {{
  if [ -f "$PID_NODE" ]; then
    pid=$(cat "$PID_NODE" 2>/dev/null || true)
    if alive "$pid"; then echo "$pid"; return 0; fi
  fi
  if [ -f "$PID_DAEMON" ]; then
    pid=$(cat "$PID_DAEMON" 2>/dev/null || true)
    if alive "$pid"; then echo "$pid"; return 0; fi
  fi
  for proc in /proc/[0-9]*; do
    [ -d "$proc" ] || continue
    pid=${{proc#/proc/}}
    comm=$(cat "$proc/comm" 2>/dev/null || true)
    case "$comm" in node|nodejs) ;; *) continue ;; esac
    alive "$pid" || continue
    cwd=$(readlink -f "$proc/cwd" 2>/dev/null || true)
    if [ -n "$cwd" ] && [ "$cwd" = "$SL" ]; then echo "$pid"; return 0; fi
    cmd=$(tr '\0' ' ' < "$proc/cmdline" 2>/dev/null || true)
    case "$cmd" in
      *"index.mjs"*)
        case "$cmd" in
          *"$SL"*) echo "$pid"; return 0 ;;
        esac
        ;;
    esac
  done
  return 1
}}
dump_listen_ports() {{
  pid="$1"
  alive "$pid" || return 0
  inodes=" "
  for fd in /proc/$pid/fd/*; do
    t=$(readlink "$fd" 2>/dev/null || true)
    case "$t" in
      socket:\[*\])
        ino=${{t#socket:[}}
        ino=${{ino%']'}}
        inodes="$inodes$ino "
        ;;
    esac
  done
  [ "$inodes" != " " ] || return 0
  for table in /proc/net/tcp /proc/net/tcp6; do
    [ -f "$table" ] || continue
    tail -n +2 "$table" 2>/dev/null | while read -r _ local _ st _ _ _ _ _ inode _; do
      [ "$st" = "0A" ] || continue
      case "$inodes" in
        *" $inode "*)
          ph=${{local##*:}}
          echo $((0x$ph)) 2>/dev/null || true
          ;;
      esac
    done
  done
}}
pid=$(find_node_pid || true)
[ -n "$pid" ] || exit 0
dump_listen_ports "$pid"
"#
    )
}

async fn read_runtime_webui_port(host: &dyn Host, paths: &SnowLumaRemotePaths) -> Option<u16> {
    let path = format!("{}/runtime.json", paths.config_dir);
    let bytes = host.read_file(&HostPath::from_posix(&path)).await.ok()?;
    parse_webui_port_from_runtime_json(&bytes)
}

async fn read_log_webui_port(host: &dyn Host, paths: &SnowLumaRemotePaths) -> Option<u16> {
    let lines = read_remote_log_tail_lines(host, &paths.log_daemon, 80)
        .await
        .ok()?;
    parse_bound_webui_port_from_logs(&lines)
}

async fn read_node_listen_ports(host: &dyn Host, paths: &SnowLumaRemotePaths) -> Vec<u16> {
    let script = node_listen_ports_script(paths);
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = match host.run_to_string(cmd).await {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    parse_port_lines(&out.stdout)
}

/// 解析远端 SnowLuma WebUI 当前端口（导入安装可能不是 5099）。
pub async fn resolve_remote_webui_port(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
) -> Result<u16, BotBackendError> {
    let runtime_port = read_runtime_webui_port(host, paths).await;
    let log_port = read_log_webui_port(host, paths).await;
    let socket_ports = read_node_listen_ports(host, paths).await;
    Ok(pick_remote_webui_port(
        log_port,
        &socket_ports,
        runtime_port,
    ))
}

fn wait_webui_ready_script(paths: &SnowLumaRemotePaths, timeout: Duration) -> String {
    let secs = timeout.as_secs().max(1);
    let rt = shell_single_quote(&format!("{}/runtime.json", paths.config_dir));
    let log = shell_single_quote(&paths.log_daemon);
    let pid_daemon = shell_single_quote(&paths.pid_daemon);
    let pid_node = shell_single_quote(&paths.pid_node_path());
    let sl = shell_single_quote(&paths.snowluma_dir);
    format!(
        r#"deadline=$(( $(date +%s) + {secs} ))
RT={rt}
LOG={log}
PID_DAEMON={pid_daemon}
PID_NODE={pid_node}
SL={sl}
tcp_up() {{
  p="$1"
  [ -n "$p" ] && [ "$p" != "0" ] || return 1
  if command -v bash >/dev/null 2>&1 && bash -c "(: > /dev/tcp/127.0.0.1/$p)" 2>/dev/null; then return 0; fi
  if command -v nc >/dev/null 2>&1 && nc -z 127.0.0.1 "$p" 2>/dev/null; then return 0; fi
  return 1
}}
alive() {{
  pid="$1"
  [ -n "$pid" ] && [ "$pid" != "0" ] && kill -0 "$pid" 2>/dev/null
}}
find_node_pid() {{
  if [ -f "$PID_NODE" ]; then
    pid=$(cat "$PID_NODE" 2>/dev/null || true)
    if alive "$pid"; then echo "$pid"; return 0; fi
  fi
  if [ -f "$PID_DAEMON" ]; then
    pid=$(cat "$PID_DAEMON" 2>/dev/null || true)
    if alive "$pid"; then echo "$pid"; return 0; fi
  fi
  for proc in /proc/[0-9]*; do
    [ -d "$proc" ] || continue
    pid=${{proc#/proc/}}
    comm=$(cat "$proc/comm" 2>/dev/null || true)
    case "$comm" in node|nodejs) ;; *) continue ;; esac
    alive "$pid" || continue
    cwd=$(readlink -f "$proc/cwd" 2>/dev/null || true)
    if [ -n "$cwd" ] && [ "$cwd" = "$SL" ]; then echo "$pid"; return 0; fi
    cmd=$(tr '\0' ' ' < "$proc/cmdline" 2>/dev/null || true)
    case "$cmd" in
      *"index.mjs"*)
        case "$cmd" in
          *"$SL"*) echo "$pid"; return 0 ;;
        esac
        ;;
    esac
  done
  return 1
}}
dump_listen_ports() {{
  pid="$1"
  alive "$pid" || return 0
  inodes=" "
  for fd in /proc/$pid/fd/*; do
    t=$(readlink "$fd" 2>/dev/null || true)
    case "$t" in
      socket:\[*\])
        ino=${{t#socket:[}}
        ino=${{ino%']'}}
        inodes="$inodes$ino "
        ;;
    esac
  done
  [ "$inodes" != " " ] || return 0
  for table in /proc/net/tcp /proc/net/tcp6; do
    [ -f "$table" ] || continue
    tail -n +2 "$table" 2>/dev/null | while read -r _ local _ st _ _ _ _ _ inode _; do
      [ "$st" = "0A" ] || continue
      case "$inodes" in
        *" $inode "*)
          ph=${{local##*:}}
          echo $((0x$ph)) 2>/dev/null || true
          ;;
      esac
    done
  done
}}
collect_ports() {{
  if [ -f "$RT" ]; then
    sed -n 's/.*"webuiPort"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$RT" 2>/dev/null
  fi
  if [ -f "$LOG" ]; then
    grep -oE 'is in use, using [0-9]+ instead' "$LOG" 2>/dev/null | tail -n 1 | grep -oE '[0-9]+$' || true
    grep -oE 'listening https?://[^[:space:]]+' "$LOG" 2>/dev/null | tail -n 1 | grep -oE '[0-9]+$' || true
  fi
  pid=$(find_node_pid || true)
  if [ -n "$pid" ]; then
    dump_listen_ports "$pid"
  fi
}}
while [ "$(date +%s)" -lt "$deadline" ]; do
  for p in $(collect_ports | awk 'NF && $1 ~ /^[0-9]+$/ && $1+0 > 0 {{print $1+0}}' | awk '!a[$1]++'); do
    if tcp_up "$p"; then
      echo "$p"
      exit 0
    fi
  done
  sleep 1
done
exit 1
"#
    )
}

/// 等到 WebUI 在某个已发现的端口上监听，返回实际端口。
pub async fn wait_remote_webui_ready(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    timeout: Duration,
) -> Result<u16, BotBackendError> {
    let script = wait_webui_ready_script(paths, timeout);
    let bash = {
        let find = HostCommand::new("sh")
            .arg("-c")
            .arg("command -v bash 2>/dev/null || true");
        match host.run_to_string(find).await {
            Ok(o) if o.success() => {
                let line = o.stdout.lines().next().unwrap_or("").trim();
                if line.is_empty() {
                    "/bin/bash".to_string()
                } else {
                    line.to_string()
                }
            }
            _ => "/bin/bash".into(),
        }
    };
    let cmd = HostCommand::new(bash).arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        return Err(BotBackendError::Io(
            "SnowLuma WebUI 在时限内未就绪（已按进程日志/监听口探测，未写死 5099）".into(),
        ));
    }
    let line = out.stdout.lines().last().unwrap_or("").trim();
    line.parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| BotBackendError::Io(format!("SnowLuma WebUI 就绪但未返回端口: {line}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_snowluma::layout::SnowLumaRemotePaths;

    fn sample_paths() -> SnowLumaRemotePaths {
        SnowLumaRemotePaths::from_remote_home("/home/u")
    }

    #[test]
    fn stack_ready_script_does_not_hardcode_webui_port() {
        let script = stack_ready_script(&sample_paths());
        assert!(
            !script.contains("5099"),
            "stack ready must not probe a fixed WebUI port"
        );
        assert!(script.contains("index.mjs"));
        assert!(script.contains("pid_daemon"));
        assert!(script.contains("pid_node"));
    }

    #[test]
    fn wait_script_discovers_port_instead_of_assuming_5099() {
        let script = wait_webui_ready_script(&sample_paths(), Duration::from_secs(5));
        assert!(script.contains("webuiPort"));
        assert!(script.contains("listening"));
        assert!(script.contains("/proc/net/tcp"));
        // 5099 只应作为 Rust 侧最后兜底，远程等待脚本不应写死唯一探测口
        let assigned = script
            .lines()
            .filter(|l| l.contains("5099") && l.contains('='))
            .count();
        assert_eq!(assigned, 0, "wait script must not assign a fixed 5099");
    }

    #[test]
    fn parse_runtime_json_number() {
        let bytes = br#"{ "webuiPort": 5103 }"#;
        assert_eq!(parse_webui_port_from_runtime_json(bytes), Some(5103));
    }

    #[test]
    fn parse_runtime_json_string() {
        let bytes = br#"{ "webuiPort": "6111" }"#;
        assert_eq!(parse_webui_port_from_runtime_json(bytes), Some(6111));
    }

    #[test]
    fn parse_runtime_json_rejects_zero() {
        let bytes = br#"{ "webuiPort": 0 }"#;
        assert_eq!(parse_webui_port_from_runtime_json(bytes), None);
    }

    #[test]
    fn pick_prefers_log_port_over_runtime() {
        assert_eq!(
            pick_remote_webui_port(Some(5103), &[5099], Some(5099)),
            5103
        );
    }

    #[test]
    fn pick_prefers_runtime_when_socket_matches() {
        assert_eq!(
            pick_remote_webui_port(None, &[3000, 5099], Some(5099)),
            5099
        );
    }

    #[test]
    fn pick_uses_first_socket_when_no_log() {
        assert_eq!(pick_remote_webui_port(None, &[6123], None), 6123);
    }

    #[test]
    fn pick_falls_back_to_default() {
        assert_eq!(
            pick_remote_webui_port(None, &[], None),
            DEFAULT_WEBUI_PORT as u16
        );
    }

    #[test]
    fn parse_port_lines_dedupes() {
        assert_eq!(
            parse_port_lines("5099\n5099\n5100\nbad\n"),
            vec![5099, 5100]
        );
    }
}
