set -e
mkdir -p /etc/docker
if [ -f /etc/docker/daemon.json ] && [ ! -f /etc/docker/daemon.json.ncd_bak ]; then
  cp /etc/docker/daemon.json /etc/docker/daemon.json.ncd_bak
fi
cat > /etc/docker/daemon.json <<'EOF'
{
  "registry-mirrors": [
    "https://docker.1ms.run",
    "https://docker.m.daocloud.io"
  ]
}
EOF
if command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload 2>/dev/null || true
  systemctl restart docker 2>/dev/null || true
fi
