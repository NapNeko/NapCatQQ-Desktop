---
requirement_id: 2026-05-11-snowluma-remote-management
status: frozen
owner: @NapNeko/NapCatQQ-Desktop
depends_on:
  - docs/requirements/2026-05-11-snowluma-daemon-refactor.md
  - docs/plans/2026-05-10-snowluma-backend-adapter-execution-plan.md
related_plans:
  - docs/plans/2026-05-11-snowluma-remote-management-execution-plan.md
---

# SnowLuma 远程管理 (原生 Linux 进程部署) · 需求冻结

> 让 Desktop 能像管理远端 NapCat 一样管理远端 SnowLuma, 复用 `src/core/remote/` 的 SSH / 部署 / 状态 / 隧道设施, 远端以**原生 Linux 进程 + Xvfb+noVNC 图形栈**形态落地, QQ 首次扫码 UX 走**系统默认浏览器打开 noVNC**.

---

## 0. 现状对照 (Background)

### 0.1 本地 SnowLuma 已到达的能力 (2026-05-11 daemon 重构后)

- `src/core/runtime/snowluma_daemon.py` — App 级单例, 管唯一的 `node.exe` + `SnowLumaWebUIClient` (`:5099`), 持久驻留到 `QApplication.aboutToQuit`
- `src/core/runtime/snowluma_driver.py` — per-Bot QQ.exe + inject `load_process(pid)`; COLD 模式 spawn QQ.exe, HOT 模式 attach 用户已开 QQ.exe
- `src/core/runtime/snowluma_status_poller.py` — 按 UIN 聚合, `/api/processes` + `/api/qq-list` 双源
- `src/core/runtime/snowluma_webui_client.py` — 8 个 HTTP API (见 §6.2 上游真相)
- `src/core/config/config_export.py` 已能渲染 `runtime.json` / `webui.json` / `onebot_<uin>.json`
- **缺的是什么**: 以上**全部在本地 Windows**跑, `bot_process_manager` 的 `is_remote` 分支仅对 NapCat dispatch, SnowLuma 没有任何远端路径

### 0.2 NC 远程管理已建成的复用资产 (代码证据)

部署/状态/通信侧 (`src/core/remote/`):

- `@src\core\remote\ssh_client.py` — paramiko 封装: `connect / exec_command / upload_file / download_file / close`, host_key_policy 可插, 连接超时/带宽友好化
- `@src\core\remote\execution_backend.py` — `ExecutionBackend` 抽象 + `LocalExecutionBackend` + `RemoteExecutionBackend(ssh_client)`
- `@src\core\remote\models.py:LinuxCorePaths` — 远端目录布局 (base_dir/workspace_dir/runtime_dir/log_dir/pid_file/status_file/napcat_dir/qq_dir)
- `@src\core\remote\deployment.py:LinuxCoreDeployment` — probe_environment / install_linuxqq / install_napcat / 导出并上传当前配置 / `[PROGRESS]` 协议
- `@src\core\remote\status.py:RemoteRuntimeService` — 读 pid file + status JSON + tail log, start/stop/restart 骨架, `get_status_for_bot(qq_id)` 多 Bot 感知
- `@src\core\remote\tunnel.py:LocalPortForwarder` — paramiko `transport.request_port_forward` 本地端口 → 远端端口 (NC WebUI 预览靠它)
- `@src\core\remote\host_key_policy.py` — SSH 首次指纹策略
- `@src\core\remote\friendly_errors.py` — paramiko 异常翻译
- `@src\core\remote\thread_pool.py:dispatch_remote_ssh / remote_ssh_pool` — 阻塞 SSH 调用统一并发闸门
- `@src\core\remote\servers.py:ServerProfile / ServerRegistry / DeploymentState` — 服务器档案 + 持久化 (`config/servers.json`)
- `@src\core\remote\server_manager.py:ServerManager` — Qt 信号桥 (add/remove/probe/deploy/start/stop/log)
- `@src\core\remote\templates.py` — 三段 shell 脚本模板系统 (`build_install_linuxqq_script` / `build_install_napcat_script` / `build_napcat_launcher_script`), 把 Python 变量注入为 shell 常量

UI 侧入口:

- `@src\ui\page\home_page\` — 首页有 "远程服务器" 卡片, 扫 `ServerRegistry`
- `@src\ui\page\remote_operation_page\` — 远端运维中心 (安装/部署/启动/日志)
- `@src\core\operation\remote_backend.py:RemoteBackend` — `OperationBackend` 在远端模式下的映射层

### 0.3 上游 SnowLuma.Docker.Framework 给出的远端架构蓝本 (证据链)

`example/SnowLuma.Docker.Framework-main/` 里的关键构件回答了"原生 Linux 跑 SL 需要什么":

- `Dockerfile` base = `node:22-bookworm-slim`; 装依赖清单 (**关键**): `dbus-x11 fluxbox xvfb x11vnc novnc websockify supervisor fonts-wqy-zenhei fonts-noto-cjk libnss3 libatk-bridge2.0-0 libgtk-3-0 libxkbfile1 libsecret-1-0 libasound2`
- 安装顺序: apt LinuxQQ `.deb` → 解压 SnowLuma.Framework tarball 到 `/app/snowluma`
- `start.sh` 进程树: `dbus-launch` → `Xvfb :0 -screen 0 1280x720x24` → `fluxbox` → `x11vnc -display :0 -forever -shared -passwd $VNC_PASSWD -rfbport 5900` → `websockify --web /usr/share/novnc 6081 localhost:5900` → `exec supervisord`
- `supervisord.conf`:
  - `[program:qq]` command=`/usr/bin/qq --no-sandbox` (依赖 DISPLAY=:0)
  - `[program:snowluma]` command=`node /app/snowluma/index.js`
  - 各自独立 stdout/err
- 端口清单: `5900` VNC / `6081` noVNC+websockify / `5099` SnowLuma WebUI / `3000` OneBot HTTP / `3001` OneBot WS
- 配置挂载: `/app/snowluma-data/config`, `/app/.config`, `/app/.local/share`
- 关键 env (docker-compose.yml): `UID / GID / VNC_PASSWD / WEBUI_PORT / WEBUI_PASSWORD`

### 0.4 实际问题

- **SL 的登录模型依赖图形栈**: SL WebUI **没有**二维码获取 API (验证: `packages/core/src/webui/server.ts` 17 个端点全扫, `grep -r qrcode packages/core/src` 0 命中). QQ.exe 自己弹扫码窗口, 必须有 X server 显示; 故远端必须装 Xvfb
- **SL 的 WebUI 是纯 IPC 控制面**: 管 hook inject / OneBot 配置, 不是登录窗; Desktop 通过 SSH 隧道复用现有 `SnowLumaWebUIClient` 是可行的
- **NC 的远端脚本无法直接给 SL 用**:
  - NC launcher 只起 `xvfb-run qq`, 缺 `fluxbox / x11vnc / noVNC / websockify / node / supervisord` 依赖
  - NC 不需要持久的 web 端口暴露, SL 需要 `:5099` + `:6081` 两个隧道
  - NC 安装只装 LinuxQQ + NapCat; SL 要加装 node 运行时 + noVNC 全家桶
- **Desktop 本地 daemon 模型需要远端同构映射**: 本地是 `SnowLumaDaemon(QProcess)`, 远端是 "SSH launcher 托管的 node 进程 + pid/status 协议"; driver 层需要多态

---

## 1. 目标 (Goal)

### 1.1 远端 = 原生 Linux 进程 + 完整图形栈 (用户已确认选项)

- **远端形态**: 裸 Linux (Debian/Ubuntu 或 RHEL 系, 与现 NC 远端目标矩阵一致), 不依赖 Docker
- **图形栈**: 装 `Xvfb + fluxbox + x11vnc + noVNC + websockify + dbus-x11 + 字体包`, 让 QQ.exe 首次登录能在虚拟屏显示扫码窗, 用户透过 noVNC 扫码
- **语言栈**: 装 `node >= 22` (apt/nodesource 二选一, 与 Docker base 版本对齐) + `linuxqq*.deb` + SnowLuma.Framework **lite tarball** (见 §2.5 资源策略)
- **进程管理**: 远端用**由 Desktop 生成的 launcher 脚本 + pid/status 文件协议** 接管, 不引入 supervisord / systemd-user (与现 NC 策略一致)
- **生命周期**: daemon 首次启动后**持久驻留** (对齐 SL 本地 daemon 重构的 DC-1 决策); Desktop 断开不 kill daemon

### 1.2 扫码 UX = 默认浏览器打开 noVNC

- 用户在 Desktop 点 "扫码登录" → Desktop 建 SSH 隧道 `localhost:<random>` → 远端 `:6081` → 调 `webbrowser.open("http://127.0.0.1:<random>/vnc.html?autoconnect=1&password=<vnc_passwd>")`
- Desktop **不嵌** QtWebEngine, 零新增 Qt 模块依赖
- `vnc_passwd` 在远端首次部署时随机生成并写 `$workspace_dir/vnc.secret`, Desktop 通过 `cat vnc.secret` 拉回本地 (保密边界 = 那台远端 + Desktop 机密)

### 1.3 Desktop 侧以最小增量扩 Driver / Daemon 多态

- 不另建一套 `RemoteSnowLumaDriver` 类; 而是在 `SnowLumaDaemon` + `SnowLumaDriver` 里按 `runtime_target` 分支 (与 NC 2026-05-08 分布式策略一致)
- 本地路径维持现有 QProcess / `SnowLumaWebUIClient("127.0.0.1",5099,...)`
- 远端路径: daemon 通过 SSH 调 launcher `snowluma_daemon_launcher.sh start` → 等 pid/status → 建 `:5099` 隧道 → 用 `SnowLumaWebUIClient("127.0.0.1", <local_port>, password)` 沿用原客户端
- QQ.exe 生命周期也移到远端: driver 通过 SSH 调 `snowluma_bot_launcher.sh start <qq_id>` (spawn xvfb-run qq --no-sandbox -q <qq_id>, 写 `status_<qq_id>.json`)

### 1.4 复用 `src/core/remote/` 骨架, 不另起并行模块

- 新增文件只放在 `src/core/remote/snowluma/` 子包, 共享 `SSHClient` / `ExecutionBackend` / `LinuxCorePaths` / `LocalPortForwarder` / `host_key_policy` / `thread_pool`
- `ServerProfile` 扩字段 (不新建 profile 类型) 承载 SL 部署态 + SL 版本信息
- `ServerManager` 扩方法而不拆类 (与 NC dispatch 同一个 Qt 桥)

---

## 2. 交付物 (Deliverable)

### 2.1 新建模块: `src/core/remote/snowluma/` 子包 (约 6 个文件, 合计 ~1400 行)

| 文件            | 行数预估 | 职责                                                                                                                                                                                                                         |
| --------------- | -------: | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `paths.py`      |      ~80 | `SnowLumaRemotePaths` 数据类 (base/workspace/data/config/log/run/pid_daemon/pid_bot_prefix/status_daemon/status_bot_prefix/webui_secret/vnc_secret)                                                                          |
| `deployment.py` |     ~600 | `SnowLumaRemoteDeployment` 类: `probe_environment / install_graphics_stack / install_linuxqq (复用) / install_node / install_snowluma_framework / deploy_launchers / sync_config_bundle`, 所有阶段走 `[PROGRESS] N msg` 协议 |
| `status.py`     |     ~250 | `SnowLumaRemoteRuntimeService` 类: `get_daemon_status / get_bot_status(qq_id) / list_bots / tail_daemon_log / tail_bot_log(qq_id) / write_*_status_payload`; JSON schema 与本地 `snowluma_daemon` 状态语义对齐               |
| `launcher.py`   |     ~180 | Launcher 命令构造器: `daemon_start_cmd / daemon_stop_cmd / bot_start_cmd(qq_id,uin=None) / bot_stop_cmd(qq_id)`, 调 远端 shell 脚本 + 解析退出码                                                                             |
| `tunnels.py`    |     ~140 | `SnowLumaTunnelManager` 类: 托管两条 `LocalPortForwarder` (WebUI 5099 + noVNC 6081), 引用计数 + 崩溃自恢复 + 端口选择 (优先固定 47099/47609, 占用时回退随机 high port)                                                       |
| `templates.py`  |     ~160 | 渲染远端 shell 脚本: `build_install_snowluma_script / build_snowluma_daemon_launcher / build_snowluma_bot_launcher`; 与 `src/core/remote/templates.py` 同一模式                                                              |

### 2.2 新增资源脚本: `src/resource/script/remote/snowluma/` (3 个 `.sh.j2` 模板)

| 模板                             | 对应 templates.py builder        | 关键内容                                                                                                                                                                                                                                                                                                                                                                                            |
| -------------------------------- | -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `install_snowluma.sh.j2`         | `build_install_snowluma_script`  | apt 装图形栈 + node (apt 或 nodesource) + websockify/noVNC; 解压 SnowLuma.Framework lite tarball 到 `{workspace_dir}/snowluma`; 生成 vnc.secret + webui.secret; `[PROGRESS] N` 协议                                                                                                                                                                                                                 |
| `snowluma_daemon_launcher.sh.j2` | `build_snowluma_daemon_launcher` | 子命令: `start / stop / status / restart / wait-ready`. start 动作: `dbus-launch --exit-with-session` 起后台 `Xvfb :$DISPLAY_NUM` → `fluxbox` → `x11vnc -rfbport 5900 -passwd "$(cat vnc.secret)"` → `websockify --web /usr/share/novnc 6081 localhost:5900` → `node {workspace}/snowluma/dist/index.mjs`; 每步 pid 写独立 .pid 文件; daemon 总 pid 写 pid_daemon; status 汇总成 status_daemon.json |
| `snowluma_bot_launcher.sh.j2`    | `build_snowluma_bot_launcher`    | 子命令: `start <qq_id> / stop <qq_id> / status <qq_id>`. start: `DISPLAY=:0 /usr/bin/qq --no-sandbox -q <qq_id> > bot_<qq_id>.log 2>&1 & echo $! > pid_bot_<qq_id>; 写 status_bot_<qq_id>.json {pid,qq_id,started_at,uin:null}`                                                                                                                                                                     |

`.qrc` 注册所有 3 个 `.sh.j2` 到 Qt 资源; `src/resource/resource.qrc` 的 `<qresource prefix="/script/remote">` 下增节点

### 2.3 扩展 `src/core/remote/servers.py:ServerProfile`

新增字段 (保持 NC 路径向后兼容):

```python
class BackendFlavor(StrEnum):
    NAPCAT = "napcat"          # 现默认, 装 NapCat, launcher 走 napcat_launcher.sh
    SNOWLUMA = "snowluma"       # 装 SnowLuma + 图形栈, launcher 走两份 SL 脚本

@dataclass
class ServerProfile:
    ...
    backend_flavor: BackendFlavor = BackendFlavor.NAPCAT  # 新增
    # 以下字段仅在 SNOWLUMA flavor 有意义
    snowluma_framework_version: str | None = None
    snowluma_daemon_pid: int | None = None
    snowluma_webui_tunnel_local_port: int | None = None  # 上次成功的 WebUI 本地隧道端口 (UI 可显示)
    snowluma_vnc_tunnel_local_port: int | None = None
```

- `ServerRegistry` 序列化增 2 个字段; `_migrate_legacy_profile` 旧文件默认 `NAPCAT` + 其余 None
- UI `AddServerDialog` 增下拉选 "后端种类" (napcat / snowluma); 改 flavor 后需重新 probe + deploy (不允许运行时热切)

### 2.4 扩展 `src/core/remote/server_manager.py:ServerManager`

新增方法 (保持现有 `deploy_server` / `probe_server` / `start_remote_runtime` 对 NC 不变):

| 新方法                                           | 签名                           | 说明                                                                                                                      |
| ------------------------------------------------ | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| `probe_snowluma(server_id)`                      | → `SnowLumaProbeReport` (异步) | 检测 SL 特有依赖: node / Xvfb / noVNC / websockify / fluxbox / dbus-x11 / 已装 SnowLuma.Framework 版本                    |
| `deploy_snowluma(server_id)`                     | 异步 + Qt 信号                 | 分 3 阶段: 装图形栈 → 装 LinuxQQ (复用) → 装 SnowLuma.Framework; 每阶段 `[PROGRESS]` 驱动 progress bar                    |
| `start_snowluma_daemon(server_id)`               | 异步                           | SSH 调 daemon_launcher start; 成功后建两条隧道                                                                            |
| `stop_snowluma_daemon(server_id)`                | 异步                           | SSH 调 daemon_launcher stop; 关两条隧道                                                                                   |
| `start_snowluma_bot(server_id, qq_id)`           | 异步                           | SSH 调 bot_launcher start <qq_id>; 成功后 Desktop 侧 `SnowLumaWebUIClient.load_process(pid)`                              |
| `stop_snowluma_bot(server_id, qq_id)`            | 异步                           | `SnowLumaWebUIClient.unload_process(pid)` → SSH 调 bot_launcher stop <qq_id>                                              |
| `open_snowluma_vnc(server_id)`                   | 同步, 返回 `None`              | 确保 daemon 在 + vnc 隧道在; `webbrowser.open(f"http://127.0.0.1:{local_port}/vnc.html?autoconnect=1&password={secret}")` |
| `tail_snowluma_daemon_log(server_id, lines=200)` | 异步                           | 包 `RemoteRuntimeService.tail_log`                                                                                        |

Qt 信号增:

- `snowluma_deploy_progress_signal(server_id: str, phase: str, percent: int, message: str)`
- `snowluma_daemon_state_signal(server_id: str, state: str)` — state ∈ `{stopped, starting, ready, crashed, stopping}`
- `snowluma_bot_state_signal(server_id: str, qq_id: str, state: str)` — 与本地 `snowluma_login_state_signal` payload 语义对齐

### 2.5 资源交付策略: SnowLuma.Framework **lite tarball**

- **问题**: SL Framework 不是小包; 完整 node_modules 上传 SFTP 慢
- **决策**: 采用 **lite tarball** (实际产物结构, OQ1 修订: 含**仓库根 `dist/**`** [vite 输出 core+webui 合并产物] + `packages/runtime/native/snowluma-linux-{x64,arm64}.{node,so}` + `packages/runtime/native/websocket-linux-{x64,arm64}.node` + `packages/runtime/native/ffmpeg/ffmpegAddon.linux.{x64,arm64}.node` + `packages/runtime/{launcher.sh,package.json}` + 根 `package.json` + `LICENSE`; **不含** `packages/sdk/**` / win32+darwin native / `**/src/**` / `node_modules/**` / `*.map`); Desktop 打包期内置 `src/resource/runtime/snowluma_framework_lite.tar.gz` (绝对路径通过 Qt resource 展开到临时目录再 SFTP 上传)
- **版本对齐**: lite tarball 版本号来自构建脚本读 `example/SnowLuma-main/package.json:version`; Desktop 启动时把版本号暴露为 `VersioningService.snowluma_framework_bundled_version`, 在 `ServerProfile.snowluma_framework_version` 与 bundled 版本不一致时 UI 显示"可升级"
- **首版范围**: 不做远端增量升级 (只做全量 replace); 下一期再考虑 rsync 式增量
- **SFTP 传输进度**: deploy 的"装 SnowLuma.Framework" 阶段走 paramiko `put` 的 callback, 映射为 `[PROGRESS] 30..60%`

### 2.6 配置同步: 远端 SnowLuma 配置三件套

远端需要的配置 (与本地 daemon 渲染语义一致):

- `{workspace}/snowluma/config/runtime.json` — daemon 全局, 字段 `webuiPort: 5099`
- `{workspace}/snowluma/config/webui.json` — daemon 全局, 字段 `password: <AppConfig.snowluma.webui_password_override 或自动生成>`
- `{workspace}/snowluma/config/onebot_<uin>.json` — per-Bot OneBot 配置, 由 `@src\core\config\config_export.py:render_onebot_json` 生成

渲染时机与路径:

- **daemon 启动前**: `SnowLumaRemoteDeployment.sync_daemon_configs()` 生成 runtime.json + webui.json 到本地 tmp, SFTP 覆盖远端
- **per-Bot 启动前**: `SnowLumaRemoteDeployment.sync_onebot_for(qq_id, uin)` 生成 onebot_<uin>.json, SFTP 覆盖远端 (uin 未知时先 start bot, UIN detected 后再二次同步 — 同本地一致)

### 2.7 Desktop 上层 driver / daemon 多态

改造 `src/core/runtime/snowluma_daemon.py` 与 `snowluma_driver.py` (daemon 重构产物):

- `SnowLumaDaemon` 增 `_target: RuntimeTarget = LOCAL` 字段, `ensure_running()` 内分支:
  - LOCAL 路径保持现 QProcess spawn
  - REMOTE 路径: 取 `ServerManager.start_snowluma_daemon(server_id)` future, 等 ready, 建立 WebUI 隧道, 构造 `SnowLumaWebUIClient("127.0.0.1", <tunnel_port>, password)` + login
- 关键约束: **每个远端 server_id 对应独立的 daemon 实例**; 不再是进程级全局单例, 改为 `dict[tuple[RuntimeTarget, server_id], SnowLumaDaemon]`; 仍用 `creart` 注册 registry
- `SnowLumaDriver.start_async(config, start_mode, attach_pid)`:
  - 查 `config.runtime_target`; LOCAL / remote_<id>
  - REMOTE: 不再 spawn QProcess, 改调 `ServerManager.start_snowluma_bot(server_id, qq_id)`; daemon 的 WebUIClient 共用
  - REMOTE 仅支持 COLD (HOT 在远端无意义: Desktop 不拥有远端 QQ 进程; 表单 §W6 禁用)
- `SnowLumaStatusPoller` 不变 (client 是同一种, 只是隧道背后是远端 node)

### 2.8 UI 侧

- `src/ui/page/bot_page/widget/card.py`:
  - `_start_bot_clicked` 的 SnowLuma 分支: 如果 `config.runtime_target != LOCAL` 且 SL daemon 远端状态 != READY → 先 UI 等 daemon 起 (进度条从 ServerManager 信号透出)
  - "WebUI 预览"按钮: 远端模式下 URL 改用 `snowluma_webui_tunnel_local_port`
  - **新按钮**: "扫码登录(远端)"仅在 SL 远端 + Bot 状态 = `waiting_for_qr_scan` 时启用, 点击走 `ServerManager.open_snowluma_vnc`
- `src/ui/page/remote_operation_page/` 新增 "SnowLuma" 子页 (原页是 NC 专属):
  - 顶部三卡片: 图形栈状态 / LinuxQQ 状态 / SnowLuma.Framework 版本
  - 中部: daemon 状态 + start/stop/restart 按钮
  - 日志 tab: daemon.log + 各 bot log 分栏
- `src/ui/page/add_server_page/` (或 dialog): 新增 "后端种类" 单选 (napcat / snowluma)
- 首页 (`home_page`) 远程服务器卡片: backend_flavor=snowluma 时图标/角标区分

### 2.9 `BotProcessManager` 的远端 dispatch

- `_snowluma_driver.start_bot(config)` 路径在 `bot_process_manager.py` 的 `is_remote` 分支里新增映射 (目前只走 NC `remote_runtime_service`); 新分支调 `_snowluma_driver.start_async_remote(config, server_id)` 或在 driver 内部完全按 `runtime_target` 分流 — 推荐后者 (interface 对称, dispatch 面小)
- `NotificationService` / `snowluma_login_state_signal` 语义不变 (上游使用方零感知)

### 2.10 `.gitignore` / 构建 filter 审计

- `config/servers.json` 新增的 SL 专属字段不含密钥 (密钥在远端); 仍可入 git
- `vnc.secret` / `webui.secret` / `*.pid` 都在远端 `$workspace_dir`, 不会进 Desktop 机器; Desktop 本地只 cache 在 `ServerProfile` 里的**隧道端口号**, 不 cache 密码
- `src/resource/runtime/snowluma_framework_lite.tar.gz` 需加入 `.gitignore` (由构建脚本打包前下载生成; 类似 `linuxqq_*.deb` 现有处理)
- PyInstaller spec: `snowluma_framework_lite.tar.gz` + 3 个 `.sh.j2` 模板加入 `datas` 收集

---

## 3. 非目标 (Non-Goals)

- **不支持 Docker 形态远端** (本期): 用户明确选项 B (原生 Linux 进程部署)
- **不支持 HOT 启动**远端 SnowLuma Bot (远端无用户态 QQ.exe 可 attach)
- **不实现**远端 SnowLuma.Framework 增量升级 (全量替换)
- **不引入** supervisord / systemd-user 在远端; 用 launcher 脚本 + pid 文件与 NC 对齐
- **不做** noVNC 嵌入 QtWebEngine (选项 1); 不做外部 VNC 客户端引导 (选项 3)
- **不支持**同一台远端服务器**同时**跑 NapCat + SnowLuma 两个 flavor (per-server backend_flavor 互斥; 想要并存需加第二个 ServerProfile 指向同一 SSH 但不同 workspace_dir — 用户自行创建)
- **不实现**远端 QQ 版本号探测 (复用 NC 侧 `installed_qq_version` 即可)
- **不改动** `SnowLumaWebUIClient` 的 8 个 API 签名 (daemon 重构约束延续)
- **不改动** `BotConfig.runtime_target` 字段结构 (已存在, 复用)

---

## 4. 验收标准 (Acceptance Criteria)

### 4.1 产品可观察行为

- **AC1 (部署一次成功)**: 对一台 Debian 12 amd64 裸机, 添加 ServerProfile(flavor=snowluma) → 点 "一键部署", 10 分钟内完成三阶段装包 (图形栈 / LinuxQQ / SnowLuma.Framework), 末尾 probe 全绿
- **AC2 (daemon 持久)**: 部署完点 "启动 daemon", 远端 `ps aux` 可见 Xvfb / fluxbox / x11vnc / websockify / node 5 个进程, `pgrep -F pid_daemon` 返回非空; Desktop 关闭后再开, daemon 仍在跑 (对齐本地 daemon 的持久模型)
- **AC3 (WebUI 隧道)**: daemon 起来后, Desktop `netstat -an` 可见 `127.0.0.1:47099` LISTEN; `curl http://127.0.0.1:47099/api/status` 返回 401 (未带 token) 或 200
- **AC4 (远端冷启动 Bot)**: 添加 1 个 BotConfig (runtime_target=server_X, backend=snowluma, start_mode=COLD), 点启动; Desktop 侧 BotCard 状态变 `starting → waiting_for_qr_scan`; 点 "扫码登录(远端)" → 系统默认浏览器打开 noVNC 页自动连通 → 用户看见 QQ 登录二维码 → 扫码 → 状态翻 `logged_in`; Desktop 收到 `snowluma_login_state_signal` 与本地模式一致
- **AC5 (远端多 Bot)**: 同服务器启动第 2 个 SL Bot (不同 UIN), daemon 不重启, 两条 BotCard 并存, `/api/qq-list` 返回两条
- **AC6 (隧道崩溃自愈)**: 手动 `ssh kill` 某条本地端口转发, Desktop 2s 内 `SnowLumaTunnelManager` 重建, client 无感知
- **AC7 (远端 crash 传播)**: 远端 `pkill -f 'node.*snowluma'`, Desktop 所有依附 Bot 在 10s 内翻 `NotRunning`, UI 弹 `notification_signal(error, "SnowLuma daemon 远端崩溃")`
- **AC8 (日志可观察)**: 日志 tab 能 tail daemon.log (最近 200 行), 切到 bot 子 tab 能 tail bot_<qq_id>.log
- **AC9 (配置回写)**: 在 Desktop 改 BotConfig.network.http_servers.port, 点"应用到远端", SFTP 覆盖 onebot_<uin>.json, `POST /api/config/:uin` 热重载成功 (reloaded=true)
- **AC10 (图形栈正常退出)**: 点 "停止 daemon", 远端 5 个辅助进程全部退出 (`pgrep xvfb` / `pgrep fluxbox` 等空), pid_daemon 文件被 launcher 清理

### 4.2 代码可观察约束

- **CC1**: `grep -r "QProcess" src/core/remote/snowluma/` 0 命中 (远端子包不持 Qt 本地进程)
- **CC2**: `SnowLumaDaemon` 按 `(RuntimeTarget, server_id)` 键多实例化, 本地键 = `(LOCAL, "")`
- **CC3**: `SnowLumaWebUIClient` 签名不变; 在远端模式下 host 恒为 `"127.0.0.1"`, port 来自 `SnowLumaTunnelManager.webui_local_port()`
- **CC4**: 所有阻塞 SSH 调用均走 `remote_ssh_pool` (grep `snowluma` 相关目录, 0 处直接 `ssh_client.exec_command` 在主线程调用)
- **CC5**: 没有新建独立 `RemoteSnowLumaDriver` 类 (检索 `class.*Remote.*Driver` 只有一个文件命中 NC 原来那个)
- **CC6**: 3 个 `.sh.j2` 模板不嵌 Python f-string; 变量由 `templates.py` 按 `{{name}}` 占位替换 (与 NC templates.py 一致风格)
- **CC7**: `ServerProfile` 新字段全部可选 (default None / False / NAPCAT), 加载旧 `servers.json` 0 warning

### 4.3 手工 smoke 列表 (W9 集中做)

开发者在最后一个 Wave 做, 附截图/日志到 PR:

1. 裸 Debian 12 amd64 VM → Add Server (flavor=snowluma) → Deploy → smoke AC1
2. daemon 启停 3 轮 → AC2/AC10
3. 冷启单 Bot 扫码 → AC4
4. 同服务器冷启双 Bot → AC5
5. 外部 kill node → 自动恢复 → AC7
6. 改 BotConfig.http_servers.port → 热重载 → AC9
7. Ubuntu 24.04 arm64 (可选矩阵) 重跑 1+3, 验证架构无关

---

## 5. 不变量 (Invariants)

- `SSHClient` / `ExecutionBackend` / `LinuxCorePaths` / `LocalPortForwarder` / `host_key_policy` / `thread_pool` / `friendly_errors` 0 改动 (本期只新增 `src/core/remote/snowluma/` 子包)
- `src/core/remote/deployment.py` 既有 NC 部署路径 0 改动
- `src/core/remote/status.py:RemoteRuntimeService` 0 改动 (NC pid/status 协议独立, SL 新写一份)
- `SnowLumaWebUIClient` 8 个 API 签名不变
- `render_onebot_json(snowluma_path, qqid, connect=..., music_sign_url=...)` 签名不变; 仅 SFTP 落地路径变远端
- `BotConfig` schema 0 新增字段 (`runtime_target` 已有)
- `runtime.json` / `webui.json` / `onebot_<uin>.json` 文件格式与 schema 不变 (直接复用)
- `snowluma_login_state_signal` / `process_changed_signal` / `notification_signal` payload 不变
- VNC / WebUI 密码不入 Desktop 本地配置 (cache 的只是隧道端口号)
- **未部署**时 `ServerProfile.backend_flavor = SNOWLUMA` 仅作用于 UI 识别; `start_snowluma_daemon` 在 probe 未通过时必须 raise preflight 错误 (对齐 NC deploy_server preflight)
- Desktop **绝不**在远端执行 `sudo rm -rf` 或 `chmod 777`; launcher 脚本只碰 `$workspace_dir` 与其子树 (复用 NC 现约束)

---

## 6. 上游真相与外部依赖

### 6.1 SnowLuma.Docker.Framework 依赖清单 (权威)

```
apt-get install -y \
  dbus-x11 fluxbox xvfb x11vnc novnc websockify \
  supervisor \
  fonts-wqy-zenhei fonts-noto-cjk \
  libnss3 libatk-bridge2.0-0 libgtk-3-0 libxkbfile1 libsecret-1-0 libasound2 \
  curl ca-certificates
# node 22.x:
curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
apt-get install -y nodejs
```

Desktop 复用 NC 已有 `linuxqq_amd64.deb` / `linuxqq_arm64.deb` 资源; 本需求**不再重新下载** LinuxQQ, 只扩图形栈与 node 的 apt 步骤.

### 6.2 SnowLuma WebUI API 真相 (已验证)

`@example/SnowLuma-main/packages/core/src/webui/server.ts` 的 17 个端点:

```
POST /api/login          POST /api/logout
GET  /api/auth/state     POST /api/auth/check-strength
POST /api/auth/change-password
GET  /avatar/:uin        GET  /api/system       GET  /api/status
GET  /api/qq-list        GET  /api/logs         GET  /api/logs/stream
GET  /api/processes      POST /api/processes/:pid/load
POST /api/processes/:pid/unload
POST /api/processes/:pid/refresh
GET  /api/config/:uin    POST /api/config/:uin
```

**无二维码 API** (全仓 grep `qrcode|qr_code|QRCode` 0 命中) → 扫码必须走图形栈.

### 6.3 进程树图 (部署完成后远端最终状态)

```
$workspace_dir/
├─ snowluma/                 (lite tarball 解压, OQ1 修订实际结构)
│  ├─ dist/                  (vite 输出: index.mjs + webui 静态资源)
│  │  └─ index.mjs           (daemon 入口)
│  ├─ packages/runtime/
│  │  ├─ launcher.sh
│  │  ├─ package.json
│  │  └─ native/             (Linux x64/arm64: snowluma+websocket+ffmpeg .node/.so)
│  └─ config/
│     ├─ runtime.json       (Desktop 渲染)
│     ├─ webui.json         (Desktop 渲染)
│     └─ onebot_<uin>.json  (Desktop 渲染, per-Bot)
├─ runtime/
│  ├─ pid_daemon            (daemon launcher 写)
│  ├─ pid_xvfb / pid_fluxbox / pid_x11vnc / pid_websockify / pid_node
│  ├─ status_daemon.json
│  ├─ pid_bot_<qq_id>
│  └─ status_bot_<qq_id>.json
├─ log/
│  ├─ daemon.log
│  └─ bot_<qq_id>.log
├─ vnc.secret               (install 时生成; mode 600)
└─ webui.secret             (install 时生成; mode 600)

进程树:
dbus-launch → Xvfb :0
           → fluxbox
           → x11vnc -display :0 -rfbport 5900 -passwd $(cat vnc.secret)
           → websockify --web /usr/share/novnc 6081 localhost:5900
           → node snowluma/dist/index.mjs    (OQ1 修订入口)
                ↳ (加载后) hook inject via `process.dlopen` packages/runtime/native/snowluma-linux-*.node
                ↳ spawned /usr/bin/qq --no-sandbox -q <qq_id>
```

### 6.4 Desktop 隧道拓扑

```
Desktop (Win) :47099  ──SSH LocalForward──►  RemoteHost :5099  (WebUI)
Desktop (Win) :47609  ──SSH LocalForward──►  RemoteHost :6081  (noVNC + websockify)
Desktop (Win) webbrowser.open("http://127.0.0.1:47609/vnc.html?autoconnect=1&password=***")
```

---

## 7. 风险与回滚

### 7.1 主要风险

- **R1 (图形栈依赖分布差异)**: Debian 12 / Ubuntu 24.04 / RHEL 9 系的 `novnc / websockify` 包名/路径不完全一致. 缓解: `distro_matrix.py` 按家族分支命令, 未命中发行版走 best-effort + warn
- **R2 (lite tarball 体积)**: SL Framework dist + prebuilds 压缩后可能 ~20-40MB; 打进 Desktop installer 会增体积. 缓解: 与现有 linuxqq.deb 一样, 构建脚本按架构分支选择性打包 (amd64 installer 只带 amd64 prebuilds)
- **R3 (SSH 隧道稳定性)**: 长连接断开导致 5099/6081 不可达. 缓解: `SnowLumaTunnelManager` 用 paramiko `transport.is_active()` 心跳 + 自动重建 (NC `LocalPortForwarder` 已有类似机制, 可抽公共 base)
- **R4 (并发 deploy 撞车)**: 多 Desktop 实例同时对同一 Server 部署 → apt 锁竞争. 缓解: launcher 脚本首行 `flock -n /var/lock/snowluma-deploy || { echo [PROGRESS] 100 BUSY; exit 3; }`
- **R5 (VNC 密码泄漏)**: Desktop `webbrowser.open` URL 里带 password query, 会进浏览器历史. 缓解: 使用 noVNC 的 localStorage 授权方式 (`autoconnect=1` 但不拼 password, 弹提示用户手工粘贴一次, cache 在 localStorage); 或首版先带 query 并 doc warn
- **R6 (node 版本冲突)**: 远端已装过 node 18.x 会干扰. 缓解: probe 阶段识别 `node -v`, < 22 时走 nodesource install, ≥ 22 跳过
- **R7 (首次扫码弹不出 QQ 窗口)**: Xvfb 启动失败或 DISPLAY 未生效. 缓解: daemon launcher start 返回前校验 `xdpyinfo -display :0` 成功; status_daemon.json.graphics_ready=true 才算 READY
- **R8 (daemon crash 时 AC7 误杀)**: node crash 但 Xvfb/fluxbox 仍活; daemon_launcher stop 需幂等清理所有 5 个辅助 pid

### 7.2 回滚策略

- Wave 粒度独立 commit; 任何 Wave 失败可独立 revert
- W1 (Profile schema + migration) 升 `ServerProfile` schema version? → 由于字段全是 Optional + default 向后兼容, **不升 schema_version**; 单纯 revert 代码不会破坏已写入磁盘的 servers.json
- W7 (资源嵌入 lite tarball) 失败: 可临时让 Desktop 走 "SSH 下载 URL" fallback 模式 (构建脚本生成 manifest, 远端 curl 拉 GH release tarball); 仅作备案, 首版不实现

---

## 8. Glossary (术语表)

| 术语                           | 含义                                                                                                            |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| `backend_flavor`               | ServerProfile 的后端种类: `NAPCAT` / `SNOWLUMA` (互斥, per-server)                                              |
| `lite tarball`                 | SnowLuma.Framework 的精简版, 只含 `dist/*` + `prebuilds/*` + 配置样例, 体积缩减 70%+                            |
| daemon (远端语义)              | 远端装 SL 后, 由 launcher 托管的 "Xvfb+fluxbox+x11vnc+websockify+node" 5 进程集合, 对 Desktop 抽象为单一 daemon |
| `SnowLumaRemoteRuntimeService` | SL 远端版 `RemoteRuntimeService`, 不复用 NC 那个 (pid/status 协议字段不同)                                      |
| noVNC 隧道                     | SSH 本地端口 → 远端 `:6081` 的 port forward; 浏览器访问 `http://127.0.0.1:<tunnel>/vnc.html`                    |
| WebUI 隧道                     | SSH 本地端口 → 远端 `:5099`; `SnowLumaWebUIClient` 透过它调 HTTP API                                            |
| `vnc.secret` / `webui.secret`  | 远端首次部署时随机生成的密码; 分别给 noVNC 与 SnowLuma WebUI 用; `mode 600`, 只在远端存                         |

---

## 9. 锁定决策列表 (Frozen Decisions)

| ID  | 决策                                     | 选项                                                                     | 选定                                                | 依据                                                                    |
| --- | ---------------------------------------- | ------------------------------------------------------------------------ | --------------------------------------------------- | ----------------------------------------------------------------------- |
| D1  | 远端目标形态                             | A(Docker) / B(原生) / C(双支持) / D(A+自带装 Docker)                     | **B**                                               | 用户明确: 零 Docker 依赖                                                |
| D2  | 首次扫码 UX                              | 1(本地渲码) / 2(QtWebEngine嵌 noVNC) / 3(默认浏览器 noVNC) / 4(外部 VNC) | **3**                                               | 用户选, 最低 Desktop 侵入 + SL 无二维码 API                             |
| D3  | lite 还是 full tarball                   | lite / full                                                              | **lite**                                            | 减打包体积, dist+prebuilds 足够                                         |
| D4  | 进程管理                                 | supervisord / systemd-user / nohup + pid                                 | **nohup + pid (launcher 脚本)**                     | 与 NC 对齐, 不引入新依赖                                                |
| D5  | 多 SL daemon per server                  | 单 daemon / 多 daemon (不同 workspace)                                   | **单 daemon**                                       | per-server 语义最简; 多 workspace 用户自建多个 ServerProfile            |
| D6  | daemon 生命周期                          | 随最后 Bot stop 退 / 持久驻留                                            | **持久驻留**                                        | 对齐本地 daemon 重构 DC-1; 省反复 30s spawn                             |
| D7  | HOT 模式远端支持                         | 支持 / 不支持                                                            | **不支持**                                          | 远端无 Desktop 不拥有的 QQ.exe attach 语义                              |
| D8  | ServerProfile 扩字段 vs 新建 ProfileType | 扩字段 / 新建                                                            | **扩字段**                                          | schema 向后兼容, UI dispatch 面小                                       |
| D9  | WebUI 密码 override 源                   | per-server / App 级                                                      | **per-server (从 ServerProfile 派生)** + App 级默认 | 多服务器可用不同密码; 未填则 fallback App 级 (与 daemon 重构 §2.4 共享) |
| D10 | VNC 密码传递方式                         | URL query / noVNC localStorage / 提示手贴                                | **URL query (首版)**                                | UX 最顺; 安全 caveat 记 R5                                              |

---

## 10. 参考资料 (非规范性)

- SnowLuma 上游主仓: `example/SnowLuma-main/packages/core/src/{index.ts,hook/*,bridge/*,onebot/*,webui/*}`
- SnowLuma Docker Framework: `example/SnowLuma.Docker.Framework-main/{Dockerfile,scripts/start.sh,artifacts/supervisord.conf,docker-compose.yml}`
- NC Desktop 远端部署参考: `src/core/remote/{deployment.py,status.py,templates.py,server_manager.py}`
- NC Desktop 资源脚本参考: `src/resource/script/remote/{install_linuxqq.sh.j2,install_napcat.sh.j2,napcat_launcher.sh.j2}`
- SL Desktop daemon 重构: `docs/requirements/2026-05-11-snowluma-daemon-refactor.md` (本需求的直接前置)
- NC 分布式策略前置: `docs/plans/2026-05-08-ssh-distro-expansion-execution-plan.md`
