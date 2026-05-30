---
requirement_id: 2026-05-11-snowluma-daemon-refactor
status: frozen
owner: @NapNeko/NapCatQQ-Desktop
supersedes: docs/requirements/2026-05-10-snowluma-bot-form-backend-aware.md §2.15 单实例守护条款
related_plans:
  - docs/plans/2026-05-11-snowluma-daemon-refactor-execution-plan.md
---

# SnowLuma Daemon 解耦重构 · 需求冻结

> 把 Desktop 对 SnowLuma 的建模从 "1 Bot = 1 独立 SL node" 改为 "N Bot 共享 1 SL daemon, 每 Bot 挂 1 个 QQ 会话 (UIN)", 对齐上游原生架构.

---

## 0. 现状对照 (Background)

### 0.1 上游 SnowLuma 原生架构 (证据链)

上游 `example/SnowLuma-main/` 里 SnowLuma 是 **单 node 进程 = 全局控制面**, 天然注入多个 QQ:

- `packages/core/src/index.ts:9-15` 一次性创建 `NtqqHandler` / `BridgeManager` / `OneBotManager` / `HookManager` 各 **1 份**, 不随 QQ 个数伸缩
- `packages/core/src/hook/hook-manager.ts:59` `states = new Map<pid, HookProcessState>()` — HookManager 天然管 **N 个 QQ PID**, 后台 `tickWatcher()` (1.5s 间隔) 扫 `listHookProcesses()` + `QqHookClient.listLivePipes()` 自动增删
- `packages/core/src/hook/injector.ts:99-108` `listHookProcesses()` 返回 `[...new Set(addon.getAllMainProcess())]`; native addon 可能返回多个 main PID (QQ.exe Electron 多进程), Set 去重后按 PID 整数排序
- `packages/core/src/bridge/manager.ts:22-26` `sessions_: Map<uin, QQSession>` — **UIN 才是 Bridge session 主键**; `pidToUin_: Map<pid, uin>` + `pidPacketClients_: Map<pid, sender>` 支持同 UIN 多 PID 聚合 (`bridge.attachPid(pid)` / `bridge.detachPid(pid)`)
- `packages/core/src/onebot/manager.ts:11-18` `instances: Map<uin, OneBotInstance>` — 一 UIN 一 OneBot 实例, 由 `bridgeManager.setSessionStartedCallback((uin, qqInfo, bridge) => ...)` 驱动
- `packages/core/src/webui/server.ts` WebUI (port 来自 `config/runtime.json`) 暴露 `/api/processes` (全 PID) 与 `/api/qq-list` (全 UIN 实例), 两个端点都是全局视图
- `packages/core/config/runtime.json` 与 `webui.json` 是**全局单份**配置, 只归属于 daemon; `onebot_<uin>.json` 则按 UIN 渲染

**结论**: 上游拓扑为 `(1 SL node) → (N QQ PID) → (M UIN session) → (M OneBot instance)`, PID 与 UIN **多对一**, UIN 与 OneBot 实例 **一对一**.

### 0.2 当前 Desktop 错位点 (代码证据)

Desktop 端把 SL node 绑成 per-Bot 进程, 与上游拓扑**倒置**:

- `@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\runtime\snowluma_driver.py:165-177` `SnowLumaDriver._processes: dict[qq_id, SnowLumaProcessModel]` + `_pollers: dict[qq_id, SnowLumaStatusPoller]` — 按 Bot 维度持 node.exe + webui_client + poller
- `@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\runtime\snowluma_driver.py:318-323` `if self._processes: raise RuntimeError("一期仅支持 1 个 SnowLuma Bot")` — 硬编码单实例守护, 因为 `webuiPort=5099` 固定, 起 2 个 node 必撞端口
- `@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\runtime\snowluma_driver.py:521-526` `SnowLumaWebUIClient(host=..., port=_SNOWLUMA_WEBUI_PORT, password=model.effective_password)` — 每个 Bot 各自建 client、各自 login, **共用端口但不共享会话**
- `@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\runtime\snowluma_driver.py:683-765` `_render_configs()` 每次启 Bot 都**重写** `runtime.json` + `webui.json`, 把本该是 daemon 级的全局配置塞进 Bot 启动路径
- `@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\runtime\snowluma_status_poller.py:138-147` poller 构造参数 `(qq_id, qq_pid, webui_client)` — 锁 1 个 PID
- `@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\runtime\snowluma_status_poller.py:251-254` `next((p for p in processes if p.pid == self._qq_pid), None)` — 只看匹配 `qq_pid` 的**单条**记录. 当 QQ.exe 衍生多 main PID (Electron 多窗/重登) 时, UIN 已经登录但我们恰好不扫匹配的那条记录 → 状态退化成 "(无 process)" fallback, UIN 检测经常靠 `/api/qq-list` 兜底
- `@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\config\config_model.py:531-534` `BotConfig.snowluma_webui_password_override` — **per-Bot 密码字段**, 与上游全局密码语义冲突; 启动多 Bot 会把后来者的密码覆写到 daemon, 先起来的 Bot 静默失效
- `@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\runtime\snowluma_session.py:143` `resolve_effective_password(config)` 依赖 `BotConfig`, 绑死了"密码归属某个 Bot"的错误语义

### 0.3 实际问题 (用户反馈)

- **多 Bot 根本起不起来**: 第 2 个 SnowLuma Bot 直接被单实例守护 `RuntimeError` 拒绝
- **资源浪费**: 若解除守护, 每 Bot 各起 1 份 node.exe 包含 Bridge / OneBot / WebUI 全家桶, 但它们都要抢 `:5099`
- **语义偏离上游**: 1 个 SL node 天然能注入 N 个 QQ, Desktop 主动扔掉了这个能力, 甚至把 UIN 检测弄复杂 (需要 `/api/processes` + `/api/qq-list` 双源 fallback)
- **QQ 多 PID 场景的 poller 退化**: QQ.exe Windows 版 (Electron 架构) 有多个 main-ish 进程, 注入后 `/api/processes` 可能返回不止一条, poller 按 `qq_pid` filter 可能命中 "子进程 PID" 而非 "登录 PID", 状态翻译链路弱化

---

## 1. 目标 (Goal)

### 1.1 把 SL node 提升为 App 级 daemon

- 全局单例 `SnowLumaDaemon`: 包含唯一的 `node.exe` 子进程 + 唯一的 `SnowLumaWebUIClient` + 唯一的 `runtime.json` / `webui.json` 渲染点
- **持久 daemon 生命周期 (2026-05-11 设计变更, 详见 plan §DC-1)**: 首个 SnowLuma Bot 启动时 `ensure_running()` 拉起 daemon; 后续 `release()` 仅扣 `ref_count` 用于诊断, **不**触发 terminate; daemon 一直活到 App 退出, 由 `QApplication.aboutToQuit` 钩子调 `daemon.shutdown()` 优雅清理. 取舍: 启过一次后 ~100MB 常驻, 但反复启停 Bot 无 30s spawn 等待
- **上层 Bot 不再 spawn node, 也不再持 WebUI client**; Bot 只持 QQ.exe (COLD 模式) + 注入 PID + per-session poller

### 1.2 Bot session 按 UIN 聚合 (支持多 PID)

- `SnowLumaBotSession` 主键 = `uin`, 次要字段 `primary_pid` (driver 显式 inject 的那个) + `ancillary_pids: set[int]` (watcher 自动发现的同 UIN 其他 PID)
- `SnowLumaStatusPoller` 从 "按 `qq_pid` 筛选" 改为 "按 `uin` 聚合", 命中多条 PID 时合成状态 (任一 online 即 online; 全 disconnected 才 disconnected)
- 上游 `/api/qq-list` 作为 UIN 索引的**主源**, `/api/processes` 仅用于辅助显示 PID 列表 (对齐上游 `bridge.attachPid` / `bridge.detachPid` 语义)

### 1.3 WebUI 密码迁到 App 级

- 密码 override 字段从 `BotConfig.snowluma_webui_password_override` **彻底删除**, 迁到 App 级 (如 `config/config.json` 的 `app.snowluma.webui_password_override` 或新建 `config/snowluma.json`, 见 §2.4 选型)
- `resolve_effective_password()` 不再吃 `BotConfig`, 改吃 App 配置
- daemon 启动时一次性渲染 `webui.json`; per-Bot 启动不再碰 `webui.json`

### 1.4 项目未上线, 不留兼容层

- 用户明确: 项目未上线 (Q3 回答), 不保留 legacy override 字段读路径, 只做一次性迁移 (旧 bot.json 的 `snowluma_webui_password_override` 在 `_migrate_legacy_*` 里**搬到** App 配置后**删除**, 多 Bot 冲突时以最后一个非空值为准并 log warn)
- `BOT_CONFIG_COMPAT_VERSION` 升 `v2.0 → v2.1` (删字段是 breaking change)

---

## 2. 交付物 (Deliverable)

### 2.1 新建模块: `src/core/runtime/snowluma_daemon.py`

新增文件, 约 400 行, 职责:

- 类 `SnowLumaDaemon`: 进程级单例 (通过 `creart` 注册), 字段含:
  - `_node_process: QProcess | None` — 唯一的 SL node.exe
  - `_webui_client: SnowLumaWebUIClient | None` — 共享 client, login 一次, 持 Bearer token
  - `_ref_count: int` — 当前挂着的 Bot 数
  - `_state: DaemonState` enum (`STOPPED` / `STARTING` / `READY` / `STOPPING` / `CRASHED`)
- API:
  - `ensure_running() -> SnowLumaWebUIClient` (阻塞**主线程 ≤ 10s** 或 async variant): 若 `STOPPED` 则 spawn node + wait_ready + login; 若 `READY` 直接返回 `_webui_client`; `ref_count` +1
  - `release()`: `ref_count` -1; 归 0 则 fire-and-forget logout + terminate_async(node) + 清 state 回到 `STOPPED`
  - `webui_client() -> SnowLumaWebUIClient` 访问器, state != READY 时 raise
  - `is_running() -> bool`
  - Qt 信号 `crashed(str)` — node.exe 意外 finished; 供 BotProcessManager 通知所有依附 Bot
- **不**负责渲染 `onebot_<uin>.json` (归 Bot session)
- 单元测试 `script/test/test_snowluma_daemon.py`: mock QProcess + mock WebUI server, 覆盖 ensure_running 幂等 / ref_count / crash 传播

### 2.2 重构 `src/core/runtime/snowluma_driver.py`

- **删除** `SnowLumaDriver._processes` 单实例守护 (`@...\snowluma_driver.py:318-323`) 的 `RuntimeError`
- **删除** `_SnowLumaPhaseBCWorker` 里的 "WebUI ready + login" 逻辑 (归 daemon)
- `SnowLumaProcessModel` 字段裁剪:
  - 保留: `qq_id`, `qq_process`, `qq_pid`, `state`, `started_at`, `dead_event`
  - 删除: `node_process`, `webui_client`, `auth_token`, `effective_password` (归 daemon / App 配置)
  - 新增: `uin: str = ""` (Phase D poller 首次 UIN 探测后回填), `ancillary_pids: set[int] = field(default_factory=set)`
- `SnowLumaDriver.start_async(config, *, start_mode, attach_pid)` 路径重写:
  - Phase A: `daemon = it(SnowLumaDaemon); client = daemon.ensure_running()`; spawn QQ.exe (COLD) 或 validate attach_pid (HOT); 构造 `SnowLumaProcessModel`
  - Phase B: **删除** (WebUI ready + login 归 daemon)
  - Phase C: `client.load_process(model.qq_pid)` 注入
  - Phase D: 启动 `SnowLumaStatusPoller(qq_id, initial_pid=model.qq_pid, webui_client=client)`
- `SnowLumaDriver.stop(qq_id)`:
  - 停 poller
  - `client.unload_process(model.qq_pid)` (fire-and-forget)
  - COLD 模式 `terminate_async(model.qq_process)`; HOT 模式不动
  - `daemon.release()` — 若本 Bot 是最后一个, daemon 自动关 node
- `_render_configs()` 拆成 **两个函数**:
  - `daemon.render_globals()` (渲染 `runtime.json` + `webui.json`, daemon 启动时调 1 次, 读 App 级密码)
  - `session.render_onebot_for(config)` (渲染 `onebot_<uin>.json`, 每 Bot 启动时调, 签名: `(snowluma_path, qqid, connect, music_sign_url)`, 保持现有 `render_onebot_json` 不变, 只是调用点挪到 driver)
- 旧 `build_node_process()` 方法迁到 `snowluma_daemon.py`

### 2.3 重构 `src/core/runtime/snowluma_status_poller.py`

- 构造签名改 `(qq_id: str, initial_pid: int, webui_client: SnowLumaWebUIClient, parent=None)`; `initial_pid` 只用于**第一次 UIN 探测**, 拿到 UIN 后 poller 内部 lock 到 UIN, 不再 filter PID
- `_on_processes(processes, qq_instances)` 改造:
  - 第一次 tick: 按 `initial_pid` 找 PID → 拿 UIN; 拿不到时 fallback `qq_instances[0].uin` (保持 W7 双源兜底)
  - 首次 UIN 确定后: 把本 poller 关联 `self._uin = <uin>`, 并 emit `uin_detected`
  - 后续 tick: **按 UIN 聚合**:
    - `matched_processes = [p for p in processes if p.uin == self._uin]`
    - 合成状态: 有任一 `online` → `logged_in`; 全 `disconnected` → `disconnected`; 有 `loaded` 无 `online` → `waiting_for_qr_scan`; 其他中间态 → `starting`
    - 当 `matched_processes` 为空但 `qq_instances` 匹 UIN 非空 → 降级 `logged_in` (W7 fallback 语义保持)
  - `matched_processes` 的 `pid` 集合回填到 `SnowLumaProcessModel.ancillary_pids` (通过新增 signal `pid_set_changed: (qq_id, list[int])`, driver 在主线程接 + 写 model); `primary_pid` 即 `model.qq_pid` 不变
- 输出信号保留: `state_changed(qq_id, state_name)`, `uin_detected(qq_id, uin)`; 新增 `pid_set_changed(qq_id, list)`
- `_POLL_INTERVAL_MS` 保持 2000; `_MAX_CONSECUTIVE_FAILURES` 保持 3

### 2.4 App 级配置: 新增 `AppSnowLumaConfig`

- `src/core/config/config_model.py` 新增模型 (推荐挂在已有 `AppConfig` / `GeneralConfig` 之下, 而非单独文件; 避免新建 `config/snowluma.json` 带来的 `creart` 单例复杂度):
  ```python
  class AppSnowLumaConfig(BaseModel):
      """App 级 SnowLuma 全局设置 (daemon 共享)."""
      webui_password_override: str = Field(default="", description="空=走 snowluma-session.json 自动生成")
  ```
- 挂到 `AppConfig.snowluma: AppSnowLumaConfig = Field(default_factory=AppSnowLumaConfig)`
- `BotConfig.snowluma_webui_password_override` **删除** (@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\core\config\config_model.py:531-534)
- `_migrate_legacy_*`:
  - 检测旧 bot.json 的 `snowluma_webui_password_override` 非空 → 搬到 `AppConfig.snowluma.webui_password_override` (多 Bot 冲突时: 取第一个非空值, 其余 log warn); 然后从 bot.json 剔除该字段
  - `rules_applied.append("bot.snowluma_webui_password_override migrated to app.snowluma.webui_password_override")`
- `BOT_CONFIG_COMPAT_VERSION` 升 `v2.0 → v2.1`

### 2.5 重构 `src/core/runtime/snowluma_session.py`

- `resolve_effective_password()` 签名从 `(config: Config)` 改为 `(*, override: str = "")`:
  ```python
  def resolve_effective_password(*, override: str = "") -> str:
      override = (override or "").strip()
      if override:
          return override
      session = load_session()
      if session is None:
          session = create_session()
      return session.password
  ```
- 调用点: 只剩 `SnowLumaDaemon.ensure_running()` (首次启动) 与 UI 预览 (`@d:\NapCat-Project\NapCatQQ-Desktop-V1\src\ui\page\bot_page\widget\card.py:556-562` 改读 `AppConfig.snowluma.webui_password_override`)

### 2.6 UI 改动

- **组件页 SnowLuma tab** (`src/ui/page/setup_page/` 或组件页实际路径, 待 W6 落位时确认): 新增一张 "全局 WebUI 密码 override" 输入卡片, 双向绑定 `AppConfig.snowluma.webui_password_override`
- `src/ui/page/bot_page/widget/config.py:288-314`:
  - `save_config` 不再写 `snowluma_webui_password_override`
  - `fill_value` 不再读该字段
  - 删 `snowluma_webui_password_card` 整张卡片
- `src/ui/page/bot_page/widget/card.py:556-562` WebUI 预览按钮: 改读 `AppConfig.snowluma.webui_password_override` (而非 `self._config.bot.snowluma_webui_password_override`), 其他逻辑不变
- `AdvancedConfigWidget` / `ChooseConfigTypeDialog` 若有相关显隐规则 (`@docs/plans/2026-05-10-snowluma-bot-form-backend-aware-execution-plan.md` §W6) 需要同步摘除 per-Bot 密码 row

### 2.7 `BotProcessManager` 调整

- `_snowluma_driver` 的 start/stop dispatch 逻辑不变 (仍是 `qq_id` → driver 方法)
- 新增: manager `__init__` 里注册 `SnowLumaDaemon` 到 `creart`, `_snowluma_driver` 通过 `it(SnowLumaDaemon)` 访问
- 新增信号处理: `daemon.crashed` 信号 → 对所有当前 `_snowluma_driver._processes` 里 Bot emit `process_changed_signal(qq_id, NotRunning)` + `notification_signal("error", "SnowLuma daemon 崩溃, 所有 SnowLuma Bot 已停止, 请查看日志")`; 然后 `daemon.release()` × N (清零 ref_count)
- `snowluma_login_state_signal` / `notification_signal` / `process_changed_signal` 三个 signal 名字**不变**, 接收方 (`BotCard` 等) 零感知

### 2.8 `.gitignore` / 构建 filter 审计

- `config/snowluma-session.json` (含明文密码) 不得入 git / 打包产物; 既有 `.gitignore` + `script/build_scripts/collection_filters.py` 已有规则的需核对, 若 App 配置新增 `snowluma.webui_password_override` 字段, `config/config.json` 本来就入 git? — 核对 `.gitignore` 若 `config/config.json` 入 git, 则**不能**把明文密码 override 放到 `AppConfig` 里, 改放到单独的 `config/snowluma-app.json` (也入 `.gitignore`). 此决策待 W4 落地时与用户再确认 (见 xl_plan §Open Questions).

---

## 3. 非目标 (Non-Goals)

- **不动** NapCat 路径的任何行为
- **不动** OneBot 协议 / `ConnectConfig` 结构
- **不动** BotCard UI 视觉 / 状态翻译表 (`_STATUS_TRANSLATION_TABLE`)
- **不实现** "一个 UIN 多人多端登录协同" 的业务逻辑 (SL Bridge 已经支持, 但 Desktop UI 不为此做专门展示, 仅保证不出错)
- **不引入** daemon 的 HTTP 代理 / 端口动态分配 (保持 `:5099` 硬编码, 但 `runtime.json` 依然是 SOT, 后续有需求再扩)
- **不迁移** 项目已上线后的用户数据 (项目未上线, 只做本地开发迁移)

---

## 4. 验收标准 (Acceptance Criteria)

### 4.1 产品可观察行为

- **AC1**: 连续启动 2 个 SnowLuma Bot (COLD + COLD, 或 COLD + HOT), 均进入 `logged_in`, 无 `RuntimeError("一期仅支持 1 个 SnowLuma Bot")` 拦截
- **AC2**: 同时运行 2 个 Bot 时, `netstat` / `Get-NetTCPConnection` 显示 **只有 1 个 node.exe 监听 :5099**
- **AC3 (2026-05-11 修订, 持久 daemon)**: 停止第 1 个 Bot 后第 2 个 Bot 仍 `logged_in`; 停止所有 SnowLuma Bot 后 node.exe **保持运行** (持久 daemon 模型); 关闭 Desktop 应用时由 `QApplication.aboutToQuit` 触发 `daemon.shutdown()` 优雅退出. ref_count 仅用于诊断, 不再驱动生命周期 (与 plan §DC-1 一致)
- **AC4**: QQ.exe 意外 kill → 对应 Bot `snowluma_login_state_signal` 2s 内翻到 `disconnected`, **不影响**同 daemon 上其他 Bot
- **AC5**: SL daemon 的 node.exe 意外 kill → 所有依附 Bot 同步进入 `NotRunning`, 弹 `notification_signal` 错误提示; 再次启动任一 Bot 可触发 daemon 重新拉起
- **AC6 (2026-05-11 修订, 持久 daemon)**: 设置页 → 常规 tab → SnowLuma 组改全局 WebUI 密码 → 下次启动 daemon 时生效. 由于 daemon 持久驻留 (DC-1), 改完密码后需**重启 Desktop 应用**才会触发新 daemon spawn 并应用新密码 (本期不做 daemon 热重启). 用户体验语义: 改 → 重启 App → 生效
- **AC7**: 旧 bot.json (含 `snowluma_webui_password_override`) 启动应用一次 → 字段自动迁移到 `AppConfig.snowluma.webui_password_override`, bot.json 该字段被移除, 日志含 1 条 `INFO: bot.snowluma_webui_password_override migrated ...`
- **AC8**: 同 UIN 在 `/api/processes` 中出现多条 PID (如 Windows QQ.exe Electron 衍生), poller 任一 PID 状态为 `online` 即整体 `logged_in`; 没有任何 PID 为 `online` 但有 `loaded` 则 `waiting_for_qr_scan`

### 4.2 代码可观察约束

- **CC1**: `grep -r snowluma_webui_password_override src/` 搜不到任何引用 (除 `_migrate_legacy_*` 一次性迁移点)
- **CC2**: `SnowLumaDriver._processes` 维持 `dict[qq_id, SnowLumaProcessModel]` 结构, 但字典大小不再被 `RuntimeError` 限为 1
- **CC3**: `SnowLumaDaemon` 只有 1 个 `creart` Creator 注册 (App 级单例验证)
- **CC4**: `SnowLumaStatusPoller._uin` 在第一次 `uin_detected` 后被 lock; 后续 tick 不再用 `initial_pid` 做匹配
- **CC5**: `SnowLumaWebUIClient.login()` 在 2 个 Bot 场景下只被调用 1 次 (通过 daemon 单例保证); 单测可验

### 4.3 手工 smoke 列表

用户侧冒烟, 开发者在 W8 阶段执行并附截图 / 日志到 PR:

1. **单 Bot 冷启动**: 新起 1 个 SnowLuma Bot (COLD), 扫码登录, 状态 `logged_in`; 停止, daemon 自动回收
2. **双 Bot 并发冷启动**: 起 Bot A (COLD) → 扫码 → `logged_in`; 起 Bot B (COLD, 不同 UIN) → 扫码 → `logged_in`; 两条 BotCard 并存
3. **双 Bot 混合 (冷+热)**: 起 Bot A (COLD); 手动在 Windows 启动另一个 QQ.exe; 起 Bot B (HOT, attach_pid 指向手动那个); 两条 BotCard 并存
4. **密码迁移**: 准备一份含旧 `snowluma_webui_password_override` 的 bot.json, 启动应用, 观察日志 + 配置文件变化
5. **daemon crash 恢复**: 双 Bot 跑期间, 外部 `Stop-Process -Name node`; 观察 Desktop 两条 Bot 同时标红; 再点任一 Bot 启动, daemon 拉起, Bot 能重新注入

---

## 5. 不变量 (Invariants)

- `process_changed_signal` / `notification_signal` / `snowluma_login_state_signal` 3 个对外 signal 名字 / payload 不变; 接收方代码零修改
- `SnowLumaWebUIClient` 公开 API 8 个 (`wait_ready` / `login` / `logout` / `list_processes` / `list_qq_instances` / `load_process` / `unload_process` / `get_auth_state` / `change_password`) 签名**不变**; 仅持有方从 per-Bot 改为 daemon
- `HookProcessInfo` / `OneBotInstanceInfo` dataclass 不变
- `render_onebot_json(snowluma_path, qqid, connect=..., music_sign_url=...)` 签名不变; 仅调用点迁移
- `runtime.json` / `webui.json` / `onebot_<uin>.json` 文件路径与 schema 不变
- Desktop 启动 daemon 时**单向覆盖** `webui.json` (D2 决策延续); 用户在 SnowLuma WebUI 手改密码 → 下次 daemon 重启失效
- `snowluma-session.json` 不入 git / 打包产物 (既有约束)
- COLD 模式 stop 必 kill Desktop-spawned QQ.exe; HOT 模式 stop **绝不** kill 用户 QQ.exe (Q2 UIN 匹配语义不变)
- UIN 不匹配时 (热启动场景) 仍 `stop_bot(qq_id)` + emit error (现有语义保留)
- 不新增对 `/api/processes` / `/api/qq-list` 外的 WebUI 端点依赖 (`get_auth_state` / `change_password` 仍仅保留未调用)

---

## 6. 风险与回滚

### 6.1 主要风险

- **R1 (daemon 单例竞态)**: `ensure_running` 并发调用可能双 spawn. 缓解: 内部 threading.Lock + state 机对 `STARTING` 去重
- **R2 (ref_count 泄漏)**: stop 异常路径若漏调 `release()` → daemon 永远不退. 缓解: `BotProcessManager._handle_process_finished` 兜底 release, 并加 daemon level "空闲 30s 自动关" watchdog (可选, 若测试暴露再加)
- **R3 (迁移覆盖冲突)**: 多 bot.json 各自有不同 `snowluma_webui_password_override` → 只能挑一个. 缓解: 取第一个非空 + warn; 用户可在组件页手改覆盖
- **R4 (多 UIN 同 PID)**: 极端场景 (PID reuse) 理论可能, 但 SL 的 `isRealUin` 规则 + watcher 1.5s 心跳在 pid 被回收时会 detach_pid, 实际不会卡住. 保持现有 SL 上游逻辑信任
- **R5 (UI 改动面广)**: 组件页加卡片 + BotConfigWidget 移字段 + BotCard 预览按钮改源; 容易漏一处导致 UI 回归. 缓解: W6 集中落, 并配 smoke §4.3 step 4

### 6.2 回滚策略

- 每个 Wave 独立 commit, 单 Wave 回滚即可
- `BOT_CONFIG_COMPAT_VERSION` 升版不可逆, 但项目未上线, 无历史用户数据需要复原
- daemon 模块独立文件, 回滚只需 `git revert` 整个 W1 即可; driver 重构 (W2) 依赖 W1, 回滚时需一起
