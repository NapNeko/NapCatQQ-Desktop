#!/usr/bin/env bash
# 远端 NapCat 安装脚本（P1 拆分版）
#
# 职责:
#   - 下载并解压 NapCat.Shell.zip 到 $target_folder/napcat
#   - 注入 loadNapCat.js 到 LinuxQQ 的 resources/app
#   - patch package.json 的 main 字段
#   - 该脚本要求 LinuxQQ 已通过 remote_install_linuxqq.sh 安装就绪
#
# 进度协议:
#   stdout 中以 `[PROGRESS] <0-100> <message>` 形式输出阶段进度,
#   由 Desktop 的 RemoteBackend 解析转发给 ProgressCallback。
#
# 环境变量:
#   FORCE_NAPCAT_UPDATE=1  强制重新下载并解压 NapCat (默认仅在缺失时下载)
#   NAPCAT_DOWNLOAD_URL    自定义下载地址 (默认从官方 GitHub Release latest 下载)
#   NAPCAT_EXPECTED_SHA512 期望的 NapCat.Shell.zip SHA512 (128 位 hex). 设置后强制
#                          校验下载产物; 不一致 / 工具缺失则中断并退出 36
#
# 退出码:
#   0   成功
#   31  参数无效或前置条件不满足
#   32  失败 (其他异常)
#   33  下载失败
#   34  解压工具缺失 (unzip 与 python3 都不可用)
#   35  package.json patch 工具缺失 (python3 与 jq 都不可用)
#   36  SHA512 完整性校验失败 (P5 F1.4)
#   37  napcat archive 路径未设置
#   38  LinuxQQ 未安装 (前置条件检查)

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
target_folder="${target_folder:-$qq_base_path/resources/app/app_launcher}"
qq_executable="${qq_executable:-$qq_base_path/qq}"
qq_package_json_path="${qq_package_json_path:-$qq_base_path/resources/app/package.json}"
launcher_script="${launcher_script:-$workspace_dir/napcat.sh}"
status_file="${status_file:-$runtime_dir/status.json}"
log_file="${log_file:-$log_dir/napcat.log}"

staging_dir="${tmp_dir}/deploy-staging"
napcat_unpack_dir="${staging_dir}/NapCat"
backup_napcat_config_dir="${tmp_dir}/napcat-config-backup"
napcat_archive_path="${napcat_archive_path:-${package_dir}/NapCat.Shell.zip}"
napcat_download_url="${NAPCAT_DOWNLOAD_URL:-https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.zip}"

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

# ============================================================
# GitHub 镜像策略 (并行探测 + 首个响应)
# ============================================================
#
# 默认行为:
#   1. 在第一次需要下载 github 资源时, 同时向所有候选镜像 (含直连)
#      发起 HEAD 探测 (3s connect / 5s max), 收集响应的镜像
#   2. 按候选优先级排序, 选定第一个有响应的作为本次部署的主镜像
#   3. 下载失败时按优先级依次降级到其他响应过的镜像
#
# 环境变量覆盖:
#   GITHUB_MIRROR_LIST    空格分隔的候选列表, 完全替换内置默认.
#                         空字符串元素表示"直连". 例:
#                           "" "https://gh-proxy.com/" "https://ghproxy.net/"
#   GITHUB_MIRROR_PREFIX  单个镜像前缀; 设置时跳过探测, 直接使用该值.
#                         设为 "DIRECT" 表示强制直连. 兼容旧版.
#
# 内置候选列表 (按优先级排序, 直连放第一便于海外 / 无 GFW 用户)
DEFAULT_MIRROR_CANDIDATES=(
  ""                                   # 0. 直连
  "https://gh-proxy.com/"              # 1. gh-proxy
  "https://mirror.ghproxy.com/"        # 2. ghproxy 镜像
  "https://ghproxy.net/"               # 3. ghproxy 备用域名
  "https://github.moeyy.xyz/"          # 4. moeyy 个人维护
  "https://gh.api.99988866.xyz/"       # 5. 99988866
  "https://hub.gitmirror.com/"         # 6. gitmirror
  "https://ghps.cc/"                   # 7. ghps
)

# 全局: 探测后按优先级排序的可用镜像列表 (初始为空; 探测后填充)
PROBED_MIRRORS=()
PROBE_DONE=0

_build_candidate_list() {
  # 优先使用用户自定义列表, 否则用内置默认
  if [ -n "${GITHUB_MIRROR_LIST:-}" ]; then
    # shellcheck disable=SC2086
    read -r -a MIRROR_CANDIDATES <<<"$GITHUB_MIRROR_LIST"
  else
    MIRROR_CANDIDATES=("${DEFAULT_MIRROR_CANDIDATES[@]}")
  fi
}

_probe_one_mirror() {
  # 探测单个镜像, 把 HTTP 状态码写入 $tmpdir/probe_$index
  local index="$1"
  local prefix="$2"
  local tmpdir="$3"
  local probe_target="https://github.com/NapNeko/NapCatQQ"
  local probe_url
  if [ -z "$prefix" ]; then
    probe_url="$probe_target"
  else
    probe_url="${prefix}${probe_target}"
  fi
  local code
  code=$(curl -k -s -o /dev/null -w "%{http_code}" \
    --connect-timeout 3 --max-time 5 \
    -I "$probe_url" 2>/dev/null || echo "000")
  echo "$code" >"${tmpdir}/probe_${index}"
}

_probe_all_mirrors() {
  # 并行探测所有候选; 仅执行一次 (幂等)
  if [ "$PROBE_DONE" = "1" ]; then
    return
  fi
  PROBE_DONE=1

  # 用户显式指定单一镜像: 跳过探测
  if [ -n "${GITHUB_MIRROR_PREFIX:-}" ]; then
    if [ "$GITHUB_MIRROR_PREFIX" = "DIRECT" ]; then
      PROBED_MIRRORS=("")
      log_info "using explicit mirror: direct"
    else
      PROBED_MIRRORS=("$GITHUB_MIRROR_PREFIX")
      log_info "using explicit mirror: ${GITHUB_MIRROR_PREFIX}"
    fi
    return
  fi

  _build_candidate_list

  log_info "probing ${#MIRROR_CANDIDATES[@]} github mirror(s) in parallel (5s cap)..."

  local tmpdir
  tmpdir=$(mktemp -d)

  # 启动并行探测
  local i
  for i in "${!MIRROR_CANDIDATES[@]}"; do
    _probe_one_mirror "$i" "${MIRROR_CANDIDATES[$i]}" "$tmpdir" &
  done
  wait

  # 按优先级收集响应
  for i in "${!MIRROR_CANDIDATES[@]}"; do
    local code
    code=$(cat "${tmpdir}/probe_${i}" 2>/dev/null || echo "000")
    local label="${MIRROR_CANDIDATES[$i]:-direct}"
    case "$code" in
      200|301|302|403)
        # 403 也算 OK (某些镜像对 HEAD 返 403 但 GET 正常)
        log_info "  [OK]   ${label} -> ${code}"
        PROBED_MIRRORS+=("${MIRROR_CANDIDATES[$i]}")
        ;;
      *)
        log_info "  [FAIL] ${label} -> ${code}"
        ;;
    esac
  done
  rm -rf "$tmpdir"

  if [ "${#PROBED_MIRRORS[@]}" = "0" ]; then
    log_warn "no mirror responded; will still try direct connection as last resort"
    PROBED_MIRRORS=("")
  else
    log_info "selected primary mirror: ${PROBED_MIRRORS[0]:-direct} (+${#PROBED_MIRRORS[@]} fallback(s))"
  fi
}

_try_download() {
  # 单次下载尝试; 返回 0 表示成功, 1 表示失败, 2 表示无可用下载工具
  #
  # 关键参数:
  #   --connect-timeout 8     连接阶段最多 8s, github 不可达时快速失败让 mirror 回退接管
  #   --retry 0               不做内置重试, 由外层逻辑决定下一步(直连 -> mirror)
  #   --silent --show-error   关闭 curl 进度条避免控制台被 \r 刷屏冲掉, 但仍输出错误
  #   -L                      跟随重定向 (release CDN 会跳到 objects.githubusercontent.com)
  #   --fail                  HTTP >=400 即视为失败
  local url="$1"
  local target_path="$2"
  if command -v curl >/dev/null 2>&1; then
    if curl -k -L --fail --retry 0 --connect-timeout 8 \
         --silent --show-error \
         "$url" -o "$target_path"; then
      return 0
    fi
    rm -f "$target_path" 2>/dev/null || true
    return 1
  fi
  if command -v wget >/dev/null 2>&1; then
    # wget: 4s DNS + 8s connect cap, 单次尝试, 静默
    if wget --quiet --tries=1 --connect-timeout=8 --dns-timeout=4 \
         -O "$target_path" "$url"; then
      return 0
    fi
    rm -f "$target_path" 2>/dev/null || true
    return 1
  fi
  return 2
}

_is_github_url() {
  case "$1" in
    https://github.com/*|https://raw.githubusercontent.com/*|https://objects.githubusercontent.com/*|https://gist.githubusercontent.com/*)
      return 0
      ;;
  esac
  return 1
}

download_file() {
  local url="$1"
  local target_path="$2"

  if [ -z "$url" ] || [ -z "$target_path" ]; then
    log_error "download url or target path is empty"
    exit 33
  fi

  log_info "download file: ${url} -> ${target_path}"
  mkdir -p "$(dirname "$target_path")"

  # 非 github 域名 (如腾讯 CDN), 直接尝试一次, 不走镜像逻辑
  if ! _is_github_url "$url"; then
    local rc=0
    _try_download "$url" "$target_path" || rc=$?
    if [ "$rc" = "0" ]; then
      return
    fi
    if [ "$rc" = "2" ]; then
      log_error "neither curl nor wget is available for downloading ${url}"
      exit 33
    fi
    log_error "download failed: ${url}"
    exit 33
  fi

  # github 资源: 按探测后的镜像列表依次尝试
  _probe_all_mirrors

  local prefix
  for prefix in "${PROBED_MIRRORS[@]}"; do
    local effective_url
    if [ -z "$prefix" ]; then
      effective_url="$url"
    else
      effective_url="${prefix}${url}"
    fi
    local label="${prefix:-direct}"
    log_info "trying download via: ${label}"

    local rc=0
    _try_download "$effective_url" "$target_path" || rc=$?
    if [ "$rc" = "0" ]; then
      if [ -n "$prefix" ]; then
        log_info "download succeeded via mirror: ${prefix}"
      fi
      return
    fi
    if [ "$rc" = "2" ]; then
      log_error "neither curl nor wget is available"
      exit 33
    fi
    log_warn "download failed via ${label}, trying next..."
  done

  log_error "all ${#PROBED_MIRRORS[@]} candidate(s) failed for: ${url}"
  exit 33
}

# 计算并校验 NapCat archive 的 SHA512 (P5 F1.4)
#
# - 当 ``$NAPCAT_EXPECTED_SHA512`` 未设置 / 为空时, 退化为"仅警告不阻断", 兼容老
#   Desktop 客户端调用本脚本的场景.
# - 当 ``$NAPCAT_EXPECTED_SHA512`` 设置但 ``sha512sum`` / ``openssl`` 都不可用时,
#   按"远端环境异常时偏严"策略, 删除 archive 并退出 36 (不静默放行).
# - 当 hash 不一致时, 删除 archive 防止被解压并退出 36.
verify_napcat_archive_sha512() {
  local archive_path="$1"
  local expected="${NAPCAT_EXPECTED_SHA512:-}"

  if [ -z "$expected" ]; then
    log_warn "NAPCAT_EXPECTED_SHA512 not provided, integrity check skipped (legacy desktop)"
    return 0
  fi

  log_progress 55 "verifying napcat shell sha512"
  local actual=""
  if command -v sha512sum >/dev/null 2>&1; then
    actual=$(sha512sum "$archive_path" | awk '{print $1}')
    # GNU sha512sum 在文件名含 ``\`` / ``\n`` 时, 会在 hash 前加 ``\`` 转义标记
    # (BSD compat 行为). Linux 生产路径不会含 ``\``, 但跨平台测试 / 奇怪挂载点
    # (例如 ``/mnt/c/...``) 可能触发. 用 bash 参数展开剥前导反斜杠, 避开 sed
    # 在 MSYS Git Bash 下的路径转换坑. 单引号 pattern 避免双引号下反斜杠折叠.
    actual=${actual#'\'}
  elif command -v openssl >/dev/null 2>&1; then
    # OpenSSL 输出形如 ``SHA512(file)= <hex>`` 或 ``SHA2-512(file)= <hex>``;
    # 取最后一个空格后的 token 即为 hex 摘要.
    actual=$(openssl dgst -sha512 "$archive_path" 2>/dev/null | awk '{print $NF}')
  else
    log_error "neither sha512sum nor openssl is available, cannot verify integrity"
    rm -f "$archive_path" 2>/dev/null || true
    exit 36
  fi

  if [ -z "$actual" ]; then
    log_error "sha512 calculation produced empty output"
    rm -f "$archive_path" 2>/dev/null || true
    exit 36
  fi

  # 大小写不敏感比较: 用户传入的 hex 可能是大写, sha512sum 输出小写
  local expected_lc actual_lc
  expected_lc=$(printf "%s" "$expected" | tr '[:upper:]' '[:lower:]')
  actual_lc=$(printf "%s" "$actual" | tr '[:upper:]' '[:lower:]')
  if [ "$actual_lc" != "$expected_lc" ]; then
    log_error "sha512 mismatch: expected=${expected_lc} actual=${actual_lc}"
    rm -f "$archive_path" 2>/dev/null || true
    exit 36
  fi

  log_info "sha512 verified ok"
  return 0
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

detect_existing_napcat() {
  if [ -d "${target_folder}/napcat" ] && [ -f "${target_folder}/napcat/napcat.mjs" ]; then
    log_info "detected existing NapCat installation at: ${target_folder}/napcat"
    return 0
  fi
  return 1
}

ensure_linuxqq_present() {
  if [ ! -x "$qq_executable" ] || [ ! -f "$qq_package_json_path" ]; then
    log_error "LinuxQQ is not installed at ${qq_executable}; run remote_install_linuxqq.sh first"
    exit 38
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

write_load_napcat_inject() {
  mkdir -p "${qq_base_path}/resources/app"
  cat > "${qq_base_path}/resources/app/loadNapCat.js" <<EOF
(async () => {await import('file:///${target_folder}/napcat/napcat.mjs');})();
EOF
}

ensure_napcat_installed() {
  if detect_existing_napcat && [ "${FORCE_NAPCAT_UPDATE:-0}" != "1" ]; then
    log_info "skipping NapCat download (FORCE_NAPCAT_UPDATE!=1)"
    log_progress 70 "napcat already installed, ensuring inject"
    if [ ! -f "${qq_base_path}/resources/app/loadNapCat.js" ]; then
      log_info "creating loadNapCat.js inject script"
      write_load_napcat_inject
      patch_package_json_main "$qq_package_json_path"
    fi
    return
  fi

  if [ -z "$napcat_archive_path" ]; then
    log_error "napcat_archive_path is not set"
    exit 37
  fi

  log_progress 25 "downloading napcat shell"
  if [ ! -f "$napcat_archive_path" ] || [ "${FORCE_NAPCAT_UPDATE:-0}" = "1" ]; then
    log_info "downloading NapCat from: ${napcat_download_url}"
    download_file "$napcat_download_url" "$napcat_archive_path"
  else
    log_info "reuse cached NapCat package: ${napcat_archive_path}"
  fi

  verify_napcat_archive_sha512 "$napcat_archive_path"

  log_progress 60 "extracting napcat"
  rm -rf "$napcat_unpack_dir"
  mkdir -p "$napcat_unpack_dir"
  log_info "extract NapCat shell package"
  extract_zip_to "$napcat_archive_path" "$napcat_unpack_dir"

  backup_napcat_config
  mkdir -p "${target_folder}/napcat"
  cp -a "$napcat_unpack_dir/." "${target_folder}/napcat/"
  chmod -R +x "${target_folder}/napcat"
  restore_napcat_config

  log_progress 80 "injecting loadNapCat.js"
  write_load_napcat_inject
  patch_package_json_main "$qq_package_json_path"
}

handle_error() {
  local exit_code="$1"
  local line_no="$2"
  local failed_command="$3"
  local error_text="install_napcat failed at line ${line_no}: ${failed_command}"

  log_error "$error_text"
  write_status false "install_napcat_failed" null "$(escape_json_string "$error_text")"
  exit "$exit_code"
}

trap 'handle_error $? $LINENO "$BASH_COMMAND"' ERR

log_progress 0 "preparing workspace"
log_info "prepare workspace directories"
mkdir -p "$workspace_dir" "$runtime_dir" "$log_dir" "$tmp_dir" "$package_dir"
write_status false "install_napcat" null null

log_progress 5 "verifying linuxqq prerequisite"
ensure_linuxqq_present

ensure_command "cp" "required for file synchronization"

rm -rf "$staging_dir"
mkdir -p "$staging_dir"

ensure_napcat_installed

log_progress 100 "napcat install finished"
log_info "napcat install finished"
write_status false "install_napcat_done" null null
echo "[OK] napcat install finished"
