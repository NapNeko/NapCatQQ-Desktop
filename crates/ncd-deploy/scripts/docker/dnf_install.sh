set -e
if command -v apt-get >/dev/null 2>&1; then exit 0; fi

# 优先使用 dnf5(Fedora 41+),回退到 dnf(Fedora 22+),最后 yum
DNF_CMD=""
if command -v dnf5 >/dev/null 2>&1; then
  DNF_CMD="dnf5"
elif command -v dnf >/dev/null 2>&1; then
  DNF_CMD="dnf"
else
  exit 0  # 交给 yum_install 阶段处理
fi

ALI="https://mirrors.aliyun.com/docker-ce"
$DNF_CMD install -y dnf-plugins-core
$DNF_CMD config-manager --add-repo "$ALI/linux/centos/docker-ce.repo"
sed -i "s#download.docker.com#mirrors.aliyun.com/docker-ce#g" /etc/yum.repos.d/docker-ce.repo
$DNF_CMD install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
