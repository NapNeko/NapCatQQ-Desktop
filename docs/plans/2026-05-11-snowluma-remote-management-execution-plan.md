---
plan_id: 2026-05-11-snowluma-remote-management-execution-plan
status: in_progress
owner: @NapNeko/NapCatQQ-Desktop
requirement: docs/requirements/2026-05-11-snowluma-remote-management.md
vibe_stage: plan_execute
internal_grade: L
progress: W1-W10a 完成 + Code Review 全部 fix (282 PASS @ Python 3.12) · W10b UI + W11 真机 smoke backlog
---

> **执行进度** (2026-05-11, Code Review 后)
>
> | Wave | 状态 | 测试 | 备注 |
> |------|------|------|------|
> | W1 子包骨架 | ✅ | 39 PASS | `paths.py` + `templates.py` |
> | W2 .sh.j2 脚本 | ✅ | 23 PASS | 3 个模板 + .qrc + resource.py 重生成 |
> | W3 lite tarball 构建 | ✅ | 30 PASS | `build_snowluma_framework_lite.py` + PyInstaller hook + `VersioningService` |
> | W4 deployment | ✅ | 15 PASS | `SnowLumaDeployment` 复用 NC `LinuxCoreDeployment` |
> | W5 status + launcher | ✅ | 39 PASS | `SnowLumaRemoteRuntimeService` + `SnowLumaLauncherCommands` |
> | W6 tunnels | ✅ | 13 PASS | `SnowLumaTunnelManager` 双隧道 + watchdog + 边沿触发 + NC `LocalPortForwarder` 扩展 |
> | W7 ServerProfile | ✅ | 13 PASS | `BackendFlavor` + SL 字段 + 向后兼容 |
> | W8 ServerManager SL 路径 | ✅ | 9 PASS | `_deploy_snowluma_flavor` 5 阶段流程 |
> | W9 RemoteDaemon | ✅ | 17 PASS | 远端 daemon 控制器 + Qt 信号 + crash 桥接 + 惰性 tunnel manager (P9) |
> | W10a vnc_launcher | ✅ | 14 PASS | `open_snowluma_vnc` 返脱敏端点 (P10) |
> | **Code Review fix** | ✅ | +2 \| **P1-P15 全部修复** | 见下表 |
> | **W10b UI 接入** | ⏸ backlog | - | `AddServerDialog` flavor 单选 + `BotCard` SL 按钮 + `BotProcessManager` driver 分发 (UI 层手工 review) |
> | **W11 smoke + 文档** | 文档完成 | - | 文档 `docs/guides/snowluma-remote-smoke-test.md` 已写; 真机 smoke 需用户在 VPS 上跑 |
>
> **累计**: 净增 ~4500 行代码 / 测试; **282 个自动化测试全 PASS @ Python 3.12** (项目实际要求版本) / 14.74s; NC 既有测试无回归.
>
> **Code Review 修复矩阵** (P1-P15):
>
> | # | 严重度 | 问题 | 修复位置 | 单测 |
> |---|--------|------|----------|------|
> | P1 | Critical | `PurePosixPath.full_match` 在 Python 3.12 不存在 | `script/build_scripts/build_snowluma_framework_lite.py` (`_glob_to_regex`) | 现有 18 测试 (3.12 全过) |
> | P2 | Critical | `pid_daemon` 写 launcher shell `$$` 而非 node PID | `snowluma_daemon_launcher.sh.j2` step 7 | 模板既有测试 + 真机 smoke |
> | P3 | Critical | watchdog `crashed` 信号每 watchdog 周期重复 emit | `tunnels.py` `_crashed_emitted` 边沿触发 | **+1 回归** `test_crash_callback_edge_triggered_not_repeated` |
> | P4 | Critical | `acquire` race 半状态隧道泄漏 | `tunnels.py` race 分支调 `_stop_tunnels_locked` | 既有 acquire 测试 |
> | P5 | High | `cmd_stop` 顺序错: Xvfb 先死导致 node SIGPIPE | `snowluma_daemon_launcher.sh.j2` (node → websockify → x11vnc → fluxbox → xvfb) | 真机 smoke |
> | P6 | High | VNC 密码 `hex 12` 表面 96 bit 但 RFB DES 截断到 32 bit | `install_snowluma.sh.j2` 改 `hex 4` + smoke 文档明示 | - |
> | P7 | Medium | `pgrep` 失败 `\|\| true` 导致 stop 漏 kill x11vnc/websockify | `snowluma_daemon_launcher.sh.j2` 空 PID die | 真机 smoke |
> | P8 | High | 并发 SSH 调 `cmd_start` 双 spawn | daemon launcher FD 200 + bot launcher FD 201 per-qq_id `flock` | 真机 smoke |
> | P9 | High | `RemoteSnowLumaDaemon.__init__` 调 `ssh_client.transport` 有 IO 副作用 | `daemon.py` `_ensure_tunnel_manager_locked` 惰性构造 | 既有 daemon 测试 (Mock SSH 不连接也能构造) |
> | P10 | High | `open_snowluma_vnc` 返回的 message 含明文密码 URL | `vnc_launcher.py` 改返脱敏端点 | **+1 回归** `test_success_returns_sanitized_endpoint_not_url` |
> | P11 | Medium | `_validate_linux_path` 错误消息硬写 "LinuxCorePaths" 误导 | `models.py` 加 `cls_name` 参数 + `paths.py` 调用方传 SL 类名 | 现有 paths 测试 |
> | P13 | Medium | `ServerProfile.create` docstring 写抛 ValueError 但代码用默认值 | `servers.py` docstring 与代码对齐 | - |
> | P15 | Low | `tunnel_manager.stop` 在 lock 内 join watchdog 可能 5s hang | `tunnels.py` `stop()` 入口先 `_stop_event.set()` | 既有 stop 测试 |

# SnowLuma 远程管理 (原生 Linux 进程部署) · 执行计划 (xl_plan)

> 当前对应 requirement 文档: `docs/requirements/2026-05-11-snowluma-remote-management.md`
> 本计划停在 `xl_plan` 阶段, 待用户批准后通过 `vibe` 或 `vibe-do` 入口接管 `plan_execute`.

---

## Vibe 契约字段

- **runtime**: `interactive_governed` (root)
- **wrapper entry**: `Vibe: How Do We Do It?` (`vibe-how`) → **stop target: `xl_plan`**
- **internal grade decision**: **L** — 单 agent 串行. 证据:
  - W1-W5 构成"骨架 → 资源 → 部署 → 运行时 → 隧道"依赖链, 强耦合 (`templates.py` 输出被 `deployment.py` 消费, `launcher.py` 输出被 `server_manager.py` 消费)
  - W6 (`ServerProfile` schema 扩展) 必先于 W7 (`ServerManager` 扩新方法), 两者共享 `src/core/remote/servers.py` 与 `server_manager.py` 写权限
  - W7-W9 有单向依赖 (`ServerManager` → `SnowLumaDaemon` → UI), fan-out 会引发 merge 冲突
  - W10 (集成 smoke) 必串在所有功能 Wave 之后, 与 daemon-refactor plan 的 L grade 判据一致
  - XL fan-out 理论省 ~25% 时间, 但 `snowluma_driver.py` / `snowluma_daemon.py` / `server_manager.py` 三文件写冲突代价 > 收益
- **总规模估算**: 净增源代码 ~1400 行 (远端子包) + ~500 行 (driver/daemon/UI 改造) + ~400 行 shell 模板 + ~600 行测试 + ~300 行构建脚本; 净新增源文件 ~8 个 (`src/core/remote/snowluma/*.py`), 资源脚本 3 个 (`.sh.j2`), 资源 tarball 1 份, 测试文件 4-5 个

---

## Wave 结构

| Wave | 名称                                                                          | 依赖        | 串行/并行         | 估算行数 |
| ---- | ----------------------------------------------------------------------------- | ----------- | ----------------- | -------- |
| W1   | `src/core/remote/snowluma/` 子包骨架 + `paths.py` + `templates.py`            | —           | 串行              | ~300     |
| W2   | 3 个 `.sh.j2` 资源脚本 + `.qrc` 注册 + 渲染往返单测                           | W1          | 串行              | ~400     |
| W3   | SnowLuma.Framework **lite tarball** 构建脚本 + 资源嵌入 + PyInstaller datas   | —           | 串行 (可并行规划) | ~300     |
| W4   | `deployment.py` 完整实现 (probe + 3 阶段 install + config sync)               | W1, W2, W3  | 串行 (原子)       | ~700     |
| W5   | `status.py` + `launcher.py` (远端 pid/status 协议 + launcher 命令构造)        | W1, W2      | 串行              | ~450     |
| W6   | `tunnels.py` (SnowLumaTunnelManager 双隧道 + 自愈)                            | W1          | 串行              | ~250     |
| W7   | `ServerProfile` schema 扩展 + `ServerRegistry` migration + 单测               | —           | 串行              | ~180     |
| W8   | `ServerManager` 扩远端 SL 方法 + Qt 信号 + 远端编排集成                       | W4,W5,W6,W7 | 串行              | ~400     |
| W9   | `SnowLumaDaemon` + `SnowLumaDriver` 远端多态 + `BotProcessManager` dispatch   | W8          | 串行              | ~350     |
| W10  | UI 改动: AddServer dialog / Remote Ops SL 子页 / BotCard 远端分支 / Home 角标 | W7,W8,W9    | 串行              | ~450     |
| W11  | 集成 smoke + 手工验收 + phase_cleanup                                         | W1-W10 完成 | 串行              | —        |

> **依赖说明**: W1 → W2/W6 双分支 (模板基座被 W2 脚本 + W6 隧道路径选择共用); W3 独立资源构建 (理论可与 W1 并行开发, 但落地必先于 W4); W4 聚合 W1+W2+W3 产物 (probe/install/upload 三件事共用脚本+资源); W5 与 W4 并行开发但集成测试要 W4 部署完成的远端环境; W6 独立工具类; W7 是 W8 的 schema 前置; W8 聚合 W4+W5+W6+W7 所有远端设施; W9 让上层 daemon/driver 识别远端; W10 所有 UI 最后集成; W11 smoke 收尾.

---

## 跨 wave 不变量 (Invariants)

- **I1** NapCat 路径零改动 (`src/core/remote/deployment.py` / `status.py` / `templates.py` / `server_manager.py` NC 方法全部零行改动). git diff 在 W8 之前对这些文件 0 命中; W8 仅追加 SL 方法到 `server_manager.py` (不修改 NC 方法)
- **I2** `SSHClient` / `ExecutionBackend` / `LinuxCorePaths` / `LocalPortForwarder` / `host_key_policy` / `thread_pool` / `friendly_errors` 全部零改动 (仅被 SL 子包引用)
- **I3** `SnowLumaWebUIClient` 的 8 个 API 签名保持不变 (daemon-refactor I2 的延续); W9 调用点从 "本地 127.0.0.1:5099" 变为 "隧道端口 127.0.0.1:<tunnel>", host 在 `ensure_running` 阶段注入, client 代码不动
- **I4** `render_onebot_json(snowluma_path, qqid, *, connect, music_sign_url)` / `render_runtime_json` / `render_webui_json` 三个渲染器签名不变; W4/W8 调用点变化仅为**落地路径**从本地 `%APPDATA%/SnowLuma/config/` → SFTP 写远端 `$workspace/snowluma/config/`
- **I5** `BotConfig` / `ServerProfile` schema 向后兼容: 旧 `servers.json` 加载 0 warning, 旧 `bot.json` 加载路径不新增分支 (SL 远端 bot 的 `runtime_target` 字段是 daemon-refactor 前置已就位的)
- **I6** `snowluma_login_state_signal` / `process_changed_signal` / `notification_signal` payload 不变 (BotCard / BotLogPage 零改动)
- **I7** Desktop **绝不**在远端执行 `sudo rm -rf` / `chmod 777` / 修改 `$workspace_dir` 之外任何路径; 所有 launcher 脚本首行 `set -euo pipefail` + 路径前置 `: "${WORKSPACE:?}"` 防御空变量 (复用 NC templates.py 模式)
- **I8** VNC / WebUI 密码**不**入 Desktop 本地任何文件 (仅 cache 隧道端口号至 `ServerProfile`); 密码生命周期只在远端 `$workspace/vnc.secret` + `$workspace/webui.secret`
- **I9** 每个 Wave 独立 commit; W4-W8 因 git 依赖环可一 `git revert HEAD~5..HEAD` 整体回滚; W7 schema 字段全 Optional, 回滚不破坏磁盘数据
- **I10** 本计划**不写**单独架构文档 (与 daemon-refactor 同决策; `requirement.md` §2 已含完整交付清单)

---

## W1 — 子包骨架 + `paths.py` + `templates.py`

### Owner boundary

- **新增**: `src/core/remote/snowluma/__init__.py` (~20 行, re-export)
- **新增**: `src/core/remote/snowluma/paths.py` (~80 行)
- **新增**: `src/core/remote/snowluma/templates.py` (~160 行)
- **新增**: `script/test/test_snowluma_paths.py` (~80 行)
- **修改**: 无

### 实现要点

1. **`paths.py` 定义 `SnowLumaRemotePaths` 数据类** (`@dataclass(slots=True, frozen=True)`):
   - `base_dir: str` (默认 `"~/snowluma-remote"`)
   - `workspace_dir: str` (派生 `{base_dir}/workspace`)
   - `snowluma_framework_dir` / `config_dir` / `runtime_dir` / `log_dir`
   - `pid_daemon: str` / `status_daemon: str` (`runtime/status_daemon.json`)
   - `pid_bot_prefix: str` (`runtime/pid_bot_`) + 方法 `pid_bot(qq_id)`
   - `status_bot_prefix: str` + 方法 `status_bot(qq_id)`
   - `log_daemon: str` / 方法 `log_bot(qq_id)`
   - `vnc_secret: str` / `webui_secret: str`
   - 类方法 `from_base(base_dir) -> SnowLumaRemotePaths` 一次性派生全部字段
2. **`templates.py` 提供 3 个 builder**:
   - `build_install_snowluma_script(paths, *, framework_archive_name, enable_nodesource, vnc_port=5900, novnc_port=6081, webui_port=5099, display_num=0) -> str`
   - `build_snowluma_daemon_launcher(paths, *, display_num=0, vnc_port=5900, novnc_port=6081) -> str`
   - `build_snowluma_bot_launcher(paths, *, display_num=0) -> str`
   - 渲染方式与 `src/core/remote/templates.py` 的 NC 版一致: 读 Qt 资源 `.sh.j2` 文本 → Python `.replace("{{name}}", value)` (不引入 jinja2 依赖)
3. **不引入** Qt 资源加载 (`QFile(":/script/remote/snowluma/*.sh.j2")`) 在本 Wave; 先用相对文件路径 (W2 才注册 .qrc); `templates.py` 以 `_RESOURCE_PATHS` 常量待 W2 填路径并切换

### 单测覆盖 (`test_snowluma_paths.py`)

- `SnowLumaRemotePaths.from_base("/opt/snowluma")` 所有派生字段值正确
- `pid_bot("12345")` / `status_bot("12345")` / `log_bot("12345")` 路径拼接正确 (POSIX 风格, 无 Windows 斜杠)
- `base_dir` 含空格时不 raise (用户自定义 `~/my snowluma`), 所有路径保持带空格

### 验证/回滚

- `python -m pytest script/test/test_snowluma_paths.py -v` 全绿
- 回滚: 单 `git revert <hash>` 即可 (仅新增文件, 无他 Wave 依赖)

---

## W2 — 资源脚本 `.sh.j2` 模板 + `.qrc` 注册 + 渲染往返单测

### Owner boundary

- **新增**: `src/resource/script/remote/snowluma/install_snowluma.sh.j2` (~120 行 shell)
- **新增**: `src/resource/script/remote/snowluma/snowluma_daemon_launcher.sh.j2` (~130 行)
- **新增**: `src/resource/script/remote/snowluma/snowluma_bot_launcher.sh.j2` (~80 行)
- **修改**: `src/resource/resource.qrc` (加 `<qresource prefix="/script/remote/snowluma">` 节点, 3 个 file 条目)
- **修改**: `src/core/remote/snowluma/templates.py` (`_RESOURCE_PATHS` 改读 `:/script/remote/snowluma/*.sh.j2` via `QFile`)
- **新增**: `script/test/test_snowluma_templates.py` (~150 行)

### 实现要点

#### `install_snowluma.sh.j2` 脚本契约

- 参数 (由 `templates.py` 注入): `{{workspace_dir}}` / `{{framework_archive_path}}` / `{{framework_archive_name}}` / `{{enable_nodesource}}` / `{{vnc_port}}` / `{{novnc_port}}` / `{{webui_port}}` / `{{display_num}}`
- 流程:
  1. `[PROGRESS] 0 检查 OS`
  2. `set -euo pipefail; umask 077`
  3. 识别 `dpkg` 还是 `rpm` (复用 NC `install_linuxqq.sh` 里的 pattern)
  4. `[PROGRESS] 5 更新包索引`; apt/yum 对应命令
  5. `[PROGRESS] 10 安装图形栈`:
     ```
     apt-get install -y dbus-x11 fluxbox xvfb x11vnc novnc websockify \
       fonts-wqy-zenhei fonts-noto-cjk \
       libnss3 libatk-bridge2.0-0 libgtk-3-0 libxkbfile1 libsecret-1-0 libasound2 \
       curl ca-certificates
     ```
  6. `[PROGRESS] 40 安装 node` (OQ2 决策, 3 级 fallback):
     - **L1**: `command -v node && node -v | grep -E '^v(2[2-9]|[3-9][0-9])'` 已装 ≥ 22 → 跳过
     - **L2**: `apt-get install -y nodejs`; 装完检测版本 ≥ 22 → ok
     - **L3** (L2 装的 < 22 或失败): `curl --max-time 10 -fsSL https://deb.nodesource.com/setup_22.x | bash -` + `apt-get install -y nodejs`
     - L3 仍失败 → `echo "[PROGRESS] 100 ERROR_NODE_VERSION_TOO_LOW"; exit 1`; UI 拦截后提示用户手装 ≥ 22 后重试 deploy
     - 注: `{{enable_nodesource}}=0` 强制关 L3 (供 air-gapped 部署或测试)
  7. `[PROGRESS] 60 解压 SnowLuma.Framework` (OQ1 修订, 实际产物结构):
     - `mkdir -p {{workspace_dir}}/snowluma`
     - `tar -xzf {{framework_archive_path}} -C {{workspace_dir}}/snowluma --strip-components=1`
     - 解压后 `{{workspace_dir}}/snowluma/` 含: `dist/index.mjs` (vite ES module 产物) + `packages/runtime/native/snowluma-linux-{x64,arm64}.{node,so}` 等
     - `find {{workspace_dir}}/snowluma -name '*.node' -exec chmod 644 {} \;` (确保 dlopen 可读)
  8. `[PROGRESS] 80 生成密钥 + 目录`:
     - `mkdir -p {{workspace_dir}}/runtime {{workspace_dir}}/log {{workspace_dir}}/snowluma/config`
     - `: > {{workspace_dir}}/vnc.secret && chmod 600 {{workspace_dir}}/vnc.secret && openssl rand -hex 12 > {{workspace_dir}}/vnc.secret`
     - 同上 `{{workspace_dir}}/webui.secret`
  9. `[PROGRESS] 95 冒烟验证`:
     - `command -v Xvfb fluxbox x11vnc websockify novnc_proxy node >/dev/null`
     - `node -v` 确认 ≥ 22
  10. `[PROGRESS] 100 完成`

#### `snowluma_daemon_launcher.sh.j2` 契约

- 参数: `{{workspace_dir}}` / `{{display_num}}` / `{{vnc_port}}` / `{{novnc_port}}`
- 子命令:
  - `start`:
    1. 幂等: 若 `{{workspace_dir}}/runtime/pid_daemon` 存在且 `kill -0 $(cat pid_daemon)` 成功, 退 0 且写 status
    2. `export DISPLAY=:{{display_num}}`
    3. `dbus-launch --exit-with-session --sh-syntax > {{workspace_dir}}/runtime/dbus.env`; `source` 该文件
    4. `Xvfb :{{display_num}} -screen 0 1280x720x24 -nolisten tcp & echo $! > pid_xvfb`
    5. `sleep 0.5 && xdpyinfo -display :{{display_num}} >/dev/null` 不成功则 exit 1
    6. `fluxbox & echo $! > pid_fluxbox`
    7. `x11vnc -display :{{display_num}} -forever -shared -rfbport {{vnc_port}} -passwdfile {{workspace_dir}}/vnc.secret -bg -o {{workspace_dir}}/log/x11vnc.log`; pid 从 x11vnc 自身输出抓或 `pgrep -f "x11vnc.*rfbport {{vnc_port}}" > pid_x11vnc`
    8. `websockify --daemon --web /usr/share/novnc {{novnc_port}} localhost:{{vnc_port}} --pidfile pid_websockify`
    9. `cd {{workspace_dir}}/snowluma && nohup node dist/index.mjs >> {{workspace_dir}}/log/daemon.log 2>&1 & echo $! > pid_node` (OQ1 修订: 入口为 vite 输出的 `dist/index.mjs`)
    10. `(echo $$ > pid_daemon)` (守护外壳 pid, 非 node pid, 用于 stop 汇总)
    11. 写 `status_daemon.json`: `{"running":true,"started_at":"...","pids":{"xvfb":...,"fluxbox":...,...}}` (用 `jq` 或 shell printf, 保持 key 顺序)
    12. 轮询 `curl -fs http://127.0.0.1:{{webui_port}}/api/status` 最多 30s, 不成功则 status `graphics_ready:false, webui_ready:false` 并 exit 2
    13. exit 0
  - `stop`: 逆序 `kill $(cat pid_<each>)`; 清所有 `pid_*` 与 `status_daemon.json.running=false`
  - `status`: `cat status_daemon.json`
  - `restart`: `stop && start`
  - `wait-ready`: 轮询 `status_daemon.json.webui_ready=true` (供 Desktop 无 polling 逻辑)

#### `snowluma_bot_launcher.sh.j2` 契约

- 参数: `{{workspace_dir}}` / `{{display_num}}`
- 子命令:
  - `start <qq_id>`:
    1. 前置: `{{workspace_dir}}/runtime/pid_daemon` 存在且活
    2. `DISPLAY=:{{display_num}} nohup /usr/bin/qq --no-sandbox -q <qq_id> >> {{workspace_dir}}/log/bot_<qq_id>.log 2>&1 & echo $! > {{workspace_dir}}/runtime/pid_bot_<qq_id>`
    3. 写 `status_bot_<qq_id>.json`: `{"qq_id":"<qq>","pid":<pid>,"started_at":"...","uin":null}`
    4. exit 0
  - `stop <qq_id>`: `kill $(cat pid_bot_<qq_id>) 2>/dev/null || true`; 清 pid + status
  - `status <qq_id>`: `cat status_bot_<qq_id>.json || echo '{}'`
- **不做** UIN 回填 (UIN 来自 WebUI `/api/qq-list`, Desktop 端 poller 处理)

#### `.qrc` 节点

```xml
<qresource prefix="/script/remote/snowluma">
  <file>install_snowluma.sh.j2</file>
  <file>snowluma_daemon_launcher.sh.j2</file>
  <file>snowluma_bot_launcher.sh.j2</file>
</qresource>
```

### 单测覆盖 (`test_snowluma_templates.py`)

- `build_install_snowluma_script(paths, framework_archive_name="sl.tar.gz", enable_nodesource=True, ...)` 产物含关键行 (`apt-get install -y dbus-x11`, `tar -xzf ... sl.tar.gz`, `[PROGRESS] 100 完成`)
- `build_snowluma_daemon_launcher(paths, display_num=0, ...)` 产物含 `DISPLAY=:0`, `pid_daemon`, `echo $!`
- `build_snowluma_bot_launcher(paths, display_num=0)` 含 `qq --no-sandbox -q`, `pid_bot_`
- 用 `bash -n <rendered>` 语法检查 3 个脚本 (CI 若有 bash, 否则 skip)

### 验证/回滚

- `python -m pytest script/test/test_snowluma_templates.py -v` 全绿
- 运行 `pyside6-rcc src/resource/resource.qrc -o src/resource/resource.py` 重新生成, `import src.resource.resource` 无 error
- 回滚: `git revert` (仅新增文件 + .qrc 新增节点, 不破坏 NC 已有资源)

---

## W3 — SnowLuma.Framework **lite tarball** 构建脚本 + 资源嵌入 + PyInstaller datas

### Owner boundary

- **新增**: `script/build_scripts/build_snowluma_framework_lite.py` (~200 行)
- **新增**: `src/resource/runtime/.gitkeep` (空文件占位; tarball 本体 `.gitignore` 忽略)
- **修改**: `.gitignore` (追加 `src/resource/runtime/snowluma_framework_lite*.tar.gz`)
- **修改**: `script/build_scripts/*.spec` (PyInstaller `datas` 增加 `('src/resource/runtime/snowluma_framework_lite.tar.gz', 'resource/runtime')`)
- **修改**: `src/core/versioning/service.py` (加属性 `snowluma_framework_bundled_version`, 读 tarball 同级 `version.txt`; 无 tarball 时 None + 日志 warn)
- **新增**: `script/test/test_build_snowluma_framework_lite.py` (~120 行, 用 fake tree fixture)

### 实现要点

1. **`build_snowluma_framework_lite.py`**:
   - 入参: `--source example/SnowLuma-main` / `--out src/resource/runtime/snowluma_framework_lite.tar.gz`
   - 步骤:
     1. 读 `source/package.json:version` → 写临时 `version.txt`
     2. 从 source 复制白名单文件到 staging (OQ1 修订: 实际产物结构 6 packages):
        - `dist/**` (vite 输出, core + webui 合并产物; `outDir` 在仓库根)
        - `packages/runtime/launcher.sh`
        - `packages/runtime/package.json`
        - `packages/runtime/native/snowluma-linux-{x64,arm64}.{node,so}` (4 文件)
        - `packages/runtime/native/websocket-linux-{x64,arm64}.node` (2 文件)
        - `packages/runtime/native/ffmpeg/ffmpegAddon.linux.{x64,arm64}.node` (2 文件)
        - `package.json` (根, 取 version 字段)
        - `LICENSE`
     3. **严格排除**:
        - `node_modules/**`, `**/*.map`, `**/test/**`, `**/tests/**`, `.git/**`
        - `**/src/**` (TS 源)
        - `packages/sdk/**` (用户侧 SDK, daemon 运行时不需要)
        - `packages/runtime/native/snowluma-win32-*.{dll,node}` (Linux 部署用不上)
        - `packages/runtime/native/snowluma-darwin-**`, `ffmpegAddon.{win32,darwin}.**`
        - `packages/{core,webui,websocket}/src/**` (TS 源, dist 已含产物)
        - `packages/webui/dist/**` (已合并到根 `dist/`)
     4. 关键校正: SL `packages/core/vite.config.ts` 的 `outDir` 指向**monorepo 根 `dist/`** 不是 `packages/core/dist/`; whitelist 必须按根 `dist/` 抓
     5. `tarfile.open(out, "w:gz")` 打成 tar.gz (strip leading path to only `snowluma-framework/`, 对应脚本 `tar --strip-components=1`)
     6. 验证产物大小在预期范围 (5MB < size < 100MB, 否则 error exit)
     7. 写 sibling `src/resource/runtime/snowluma_framework_lite.version.txt`
2. **`VersioningService.snowluma_framework_bundled_version`**:
   - 启动时 `open(":/runtime/snowluma_framework_lite.version.txt")` 读取; 失败返 None
   - 暴露到 UI `AboutPage` + `AddServerDialog` 的 "框架版本" 提示
3. **构建流程集成**:
   - `.github/workflows/build-msi.yml` 在 build 步骤前加 `python script/build_scripts/build_snowluma_framework_lite.py --source ./example/SnowLuma-main --out src/resource/runtime/snowluma_framework_lite.tar.gz` (需要 `example/SnowLuma-main` 已执行过 `npm run build`; 否则 skip with warn)
   - 本地开发: README 新增 "准备 SnowLuma 远端资源" 段落, 说明手工跑构建脚本

### 单测覆盖 (`test_build_snowluma_framework_lite.py`)

- 用 `tmp_path` 造假 source 树 (5 个 whitelist 文件 + 3 个 blacklist 文件)
- 运行 builder, 断言产物:
  - 是合法 gzip
  - `tar tzf` 列表只含 whitelist 项
  - blacklist (`node_modules/**`) 不在列表
  - 产物大小 > 1KB (非空)
  - `version.txt` 值与 fake `package.json` 一致

### 验证/回滚

- `python script/build_scripts/build_snowluma_framework_lite.py --source <real SL path> --out /tmp/test.tar.gz` 本地验证产物
- `tar -tzf /tmp/test.tar.gz | head` 确认结构
- 回滚: `git revert` (不影响已有资源; 但 PyInstaller spec 修改回滚后需确认 NC installer 仍能 build)

---

## W4 — `deployment.py` 完整实现 (probe + 3 阶段 install + config sync)

### Owner boundary

- **新增**: `src/core/remote/snowluma/deployment.py` (~700 行)
- **新增**: `script/test/test_snowluma_deployment.py` (~250 行, mock SSH backend)
- **新增**: `script/test/fixtures/fake_snowluma_remote/` (mock remote fs)

### 实现要点

1. **数据类**:
   - `SnowLumaRemoteProbeReport` (继承 NC `LinuxCoreDeploymentProbe` 字段) 追加:
     - `has_node: bool` / `installed_node_version: str | None`
     - `has_xvfb: bool` / `has_fluxbox: bool` / `has_x11vnc: bool`
     - `has_novnc: bool` / `has_websockify: bool` / `has_dbus_launch: bool`
     - `installed_framework_version: str | None`
     - `framework_bundled_vs_installed: Literal["equal","upgradable","downgradable","missing","unknown"]`
   - `SnowLumaInstallStepResult` 同 NC `InstallStepResult` 字段
2. **`SnowLumaRemoteDeployment` 类**:
   - 构造: `(backend: ExecutionBackend, ssh_client: SSHClient, paths: SnowLumaRemotePaths, bundled_archive_resource_path: str, bundled_version: str | None)`
   - 方法:
     - `probe_environment() -> SnowLumaRemoteProbeReport`: 跑 13 个并行 `command -v` / `dpkg -l` 查询, 用 `&&` 组合后 `backend.run` 一次拉回所有结果
     - `install_graphics_stack(progress: ProgressCallback, log_line: LogLineCallback) -> SnowLumaInstallStepResult`: 渲染 install_snowluma.sh 的**图形栈段**? 或拆独立脚本? — **决策**: 不拆独立脚本, 用单个 `install_snowluma.sh` 并加环境变量 `STAGE=graphics_only|node_only|framework_only|all` 控制阶段 (减少脚本数; Desktop 侧按阶段调同一脚本带不同 STAGE)
     - 或者更简单: **只暴露 `install_all(progress, log_line)`**, 不分阶段 API (UI 只需要整体 progress, 3 个 `[PROGRESS]` 区段内已由脚本自己驱动); 首版采纳此方案
     - `ensure_workspace() -> None`: SFTP `mkdir -p` workspace + runtime + log + snowluma/config
     - `upload_framework_archive(progress: ProgressCallback) -> str`: SFTP 把 bundled lite tarball 传到 `{workspace}/snowluma_framework_lite.tar.gz` (paramiko `put(callback=...)` 映射到 30-60% 段); 返回远端路径
     - `install_all(progress, log_line) -> SnowLumaInstallStepResult`: 编排 `ensure_workspace → upload_framework_archive → upload_and_run_install_script`; 解析 `[PROGRESS] N msg` 回传
     - `sync_daemon_configs(webui_password: str, webui_port: int = 5099) -> None`: 本地 `render_runtime_json` + `render_webui_json` → SFTP 覆盖 `{workspace}/snowluma/config/runtime.json` / `webui.json`
     - `sync_onebot_config(qq_id: str, uin: str, connect: ConnectConfig, music_sign_url: str) -> None`: `render_onebot_json` → SFTP 覆盖
     - `uninstall(progress: ProgressCallback)` (可选, W4 可不做, 延到 phase_cleanup 之外)
3. **进度协议**:
   - 脚本输出 `[PROGRESS] N msg` (与 NC 一致), Python 端 `_PROGRESS_LINE_PATTERN` 复用 NC 正则
   - SFTP upload 走 paramiko callback, 手动映射 `upload_percent * 0.3 + 30` → `[PROGRESS] <percent> 上传 SnowLuma.Framework`
4. **LinuxQQ 复用策略**:
   - 不复制 `LinuxCoreDeployment.install_linuxqq`, 而是 `from src.core.remote.deployment import LinuxCoreDeployment; nc_deployer = LinuxCoreDeployment(...); nc_deployer.install_linuxqq(...)` 直接调用
   - SL deployer 的 `install_all` 流程: (图形栈 + node via SL 脚本) → (LinuxQQ via NC deployer) → (SnowLuma.Framework via SL 脚本); progress 段分配 0-40 / 40-70 / 70-100
5. **错误分类**:
   - `SnowLumaDeploymentError(RemoteCommandError)` 子类化, 字段 `phase: Literal["probe","graphics","node","linuxqq","framework","config"]`

### 单测覆盖 (`test_snowluma_deployment.py`)

- Mock `ExecutionBackend.run` 按命令返回固定 stdout, 覆盖:
  - `probe_environment` 在"全绿/图形栈缺失/node 版本低/framework 未装"4 场景的字段正确
  - `install_all` 解析 `[PROGRESS]` 序列顺序
  - `sync_daemon_configs` 渲染 + SFTP put 调用次数与参数
- 集成测试 **不在 W4** 做 (等 W11)

### 验证/回滚

- 单测全绿
- 回滚: 单 `git revert`, 其他 Wave 未 import 此模块则零影响 (W8 才 import)

---

## W5 — `status.py` + `launcher.py` (远端 pid/status 协议 + launcher 命令构造)

### Owner boundary

- **新增**: `src/core/remote/snowluma/status.py` (~250 行)
- **新增**: `src/core/remote/snowluma/launcher.py` (~180 行)
- **新增**: `script/test/test_snowluma_remote_status.py` (~150 行)
- **新增**: `script/test/test_snowluma_launcher.py` (~80 行)

### 实现要点

#### `status.py`

1. 数据类:
   - `RemoteSnowLumaDaemonStatus`: `running: bool`, `started_at: str | None`, `pids: dict[str, int]` (xvfb/fluxbox/x11vnc/websockify/node), `webui_ready: bool`, `graphics_ready: bool`, `raw_payload: dict`
   - `RemoteSnowLumaBotStatus`: `qq_id: str`, `running: bool`, `pid: int | None`, `uin: str | None`, `started_at: str | None`, `raw_payload: dict`
2. `SnowLumaRemoteRuntimeService` 类 (**不复用 NC `RemoteRuntimeService`**, 字段 schema 不同):
   - `get_daemon_status() -> RemoteSnowLumaDaemonStatus`: `cat status_daemon.json` 解析; fallback `test -f pid_daemon && kill -0 $(cat pid_daemon)`
   - `list_bots() -> list[str]`: `ls runtime/pid_bot_* 2>/dev/null` → 解析出 qq_id 列表
   - `get_bot_status(qq_id) -> RemoteSnowLumaBotStatus`
   - `tail_daemon_log(lines=200) -> RemoteLogTail` (复用 NC `RemoteLogTail` 数据类)
   - `tail_bot_log(qq_id, lines=200) -> RemoteLogTail`
   - `read_webui_secret() -> str`: `cat {workspace}/webui.secret` (缓存到 attribute 避免反复 SSH)
   - `read_vnc_secret() -> str`: 同上

#### `launcher.py`

1. `SnowLumaLauncherCommands` dataclass:
   - 构造 `(paths: SnowLumaRemotePaths)`
   - 属性/方法返回**string**命令 (由上层 backend.run 执行):
     - `daemon_script_path` = `f"{paths.workspace_dir}/snowluma_daemon_launcher.sh"`
     - `bot_script_path` = `f"{paths.workspace_dir}/snowluma_bot_launcher.sh"`
     - `daemon_start() -> str`: `f'bash "{self.daemon_script_path}" start'`
     - `daemon_stop() / daemon_status() / daemon_wait_ready(timeout=30)` 同理
     - `bot_start(qq_id) / bot_stop(qq_id) / bot_status(qq_id)`
   - **不**持 backend 引用, 只做命令组装; 执行由上层负责 (让单测零 SSH)
2. `SnowLumaLauncherDeployer` 类 (脚本上传职责):
   - `deploy_launchers(backend, templates_module) -> None`:
     - 渲染 `snowluma_daemon_launcher.sh` / `snowluma_bot_launcher.sh`
     - SFTP 上传到 `{workspace}/` (与 install 脚本同目录)
     - `chmod +x`
   - 被 W4 `deployment.install_all` 在 "framework 解压后" 调一次

### 单测覆盖

- `test_snowluma_remote_status.py`: Mock backend 返回固定 JSON, 断言 `get_daemon_status` / `list_bots` / `get_bot_status` 字段
- `test_snowluma_launcher.py`: `SnowLumaLauncherCommands.daemon_start()` 字符串 exactly 匹配预期 (quoting 不漏)

### 验证/回滚

- 单测全绿
- 回滚: 单 `git revert`

---

## W6 — `tunnels.py` (SnowLumaTunnelManager 双隧道 + 自愈)

### Owner boundary

- **新增**: `src/core/remote/snowluma/tunnels.py` (~250 行)
- **新增**: `script/test/test_snowluma_tunnels.py` (~120 行, mock paramiko transport)

### 实现要点

1. `SnowLumaTunnelManager(QObject)` 类:
   - 构造: `(ssh_client: SSHClient, remote_webui_port: int = 5099, remote_novnc_port: int = 6081, preferred_local_webui: int = 47099, preferred_local_novnc: int = 47609)`
   - 字段:
     - `_webui_forward: LocalPortForwarder | None`
     - `_novnc_forward: LocalPortForwarder | None`
     - `_webui_local_port: int | None` / `_novnc_local_port: int | None`
     - `_watchdog_timer: QTimer` (2s 心跳)
   - 方法:
     - `ensure_webui() -> int` (返回 local port, 已在则复用, 否则新建): 先试 `preferred_local_webui`, 端口被占走 `_find_free_port()` 随机 high port (20000-60000)
     - `ensure_novnc() -> int`
     - `close_all()`
     - `is_alive() -> bool`: `transport.is_active() and forwarder.is_alive()` for each
   - 信号:
     - `webui_tunnel_changed(int)` / `novnc_tunnel_changed(int)` — 端口号变化时 emit (UI 可刷新显示)
     - `tunnel_lost(str)` — watchdog 检测到断开
2. Watchdog 逻辑:
   - `_watchdog_timer.timeout` → 检查两条 forwarder, 发现掉 → emit `tunnel_lost("webui"/"novnc")` → 尝试 `ensure_*` 重建
   - 连续 3 次失败 → stop watchdog + emit `tunnel_lost("dead")` (上层决策是否重新 ensure)
3. 端口选择策略:
   - `_find_free_port()`: `socket.socket().bind(("127.0.0.1", 0))` → `getsockname()[1]` → close; 返回值
4. `LocalPortForwarder` 复用 `src/core/remote/tunnel.py` (核心功能不动; SL 场景同时起 2 个实例, 不冲突因为 paramiko transport 是 per-SSH 的, 同一 SSH connection 能挂多个 channel)

### 单测覆盖

- Mock `paramiko.Transport` 的 `request_port_forward` 与 `is_active`
- 场景: `ensure_webui` 首次成功 / 端口占用 fallback / watchdog 检测断开 / 重建成功 / 连续失败
- `webui_tunnel_changed` signal 被 emit 1 次且值 ≥ 1024

### 验证/回滚

- 单测全绿
- 回滚: 单 `git revert`

---

## W7 — `ServerProfile` schema 扩展 + `ServerRegistry` migration + 单测

### Owner boundary

- **修改**: `src/core/remote/servers.py` (加 `BackendFlavor` enum + `ServerProfile` 新字段 + migration)
- **修改**: `script/test/test_server_registry.py` (加 migration 场景)
- **新增**: `script/test/test_server_profile_snowluma_flavor.py` (~100 行)

### 实现要点

1. `BackendFlavor` 定义 (StrEnum, 值 = `"napcat"` / `"snowluma"`)
2. `ServerProfile` 新增字段 (全 Optional, default 向后兼容):
   - `backend_flavor: BackendFlavor = BackendFlavor.NAPCAT`
   - `snowluma_framework_version: str | None = None`
   - `snowluma_daemon_pid: int | None = None`
   - `snowluma_webui_tunnel_local_port: int | None = None`
   - `snowluma_vnc_tunnel_local_port: int | None = None`
3. `ServerRegistry._migrate_legacy_profile`:
   - 旧 profile 无 `backend_flavor` key → 默认 `NAPCAT`
   - 旧 profile 无 `snowluma_*` → 默认 None
   - 写回磁盘时以完整 schema 落盘 (让下次读取无 warn)
4. `ServerProfile.is_snowluma: bool` property (语法糖, 避免满屏 `== BackendFlavor.SNOWLUMA`)
5. `DeploymentState` 枚举**不变**; SL 与 NC 共享相同的 `PROBED / INSTALLED / RUNNING / ERROR` 状态机 (语义对上层透明)

### 单测覆盖

- 加载老 `servers.json` (无 backend_flavor), 0 warning
- 保存后再加载, 所有新字段有默认值
- 修改 `backend_flavor` 存/读往返

### 验证/回滚

- `python -m pytest script/test/test_server_registry.py script/test/test_server_profile_snowluma_flavor.py -v` 全绿
- 回滚: 单 `git revert`; 磁盘已写入的新字段在旧代码下被 json 忽略不报错 (`@dataclass` 的 `from_dict` 需忽略未知 key, 已有; 若无则补上)

---

## W8 — `ServerManager` 扩远端 SL 方法 + Qt 信号 + 远端编排集成

### Owner boundary

- **修改**: `src/core/remote/server_manager.py` (追加 SL 方法, 不改 NC 方法)
- **修改**: `src/core/remote/__init__.py` (re-export `SnowLumaRemotePaths`, `BackendFlavor`)
- **新增**: `script/test/test_server_manager_snowluma.py` (~250 行, mock SSH backend + tunnel + deployment)

### 实现要点

1. **新增字段** (manager instance 级):
   - `_snowluma_deployments: dict[str, SnowLumaRemoteDeployment]` keyed by `server_id`
   - `_snowluma_runtime_services: dict[str, SnowLumaRemoteRuntimeService]`
   - `_snowluma_tunnel_managers: dict[str, SnowLumaTunnelManager]`
   - `_snowluma_launcher_commands: dict[str, SnowLumaLauncherCommands]`
2. **新增方法** (对应 requirement §2.4 清单):
   - `probe_snowluma(server_id: str) -> None` (异步, 发 `snowluma_probe_finished_signal(server_id, report_dict)`)
   - `deploy_snowluma(server_id)` 异步分阶段: `ensure_workspace → install_all → deploy_launchers → sync_daemon_configs`; 每阶段用 `snowluma_deploy_progress_signal(server_id, phase, percent, message)` 发送进度
   - `start_snowluma_daemon(server_id)`: SSH 调 `launcher.daemon_start()` → 轮询 `launcher.daemon_wait_ready()` → `tunnel_manager.ensure_webui()` → `ensure_novnc()` → emit `snowluma_daemon_state_signal(server_id, "ready")`
   - `stop_snowluma_daemon(server_id)`: `launcher.daemon_stop()` → `tunnel_manager.close_all()` → emit `snowluma_daemon_state_signal(server_id, "stopped")`
   - `start_snowluma_bot(server_id, qq_id)`: 前置 daemon 必须 ready; `launcher.bot_start(qq_id)` → 读 `pid_bot_<qq_id>` → emit `snowluma_bot_state_signal(server_id, qq_id, "started", pid=...)`
   - `stop_snowluma_bot(server_id, qq_id)`: `launcher.bot_stop(qq_id)` + emit
   - `sync_snowluma_onebot(server_id, qq_id, uin, config)`: 调 `deployment.sync_onebot_config(...)` → 上层可紧接 `SnowLumaWebUIClient.update_onebot_config(uin, ...)`
   - `open_snowluma_vnc(server_id) -> None`: 确保 daemon ready + novnc 隧道 + 读 `vnc.secret` → `webbrowser.open(f"http://127.0.0.1:{local_port}/vnc.html?autoconnect=1&password={secret}")`
   - `tail_snowluma_daemon_log(server_id, lines=200) -> RemoteLogTail` (同步, Qt widget 直接调)
   - `tail_snowluma_bot_log(server_id, qq_id, lines=200)` 同上
3. **Qt 信号** (全部新增, NC 信号不改):
   - `snowluma_probe_finished_signal = Signal(str, dict)`
   - `snowluma_deploy_progress_signal = Signal(str, str, int, str)` # server_id, phase, percent, message
   - `snowluma_deploy_finished_signal = Signal(str, bool, str)` # ok, message
   - `snowluma_daemon_state_signal = Signal(str, str)` # state ∈ {stopped,starting,ready,stopping,crashed}
   - `snowluma_bot_state_signal = Signal(str, str, str, object)` # server_id, qq_id, event, extra_payload(pid/uin/err)
   - `snowluma_webui_tunnel_changed_signal = Signal(str, int)` # server_id, local_port
   - `snowluma_vnc_tunnel_changed_signal = Signal(str, int)`
4. **线程模型**:
   - 所有阻塞 SSH 调用走 `dispatch_remote_ssh(server_id, callable, on_done, on_error)` (复用 NC 的 `thread_pool.py`)
   - 信号 emit 在主线程 (`QMetaObject.invokeMethod` 或 `dispatch_remote_ssh` 已内置)
5. **崩溃传播**:
   - `_snowluma_tunnel_managers[sid].tunnel_lost` → 桥 到 `snowluma_daemon_state_signal(sid, "crashed")`
   - daemon_launcher 启动失败 → `snowluma_daemon_state_signal(sid, "crashed")` + `snowluma_deploy_finished_signal(sid, False, msg)`

### 单测覆盖

- Mock deployment / launcher / tunnel_manager, 断言:
  - `deploy_snowluma` 编排顺序 (ensure_workspace → install_all → deploy_launchers → sync_daemon_configs)
  - `start_snowluma_daemon` 发 2 个 signal (daemon_state "starting" → "ready") 且建 2 条隧道
  - `tunnel_lost` 传播 crashed state
  - `sync_snowluma_onebot` 正确调 render + SFTP

### 验证/回滚

- 单测全绿
- 回滚: 因 W9 依赖此 Wave, 需联动 revert; W7 / W1-W6 保留不影响

---

## W9 — `SnowLumaDaemon` + `SnowLumaDriver` 远端多态 + `BotProcessManager` dispatch

### Owner boundary

- **修改**: `src/core/runtime/snowluma_daemon.py` (加 `RuntimeTarget` 分支; 从单例改 registry)
- **修改**: `src/core/runtime/snowluma_driver.py` (start/stop 按 `config.runtime_target` 分流)
- **修改**: `src/core/runtime/bot_process_manager.py` (SL 远端 dispatch 入口接入)
- **新增**: `script/test/test_snowluma_daemon_remote.py` (~200 行)

### 实现要点

1. **`SnowLumaDaemon` 多态**:
   - 新 attr `_target_key: tuple[RuntimeTarget, str]` (LOCAL 键 = `(LOCAL, "")`, remote 键 = `(REMOTE, server_id)`)
   - Daemon **registry** (替代单例): `SnowLumaDaemonRegistry(QObject)` 持 `dict[tuple, SnowLumaDaemon]`, 用 `creart` 注册 **registry** 而非 daemon 本身
   - `ensure_running()` 在 REMOTE 分支:
     - 调 `server_manager.start_snowluma_daemon(server_id)` → 监听 `snowluma_daemon_state_signal(sid, "ready")` (事件/future 化)
     - 拿到 `_snowluma_webui_tunnel_local_port` 后构造 `SnowLumaWebUIClient("127.0.0.1", local_port, password=server_manager.read_webui_secret(server_id))`
     - `client.wait_ready(30)` + `client.login()`
     - state → READY, 返 client
   - `release()` 在 REMOTE: 不 kill 远端 node, 只清本地 client + 记 ref_count (远端 daemon 持久)
2. **`SnowLumaDriver.start_async(config, start_mode, attach_pid)`**:
   - 前置: `runtime_target = config.runtime_target` (LOCAL / `remote_<server_id>`)
   - 远端分支:
     - `daemon = daemon_registry.for_target(REMOTE, server_id).ensure_running()` (client 持 host=127.0.0.1, port=tunnel)
     - `server_manager.start_snowluma_bot(server_id, qq_id)` → 拿 pid
     - `daemon.webui_client().load_process(pid)` (通过隧道)
     - 启动 `SnowLumaStatusPoller(qq_id, initial_pid=pid, webui_client=daemon.webui_client())` — 与本地一致
   - **禁用 HOT**: 远端 start_mode=HOT 直接 `raise ValueError("远端 SnowLuma 不支持 HOT 模式")`
3. **`SnowLumaDriver.stop(qq_id)` 远端**:
   - `daemon.webui_client().unload_process(model.qq_pid)`
   - `server_manager.stop_snowluma_bot(server_id, qq_id)`
   - `daemon.release()` (不 kill 远端)
4. **`BotProcessManager.start_bot(config)` dispatch**:
   - 现有 `is_remote` 分支仅 NC → 改成:
     ```python
     if is_remote:
         if config.backend == BackendType.NAPCAT:
             self._remote_napcat_runtime_service.start(...)
         else:  # SNOWLUMA
             self._snowluma_driver.start_async(config, start_mode, attach_pid)  # driver 内部分流
     else:
         self._snowluma_driver.start_async(config, start_mode, attach_pid)
     ```
   - 同样思路改 `stop_bot`
5. **错误传播**:
   - `snowluma_daemon_state_signal(sid, "crashed")` → 对该 server 所有正在跑的 SL Bot emit `process_changed_signal(qq_id, NotRunning)` + `notification_signal("error", "SnowLuma daemon 远端崩溃, 相关 Bot 已停止")`

### 单测覆盖

- Mock `ServerManager` + `SnowLumaWebUIClient`, 断言:
  - `ensure_running` 在 REMOTE 模式调 `start_snowluma_daemon` + 建 client + login 顺序
  - `release` 在 REMOTE 模式**不**调 `stop_snowluma_daemon`
  - `start_async` REMOTE + HOT raise
  - daemon crashed 信号传播到所有依附 Bot

### 验证/回滚

- `python -m pytest script/test/test_snowluma_daemon.py script/test/test_snowluma_daemon_remote.py script/test/test_snowluma_driver.py -v` 全绿 (本地测试矩阵不回归)
- 回滚: 需与 W8 联动 revert

---

## W10 — UI 改动: AddServer / Remote Ops / BotCard / Home 角标

### Owner boundary

- **修改**: `src/ui/page/add_server_page/` 或对应 dialog (加 flavor 下拉)
- **修改**: `src/ui/page/remote_operation_page/` (加 SnowLuma 子页, 与现 NC 子页并列)
- **修改**: `src/ui/page/bot_page/widget/card.py` (WebUI 预览按钮 + 扫码按钮远端路径)
- **修改**: `src/ui/page/home_page/` (远程服务器卡片按 backend_flavor 角标)
- **修改**: `src/ui/components/` 如有 ChooseConfigTypeDialog (与 daemon-refactor §2.6 协同)
- **新增**: `script/test/test_ui_snowluma_remote.py` (UI level smoke, 走 `QTest`)

### 实现要点

1. **AddServer dialog / page**:
   - 顶部新增 `ComboBox` "后端种类": `NapCat (默认)` / `SnowLuma`
   - 选 SnowLuma 时动态 disable 不适用字段 (如 `napcat_version` 手动覆盖)
   - 保存时把 flavor 值写入 `ServerProfile`
2. **Remote Operation page** 加 SnowLuma 子页:
   - 路由判断: 当前 ServerProfile.flavor == SNOWLUMA 时显示此子页 (NC 子页隐藏)
   - 上部 3 卡片 (图形栈状态 / LinuxQQ 状态 / 框架版本), 订阅 `snowluma_probe_finished_signal`
   - 中部 daemon 控制区: 状态灯 + Start/Stop/Restart 按钮, 订阅 `snowluma_daemon_state_signal`
   - 日志 tab: QTabWidget 含 daemon / 各 bot 子 tab, 订阅 `tail_snowluma_*_log` 定时拉 (5s 间隔)
   - "部署"按钮触发 `deploy_snowluma`, 进度条绑 `snowluma_deploy_progress_signal`
3. **BotCard 远端分支**:
   - WebUI 预览按钮: `config.runtime_target == LOCAL` 用本地 `5099`; 远端用 `ServerProfile.snowluma_webui_tunnel_local_port`
   - 新按钮 "扫码登录(远端)" (用 `FluentIcon.SCAN_QRCODE` 或类似): 仅 SL 远端 + state=waiting_for_qr_scan 时 enable, 点击调 `server_manager.open_snowluma_vnc(server_id)`
4. **Home 卡片角标**:
   - ServerCard 右上角小 icon 区分 flavor (NapCat 徽标 / SnowLuma 徽标)
5. **ChooseConfigTypeDialog / AdvancedConfigWidget 协同**:
   - 远端 + SnowLuma 组合下**禁用 HOT** (灰出 radio + tooltip "远端不支持 HOT 模式")

### 单测/UI 测试

- UI 层用 `QTest` 模拟点击 AddServer → flavor=SnowLuma → 保存, 验证 ServerProfile 入库
- 启动 ResourcePage, mock 信号发射, 验证状态卡片刷新
- 非 UI 侧: W8/W9 的信号 payload 已测, UI 只需"连线对了即可"

### 验证/回滚

- 手工跑 Desktop 过所有 UI 路径; 单 `git revert` 回滚 (UI 与 core 解耦, 不影响后台功能)

---

## W11 — 集成 smoke + 手工验收 + phase_cleanup

### Owner boundary

- **新增**: `docs/guides/snowluma-remote-setup-guide.md` (~200 行用户侧 onboarding 文档)
- **新增**: `script/test/integration/test_snowluma_remote_smoke.py` (~300 行, 需要真实 VM, 默认 `@pytest.mark.skipif` 无 `SL_REMOTE_SMOKE=1` 跳过)
- **验收证据收集**: 截图 / SSH session 日志 / Desktop 日志 附到 PR description

### 实现要点

1. **集成 smoke 步骤** (需要 1 台 Debian 12 amd64 VM, SSH 端口 22 或自定义):
   - `export SL_REMOTE_SMOKE=1 SL_REMOTE_HOST=192.168.x.x SL_REMOTE_USER=...`
   - pytest 自动跑 7 个 AC 场景 (requirement §4.1 AC1-AC10 选代表性 7 个):
     - AC1 (一次部署成功)
     - AC2 (daemon 持久)
     - AC3 (WebUI 隧道)
     - AC4 (冷启动 Bot + 扫码, 本测**半自动**: pytest 阻塞等用户手扫二维码, 10 min 超时)
     - AC5 (远端多 Bot)
     - AC7 (crash 传播)
     - AC9 (config 热重载)
   - 每个 AC 打印关键 `ps aux` / `netstat` / `curl` 输出到 pytest log
2. **手工 smoke 清单** (开发者执行 7 项, 截图留档):
   - 与 requirement §4.3 一致
3. **User onboarding 文档** `docs/guides/snowluma-remote-setup-guide.md`:
   - "准备一台 Linux 服务器"需求清单 (Debian 12+ / Ubuntu 22+ / RHEL 9+; amd64/arm64; SSH 能连; 有 apt/yum; 能 sudo 或 root)
   - Add Server → 选 SnowLuma → Deploy → Start daemon → 新建 Bot → 扫码 流程截图
   - 常见错误: apt 锁 / node 版本低 / 防火墙 5099 / SFTP 超时
4. **phase_cleanup**:
   - `.nexus-map/` 如有则更新 `systems.md` 新增 `snowluma_remote` 节点
   - `docs/CHANGELOG.md` 新增 entry "feat(remote): SnowLuma 远程管理 (原生 Linux)"
   - requirement 文档 frontmatter 改 `status: delivered` (PR merge 时)
   - plan 文档 frontmatter 改 `status: complete`

### 验证/回滚

- 所有 AC 通过; 手工 smoke 截图齐全
- 回滚: 整个任务 `git revert HEAD~N..HEAD` (但项目未上线, 大概率不需要)

---

## Open Questions (已答, 2026-05-11)

> 6 个问题已全部裁决, 答案落 plan 与对应 Wave 实现要点; 此区块作为决策日志保留, 后续若用户回退某项决策可对照修订.

### OQ1 (W3 lite tarball whitelist) — **已修订 plan**

**事实校正**: SL 上游 6 packages, 实际产物结构与原假设不符:
- `packages/core/vite.config.ts` 的 `outDir` 指向**monorepo 根 `dist/`** (非 `packages/core/dist/`)
- `packages/runtime/native/` 已是预编译 12 个 `.node`/`.so`/`.dll` (Linux/Win × x64/arm64 + ffmpeg)
- `packages/sdk/` 是用户侧 SDK, daemon 不需要
- `packages/websocket/` 走 `process.dlopen()` 加载 `runtime/native/websocket-*.node`, 无独立 dist

**决策**: W3 §"实现要点 1.步骤 2-3" 已按真实结构修订 whitelist; 严格排除 win32/darwin native 与 sdk

### OQ2 (W4 node 安装 fallback) — **已纳入 W2 脚本**

**事实**: 国内 VPS curl `deb.nodesource.com` 易超时.

**决策**: `install_snowluma.sh.j2` 内置 3 级 fallback (W2 脚本契约段补充):
1. 已装 node ≥ 22 → 跳过
2. apt 直装 nodejs, 检测版本: ≥ 22 → ok; < 22 → 走 step 3
3. nodesource setup_22.x (`curl --max-time 10 -fsSL ...`); 失败 → `[PROGRESS] 100 ERROR_NODE_VERSION_TOO_LOW` + 退 1, UI 提示用户手装

行 ~25 shell 增量, 不变需求文档

### OQ3 (LinuxQQ `.deb` 跨 flavor 共享) — **首版独立**

**事实**: NC 落 `~/napcat-remote/packages/*.deb`; SL 默认 `~/snowluma-remote/`; workspace 互斥.

**决策**: 首版**各装一份** (~200MB 远端磁盘冗余可接受). 共享方案 (`~/.linuxqq-deb-cache/`) 进 backlog, 理由:
- D8 已锁 ServerProfile per-server flavor 互斥, 共享会污染抽象边界
- 200MB 远端磁盘对 VPS 通常不构成压力
- 跨 workspace lock 协调成本 > 节省的上传时间

### OQ4 (VNC URL password 安全) — **首版 query, 文档缓解**

**事实**: noVNC 支持 `?password=` query 与 localStorage; query 进浏览器历史/可能 leak referer.

**决策**: 首版维持 URL query, 缓解 3 项写入 W11 用户文档:
- `vnc.secret` 文件 mode 600, 仅本机读
- 文档 caveat: "扫码完成后请关闭扫码 tab, 避免 URL 残留"
- W11 backlog: ephemeral token (5min TTL) + daemon 启动时随机轮换

### OQ5 (扫码图标) — **复用 FluentIcon.SCAN**

**事实**: 项目用 `qfluentwidgets`, 自带 `FluentIcon.SCAN` / `.QRCODE`; 自定义 icon 资源 grep 0 命中.

**决策**: W10 BotCard 用 `FluentIcon.SCAN`, 按钮文案 `"扫码登录(远端)"`. 不做自定义资源.

### OQ6 (crash 状态 2s 延迟) — **维持 watchdog**

**事实**: `SnowLumaTunnelManager` 2s 心跳 + Qt 信号 ~50ms, 总延迟 ~2s.

**决策**: 维持现 2s 心跳模型, 不加"立即 probe"触发. 理由:
- 用户人眼 2s 几乎不可察觉
- 立即 probe 需在 daemon stderr 流监听层接信号, 增加 SSH 复杂度
- 极端情况 (用户连续点击) 已隐含 `tunnel_manager.is_alive()` 同步快速路径
- 若 W11 smoke 期间暴露体感问题, 再补 immediate probe trigger

---

## 估算与里程碑

| 里程碑      | 期望完成时间 | 标志                                          |
| ----------- | ------------ | --------------------------------------------- |
| M1 (W1-W3)  | D+3          | 资源基座完成, lite tarball 可生成, 模板可渲染 |
| M2 (W4-W6)  | D+7          | 远端子包 3 个核心类可独立单测通过             |
| M3 (W7-W8)  | D+10         | `ServerManager` SL 路径闭环, 可跑 mock 部署   |
| M4 (W9-W10) | D+14         | Desktop UI 全端连通, 进入 dogfooding          |
| M5 (W11)    | D+17         | 集成 smoke 通过, PR 就绪                      |

> D 指 plan 被批准进入 `plan_execute` 的日期; 日均按 2-3 小时 L grade 吞吐估算. 若 W4 遇 OQ2/OQ3 需多轮沟通, M2 可能 +2 天.

---

## 回归风险清单 (Smoke Focus)

- **RR1** NC 远端部署 100% 不回归: W8 追加 SL 方法时 `server_manager.py` 的 `deploy_server` / `probe_server` / `start_remote_runtime` 不触碰, pytest `test_server_manager.py` NC 场景全绿
- **RR2** 本地 SnowLuma 路径 100% 不回归: W9 改 `snowluma_daemon.py` 时 LOCAL 键路径保持原行为, `test_snowluma_daemon.py` (W1 from daemon-refactor) 单测全绿
- **RR3** UI 首页远程卡片加载: 混合 flavor 的 servers.json (1 NC + 1 SL) 显示两张卡片, 图标区分
- **RR4** 打包体积回归: PyInstaller 产物 W3 之后应 +20-40MB (lite tarball); NC installer 的 NC .deb 数量不变
- **RR5** 非 Windows 开发者工作流: SL 远端脚本的 `.sh.j2` 模板在 macOS dev 机上渲染 + `bash -n` 检查能通过 (W2 单测)

---

## 附录: Wave 粒度 Git Commit 模板

每个 Wave 一条原子 commit, 消息格式:

```
<type>(remote-snowluma): W<N> <短标题>

- 详细变更 1
- 详细变更 2
- 详细变更 3

Refs: docs/plans/2026-05-11-snowluma-remote-management-execution-plan.md#w<n>
```

`<type>` 选取:
- W1/W5/W6/W7: `feat`
- W2/W3/W10: `feat` (+资源/UI)
- W4/W8/W9: `feat` (核心路径)
- W11: `docs` + `test`
