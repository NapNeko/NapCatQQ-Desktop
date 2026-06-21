set -e
if ! command -v apt-get >/dev/null 2>&1; then exit 0; fi
export DEBIAN_FRONTEND=noninteractive

# 等待其他 apt/dpkg 进程释放锁(最多等 2 分钟)
wait_count=0
max_wait=24  # 24 * 5s = 120s
while fuser /var/lib/dpkg/lock-frontend >/dev/null 2>&1 || fuser /var/lib/apt/lists/lock >/dev/null 2>&1; do
  if [ $wait_count -ge $max_wait ]; then
    echo "等待 dpkg/apt 锁超时,请检查是否有其他包管理器进程在运行" >&2
    exit 1
  fi
  echo "dpkg/apt 被占用,等待释放..." >&2
  sleep 5
  wait_count=$((wait_count + 1))
done

apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
