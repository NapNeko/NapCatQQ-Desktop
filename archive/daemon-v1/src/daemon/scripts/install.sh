#!/bin/bash
# NapCat Daemon 一键安装脚本
# 用途：自动在远程 Linux 服务器上部署 NapCat Daemon
# 使用方法：curl -fsSL https://.../install.sh | bash

set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 日志函数
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# 配置变量
DAEMON_NAME="napcat-daemon"
DAEMON_VERSION="${DAEMON_VERSION:-1.0.0}"
INSTALL_DIR="${INSTALL_DIR:-/opt/napcat-daemon}"
CONFIG_DIR="${CONFIG_DIR:-/etc/napcat-daemon}"
DATA_DIR="${DATA_DIR:-/var/lib/napcat-daemon}"
LOG_DIR="${LOG_DIR:-/var/log/napcat-daemon}"
SERVICE_USER="${SERVICE_USER:-napcat}"
SERVICE_GROUP="${SERVICE_GROUP:-napcat}"

# 架构检测
detect_arch() {
    local arch=$(uname -m)
    case $arch in
        x86_64|amd64) echo "amd64" ;;
        aarch64|arm64) echo "arm64" ;;
        armv7l|armhf) echo "arm" ;;
        *)
            log_error "不支持的架构: $arch"
            exit 1
            ;;
    esac
}

# 系统检测
detect_os() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        echo $ID
    else
        echo "unknown"
    fi
}

# 检查依赖
check_dependencies() {
    log_info "检查系统依赖..."

    local deps="curl wget systemctl"
    local missing=""

    for dep in $deps; do
        if ! command -v $dep &> /dev/null; then
            missing="$missing $dep"
        fi
    done

    if [ -n "$missing" ]; then
        log_error "缺少必要依赖:$missing"
        log_info "请先安装依赖:"
        log_info "  Debian/Ubuntu: apt-get update && apt-get install -y curl wget systemd"
        log_info "  RHEL/CentOS:   yum install -y curl wget systemd"
        exit 1
    fi

    log_success "依赖检查通过"
}

# 创建用户和组
create_user() {
    log_info "创建服务用户..."

    if ! getent group $SERVICE_GROUP &> /dev/null; then
        groupadd --system $SERVICE_GROUP
        log_success "创建组: $SERVICE_GROUP"
    fi

    if ! id $SERVICE_USER &> /dev/null; then
        useradd --system \
            --gid $SERVICE_GROUP \
            --home-dir $DATA_DIR \
            --shell /usr/sbin/nologin \
            --comment "NapCat Daemon Service" \
            $SERVICE_USER
        log_success "创建用户: $SERVICE_USER"
    fi
}

# 创建目录结构
create_directories() {
    log_info "创建目录结构..."

    mkdir -p $INSTALL_DIR/bin
    mkdir -p $CONFIG_DIR
    mkdir -p $DATA_DIR
    mkdir -p $LOG_DIR

    # 设置权限
    chown -R root:root $INSTALL_DIR
    chown $SERVICE_USER:$SERVICE_GROUP $DATA_DIR
    chown $SERVICE_USER:$SERVICE_GROUP $LOG_DIR
    chmod 755 $INSTALL_DIR
    chmod 750 $DATA_DIR
    chmod 750 $LOG_DIR
    chmod 700 $CONFIG_DIR

    log_success "目录创建完成"
}

# 下载并安装二进制文件
install_binary() {
    log_info "下载 NapCat Daemon..."

    local arch=$(detect_arch)
    local download_url="${DOWNLOAD_URL:-https://github.com/yourorg/napcat-daemon/releases/download/v${DAEMON_VERSION}/napcat-daemon-${DAEMON_VERSION}-linux-${arch}.tar.gz}"
    local temp_dir=$(mktemp -d)

    log_info "下载地址: $download_url"
    log_info "目标架构: $arch"

    # 下载
    if ! curl -fsSL -o "$temp_dir/napcat-daemon.tar.gz" "$download_url"; then
        # 如果下载失败，尝试本地构建（开发环境）
        if [ -f "/tmp/napcat-daemon" ]; then
            log_warn "下载失败，使用本地二进制文件"
            cp /tmp/napcat-daemon $INSTALL_DIR/bin/
        else
            log_error "下载失败，且未找到本地二进制文件"
            rm -rf $temp_dir
            exit 1
        fi
    else
        # 解压
        tar -xzf "$temp_dir/napcat-daemon.tar.gz" -C "$temp_dir"

        # 安装二进制文件
        cp "$temp_dir/napcat-daemon" $INSTALL_DIR/bin/
        chmod +x $INSTALL_DIR/bin/napcat-daemon
    fi

    # 创建软链接
    ln -sf $INSTALL_DIR/bin/napcat-daemon /usr/local/bin/napcat-daemon

    rm -rf $temp_dir
    log_success "二进制文件安装完成"
}

# 生成配置文件
generate_config() {
    log_info "生成配置文件..."

    local config_file="$CONFIG_DIR/config.yaml"

    # 生成随机 Token
    local token=$(openssl rand -hex 32 2>/dev/null || cat /dev/urandom | tr -dc 'a-zA-Z0-9' | fold -w 64 | head -n 1)

    cat > $config_file << EOF
# NapCat Daemon 配置文件
# 自动生成于 $(date -Iseconds)

server:
  bind: "0.0.0.0:8443"
  tls:
    cert: "$CONFIG_DIR/server.crt"
    key: "$CONFIG_DIR/server.key"

auth:
  token: "$token"

napcat:
  workspace: "$DATA_DIR"
  auto_start: false

logging:
  level: "info"
  file: "$LOG_DIR/daemon.log"
  audit_file: "$LOG_DIR/audit.log"
EOF

    chmod 600 $config_file
    chown root:root $config_file

    log_success "配置文件生成完成"
    log_warn "请保存以下 Token，用于客户端连接:"
    echo ""
    echo "  $token"
    echo ""
    echo "Token 也保存在: $config_file"
}

# 生成自签名证书
generate_certificate() {
    log_info "生成 TLS 证书..."

    local cert_file="$CONFIG_DIR/server.crt"
    local key_file="$CONFIG_DIR/server.key"

    if [ -f "$cert_file" ] && [ -f "$key_file" ]; then
        log_warn "证书已存在，跳过生成"
        return
    fi

    openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
        -keyout "$key_file" \
        -out "$cert_file" \
        -subj "/CN=napcat-daemon/O=NapCat/C=CN" \
        -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

    chmod 600 "$key_file"
    chmod 644 "$cert_file"
    chown root:root "$key_file" "$cert_file"

    log_success "TLS 证书生成完成"
}

# 创建 systemd 服务
create_systemd_service() {
    log_info "创建 systemd 服务..."

    local service_file="/etc/systemd/system/${DAEMON_NAME}.service"

    cat > $service_file << 'EOF'
[Unit]
Description=NapCat Daemon - Remote management service for NapCat
Documentation=https://github.com/yourorg/napcat-daemon
After=network.target
Wants=network.target

[Service]
Type=simple
User=napcat
Group=napcat
ExecStart=/opt/napcat-daemon/bin/napcat-daemon -config /etc/napcat-daemon/config.yaml
ExecReload=/bin/kill -HUP $MAINPID
Restart=always
RestartSec=5
StandardOutput=append:/var/log/napcat-daemon/daemon.log
StandardError=append:/var/log/napcat-daemon/daemon.log

# 安全加固
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/napcat-daemon /var/log/napcat-daemon
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
AmbientCapabilities=CAP_NET_BIND_SERVICE

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    log_success "systemd 服务创建完成"
}

# 防火墙配置
configure_firewall() {
    log_info "配置防火墙..."

    local port=8443

    # UFW (Ubuntu)
    if command -v ufw &> /dev/null; then
        ufw allow $port/tcp comment 'NapCat Daemon'
        log_success "UFW 防火墙规则已添加"
    fi

    # firewalld (RHEL/CentOS)
    if command -v firewall-cmd &> /dev/null; then
        firewall-cmd --permanent --add-port=$port/tcp
        firewall-cmd --reload
        log_success "firewalld 规则已添加"
    fi

    # iptables (通用)
    if command -v iptables &> /dev/null; then
        if ! iptables -C INPUT -p tcp --dport $port -j ACCEPT 2>/dev/null; then
            iptables -I INPUT -p tcp --dport $port -j ACCEPT
            log_success "iptables 规则已添加"
        fi
    fi
}

# 启动服务
start_service() {
    log_info "启动 NapCat Daemon 服务..."

    systemctl enable $DAEMON_NAME
    systemctl start $DAEMON_NAME

    # 等待服务启动
    sleep 2

    if systemctl is-active --quiet $DAEMON_NAME; then
        log_success "服务启动成功"
        systemctl status $DAEMON_NAME --no-pager
    else
        log_error "服务启动失败"
        journalctl -u $DAEMON_NAME --no-pager -n 20
        exit 1
    fi
}

# 显示安装信息
show_completion_info() {
    local token=$(grep "token:" $CONFIG_DIR/config.yaml | awk '{print $2}')
    local ip=$(hostname -I | awk '{print $1}')

    echo ""
    echo "========================================"
    echo "  NapCat Daemon 安装完成"
    echo "========================================"
    echo ""
    echo "  服务状态: $(systemctl is-active $DAEMON_NAME)"
    echo "  监听地址: wss://$ip:8443/ws"
    echo ""
    echo "  连接 Token:"
    echo "  $token"
    echo ""
    echo "  配置文件: $CONFIG_DIR/config.yaml"
    echo "  日志文件: $LOG_DIR/daemon.log"
    echo "  审计日志: $LOG_DIR/audit.log"
    echo ""
    echo "  管理命令:"
    echo "    查看状态: systemctl status $DAEMON_NAME"
    echo "    重启服务: systemctl restart $DAEMON_NAME"
    echo "    查看日志: journalctl -u $DAEMON_NAME -f"
    echo ""
    echo "========================================"
    echo ""
    echo "请保存好连接 Token，客户端连接时需要使用。"
    echo ""
}

# 清理函数
cleanup() {
    if [ -n "${TEMP_DIR:-}" ] && [ -d "$TEMP_DIR" ]; then
        rm -rf $TEMP_DIR
    fi
}

trap cleanup EXIT

# 主函数
main() {
    echo "========================================"
    echo "  NapCat Daemon 安装程序"
    echo "  版本: $DAEMON_VERSION"
    echo "========================================"
    echo ""

    # 检查 root 权限
    if [ "$EUID" -ne 0 ]; then
        log_error "请使用 root 权限运行此脚本"
        log_info "尝试使用: sudo $0"
        exit 1
    fi

    # 执行安装步骤
    check_dependencies
    create_user
    create_directories
    install_binary
    generate_config
    generate_certificate
    create_systemd_service
    configure_firewall
    start_service

    # 显示完成信息
    show_completion_info
}

# 处理命令行参数
case "${1:-}" in
    --uninstall)
        log_info "卸载 NapCat Daemon..."
        systemctl stop $DAEMON_NAME 2>/dev/null || true
        systemctl disable $DAEMON_NAME 2>/dev/null || true
        rm -f /etc/systemd/system/${DAEMON_NAME}.service
        systemctl daemon-reload
        rm -rf $INSTALL_DIR
        rm -rf $CONFIG_DIR
        rm -f /usr/local/bin/napcat-daemon
        log_success "卸载完成"
        log_warn "数据目录 $DATA_DIR 和日志目录 $LOG_DIR 未删除，如需清理请手动删除"
        exit 0
        ;;
    --version|-v)
        echo "NapCat Daemon Installer v$DAEMON_VERSION"
        exit 0
        ;;
    --help|-h)
        echo "用法: $0 [选项]"
        echo ""
        echo "选项:"
        echo "  --uninstall    卸载 NapCat Daemon"
        echo "  --version      显示版本信息"
        echo "  --help         显示此帮助信息"
        echo ""
        echo "环境变量:"
        echo "  DAEMON_VERSION    指定安装的版本号"
        echo "  DOWNLOAD_URL      指定自定义下载地址"
        echo "  INSTALL_DIR       安装目录 (默认: /opt/napcat-daemon)"
        echo "  SERVICE_USER      服务用户名 (默认: napcat)"
        exit 0
        ;;
    *)
        main
        ;;
esac
