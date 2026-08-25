//! 远端 SnowLuma 是否在跑、WebUI 实际端口：看进程和套接字，不写死 5099。
//!
//! 栈就绪：Desktop pid 文件，或 node 的 cwd/cmdline 指向本安装的 index.mjs。
//! WebUI 端口：日志换口 → runtime.json webuiPort → LISTEN 兜底 → 默认 5099。

use std::time::Duration;

use ncd_host::{Host, HostCommand, HostPath};
use ncd_traits::runtime_backend::BotBackendError;

use super::helpers::{read_remote_log_tail, read_remote_log_tail_lines};
use super::layout::{DEFAULT_WEBUI_PORT, SnowLumaRemotePaths, shell_single_quote};
use super::remote_bash::resolve_remote_bash;
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
        return rt;
    }
    if let Some(&p) = socket_ports.first() {
        return p;
    }
    DEFAULT_WEBUI_PORT as u16
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
    let resolved = resolve_remote_snowluma_log_targets(host, paths, &paths.log_daemon).await;
    let mut seen = std::collections::HashSet::new();
    for path in [resolved.daemon, resolved.bot] {
        if !seen.insert(path.clone()) {
            continue;
        }
        let Ok(lines) = read_remote_log_tail_lines(host, &path, 80).await else {
            continue;
        };
        if let Some(p) = parse_bound_webui_port_from_logs(&lines) {
            return Some(p);
        }
    }
    None
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

const NOVNC_LISTEN_PORTS_SCRIPT: &str = r#"
if command -v ss >/dev/null 2>&1; then
  ss -ltnp 2>/dev/null | awk '/websockify/ {print $4}' | sed -n 's/.*[:.]\([0-9][0-9]*\)$/\1/p'
fi
"#;

/// 解析远端 websockify（noVNC）实际监听口。外来图形栈（systemd 自装等）
/// 常用自定义端口，隧道不能写死 6081；同机出现多个时优先桌面约定的 6081。
pub async fn resolve_remote_novnc_port(host: &dyn Host) -> u16 {
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg(NOVNC_LISTEN_PORTS_SCRIPT);
    let Ok(out) = host.run_to_string(cmd).await else {
        return super::tunnel::REMOTE_NOVNC_PORT;
    };
    let ports = parse_port_lines(&out.stdout);
    if ports.contains(&super::tunnel::REMOTE_NOVNC_PORT) {
        return super::tunnel::REMOTE_NOVNC_PORT;
    }
    ports
        .first()
        .copied()
        .unwrap_or(super::tunnel::REMOTE_NOVNC_PORT)
}

/// 日志跟随目标：外来安装的真实日志在 framework 自带 logs 目录（日期滚动命名），
/// 桌面自装布局才是 workspace/log。按远端存在性解析；都没有则回落布局路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSnowLumaLogTargets {
    pub bot: String,
    pub daemon: String,
}

/// 单次 SSH 解析两个跟随源：bot/daemon 布局路径存在则原样沿用，
/// 缺失时回落 framework 目录里最新的一份 snowluma-*.log。
pub async fn resolve_remote_snowluma_log_targets(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    layout_bot_log: &str,
) -> ResolvedSnowLumaLogTargets {
    let fallback = ResolvedSnowLumaLogTargets {
        bot: layout_bot_log.to_string(),
        daemon: paths.log_daemon.clone(),
    };
    let script = format!(
        r#"BOT={bot}
DAEMON={daemon}
FW=$(ls -t {logs}/snowluma-*.log 2>/dev/null | head -n 1)
[ -f "$BOT" ] && echo "bot_ok=1"
[ -f "$DAEMON" ] && echo "daemon_ok=1"
[ -n "$FW" ] && echo "framework=$FW"
"#,
        bot = shell_single_quote(layout_bot_log),
        daemon = shell_single_quote(&paths.log_daemon),
        logs = shell_single_quote(&format!("{}/logs", paths.snowluma_dir)),
    );
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let Ok(out) = host.run_to_string(cmd).await else {
        return fallback;
    };
    let stdout = out.stdout;
    let framework = stdout
        .lines()
        .find_map(|line| line.strip_prefix("framework="))
        .map(str::trim)
        .filter(|stripped| !stripped.is_empty())
        .map(str::to_string);
    match framework {
        Some(fw) => ResolvedSnowLumaLogTargets {
            bot: if stdout.contains("bot_ok=1") {
                layout_bot_log.to_string()
            } else {
                fw.clone()
            },
            daemon: if stdout.contains("daemon_ok=1") {
                paths.log_daemon.clone()
            } else {
                fw
            },
        },
        None => fallback,
    }
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
grep_webui_ports() {{
  f="$1"
  [ -f "$f" ] || return 0
  grep -oE 'is in use, using [0-9]+ instead' "$f" 2>/dev/null | tail -n 1 | grep -oE '[0-9]+$' || true
  grep -oE 'listening https?://[^[:space:]]+' "$f" 2>/dev/null | tail -n 1 | grep -oE '[0-9]+$' || true
}}
preferred_ports() {{
  grep_webui_ports "$LOG"
  FW=$(ls -t "$SL"/logs/snowluma-*.log 2>/dev/null | head -n 1)
  if [ -n "$FW" ] && [ "$FW" != "$LOG" ]; then
    grep_webui_ports "$FW"
  fi
  if [ -f "$RT" ]; then
    sed -n 's/.*"webuiPort"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$RT" 2>/dev/null
  fi
}}
uniq_ports() {{
  awk 'NF && $1 ~ /^[0-9]+$/ && $1+0 > 0 {{print $1+0}}' | awk '!a[$1]++'
}}
start=$(date +%s)
seen_alive=0
dead_ticks=0
while [ "$(date +%s)" -lt "$deadline" ]; do
  pid=$(find_node_pid || true)
  if [ -n "$pid" ]; then
    seen_alive=1
    dead_ticks=0
  elif [ "$seen_alive" = 1 ]; then
    dead_ticks=$((dead_ticks + 1))
    if [ "$dead_ticks" -ge 2 ]; then
      echo "node_exited" >&2
      exit 2
    fi
  else
    now=$(date +%s)
    if [ $((now - start)) -ge 8 ]; then
      echo "node_not_started" >&2
      exit 2
    fi
  fi
  for p in $(preferred_ports | uniq_ports); do
    if tcp_up "$p"; then
      echo "$p"
      exit 0
    fi
  done
  sleep 1
done
pid=$(find_node_pid || true)
if [ -n "$pid" ]; then
  for p in $(dump_listen_ports "$pid" | uniq_ports); do
    if tcp_up "$p"; then
      echo "$p"
      exit 0
    fi
  done
fi
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
    let bash = resolve_remote_bash(host).await?;
    let cmd = HostCommand::new(bash)
        .arg("-c")
        .arg(script)
        .timeout(timeout + Duration::from_secs(30));
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        let hint = match out.exit_code {
            Some(2) if out.stderr.contains("node_exited") => {
                "SnowLuma 进程启动后退出，WebUI 未就绪"
            }
            Some(2) if out.stderr.contains("node_not_started") => {
                "SnowLuma 进程未能拉起，WebUI 未就绪"
            }
            _ => "SnowLuma WebUI 在时限内未就绪",
        };
        let tail = webui_wait_failure_log_tail(host, paths).await;
        if tail.trim().is_empty() {
            return Err(BotBackendError::Io(hint.into()));
        }
        return Err(BotBackendError::Io(format!(
            "{hint}\n--- 远端日志末尾 ---\n{tail}"
        )));
    }
    let line = out.stdout.lines().last().unwrap_or("").trim();
    line.parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| BotBackendError::Io(format!("SnowLuma WebUI 就绪但未返回端口: {line}")))
}

async fn webui_wait_failure_log_tail(host: &dyn Host, paths: &SnowLumaRemotePaths) -> String {
    let resolved = resolve_remote_snowluma_log_targets(host, paths, &paths.log_daemon).await;
    let mut chunks = Vec::new();
    for path in [resolved.daemon, resolved.bot] {
        if let Ok(text) = read_remote_log_tail(host, &path, 30).await {
            let t = text.trim();
            if !t.is_empty() {
                chunks.push(format!("{path}:\n{t}"));
            }
        }
    }
    chunks.join("\n\n")
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
    fn wait_script_fails_fast_when_node_dies() {
        let script = wait_webui_ready_script(&sample_paths(), Duration::from_secs(5));
        assert!(script.contains("node_exited"));
        assert!(script.contains("node_not_started"));
        assert!(script.contains("seen_alive"));
    }

    #[test]
    fn wait_script_polls_preferred_ports_before_listen_fallback() {
        let script = wait_webui_ready_script(&sample_paths(), Duration::from_secs(5));
        assert!(
            script.contains("preferred_ports"),
            "wait loop must poll log/runtime ports, not every node LISTEN socket"
        );
        let loop_body = script
            .split("while [ \"$(date +%s)\" -lt \"$deadline\" ]; do")
            .nth(1)
            .and_then(|rest| rest.split("\ndone\n").next())
            .unwrap_or("");
        assert!(
            loop_body.contains("preferred_ports"),
            "poll loop should only try preferred_ports"
        );
        assert!(
            !loop_body.contains("dump_listen_ports"),
            "LISTEN dump must not run every second inside the wait loop"
        );
        let after_loop = script
            .split("while [ \"$(date +%s)\" -lt \"$deadline\" ]; do")
            .nth(1)
            .and_then(|rest| rest.split("\ndone\n").nth(1))
            .unwrap_or("");
        assert!(
            after_loop.contains("dump_listen_ports"),
            "LISTEN dump is last-resort after preferred ports time out"
        );
    }

    #[test]
    fn wait_script_greps_framework_log_not_only_desktop_daemon_log() {
        let script = wait_webui_ready_script(&sample_paths(), Duration::from_secs(5));
        assert!(
            script.contains("snowluma-*.log"),
            "imported/full packages log to framework logs/, not workspace/log/daemon.log"
        );
        assert!(script.contains("/logs"));
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
    fn pick_prefers_runtime_over_unrelated_listen_ports() {
        assert_eq!(
            pick_remote_webui_port(None, &[3000, 9229], Some(5099)),
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
