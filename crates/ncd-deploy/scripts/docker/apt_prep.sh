set -e
export DEBIAN_FRONTEND=noninteractive
if ! command -v apt-get >/dev/null 2>&1; then exit 0; fi

# apt-get update 在网络不稳定时可能失败,重试最多 3 次
retry_count=0
max_retries=3
until apt-get update || [ $retry_count -eq $max_retries ]; do
  retry_count=$((retry_count + 1))
  echo "apt-get update 失败,重试 $retry_count/$max_retries..." >&2
  sleep 2
done

if [ $retry_count -eq $max_retries ]; then
  echo "apt-get update 重试 $max_retries 次后仍失败" >&2
  exit 1
fi

apt-get install -y ca-certificates curl gnupg
