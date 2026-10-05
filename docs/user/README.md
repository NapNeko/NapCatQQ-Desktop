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

## 备份与恢复配置

打开「设置 → 数据」，点击「导出 ZIP」保存当前已保存的配置。新的配置包覆盖：

| 内容 | 恢复范围 |
| --- | --- |
| 应用设置与 Bot 配置 | 外观、功能开关、运行与通知设置、Bot 连接配置 |
| 远端服务器 | 服务器档案与连接参数 |
| 应用实例 | 实例档案、Bot 关联、领养时保存的回滚快照 |
| 框架配置 | Karin、NoneBot2、AstrBot、MaiBot、Koishi、云崽、NeoBot 的核心设置、模型与插件配置；麦麦自定义提示词及版本 |
| API 调试台 | 工作区、标签页草稿、布局、收藏与文件夹 |
| SnowLuma | 全局 WebUI 端口与密码覆盖设置 |
| 界面与终端 | 终端设置、常用命令、布局、Bot 手动排序、麦麦聊天昵称 |

导入时选择配置 ZIP，或拖入配置文件夹。导入向导先校验来源并列出将恢复的项目，确认后备份目标现有配置，再一次写入；校验失败不会覆盖配置，写入失败会回滚。缺少的项目保留目标原配置。旧版 ZIP 和扁平配置文件夹继续支持；新包使用 v2 格式，并保留旧版四个文件的名称。

导入完成后重启应用，使启动相关配置生效。如果配置文件已导入，但界面存储写入失败，数据页会显示「重试恢复界面偏好」，可单独重试。原 ZIP 可保留作为后续恢复来源。

框架配置按原文件保存，保留注释、格式和自定义字段。本机与远端实例共用同一恢复流程：先确认目标目录属于对应框架且实例已经停止，再恢复配置。实例尚未安装、正在运行或远端无法连接时，配置保存在数据目录的待恢复副本中，数据页会显示「重试恢复框架配置」。完成安装、停止实例或恢复连接后即可重试；待恢复副本也会随下一次导出保存，有待恢复配置的实例会提示先恢复再启动。远端恢复还会核对 SSH 连接的地址、端口和用户名；服务器档案改变后，需要在远端页重新测试连接，避免写入旧主机。框架文件写入或 Desktop 配置提交失败时，已写入的框架文件会一起回滚；若远端连接中断导致回滚失败，会明确报告需要检查的文件。

系统密钥库、SSH 私钥、组件及应用程序、插件程序、数据库、缓存、日志、API 调用历史和聊天消息记录不随配置包恢复。配置原文里已有的 OneBot token、模型 API Key、通知凭据或密码仍随对应文件保存，因此请妥善保管配置包。SSH 密码、GitHub Token 等系统密钥库中的凭据需另外配置。

应用实例档案保留原主机与安装路径；在另一台机器恢复时，需要独立安装框架与插件程序，并检查实例安装路径，再恢复待恢复的框架配置。插件配置的恢复不会安装插件。整份数据换盘请使用「数据根目录」旁的「迁移」。

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

### 在远端 WebUI 里改的网络配置被还原了

Desktop 每次启动远端 Bot 都会按自己保存的配置重写远端的 onebot 文件，所以在 NapCat / SnowLuma 自己的 WebUI 里改的连接，下次从 Desktop 启动时会被覆盖。

改完以后打开这个 Bot 的配置页，进「连接」，点底部的「从远端读取」。Desktop 会列出远端比这边多了、少了、改了哪些连接，确认后填进表单，再点保存。

### 远端掉线通知没发出来

见 [ncd-watch 远端值守](./ncd-watch.md)。

---

## 更多帮助

- GitHub Issues: https://github.com/NapNeko/NapCatQQ-Desktop/issues
- 文档问题：提交 Issue 或 PR
