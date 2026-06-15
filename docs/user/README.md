# NapCatQQ Desktop 用户文档

## 快速开始

NapCatQQ Desktop 是 NapCat 和 SnowLuma 的桌面管理工具，支持本地和远程部署。

### 系统要求

#### Windows 本地
- Windows 10/11
- 8GB+ 内存

#### Linux 远端（推荐用于远程部署）
- Ubuntu 20.04+ / Debian 11+
- 2GB+ 内存
- 必需软件包（见下方）

---

## Linux 远端依赖安装

远端 Linux 服务器需要安装以下依赖才能运行 QQ 和 SnowLuma：

### 必需依赖

```bash
sudo apt-get update
sudo apt-get install -y \
  libatspi2.0-0 \
  libgtk-3-0 \
  libasound2 \
  libgbm1 \
  libnss3 \
  libnotify4 \
  libsecret-1-0 \
  libxss1 \
  libxtst6 \
  libxkbfile1 \
  xvfb \
  x11vnc \
  openbox \
  dbus-user-session \
  fonts-wqy-zenhei
```

### 可选依赖（推荐）

```bash
sudo apt-get install -y \
  curl \
  wget \
  ffmpeg \
  git
```

---

## 常见问题

### 远端 Bot 启动失败

**现象**: Desktop 显示"启动失败"或"启动后立即退出"

**排查步骤**:

1. **检查依赖是否安装**
   ```bash
   ldd ~/Napcat/opt/QQ/qq | grep "not found"
   ```
   如果有 `not found`，说明缺少依赖，请安装上述必需依赖。

2. **检查 Node.js 版本**
   ```bash
   ~/snowluma-remote/workspace/node/bin/node --version
   ```
   应该显示 `v22.12.0` 或更高版本。

3. **查看启动日志**
   ```bash
   # SnowLuma daemon 日志
   tail -50 ~/snowluma-remote/workspace/log/daemon.log
   
   # Bot QQ 进程日志
   tail -50 ~/snowluma-remote/workspace/log/bot_<你的QQ号>.log
   ```

4. **手动测试启动**
   ```bash
   # 测试 SnowLuma WebUI
   cd ~/snowluma-remote/workspace/snowluma
   DISPLAY=:0 ~/snowluma-remote/workspace/node/bin/node --experimental-sqlite index.mjs
   
   # 如果成功，按 Ctrl+C 退出，然后在 Desktop 中重新启动
   ```

---

## 更多帮助

- GitHub Issues: https://github.com/NapNeko/NapCatQQ-Desktop/issues
- 文档问题：提交 Issue 或 PR
