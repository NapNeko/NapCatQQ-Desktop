#!/usr/bin/env bash
# 远端 NapCat launcher 脚本（P1 拆分版）
#
# 由 remote_install_napcat.sh 部署阶段上传到 $workspace_dir/napcat.sh,
# 在 P2 阶段由 RemoteBackend.start_napcat / stop_napcat 调用。
#
# 当前 P1 仅负责把脚本部署到位, 不在部署中触发启动。

set -euo pipefail

workspace_dir="${workspace_dir:-$HOME/Napcat}"
runtime_dir="${runtime_dir:-$workspace_dir/run}"
qq_base_path="${qq_base_path:-$workspace_dir/opt/QQ}"
qq_executable="${qq_executable:-$qq_base_path/qq}"
log_dir="${log_dir:-$workspace_dir/log}"
status_file="${status_file:-$runtime_dir/status.json}"
pid_file="${pid_file:-$runtime_dir/napcat.pid}"
log_file="${log_file:-$log_dir/napcat.log}"

timestamp() {
  date +"%Y-%m-%d %H:%M:%S"
}

escape_json_string() {
  local escaped="$1"
  escaped="${escaped//\\/\\\\}"
  escaped="${escaped//\"/\\\"}"
  escaped="${escaped//$'\n'/\\n}"
  escaped="${escaped//$'\r'/\\r}"
  escaped="${escaped//$'\t'/\\t}"
  printf '"%s"' "$escaped"
}

write_status() {
  local running="$1"
  local last_action="$2"
  local pid_value="${3:-null}"
  local last_error="${4:-null}"

  mkdir -p "$(dirname "$status_file")"
  cat > "$status_file" <<JSON
{
  "running": ${running},
  "pid": ${pid_value},
  "qq": null,
  "version": null,
  "log_file": "$log_file",
  "last_action": "$last_action",
  "last_error": ${last_error},
  "updated_at": "$(date -Iseconds)"
}
JSON
}

current_pid() {
  if [ ! -f "$pid_file" ]; then
    return 1
  fi
  local pid
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  if [ -z "$pid" ]; then
    return 1
  fi
  if kill -0 "$pid" >/dev/null 2>&1; then
    printf '%s\n' "$pid"
    return 0
  fi
  rm -f "$pid_file"
  return 1
}

start_napcat() {
  mkdir -p "$runtime_dir" "$log_dir"
  touch "$log_file"

  if pid="$(current_pid)"; then
    write_status true "already_running" "$pid" null
    echo "[OK] already running pid=${pid}"
    return 0
  fi

  if [ ! -x "$qq_executable" ]; then
    local error_text="qq executable missing: $qq_executable"
    write_status false "start_failed" null "$(escape_json_string "$error_text")"
    echo "[ERROR] $error_text" >&2
    return 1
  fi

  nohup xvfb-run -a "$qq_executable" --no-sandbox >> "$log_file" 2>&1 &
  local pid="$!"
  echo "$pid" > "$pid_file"
  sleep 8

  if kill -0 "$pid" >/dev/null 2>&1; then
    write_status true "start" "$pid" null
    echo "[OK] started pid=${pid}"
    return 0
  fi

  local last_error="start command exited before readiness check"
  if [ -f "$log_file" ]; then
    last_error="$(tail -n 20 "$log_file" | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g')"
  fi
  rm -f "$pid_file"
  write_status false "start_failed" null "$(escape_json_string "$last_error")"
  echo "[ERROR] ${last_error}" >&2
  return 1
}

stop_napcat() {
  local pid=""
  if pid="$(current_pid)"; then
    kill "$pid" >/dev/null 2>&1 || true
    sleep 3
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill -9 "$pid" >/dev/null 2>&1 || true
    fi
  fi
  rm -f "$pid_file"
  write_status false "stop" null null
  echo "[OK] stopped"
}

status_napcat() {
  if pid="$(current_pid)"; then
    write_status true "status" "$pid" null
  else
    write_status false "status" null null
  fi
  cat "$status_file"
}

case "${1:-start}" in
  start)
    start_napcat
    ;;
  stop)
    stop_napcat
    ;;
  restart)
    stop_napcat
    start_napcat
    ;;
  status)
    status_napcat
    ;;
  log-path)
    printf '%s\n' "$log_file"
    ;;
  *)
    echo "Usage: $0 {start|stop|restart|status|log-path}" >&2
    exit 2
    ;;
esac
