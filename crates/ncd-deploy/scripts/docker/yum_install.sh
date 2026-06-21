set -e
if command -v apt-get >/dev/null 2>&1; then exit 0; fi
if command -v dnf >/dev/null 2>&1; then exit 0; fi
if ! command -v yum >/dev/null 2>&1; then exit 0; fi
ALI="https://mirrors.aliyun.com/docker-ce"
yum install -y yum-utils
yum-config-manager --add-repo "$ALI/linux/centos/docker-ce.repo"
sed -i "s#download.docker.com#mirrors.aliyun.com/docker-ce#g" /etc/yum.repos.d/docker-ce.repo
yum install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
