#!/usr/bin/env bash
# 远端 NapCat launcher 脚本 (P2 多 Bot 版)
#
# 由 remote_install_napcat.sh 部署阶段上传到 $workspace_dir/napcat.sh,
# 由 [`RemoteBackend.start_napcat`](src/desktop/core/operation/remote_backend.py)
# / [`RemoteBackend.stop_napcat`](src/desktop/core/operation/remote_backend.py) 调用.
#
# 调用约定 (P2):
#   bash napcat.sh start    <qq_id>     启动指定 QQ
#   bash napcat.sh stop     <qq_id>     停止指定 QQ
#   bash napcat.sh restart  <qq_id>
#   bash napcat.sh status   <qq_id>     输出 status_<qq_id>.json
#   bash napcat.sh log-path <qq_id>     输出该 Bot 日志文件绝对路径
#   bash napcat.sh list                 列出所有运行中的 NapCat Bot 与 PID
#
# 设计要点:
# - 一台远端可同时运行多个 Bot, PID / 日志 / 状态都按 qq_id 分文件.
# - 启动通过标准 NapCat invocation: ``xvfb-run -a $qq_executable --no-sandbox -q <qq_id>``,
#   该命令行格式与 [`RemoteRuntimeService`](src/desktop/core/remote/status.py) 的 pgrep
#   规则 ``pgrep -f '.*/qq --no-sandbox -q [0-9]{4,}'`` 一致.
# - PID 文件路径 ``$runtime_dir/napcat_<qq_id>.pid``;
#   日志路径 ``$log_dir/napcat_<qq_id>.log``;
#   状态路径 ``$runtime_dir/status_<qq_id>.json``.
# - 退出码: 0 成功; 2 用法错误; 3 缺失 qq_id; 4 启动后探活失败.

set -euo pipefail

workspace_dir="${workspace_dir:-$HOME/Napcat}"
runtime_dir="${runtime_dir:-$workspace_dir/run}"
qq_base_path="${qq_base_path:-$workspace_dir/opt/QQ}"
qq_executable="${qq_executable:-$qq_base_path/qq}"
log_dir="${log_dir:-$workspace_dir/log}"

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

# 校验 qq_id 仅包含 4-12 位数字 (与 status.py 的 pgrep 模式 ``[0-9]{4,}`` 兼容).
require_qq_id() {
  local qq_id="$1"
  if [ -z "$qq_id" ]; then
    echo "[ERROR] missing required argument: <qq_id>" >&2
    exit 3
  fi
  if ! [[ "$qq_id" =~ ^[0-9]{4,12}$ ]]; then
    echo "[ERROR] invalid qq_id (must be 4-12 digits): $qq_id" >&2
    exit 3
  fi
}

resolve_pid_file()    { printf '%s/napcat_%s.pid'    "$runtime_dir" "$1"; }
resolve_log_file()    { printf '%s/napcat_%s.log'    "$log_dir"     "$1"; }
resolve_status_file() { printf '%s/status_%s.json'   "$runtime_dir" "$1"; }

# 写每个 Bot 的状态文件; 字段语义与 [`RemoteRuntimeService.build_status_payload`]
# (src/desktop/core/remote/status.py) 对齐.
write_status() {
  local qq_id="$1"
  local running="$2"
  local last_action="$3"
  local pid_value="${4:-null}"
  local last_error="${5:-null}"

  local status_path
  status_path="$(resolve_status_file "$qq_id")"
  local log_path
  log_path="$(resolve_log_file "$qq_id")"

  mkdir -p "$(dirname "$status_path")"
  cat > "$status_path" <<JSON
{
  "running": ${running},
  "pid": ${pid_value},
  "qq": "${qq_id}",
  "version": null,
  "log_file": "$log_path",
  "last_action": "$last_action",
  "last_error": ${last_error},
  "updated_at": "$(date -Iseconds)"
}
JSON
}

# 通过 PID 文件读出当前 qq_id 对应的活进程 PID;
# 找不到 / PID 已死时清理 PID 文件并返回 1.
current_pid_for() {
  local qq_id="$1"
  local pid_path
  pid_path="$(resolve_pid_file "$qq_id")"

  if [ ! -f "$pid_path" ]; then
    return 1
  fi

  local pid
  pid="$(cat "$pid_path" 2>/dev/null || true)"
  if [ -z "$pid" ]; then
    return 1
  fi

  # 进程存活校验 + 命令行二次校验, 避免 PID 复用误杀.
  if kill -0 "$pid" >/dev/null 2>&1; then
    local cmdline
    cmdline="$(ps -o cmd= -p "$pid" 2>/dev/null || true)"
    if [[ "$cmdline" == *"-q $qq_id"* ]]; then
      printf '%s\n' "$pid"
      return 0
    fi
    # PID 还活着但已不是我们管理的 QQ 进程 (PID 复用) -> 清理 PID 文件
  fi
  rm -f "$pid_path"
  return 1
}

# fallback: 直接通过 pgrep 查找命令行中含 ``-q <qq_id>`` 的 qq 进程.
pgrep_pid_for() {
  local qq_id="$1"
  pgrep -f "qq --no-sandbox -q ${qq_id}\$" 2>/dev/null | head -n 1 || true
}

start_napcat() {
  local qq_id="$1"
  require_qq_id "$qq_id"

  mkdir -p "$runtime_dir" "$log_dir"
  local log_path pid_path
  log_path="$(resolve_log_file "$qq_id")"
  pid_path="$(resolve_pid_file "$qq_id")"
  touch "$log_path"

  if pid="$(current_pid_for "$qq_id")"; then
    write_status "$qq_id" true "already_running" "$pid" null
    echo "[OK] qq=${qq_id} already running pid=${pid}"
    return 0
  fi

  if [ ! -x "$qq_executable" ]; then
    local error_text="qq executable missing: $qq_executable"
    write_status "$qq_id" false "start_failed" null "$(escape_json_string "$error_text")"
    echo "[ERROR] $error_text" >&2
    return 1
  fi

  if ! command -v xvfb-run >/dev/null 2>&1; then
    local error_text="xvfb-run not installed; required for headless QQ launch"
    write_status "$qq_id" false "start_failed" null "$(escape_json_string "$error_text")"
    echo "[ERROR] $error_text" >&2
    return 1
  fi

  nohup xvfb-run -a "$qq_executable" --no-sandbox -q "$qq_id" >> "$log_path" 2>&1 &
  local pid="$!"
  echo "$pid" > "$pid_path"
  # 给 QQ + xvfb-run 启动时间; 8s 是经验值, NapCat 进入 ready 一般在 6-7s.
  sleep 8

  if kill -0 "$pid" >/dev/null 2>&1; then
    write_status "$qq_id" true "start" "$pid" null
    echo "[OK] qq=${qq_id} started pid=${pid}"
    return 0
  fi

  # 失败: 抓取最后 20 行日志作为错误摘要
  local last_error="start command exited before readiness check"
  if [ -f "$log_path" ]; then
    last_error="$(tail -n 20 "$log_path" | tr '\n' ' ' | sed 's/[[:space:]]\+/ /g')"
  fi
  rm -f "$pid_path"
  write_status "$qq_id" false "start_failed" null "$(escape_json_string "$last_error")"
  echo "[ERROR] qq=${qq_id} ${last_error}" >&2
  return 4
}

stop_napcat() {
  local qq_id="$1"
  require_qq_id "$qq_id"

  local pid_path
  pid_path="$(resolve_pid_file "$qq_id")"

  local pid=""
  if pid="$(current_pid_for "$qq_id")"; then
    kill "$pid" >/dev/null 2>&1 || true
    # 给 QQ 客户端 3s 平滑退出窗口.
    sleep 3
    if kill -0 "$pid" >/dev/null 2>&1; then
      kill -9 "$pid" >/dev/null 2>&1 || true
    fi
  else
    # PID 文件已失效, 但 pgrep 仍可能找到孤儿进程 (例如 launcher 之外被 nohup 拉起的)
    local fallback_pid
    fallback_pid="$(pgrep_pid_for "$qq_id")"
    if [ -n "$fallback_pid" ]; then
      kill "$fallback_pid" >/dev/null 2>&1 || true
      sleep 3
      if kill -0 "$fallback_pid" >/dev/null 2>&1; then
        kill -9 "$fallback_pid" >/dev/null 2>&1 || true
      fi
    fi
  fi
  rm -f "$pid_path"
  write_status "$qq_id" false "stop" null null
  echo "[OK] qq=${qq_id} stopped"
}

status_napcat() {
  local qq_id="$1"
  require_qq_id "$qq_id"

  local status_path
  status_path="$(resolve_status_file "$qq_id")"

  if pid="$(current_pid_for "$qq_id")"; then
    write_status "$qq_id" true "status" "$pid" null
  else
    write_status "$qq_id" false "status" null null
  fi
  cat "$status_path"
}

list_napcat() {
  # 列出所有匹配 ``qq --no-sandbox -q <id>`` 的进程, 输出 qq_id<TAB>pid 形式.
  pgrep -f "qq --no-sandbox -q [0-9]\\+" 2>/dev/null | while read -r pid; do
    if [ -z "$pid" ]; then continue; fi
    local cmd
    cmd="$(ps -o cmd= -p "$pid" 2>/dev/null || true)"
    if [[ "$cmd" =~ -q[[:space:]]+([0-9]+) ]]; then
      printf '%s\t%s\n' "${BASH_REMATCH[1]}" "$pid"
    fi
  done
}

case "${1:-}" in
  start)
    start_napcat "${2:-}"
    ;;
  stop)
    stop_napcat "${2:-}"
    ;;
  restart)
    stop_napcat "${2:-}"
    start_napcat "${2:-}"
    ;;
  status)
    status_napcat "${2:-}"
    ;;
  log-path)
    require_qq_id "${2:-}"
    printf '%s\n' "$(resolve_log_file "$2")"
    ;;
  list)
    list_napcat
    ;;
  *)
    echo "Usage: $0 {start|stop|restart|status|log-path} <qq_id>" >&2
    echo "       $0 list" >&2
    exit 2
    ;;
esac
