set -e
if command -v apt-get >/dev/null 2>&1; then exit 0; fi
if command -v dnf >/dev/null 2>&1; then exit 0; fi
if command -v yum >/dev/null 2>&1; then exit 0; fi
echo "未识别到 apt-get / dnf / yum 包管理器,无法自动安装 Docker" >&2
exit 1
