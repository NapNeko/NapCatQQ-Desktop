#!/usr/bin/env bash
# 远端 LinuxQQ rootless 安装脚本（P1 拆分版）
#
# 职责:
#   - 安装运行 LinuxQQ 所需的系统依赖 (apt-get / dnf, 适配 Ubuntu 24.04+ t64)
#   - 下载并解压官方 LinuxQQ deb / rpm 到 $install_base_dir (rootless 安装)
#   - 不触碰 NapCat / loadNapCat.js / 启动脚本 (后者由 remote_install_napcat.sh 负责)
#
# 进度协议:
#   stdout 中以 `[PROGRESS] <0-100> <message>` 形式输出阶段进度,
#   由 Desktop 的 RemoteBackend 解析转发给 ProgressCallback。
#
# 退出码:
#   0   成功 (含 "已存在合法安装, 跳过下载")
#   10  bash 缺失
#   20  依赖工具缺失 (curl / xvfb-run / cp 等)
#   30  不支持的包管理器 (无 dpkg / rpm2cpio)
#   31  不支持的架构 (非 amd64 / arm64)
#   33  下载失败
#   36  解压后未找到 qq 可执行文件
#   37  LinuxQQ 安装包多次下载后仍未通过完整性校验 (涵盖网络中断 / 代理截断 / 镜像源残留损坏缓存等场景)

if [ -z "${BASH_VERSION:-}" ]; then
  echo "[ERROR] this install script must run with bash" >&2
  exit 10
fi

set -euo pipefail

workspace_dir="${workspace_dir:-$HOME/Napcat}"
runtime_dir="${runtime_dir:-$workspace_dir/run}"
log_dir="${log_dir:-$workspace_dir/log}"
tmp_dir="${tmp_dir:-$workspace_dir/tmp}"
package_dir="${package_dir:-$workspace_dir/packages}"
install_base_dir="${install_base_dir:-$workspace_dir}"
qq_base_path="${qq_base_path:-$install_base_dir/opt/QQ}"
qq_executable="${qq_executable:-$qq_base_path/qq}"
qq_package_json_path="${qq_package_json_path:-$qq_base_path/resources/app/package.json}"
status_file="${status_file:-$runtime_dir/status.json}"
log_file="${log_file:-$log_dir/napcat.log}"

backup_napcat_config_dir="${tmp_dir}/napcat-config-backup"
qq_package_installer=""
qq_package_path=""
qq_download_url=""

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

log_progress() {
  # $1 = percent (0-100), $2 = message
  echo "[PROGRESS] $1 $2"
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

ensure_command() {
  local command_name="$1"
  local hint="$2"
  if command -v "$command_name" >/dev/null 2>&1; then
    return
  fi
  log_error "missing command '${command_name}': ${hint}"
  exit 20
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
    log_warn "neither apt-get nor dnf found, skip dependency installation"
    return
  fi

  if ! can_auto_install_deps; then
    log_warn "passwordless sudo unavailable, skip automatic dependency installation"
    return
  fi

  log_info "attempting to install runtime dependencies via sudo (${package_manager})"
  if [ "$package_manager" = "apt-get" ]; then
    DEBIAN_FRONTEND=noninteractive sudo apt-get update -y -qq

    local static_pkgs="curl unzip xvfb xauth procps jq python3 rpm2cpio cpio libnss3 libgbm1"

    # Ubuntu 24.04+ t64 兼容: 自动选择是否使用 t64 变体
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
      if apt-cache show "$t64_variant" >/dev/null 2>&1; then
        log_info "detected $t64_variant, will use this version"
        resolved_pkgs+=("$t64_variant")
      else
        log_info "using standard version $pkg_base"
        resolved_pkgs+=("$pkg_base")
      fi
    done

    local all_pkgs_to_install="$static_pkgs ${resolved_pkgs[*]}"
    log_info "installing packages: $all_pkgs_to_install"
    DEBIAN_FRONTEND=noninteractive sudo apt-get install -y -qq $all_pkgs_to_install || true
    return
  fi

  # RHEL 系扩展支持: CentOS / Rocky / Alma 等最小化镜像缺 ``xorg-x11-server-Xvfb``
  # 等图形依赖, 需要先启用 EPEL. 失败时不阻断, 后续 dnf install 仍会以 ``|| true``
  # 兜底, 避免 sudo 不可用时让整个安装直接退出.
  sudo dnf install -y epel-release || true
  # CRB / PowerTools 在 EL 9 上是部分依赖的来源 (例如 libXScrnSaver), 启用一下;
  # 在 Fedora 上没有这个 repo, ``config-manager --set-enabled`` 会直接失败, 走 || true.
  sudo dnf config-manager --set-enabled crb >/dev/null 2>&1 || \
    sudo dnf config-manager --set-enabled powertools >/dev/null 2>&1 || true

  # RHEL 9 / CentOS Stream 9 的两个坑:
  # 1. ``rpm2cpio`` **不是**独立包, 它由 ``rpm`` 包自带 (rpm 默认已预装),
  #    在包列表里写 rpm2cpio 会报 "No match for argument: rpm2cpio".
  # 2. dnf4 (RHEL 9) 默认是**原子事务**: 任何一个包名匹配不上, 整个 transaction
  #    直接 abort, ``|| true`` 兜不住整个进程的失败 (因为根本没装任何东西).
  #    ``--setopt=strict=0`` 等价于 dnf5 的 ``--skip-unavailable``, dnf4/dnf5 通用,
  #    会让 dnf 跳过无法解析的包名/依赖, 把能装的装上.
  # xauth 在 RHEL 系上由 ``xorg-x11-xauth`` 提供; 旧名 ``xauth`` 在 RHEL 9 不存在.
  sudo dnf install --allowerasing --setopt=strict=0 -y \
    curl unzip xorg-x11-server-Xvfb xorg-x11-xauth procps-ng jq python3 cpio \
    nss mesa-libgbm atk at-spi2-atk gtk3 alsa-lib pango cairo libdrm \
    libXcursor libXrandr libXdamage libXcomposite libXfixes libXrender libXi \
    libXtst libXScrnSaver cups-libs libxkbcommon xcb-util xcb-util-image \
    xcb-util-wm xcb-util-keysyms xcb-util-renderutil fontconfig dejavu-sans-fonts || true

  # 冗余兜底: 即便上一步整体异常或 strict=0 行为漂移, 单独再试一次 xvfb-run 提供方.
  # ``xorg-x11-server-Xvfb`` 在 RHEL 9 / Fedora 上同时提供 ``Xvfb`` 与 ``xvfb-run``,
  # 是 LinuxQQ 无头启动唯一硬依赖, 不能容忍它没装上.
  if ! command -v xvfb-run >/dev/null 2>&1; then
    sudo dnf install -y xorg-x11-server-Xvfb || true
  fi
}

download_file() {
  local url="$1"
  local target_path="$2"

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

# 校验已下载的 LinuxQQ 包是否完整可解析.
#
# 背景: ``curl --fail --retry 2`` 只能拦住 HTTP 状态码异常, 无法 100% 捕获
# TCP 中途被 RST / 代理截断 / 镜像源为了性能扱 200 后提前关连接等场景,
# curl 可能成功 exit 0 但实际落地文件不完整 (typical: 只有头部几 MB).
# 下一次脚本重跑时 ``[ -f ... ]`` 为 true -> 复用缓存 -> dpkg-deb 解压崩。
#
# 对齐 NapCat archive 侧的 ``unzip -t`` 策略 (见 ``local_napcat_fallback.py``),
# 此处用 ``dpkg-deb -I`` / ``rpm -qpi`` 试读包元信息 —— 不解压 data, 费时极小,
# 但损坏包会立刻报错. 另补一条 1MB 最小阈值逻辑抓 "4xx HTML 错误页被当成包
# 保存" 的脱身场景.
verify_qq_package() {
  local pkg_path="$1"
  if [ ! -f "$pkg_path" ]; then
    return 1
  fi
  local pkg_size
  pkg_size=$(stat -c '%s' "$pkg_path" 2>/dev/null || wc -c < "$pkg_path" | tr -d ' ')
  # LinuxQQ 包 ≈20MB+, 1MB 作为保守阈值 —— 小于这个一定损坏
  if [ "${pkg_size:-0}" -lt 1048576 ]; then
    log_warn "package file too small (${pkg_size} bytes), treat as corrupted: $pkg_path"
    return 1
  fi
  if [ "$qq_package_installer" = "dpkg" ]; then
    # 必须完整解码 data.tar 才能发现 LZMA 末尾截断 (实测场景: deb 文件大小看似正确,
    # control.tar 完好, ``dpkg-deb -I`` 通过, 但 data.tar 末尾被 TCP RST 截掉,
    # 实际 ``dpkg -x`` 解压时 lzma "unexpected end of file or stream"). 
    # ``--fsys-tarfile`` 把 data.tar 完整解码到 stdout, 重定向 /dev/null 不落盘,
    # 损坏立刻报错; 耗时 < 1s (只是 ~24MB CPU 解压), 远比下错包再失败划算.
    if ! dpkg-deb --fsys-tarfile "$pkg_path" >/dev/null 2>&1; then
      log_warn "dpkg-deb data.tar decode failed (corrupted package): $pkg_path"
      return 1
    fi
  else
    # rpm payload 完整性: ``rpm2cpio | cpio -t`` 会完整解 CPIO 流并列出条目,
    # 不写入磁盘; 任何 stream 截断 / 校验失败都会让 cpio 退出码非 0.
    # 比 ``rpm -qpi`` (仅读 header) 严格得多, 与 dpkg 侧 ``--fsys-tarfile`` 对齐.
    if ! (rpm2cpio "$pkg_path" 2>/dev/null | cpio -t >/dev/null 2>&1); then
      log_warn "rpm payload decode failed (corrupted package): $pkg_path"
      return 1
    fi
  fi
  return 0
}

# 下载 + 校验 + 重试的统一入口.
# - 缓存合法 -> 直接复用, 不走网络
# - 缓存损坏 / 不存在 -> 删除损坏文件 + 重下, 最多试 3 次
# - 全部失败 -> exit 37, RemoteBackend 会将该错误码映射为 stage 错误展示给用户
download_and_verify_qq_package() {
  local max_attempts=3
  local attempt=1
  while [ $attempt -le $max_attempts ]; do
    if verify_qq_package "$qq_package_path"; then
      if [ $attempt -eq 1 ]; then
        log_info "reuse cached QQ package: ${qq_package_path}"
      else
        log_info "QQ package integrity check passed after attempt ${attempt}: ${qq_package_path}"
      fi
      return 0
    fi
    if [ -f "$qq_package_path" ]; then
      log_warn "removing corrupted package and retry: ${qq_package_path}"
      rm -f "$qq_package_path"
    fi
    log_info "downloading QQ package (attempt ${attempt}/${max_attempts}): ${qq_download_url}"
    # download_file 遇到锁定错误会 exit 33; 这里用子 shell 包住避免中断重试循环
    if ! ( download_file "$qq_download_url" "$qq_package_path" ); then
      log_warn "download attempt ${attempt}/${max_attempts} failed (network error)"
      rm -f "$qq_package_path"
    fi
    attempt=$((attempt + 1))
  done
  log_error "QQ package download/verify failed after ${max_attempts} attempts: ${qq_download_url}"
  exit 37
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

  log_error "unsupported combination: arch=$system_arch, installer=$qq_package_installer"
  exit 31
}

detect_existing_linuxqq() {
  if [ -x "$qq_executable" ] && [ -f "$qq_package_json_path" ]; then
    log_info "detected existing LinuxQQ installation at: ${qq_executable}"
    return 0
  fi
  return 1
}

backup_napcat_config() {
  rm -rf "$backup_napcat_config_dir"
  local napcat_config_dir="${qq_base_path}/resources/app/app_launcher/napcat/config"
  if [ -d "$napcat_config_dir" ]; then
    mkdir -p "$backup_napcat_config_dir"
    cp -a "$napcat_config_dir/." "$backup_napcat_config_dir/"
    log_info "backup existing NapCat config before reinstalling LinuxQQ"
  fi
}

restore_napcat_config() {
  if [ -d "$backup_napcat_config_dir" ]; then
    local napcat_config_dir="${qq_base_path}/resources/app/app_launcher/napcat/config"
    mkdir -p "$napcat_config_dir"
    cp -a "$backup_napcat_config_dir/." "$napcat_config_dir/"
    rm -rf "$backup_napcat_config_dir"
    log_info "restored NapCat config after LinuxQQ reinstall"
  fi
}

ensure_linuxqq_rootless() {
  if detect_existing_linuxqq && [ "${FORCE_LINUXQQ_REINSTALL:-0}" != "1" ]; then
    log_info "reuse existing LinuxQQ install: ${qq_executable}"
    log_progress 95 "linuxqq already installed, skip"
    return
  fi

  log_progress 35 "detecting system arch"
  local system_arch
  system_arch="$(detect_system_arch)"
  select_qq_package "$system_arch"

  log_progress 45 "downloading linuxqq package"
  download_and_verify_qq_package

  local backup_needed=false
  local napcat_config_path="${qq_base_path}/resources/app/app_launcher/napcat/config"
  if [ -d "$napcat_config_path" ]; then
    backup_napcat_config
    backup_needed=true
  fi

  log_progress 70 "extracting linuxqq package"
  rm -rf "$install_base_dir/opt"
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

  if [ "$backup_needed" = true ]; then
    restore_napcat_config
  fi
}

handle_error() {
  local exit_code="$1"
  local line_no="$2"
  local failed_command="$3"
  local error_text="install_linuxqq failed at line ${line_no}: ${failed_command}"

  log_error "$error_text"
  write_status false "install_linuxqq_failed" null "$(escape_json_string "$error_text")"
  exit "$exit_code"
}

trap 'handle_error $? $LINENO "$BASH_COMMAND"' ERR

log_progress 0 "preparing workspace"
log_info "prepare workspace directories"
mkdir -p "$workspace_dir" "$runtime_dir" "$log_dir" "$tmp_dir" "$package_dir"
write_status false "install_linuxqq" null null

log_progress 10 "installing system dependencies"
install_missing_dependencies
ensure_command "curl" "required for downloading LinuxQQ package"
ensure_command "xvfb-run" "required for headless LinuxQQ startup"
ensure_command "cp" "required for file synchronization"

ensure_linuxqq_rootless

log_progress 100 "linuxqq install finished"
log_info "linuxqq install finished"
echo "[OK] linuxqq install finished"
