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

# Launcher 脚本版本号; 每次脚本语义有破坏性变化时 +1, 便于排错.
# v2: P3 修复 stop 不彻底 (xvfb-run wrapper PID vs qq PID) + 启动时切日志.
LAUNCHER_VERSION="2"

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

# fallback: 直接通过 pgrep 查找命令行中含 ``-q <qq_id>`` 的 qq 进程, 取首个.
pgrep_pid_for() {
  local qq_id="$1"
  pgrep -f "qq --no-sandbox -q ${qq_id}\$" 2>/dev/null | head -n 1 || true
}

# 列出所有命令行匹配 ``qq --no-sandbox -q <qq_id>`` 的进程 PID (一行一个).
# 用于 stop_napcat 兜底: nohup + xvfb-run 启动时, $! 拿到的是 xvfb-run
# wrapper PID, 杀它后 qq 子进程可能游离成孤儿; 必须 pgrep 全量收回 PID.
pgrep_pids_for() {
  local qq_id="$1"
  pgrep -f "qq --no-sandbox -q ${qq_id}\$" 2>/dev/null || true
}

# 切日志: 把当前 .log 改名为 .prev 保留排错, 同时建一个空文件让 nohup 写入.
# NapCat 启动时输出会写到干净文件, Desktop 端 ``tail_log`` / ``WebUI URL grep``
# 都只看本次启动的内容, 解决"启动日志包含历史多次启动"的 bug (P3 W3.E).
rotate_log_file() {
  local log_path="$1"
  if [ -f "$log_path" ]; then
    # mv 是 atomic on same fs; -f 覆盖旧的 .prev
    mv -f "$log_path" "${log_path}.prev" 2>/dev/null || true
  fi
  : > "$log_path"
}

start_napcat() {
  local qq_id="$1"
  require_qq_id "$qq_id"

  mkdir -p "$runtime_dir" "$log_dir"
  local log_path pid_path
  log_path="$(resolve_log_file "$qq_id")"
  pid_path="$(resolve_pid_file "$qq_id")"

  if pid="$(current_pid_for "$qq_id")"; then
    write_status "$qq_id" true "already_running" "$pid" null
    echo "[OK] qq=${qq_id} already running pid=${pid}"
    return 0
  fi

  # 兜底: PID 文件已失效但 pgrep 仍找到 qq 孤儿进程 (xvfb-run wrapper 已死,
  # qq 自己仍登录着) → 必须先彻底清掉, 否则新 launch 会撞上"已登录,无法重复登录".
  local orphan_pids
  orphan_pids="$(pgrep_pids_for "$qq_id")"
  if [ -n "$orphan_pids" ]; then
    echo "[WARN] qq=${qq_id} found orphan pids before start: $(echo "$orphan_pids" | tr '\n' ' '); cleaning up" >&2
    while IFS= read -r op; do
      [ -z "$op" ] && continue
      kill "$op" >/dev/null 2>&1 || true
    done <<< "$orphan_pids"
    sleep 2
    while IFS= read -r op; do
      [ -z "$op" ] && continue
      if kill -0 "$op" >/dev/null 2>&1; then
        kill -9 "$op" >/dev/null 2>&1 || true
      fi
    done <<< "$orphan_pids"
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

  # 切日志: 启动前把上一轮的 .log 归档为 .prev, 让本次 nohup 写入干净文件.
  rotate_log_file "$log_path"

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

# 收 PID 候选集去重后逐一 SIGTERM/SIGKILL; 不依赖 ``current_pid_for`` 的单点 PID.
_kill_all_qq_processes() {
  local qq_id="$1"
  # 候选集: PID 文件 + 命令行匹配; 用换行分隔, sort -u 去重
  local pid_path
  pid_path="$(resolve_pid_file "$qq_id")"
  local file_pid=""
  if [ -f "$pid_path" ]; then
    file_pid="$(cat "$pid_path" 2>/dev/null || true)"
  fi
  local pgrep_list
  pgrep_list="$(pgrep_pids_for "$qq_id")"

  local candidates
  candidates="$(printf '%s\n%s\n' "$file_pid" "$pgrep_list" | grep -v '^$' | sort -u || true)"
  if [ -z "$candidates" ]; then
    return 0
  fi

  # SIGTERM 阶段
  while IFS= read -r tp; do
    [ -z "$tp" ] && continue
    kill "$tp" >/dev/null 2>&1 || true
  done <<< "$candidates"
  # 给 QQ 客户端 3s 平滑退出窗口
  sleep 3
  # SIGKILL 阶段
  while IFS= read -r tp; do
    [ -z "$tp" ] && continue
    if kill -0 "$tp" >/dev/null 2>&1; then
      kill -9 "$tp" >/dev/null 2>&1 || true
    fi
  done <<< "$candidates"

  # 二次兜底: 仍有同 qq_id 的 qq 进程 → 再 pgrep 一遍 KILL
  sleep 1
  local lingering
  lingering="$(pgrep_pids_for "$qq_id")"
  if [ -n "$lingering" ]; then
    while IFS= read -r tp; do
      [ -z "$tp" ] && continue
      kill -9 "$tp" >/dev/null 2>&1 || true
    done <<< "$lingering"
  fi
}

stop_napcat() {
  local qq_id="$1"
  require_qq_id "$qq_id"

  local pid_path
  pid_path="$(resolve_pid_file "$qq_id")"

  _kill_all_qq_processes "$qq_id"

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
  version)
    # P3: Desktop 端可通过此命令探测远端 launcher 版本; 不匹配时提示用户
    # 走 "维护 ▾ → 强制更新 NapCat" 重新部署 launcher.
    printf '%s\n' "$LAUNCHER_VERSION"
    ;;
  *)
    echo "Usage: $0 {start|stop|restart|status|log-path} <qq_id>" >&2
    echo "       $0 list" >&2
    echo "       $0 version" >&2
    exit 2
    ;;
esac
