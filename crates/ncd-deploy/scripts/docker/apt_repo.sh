set -e
if ! command -v apt-get >/dev/null 2>&1; then exit 0; fi
ALI="https://mirrors.aliyun.com/docker-ce"
install -m 0755 -d /etc/apt/keyrings

# 发行版检测:优先识别 ubuntu/debian,派生发行版(Mint/Pop!_OS/Raspbian)回退到上游
. /etc/os-release
DISTRO="$ID"
case "$DISTRO" in
  ubuntu|debian) ;;
  raspbian) DISTRO=debian ;;
  linuxmint|pop|neon) DISTRO=ubuntu ;;
  *)
    # 检查 ID_LIKE 字段识别基于 debian/ubuntu 的发行版
    case "$ID_LIKE" in
      *debian*) DISTRO=debian ;;
      *ubuntu*) DISTRO=ubuntu ;;
      *) DISTRO=ubuntu ;;  # 最终兜底
    esac
    ;;
esac

curl -fsSL "$ALI/linux/$DISTRO/gpg" | gpg --batch --yes --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg

# Codename 获取:优先用 VERSION_CODENAME,Debian 老版本兜底用 lsb_release
CODENAME="${VERSION_CODENAME:-}"
if [ -z "$CODENAME" ] && command -v lsb_release >/dev/null 2>&1; then
  CODENAME="$(lsb_release --codename --short 2>/dev/null || true)"
fi
if [ -z "$CODENAME" ]; then
  echo "无法获取发行版代号,请检查 /etc/os-release 或安装 lsb-release" >&2
  exit 1
fi

ARCH="$(dpkg --print-architecture)"
echo "deb [arch=$ARCH signed-by=/etc/apt/keyrings/docker.gpg] $ALI/linux/$DISTRO $CODENAME stable" > /etc/apt/sources.list.d/docker.list
