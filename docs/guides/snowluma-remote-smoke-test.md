---
title: SnowLuma 远端部署 Smoke 测试指南
status: draft
plan: docs/plans/2026-05-11-snowluma-remote-management-execution-plan.md
audience: 内部开发 + 早期用户
---

# SnowLuma 远端部署 Smoke 测试指南 (W11)

> 这份文档配合 `docs/plans/2026-05-11-snowluma-remote-management-execution-plan.md`
> 的 W1-W10 落地交付,描述如何在 Desktop 端发起一次完整的 SnowLuma 远端部署 + 扫码登录闭环
> 以及验证关键路径不退化。

## 前置条件

### Desktop 端 (Windows)

- NapCatQQ-Desktop 已编译,**且** `src/resource/runtime/snowluma_framework_lite.tar.gz`
  与 sibling `snowluma_framework_lite.version.txt` 已就位。
  - 检查命令: `python -c "from src.core.remote.snowluma import find_bundled_lite_tarball, read_bundled_version; print(find_bundled_lite_tarball(), read_bundled_version())"`
  - 缺失时跑: `python script/build_scripts/build_snowluma_framework_lite.py --source <SnowLuma 仓库根> --out src/resource/runtime/snowluma_framework_lite.tar.gz`
- Desktop **必须**用源码态或 PyInstaller 打包态启动一次,确认 Qt 资源加载不报错
  (`pyside6-rcc src/resource/resource.qrc -o src/resource/resource.py` 必须已生成)

### 远端 VPS

- **Linux 发行版**: Debian 11+ / Ubuntu 20.04+ (其他 dpkg 系发行版理论可工作,
  RHEL 系 (CentOS / Fedora / Rocky) **当前不支持** — `install_snowluma.sh` 内只走 `apt-get`)
- **架构**: amd64 或 arm64
- **磁盘**: 至少 2GB 可用 (LinuxQQ ~200MB + SL Framework ~50MB + 缓存 + 日志)
- **网络出方向**:
  - 默认: 能访问 `deb.nodesource.com` (OQ2 决策 L3 nodesource fallback)
  - air-gapped 部署: 手工 `apt install nodejs` 预装到 ≥ 22,然后跑 deploy 时把
    `enable_nodesource` 传 `False` (UI 暴露开关)
- **端口**: 远端**不**对外开放 `5099` / `5900` / `6081` 任何一个;
  Desktop 通过 SSH local forward 把它们暴露到本机 `127.0.0.1:47099` (WebUI) 与
  `127.0.0.1:47609` (noVNC) — 整体安全模型基于 SSH 隧道,远端服务全监听 loopback
- **SSH**: 用户已有可用 SSH key 或密码; 用户对家目录有写权限

## Smoke 步骤

### 1. 创建 SL 服务器档案

在 Desktop "服务器管理" 页面,点 "新增" 按钮,**选择 backend flavor = SnowLuma**:

| 字段                   | 期望值 / 示例                                         |
| ---------------------- | ----------------------------------------------------- |
| 服务器名称             | `sl-vps-01` (任意)                                    |
| SSH host               | 你的 VPS IP / 域名                                    |
| SSH user               | `root` 或 sudo 用户 (sudo 用户需配过免密 sudo,见 OQ2) |
| SSH 端口               | `22` (默认)                                           |
| 认证方式               | key (推荐) / password                                 |
| **backend_flavor**     | **SnowLuma** (**核心选项**,D8 决策一经设定不可更改)   |
| SL workspace           | `$HOME/snowluma-remote` (默认,可改)                   |
| SL WebUI 密码 override | 可留空 (走 App fallback)                              |

> ⚠️ **W7 决策**: 一旦档案 flavor 选定就不可改;切换需删除档案重建。

### 2. 部署

点 "部署" 按钮触发,Desktop 后台执行:

| 阶段                                   | 进度    | 期望日志                                                                                            |
| -------------------------------------- | ------- | --------------------------------------------------------------------------------------------------- |
| **Stage 0** preflight                  | 0-1%    | `[PREFLIGHT] distro=Ubuntu version=22.04 arch=amd64 family=debian installer=dpkg status=supported`  |
| **Stage 1** install_linuxqq            | 1-40%   | `[LinuxQQ] [PROGRESS] N <message>` 系列                                                             |
| **Stage 2** install_snowluma_framework | 40-90%  | `[SnowLuma] [PROGRESS] 60 解压 SnowLuma.Framework` `[SnowLuma] [PROGRESS] 80 生成 VNC / WebUI 密钥` |
| **Stage 3** 上传 launcher 脚本         | 90-95%  | `上传 launcher 脚本`                                                                                |
| **Stage 4** verify + 写回              | 95-100% | `校验远端文件` `部署完成`                                                                           |

### 预期结果

- 状态 → `DEPLOYED`
- 档案 `snowluma_framework_version` 字段写入 (例 `"0.1.0"`)
- 远端目录结构:
  ```text
  ~/snowluma-remote/
  └── workspace/
      ├── snowluma/
      │   ├── dist/index.mjs           (daemon 入口, OQ1 修订)
      │   └── packages/runtime/native/ (Linux x64/arm64 .node/.so)
      ├── runtime/
      ├── log/
      ├── opt/QQ/                       (LinuxQQ 安装位置, 与 NC 同结构)
      ├── vnc.secret                   (mode 600, 12 字节 hex)
      ├── webui.secret                 (mode 600, 16 字节 hex)
      ├── snowluma_daemon_launcher.sh
      └── snowluma_bot_launcher.sh
  ```

### 3. 启动 daemon + 扫码登录

> ⚠️ **W9/W10 部分集成**: 当前版本已实现 `RemoteSnowLumaDaemon` 核心 + `open_snowluma_vnc`;
> `BotCard` UI 接入仍在 W10b backlog。短期 smoke 通过 Python REPL 验证:

```python
# 在 Desktop 进程内 (或独立 python 调试)
from creart import it
from src.core.remote.server_manager import ServerManager
from src.core.remote.snowluma import RemoteSnowLumaDaemon, open_snowluma_vnc

manager = it(ServerManager)
profile = manager.list_servers()[0]  # 假设第一个就是 SL profile
sm_creds = manager._inject_runtime_password(profile)

from src.core.remote.ssh_client import SSHClient
ssh = SSHClient(sm_creds)
ssh.connect()

daemon = RemoteSnowLumaDaemon(ssh, profile.snowluma_paths)
info = daemon.ensure_running()
print("noVNC URL endpoint:", info.tunnels.novnc.local_url)

# 启动 Bot
from src.core.remote.snowluma import SnowLumaLauncherCommands
cmds = SnowLumaLauncherCommands(profile.snowluma_paths)
ssh.run(cmds.bot_start_cmd("114514"))  # qq_id 任意, 实际登录后 uin 才确定

# 打开扫码页
from src.core.remote.execution_backend import RemoteExecutionBackend
ok, msg = open_snowluma_vnc(
    RemoteExecutionBackend(ssh),
    profile.snowluma_paths,
    info.tunnels.novnc,
)
print(ok, msg)
```

预期:浏览器打开 `http://127.0.0.1:47609/vnc.html?...`,
显示远端 Xvfb 中运行的 QQ.exe 二维码扫码页面。用手机 QQ 扫码 → 登录成功 → uin 写入
远端 `status_bot_<qq_id>.json`。

### 4. 验证状态查询

```python
from src.core.remote.snowluma import SnowLumaRemoteRuntimeService
svc = SnowLumaRemoteRuntimeService(RemoteExecutionBackend(ssh), profile.snowluma_paths)
print("daemon:", svc.get_daemon_status().state)
print("bots:", [(b.qq_id, b.uin, b.state) for b in svc.list_bots()])
```

预期:
- daemon `state = READY`
- bots 列表含至少 1 个 `state = RUNNING` 的 Bot

### 5. 清理

```python
ssh.run(cmds.bot_stop_cmd("114514"))
daemon.release()
ssh.close()
```

## 已知 caveat

### OQ4 决策遗留: noVNC URL 含明文密码

- noVNC URL `?password=xxx` 会进浏览器历史 / 可能被 referer 头泄露
- **缓解**: 扫码完成后**立即关闭** noVNC tab,避免 URL 残留
- **永久解决**: backlog 项 — daemon 启动时生成 5min TTL 的 ephemeral token
- **Python 侧不泄漏 (P10 review)**: `open_snowluma_vnc()` 返回的 message 仅含
  `http://127.0.0.1:<port>` 脱敏端点,**不含明文密码**;含密码的 URL 仅经
  `webbrowser.open()` 送给系统浏览器,Python 调用栈不留密码副本。

### P6 (review) 决策遗留: VNC 密码有效熵 32 bit

- 远端 `vnc.secret` 通过 `openssl rand -hex 4` 生成 (8 hex 字符 = 32 bit)
- 不是越长越好: **RFB 标准协议用 DES 8 字节密钥**, 客户端/服务端只取密码前 8 字符;
  早先 `rand -hex 12` (24 hex 字符) 是误解 — 表面 96 bit 实际仍只 32 bit 有效熵
- 32 bit 是 RFB 标准认证的上限,**真正的传输安全来自 SSH 隧道** (Desktop 与 VPS 之间
  的所有 VNC 流量都封在 SSH 通道内, 远端 `5900/6081` 仅 loopback 监听)
- 若需要更强保护: backlog 项 — `x11vnc -ssl` + 客户端证书 (会让 noVNC 配置变复杂)

### OQ6 决策: crash 检测 2s 延迟

- 远端 daemon 崩溃后,Desktop 端 BotCard 状态机最多有 2s 滞后才更新
- 是 `SnowLumaTunnelManager` watchdog 心跳间隔
- 当前**不**主动改进 (用户人眼不可察觉)

### W10b 待续

以下 UI 改造仍需手动接入 (代码已就位,UI 调用层 backlog):

- **AddServerDialog**: 加 `backend_flavor` 单选 (NAPCAT / SnowLuma) 与 SL 字段组
  - 后端模型已支持: `ServerProfile.create(backend_flavor=BackendFlavor.SNOWLUMA, ...)`
- **BotCard**: SL 远端 Bot 卡片加 "扫码登录(远端)" 按钮,调
  `open_snowluma_vnc(...)` 并展示状态
  - icon 用 `qfluentwidgets.FluentIcon.SCAN`,按钮文案 `"扫码登录(远端)"` (OQ5 决策)
- **BotProcessManager**: 根据 `profile.backend_flavor` 分发 `LocalSnowLumaDriver` /
  `RemoteSnowLumaDriver` (新增类,使用 `RemoteSnowLumaDaemon` + launcher 命令);
  当前 `BotProcessManager` 只识别本地 driver

## 自动化测试

```powershell
$tmpbase = Join-Path $env:TEMP 'snowluma-smoke-test'
python -m pytest `
    script/test/test_snowluma_paths.py `
    script/test/test_snowluma_templates.py `
    script/test/test_build_snowluma_framework_lite.py `
    script/test/test_snowluma_bundled.py `
    script/test/test_snowluma_deployment.py `
    script/test/test_snowluma_status_launcher.py `
    script/test/test_snowluma_tunnels.py `
    script/test/test_server_profile_snowluma.py `
    script/test/test_server_manager_snowluma_deploy.py `
    script/test/test_snowluma_remote_daemon.py `
    script/test/test_snowluma_vnc_launcher.py `
    --basetemp="$tmpbase" -p no:cacheprovider -v
```

期望: **183 PASS** (W1-W10a 全部 SnowLuma 套件)

NC 回归验证:
```powershell
python -m pytest `
    script/test/test_local_port_forwarder.py `
    script/test/test_server_registry.py `
    script/test/test_server_manager_deploy.py `
    script/test/test_versioning_snowluma.py `
    --basetemp="$tmpbase" -p no:cacheprovider
```

期望: 全 PASS,无回归。

## 故障排查

| 现象                                   | 原因                                | 解决                                                                        |
| -------------------------------------- | ----------------------------------- | --------------------------------------------------------------------------- |
| `SnowLumaFrameworkNotBundledError`     | Desktop 没打包 lite tarball         | 跑 `build_snowluma_framework_lite.py` 重新构建                              |
| preflight `unsupported_distro`         | 远端不是 Ubuntu/Debian              | 当前不支持 (W11+ 加 RHEL 支持)                                              |
| `ERROR_NODE_VERSION_TOO_LOW`           | 远端 node < 22 且 nodesource 不可达 | 手工 `apt install nodejs` 后重试,或用 nvm 装 22+                            |
| 隧道建立失败 (端口 47099 / 47609 占用) | Desktop 上已有进程占用端口          | tunnel manager 自动回退随机端口,但 noVNC URL 也变;关闭占用进程后重启 daemon |
| noVNC 黑屏 / 401                       | VNC 密码读取失败                    | 检查远端 `~/snowluma-remote/workspace/vnc.secret` 是否存在 + 权限 600       |
