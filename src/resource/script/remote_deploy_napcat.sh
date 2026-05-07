#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
  echo "[ERROR] this deploy script must run with bash" >&2
  exit 10
fi

set -euo pipefail

# 适配标准 NapCat 安装器路径
workspace_dir="${workspace_dir:-$HOME/Napcat}"
runtime_dir="${runtime_dir:-$workspace_dir/run}"
config_dir="${config_dir:-$workspace_dir/opt/QQ/resources/app/app_launcher/napcat/config}"
log_dir="${log_dir:-$workspace_dir/log}"
tmp_dir="${tmp_dir:-$workspace_dir/tmp}"
package_dir="${package_dir:-$workspace_dir/packages}"
config_archive="${config_archive:-$tmp_dir/config-export.zip}"
status_file="${status_file:-$runtime_dir/status.json}"
pid_file="${pid_file:-$runtime_dir/napcat.pid}"
log_file="${log_file:-$log_dir/napcat.log}"
install_base_dir="${install_base_dir:-$workspace_dir}"
qq_base_path="${qq_base_path:-$install_base_dir/opt/QQ}"
target_folder="${target_folder:-$qq_base_path/resources/app/app_launcher}"
qq_executable="${qq_executable:-$qq_base_path/qq}"
qq_package_json_path="${qq_package_json_path:-$qq_base_path/resources/app/package.json}"
launcher_script="${launcher_script:-$workspace_dir/napcat.sh}"
staging_dir="${tmp_dir}/deploy-staging"
runtime_config_unpack_dir="${staging_dir}/runtime-config"
napcat_unpack_dir="${staging_dir}/NapCat"
backup_napcat_config_dir="${tmp_dir}/napcat-config-backup"
napcat_archive_path="${package_dir}/NapCat.Shell.zip"
qq_package_installer=""
qq_package_path=""

timestamp() {
  date +"%Y-%m-%d %H:%M:%S"
}

log_info() {
  echo "[INFO] $(timestamp) $*"
}

log_warn() {
  echo "[WARN] $(timestamp) $*" >&2
}

log_error() {
  echo "[ERROR] $(timestamp) $*" >&2
}

write_status() {
  local running="$1"
  local last_action="$2"
  local pid_value="${3:-null}"
  local last_error="${4:-null}"

  mkdir -p "$(dirname "$status_file")"
  cat > "$status_file" <<EOF
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
EOF
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

detect_package_installer() {
  if command -v dpkg >/dev/null 2>&1; then
    qq_package_installer="dpkg"
    return
  fi
  if command -v rpm2cpio >/dev/null 2>&1 && command -v cpio >/dev/null 2>&1; then
    qq_package_installer="rpm"
    return
  fi
  log_error "unsupported package installer: require dpkg or rpm2cpio+cpio"
  exit 30
}

detect_system_arch() {
  local raw_arch
  raw_arch="$(uname -m)"
  case "$raw_arch" in
    x86_64|amd64)
      echo "amd64"
      ;;
    aarch64|arm64)
      echo "arm64"
      ;;
    *)
      log_error "unsupported system architecture: ${raw_arch}"
      exit 31
      ;;
  esac
}

can_auto_install_deps() {
  command -v sudo >/dev/null 2>&1 && sudo -n true >/dev/null 2>&1
}

install_missing_dependencies() {
  local package_manager=""
  if command -v apt-get >/dev/null 2>&1; then
    package_manager="apt-get"
  elif command -v dnf >/dev/null 2>&1; then
    package_manager="dnf"
  fi

  if [ -z "$package_manager" ]; then
    return
  fi

  if ! can_auto_install_deps; then
    log_warn "passwordless sudo unavailable, skip automatic dependency installation"
    return
  fi

  log_info "attempting to install runtime dependencies via sudo"
  if [ "$package_manager" = "apt-get" ]; then
    DEBIAN_FRONTEND=noninteractive sudo apt-get update -y -qq

    # 静态依赖包列表
    local static_pkgs="curl unzip xvfb xauth procps jq python3 rpm2cpio cpio libnss3 libgbm1"

    # 需要检查是否存在 t64 版本的动态依赖包列表 (Ubuntu 24.04+ 重命名)
    local pkgs_to_check=(
      "libglib2.0-0"
      "libatk1.0-0"
      "libatspi2.0-0"
      "libgtk-3-0"
      "libasound2"
    )

    local resolved_pkgs=()
    log_info "detecting system library versions (t64 compatibility)..."
    for pkg_base in "${pkgs_to_check[@]}"; do
      local t64_variant="${pkg_base}t64"
      # 使用 apt-cache show 检查 t64 版本的包是否存在
      if apt-cache show "$t64_variant" >/dev/null 2>&1; then
        log_info "detected $t64_variant, will use this version"
        resolved_pkgs+=("$t64_variant")
      else
        log_info "using standard version $pkg_base"
        resolved_pkgs+=("$pkg_base")
      fi
    done

    # 合并并安装所有依赖
    local all_pkgs_to_install="$static_pkgs ${resolved_pkgs[*]}"
    log_info "installing packages: $all_pkgs_to_install"
    DEBIAN_FRONTEND=noninteractive sudo apt-get install -y -qq $all_pkgs_to_install || true
    return
  fi

  sudo dnf install --allowerasing -y \
    curl unzip xorg-x11-server-Xvfb xauth procps-ng jq python3 rpm2cpio cpio \
    nss mesa-libgbm atk at-spi2-atk gtk3 alsa-lib pango cairo libdrm \
    libXcursor libXrandr libXdamage libXcomposite libXfixes libXrender libXi \
    libXtst libXScrnSaver cups-libs libxkbcommon xcb-util xcb-util-image \
    xcb-util-wm xcb-util-keysyms xcb-util-renderutil fontconfig dejavu-sans-fonts || true
}

ensure_command() {
  local command_name="$1"
  local hint="$2"
  if command -v "$command_name" >/dev/null 2>&1; then
    return
  fi
  log_error "missing command '${command_name}': ${hint}"
  exit 32
}

download_file() {
  local url="$1"
  local target_path="$2"

  # 参数校验
  if [ -z "$url" ]; then
    log_error "download url is empty"
    exit 33
  fi
  if [ -z "$target_path" ]; then
    log_error "download target path is empty"
    exit 33
  fi

  log_info "download file: ${url} -> ${target_path}"
  mkdir -p "$(dirname "$target_path")"

  if command -v curl >/dev/null 2>&1; then
    curl -k -L --fail --retry 2 --connect-timeout 20 "$url" -o "$target_path"
    return
  fi
  if command -v wget >/dev/null 2>&1; then
    wget -O "$target_path" "$url"
    return
  fi
  log_error "neither curl nor wget is available for downloading ${url}"
  exit 33
}

extract_zip_to() {
  local archive_path="$1"
  local target_dir="$2"
  mkdir -p "$target_dir"
  if command -v unzip >/dev/null 2>&1; then
    unzip -o "$archive_path" -d "$target_dir" >/dev/null
    return
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$archive_path" "$target_dir" <<'PY'
import pathlib
import sys
import zipfile

archive = pathlib.Path(sys.argv[1])
target = pathlib.Path(sys.argv[2])
target.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(archive, "r") as zip_file:
    zip_file.extractall(target)
PY
    return
  fi
  log_error "unzip and python3 are both unavailable"
  exit 34
}

patch_package_json_main() {
  local package_json_path="$1"
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$package_json_path" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
payload = json.loads(path.read_text(encoding="utf-8"))
payload["main"] = "./loadNapCat.js"
path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
    return
  fi
  if command -v jq >/dev/null 2>&1; then
    jq '.main = "./loadNapCat.js"' "$package_json_path" > "${package_json_path}.tmp"
    mv "${package_json_path}.tmp" "$package_json_path"
    return
  fi
  log_error "python3 or jq is required to patch ${package_json_path}"
  exit 35
}

select_qq_package() {
  local system_arch="$1"
  detect_package_installer

  log_info "selecting QQ package for arch=$system_arch, installer=$qq_package_installer"

  if [ "$system_arch" = "amd64" ] && [ "$qq_package_installer" = "dpkg" ]; then
    qq_package_path="${package_dir}/linuxqq_3.2.25-45758_amd64.deb"
    qq_download_url="https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_amd64.deb"
    return
  fi
  if [ "$system_arch" = "amd64" ] && [ "$qq_package_installer" = "rpm" ]; then
    qq_package_path="${package_dir}/linuxqq_3.2.25-45758_x86_64.rpm"
    qq_download_url="https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_x86_64.rpm"
    return
  fi
  if [ "$system_arch" = "arm64" ] && [ "$qq_package_installer" = "dpkg" ]; then
    qq_package_path="${package_dir}/linuxqq_3.2.25-45758_arm64.deb"
    qq_download_url="https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_arm64.deb"
    return
  fi
  if [ "$system_arch" = "arm64" ] && [ "$qq_package_installer" = "rpm" ]; then
    qq_package_path="${package_dir}/linuxqq_3.2.25-45758_aarch64.rpm"
    qq_download_url="https://dldir1.qq.com/qqfile/qq/QQNT/7516007c/linuxqq_3.2.25-45758_aarch64.rpm"
    return
  fi

  log_error "unsupported system architecture or package installer: arch=$system_arch, installer=$qq_package_installer"
  exit 31
}

sync_runtime_export_config() {
  if [ ! -f "$config_archive" ]; then
    log_error "config archive not found: $config_archive"
    exit 20
  fi

  rm -rf "$runtime_config_unpack_dir"
  mkdir -p "$runtime_config_unpack_dir" "$config_dir"
  log_info "extract runtime config archive"
  extract_zip_to "$config_archive" "$runtime_config_unpack_dir"
  cp -a "$runtime_config_unpack_dir/." "$config_dir/"
}

# 检测远端是否已有标准 NapCat 安装
detect_existing_napcat() {
  if [ -d "${target_folder}/napcat" ] && [ -f "${target_folder}/napcat/napcat.mjs" ]; then
    log_info "detected existing NapCat installation at: ${target_folder}/napcat"
    return 0
  fi
  return 1
}

# 检测远端是否已有标准 LinuxQQ 安装
detect_existing_linuxqq() {
  if [ -x "$qq_executable" ] && [ -f "$qq_package_json_path" ]; then
    log_info "detected existing LinuxQQ installation at: ${qq_executable}"
    return 0
  fi
  return 1
}

ensure_linuxqq_rootless() {
  # 优先检测并使用已有安装
  if detect_existing_linuxqq; then
    log_info "reuse existing LinuxQQ install: ${qq_executable}"
    return
  fi

  local system_arch
  system_arch="$(detect_system_arch)"
  select_qq_package "$system_arch"

  if [ ! -f "$qq_package_path" ]; then
    log_info "downloading QQ package to: ${qq_package_path}"
    download_file "$qq_download_url" "$qq_package_path"
  else
    log_info "reuse cached QQ package: ${qq_package_path}"
  fi

  # 如果已存在安装，先备份配置
  local backup_needed=false
  local napcat_config_path="${target_folder}/napcat/config"
  if [ -d "$napcat_config_path" ]; then
    log_info "backing up existing NapCat config before QQ reinstall"
    backup_napcat_config
    backup_needed=true
  fi

  rm -rf "$install_base_dir"
  mkdir -p "$install_base_dir"
  log_info "extract rootless LinuxQQ package"
  if [ "$qq_package_installer" = "dpkg" ]; then
    dpkg -x "$qq_package_path" "$install_base_dir"
  else
    (cd "$install_base_dir" && rpm2cpio "$qq_package_path" | cpio -idm >/dev/null 2>&1)
  fi

  if [ ! -x "$qq_executable" ]; then
    log_error "QQ executable missing after extraction: ${qq_executable}"
    exit 36
  fi

  # 恢复配置备份
  if [ "$backup_needed" = true ]; then
    log_info "restoring NapCat config after QQ reinstall"
    restore_napcat_config
  fi
}

backup_napcat_config() {
  rm -rf "$backup_napcat_config_dir"
  if [ -d "${target_folder}/napcat/config" ]; then
    mkdir -p "$backup_napcat_config_dir"
    cp -a "${target_folder}/napcat/config/." "$backup_napcat_config_dir/"
    log_info "backup existing NapCat config"
  fi
}

restore_napcat_config() {
  if [ -d "$backup_napcat_config_dir" ]; then
    mkdir -p "${target_folder}/napcat/config"
    cp -a "$backup_napcat_config_dir/." "${target_folder}/napcat/config/"
    rm -rf "$backup_napcat_config_dir"
    log_info "restore previous NapCat config backup"
  fi
}

clean_napcat_environment() {
  """清理 NapCat 环境（包括 QQ、NapCat 和运行时文件）。"""
  log_info "starting NapCat environment cleanup..."

  # 1. 停止运行中的 NapCat 进程
  log_info "stopping any running NapCat processes..."
  local pids
  pids=$(pgrep -f "qq --no-sandbox" 2>/dev/null || true)
  if [ -n "$pids" ]; then
    echo "$pids" | xargs kill -9 2>/dev/null || true
    log_info "stopped running processes"
  fi

  # 2. 清理运行时目录
  log_info "cleaning runtime directories..."
  rm -rf "$runtime_dir" 2>/dev/null || true
  rm -rf "$tmp_dir" 2>/dev/null || true
  rm -rf "$package_dir" 2>/dev/null || true

  # 3. 清理日志目录（保留目录本身，只清空内容）
  log_info "cleaning log directory..."
  rm -rf "$log_dir"/*.log 2>/dev/null || true
  rm -rf "$log_dir"/*.pid 2>/dev/null || true

  # 4. 清理 NapCat 安装
  log_info "removing NapCat installation..."
  rm -rf "${target_folder}/napcat" 2>/dev/null || true

  # 5. 清理 QQ 注入文件
  log_info "removing QQ injection files..."
  rm -f "${qq_base_path}/resources/app/loadNapCat.js" 2>/dev/null || true

  # 6. 清理启动脚本
  log_info "removing launcher script..."
  rm -f "$launcher_script" 2>/dev/null || true

  # 7. 恢复 QQ 的原始 package.json（如果备份存在）
  if [ -f "${qq_package_json_path}.backup" ]; then
    log_info "restoring original QQ package.json..."
    mv "${qq_package_json_path}.backup" "$qq_package_json_path" 2>/dev/null || true
  fi

  # 8. 可选：清理 QQ 安装（通过参数控制）
  if [ "${CLEAN_QQ:-0}" = "1" ]; then
    log_info "removing QQ installation (CLEAN_QQ=1)..."
    rm -rf "$qq_base_path" 2>/dev/null || true
    # 清理下载的 QQ 安装包
    rm -f "${package_dir}"/*.deb 2>/dev/null || true
    rm -f "${package_dir}"/*.rpm 2>/dev/null || true
  fi

  log_info "NapCat environment cleanup completed"
}

ensure_napcat_installed() {
  # 检测是否已有 NapCat 安装
  if detect_existing_napcat; then
    log_info "NapCat already installed, checking if update needed..."
    # 如果需要强制更新，可以设置环境变量 FORCE_NAPCAT_UPDATE=1
    if [ "${FORCE_NAPCAT_UPDATE:-0}" != "1" ]; then
      log_info "skipping NapCat download (set FORCE_NAPCAT_UPDATE=1 to force update)"
      # 确保 loadNapCat.js 存在
      if [ ! -f "${qq_base_path}/resources/app/loadNapCat.js" ]; then
        log_info "creating loadNapCat.js inject script"
        mkdir -p "${qq_base_path}/resources/app"
        cat > "${qq_base_path}/resources/app/loadNapCat.js" <<EOF
(async () => {await import('file:///${target_folder}/napcat/napcat.mjs');})();
EOF
        patch_package_json_main "$qq_package_json_path"
      fi
      return
    fi
    log_info "FORCE_NAPCAT_UPDATE=1, proceeding with update..."
  fi

  local napcat_url="https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip"

  if [ -z "$napcat_archive_path" ]; then
    log_error "napcat_archive_path is not set"
    exit 37
  fi

  if [ ! -f "$napcat_archive_path" ]; then
    log_info "downloading NapCat from: ${napcat_url}"
    download_file "$napcat_url" "$napcat_archive_path"
  else
    log_info "reuse cached NapCat package: ${napcat_archive_path}"
  fi

  rm -rf "$napcat_unpack_dir"
  mkdir -p "$napcat_unpack_dir"
  log_info "extract NapCat shell package"
  extract_zip_to "$napcat_archive_path" "$napcat_unpack_dir"

  backup_napcat_config
  mkdir -p "${target_folder}/napcat"
  cp -a "$napcat_unpack_dir/." "${target_folder}/napcat/"
  chmod -R +x "${target_folder}/napcat"
  restore_napcat_config

  mkdir -p "${qq_base_path}/resources/app"
  cat > "${qq_base_path}/resources/app/loadNapCat.js" <<EOF
(async () => {await import('file:///${target_folder}/napcat/napcat.mjs');})();
EOF

  patch_package_json_main "$qq_package_json_path"
}

write_launcher_script() {
  cat > "$launcher_script" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

workspace_dir="${workspace_dir:-$HOME/Napcat}"
qq_base_path="${qq_base_path:-$workspace_dir/opt/QQ}"
qq_executable="${qq_executable:-$qq_base_path/qq}"
log_dir="${log_dir:-$workspace_dir/log}"
status_file="${status_file:-$workspace_dir/run/status.json}"
pid_file="${pid_file:-$workspace_dir/run/napcat.pid}"
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
EOF
  chmod +x "$launcher_script"
}

handle_error() {
  local exit_code="$1"
  local line_no="$2"
  local failed_command="$3"
  local error_text="deploy failed at line ${line_no}: ${failed_command}"

  log_error "$error_text"
  write_status false "deploy_failed" null "$(escape_json_string "$error_text")"
  exit "$exit_code"
}

trap 'handle_error $? $LINENO "$BASH_COMMAND"' ERR

log_info "prepare workspace directories"
mkdir -p "$workspace_dir" "$runtime_dir" "$config_dir" "$log_dir" "$tmp_dir" "$package_dir"
write_status false "deploying" null null

install_missing_dependencies
ensure_command "curl" "required for downloading QQ and NapCat packages"
ensure_command "xvfb-run" "required for headless LinuxQQ startup"
ensure_command "cp" "required for file synchronization"

rm -rf "$staging_dir"
mkdir -p "$staging_dir"
sync_runtime_export_config
ensure_linuxqq_rootless
ensure_napcat_installed
write_launcher_script

write_status false "deploy" null null
log_info "remote deploy finished"
echo "[OK] remote deploy finished"
