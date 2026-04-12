# NapCat Daemon 快速部署指南

## 一句话描述

**输入 SSH 凭据 → 点击部署 → 自动完成所有配置 → 连接使用**

## 使用方式

### 1. 通过 Desktop UI（推荐）

```
┌──────────────────────────────────────────────────────┐
│  快速部署 NapCat Daemon                               │
├──────────────────────────────────────────────────────┤
│                                                      │
│  SSH 连接信息:                                       │
│  ┌──────────────────────────────────────────────┐   │
│  │ 主机:    [192.168.1.100                    ] │   │
│  │ 端口:    [22                                ] │   │
│  │ 用户名:  [root                             ] │   │
│  │ 密码:    [••••••••                         ] │   │
│  │ 或私钥:  [~/.ssh/id_rsa                    ] │   │
│  └──────────────────────────────────────────────┘   │
│                                                      │
│  [x] 使用默认端口 (8443)                             │
│  [ ] 自动启动服务                                    │
│                                                      │
│  [  开始一键部署  ]                                  │
│                                                      │
│  进度: ████████░░░░░░░░  40% - 上传安装脚本...      │
│                                                      │
└──────────────────────────────────────────────────────┘
```

部署完成后：
- 自动获取 Token
- 自动保存配置
- 自动连接到 Daemon
- 显示连接状态

### 2. 通过 Python API

```python
from src.desktop.core.remote import DaemonDeployer, SSHCredentials

# 准备 SSH 凭据
credentials = SSHCredentials(
    host="your-server.com",
    port=22,
    username="root",
    auth_method="password",  # 或 "key"
    password="your-password",
    # private_key_path="~/.ssh/id_rsa",
)

# 一键部署
with DaemonDeployer() as deployer:
    result = deployer.deploy(
        credentials=credentials,
        port=8443,
        progress_callback=lambda msg, pct: print(f"{pct}% - {msg}")
    )

    if result.success:
        print(f"部署成功!")
        print(f"连接地址: {result.host}:{result.port}")
        print(f"Token: {result.token}")
        # Token 已自动保存到系统密钥库
    else:
        print(f"部署失败: {result.error}")
        for log in result.logs:
            print(log)
```

### 3. 通过命令行（服务器端）

如果需要在服务器上手动安装：

```bash
# 一键安装
curl -fsSL https://your-domain.com/install.sh | sudo bash

# 或离线安装（已下载 install.sh）
sudo bash install.sh

# 安装完成后，获取 Token
cat /etc/napcat-daemon/config.yaml | grep token

# 查看状态
systemctl status napcat-daemon
```

## 自动部署流程

```
用户输入 SSH 凭据
        │
        ▼
┌─────────────────┐
│ 1. SSH 连接测试  │
│    (10%)        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ 2. 环境检测      │
│    - OS/Arch    │
│    - Systemd    │
│    - 现有安装   │
│    (25%)        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ 3. 上传安装脚本  │
│    (40%)        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ 4. 执行安装      │
│    - 创建用户   │
│    - 安装二进制 │
│    - 生成证书   │
│    - 配置服务   │
│    (70%)        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ 5. 获取 Token    │
│    (85%)        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ 6. 验证服务      │
│    (95%)        │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ 7. 保存配置      │
│    - Token → Keyring
│    - 连接配置 → JSON
│    (100%)       │
└────────┬────────┘
         │
         ▼
    部署完成！
```

## 配置管理

### Token 安全存储

Token 使用系统密钥库存储：

- **Windows**: Windows Credential Locker
- **macOS**: Keychain
- **Linux**: Secret Service API (GNOME Keyring/KWallet)

```python
from src.desktop.core.remote import DaemonConfigManager

# 获取配置管理器
manager = DaemonConfigManager()

# 列出所有配置
for conn_id, conn in manager.list_connections():
    print(f"{conn_id}: {conn.display_name}")

# 获取 Token（自动从 keyring 读取）
token = manager.get_token(conn_id)

# 测试连接
success, msg = manager.test_connection(conn_id)
```

### 配置存储位置

```
Windows: %APPDATA%/NapCatQQ/daemon_connections.json
macOS:   ~/Library/Application Support/NapCatQQ/daemon_connections.json
Linux:   ~/.config/NapCatQQ/daemon_connections.json
```

## 服务器要求

### 最低配置

- CPU: 1 核
- 内存: 256 MB
- 磁盘: 100 MB
- 系统: Linux (systemd)

### 支持系统

| 系统 | 版本 | 状态 |
|------|------|------|
| Ubuntu | 20.04+ | ✅ 完全支持 |
| Debian | 10+ | ✅ 完全支持 |
| CentOS | 7+ | ✅ 完全支持 |
| RHEL | 8+ | ✅ 完全支持 |
| Alpine | 3.14+ | ⚠️ 需要手动配置 |

### 依赖

自动安装的依赖：
- systemd
- openssl
- curl 或 wget

## 网络要求

### 端口

| 端口 | 用途 | 配置 |
|------|------|------|
| 8443 | Daemon WebSocket (wss://) | 可配置 |
| 22 | SSH 部署 | 标准 |

### 防火墙

自动配置的防火墙：
- UFW (Ubuntu)
- firewalld (RHEL/CentOS)
- iptables (通用)

## 故障排除

### 部署失败

**问题**: SSH 连接失败
```
解决: 检查用户名/密码，确认 22 端口开放
```

**问题**: 无法获取 Token
```
解决: 手动查看 /etc/napcat-daemon/config.yaml
```

**问题**: 服务启动失败
```
解决: 查看日志 journalctl -u napcat-daemon -n 50
```

### 连接失败

**问题**: 连接超时
```
检查: telnet server-ip 8443
解决: 检查防火墙，确认端口开放
```

**问题**: TLS 证书错误
```
解决: 第一次连接需要信任自签名证书
```

**问题**: Token 无效
```
解决: 重新从服务器获取 Token，或重新部署
```

## 卸载

### 通过 Desktop

```python
from src.desktop.core.remote import DaemonDeployer, SSHCredentials

deployer = DaemonDeployer()
deployer.uninstall(credentials)
```

### 通过 SSH

```bash
# 服务器上执行
sudo bash /opt/napcat-daemon/scripts/install.sh --uninstall

# 或手动
sudo systemctl stop napcat-daemon
sudo systemctl disable napcat-daemon
sudo rm -rf /opt/napcat-daemon /etc/napcat-daemon
sudo userdel napcat
```

## 更新

### 自动更新

Desktop 会检测 Daemon 版本，提示更新：

```
检测到新版本: 1.0.0 -> 1.1.0
[ 立即更新 ] [ 稍后提醒 ]
```

### 手动更新

```python
# 使用相同的 SSH 凭据重新部署
result = deployer.deploy(credentials)
# 会自动备份配置、更新二进制、恢复配置
```

## 高级配置

### 自定义端口

```python
result = deployer.deploy(
    credentials=credentials,
    port=9443,  # 自定义端口
)
```

### 使用已有证书

```bash
# 部署前上传证书
scp server.crt server.key root@server:/tmp/

# 部署后替换
ssh root@server "cp /tmp/server.* /etc/napcat-daemon/ && systemctl restart napcat-daemon"
```

### 多服务器管理

```python
servers = [
    ("prod-1", SSHCredentials(host="10.0.1.10", ...)),
    ("prod-2", SSHCredentials(host="10.0.1.11", ...)),
    ("prod-3", SSHCredentials(host="10.0.1.12", ...)),
]

for name, creds in servers:
    result = deployer.deploy(credentials=creds)
    if result.success:
        print(f"{name}: 部署成功")
```
