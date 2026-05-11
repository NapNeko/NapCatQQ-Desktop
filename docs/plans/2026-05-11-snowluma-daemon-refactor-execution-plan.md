---
plan_id: 2026-05-11-snowluma-daemon-refactor-execution-plan
status: draft_pending_approval
owner: @NapNeko/NapCatQQ-Desktop
requirement: docs/requirements/2026-05-11-snowluma-daemon-refactor.md
vibe_stage: xl_plan
internal_grade: L
---

# SnowLuma Daemon 解耦重构 · 执行计划 (xl_plan)

> 当前对应 requirement 文档: `docs/requirements/2026-05-11-snowluma-daemon-refactor.md`
> 本计划停在 `xl_plan` 阶段, 待用户批准后通过 `vibe` 或 `vibe-do` 入口接管 `plan_execute`.

---

## Vibe 契约字段

- **runtime**: `interactive_governed` (root)
- **wrapper entry**: `Vibe: How Do We Do It?` (`vibe-how`) → **stop target: `xl_plan`**
- **internal grade decision**: **L** — 单 agent 串行. 证据:
  - W1 (daemon 新建) 是后续所有 Wave 的依赖基座, 本体 ~400 行单文件聚合, 不需要 fan-out
  - W2 (driver 重构) + W3 (poller 重构) + W5 (renderer 拆分) 共享 `snowluma_driver.py` 写权限, 交错 fan-out 会频繁冲突
  - W4 (配置模型 + 迁移) 与 W6 (UI 摘除 per-Bot 字段) 有 `BotConfig.snowluma_webui_password_override` 删除的强依赖链, 必串行
  - XL fan-out 理论最多省 20%, 合并冲突 (driver + poller + manager 同时改) 代价更高
- **总规模估算**: 净增 ~1100 行 / 含删除 ~450 行; 净新增源文件 1 个 (`snowluma_daemon.py`); 净新增测试 2 个; 既有 10 文件 edit

---

## Wave 结构

| Wave | 名称                                                         | 依赖         | 串行/并行   | 估算行数 |
| ---- | ------------------------------------------------------------ | ------------ | ----------- | -------- |
| W1   | `snowluma_daemon.py` 新建 + 单例注册 + 单测                  | —            | 串行        | ~400 行  |
| W2   | `SnowLumaDriver` 去单实例守护 + 使用 daemon + 模型裁剪       | W1           | 串行 (原子) | ~500 行  |
| W3   | `SnowLumaStatusPoller` 改按 UIN 聚合 + 新 `pid_set_changed`  | W2           | 串行        | ~180 行  |
| W4   | 配置模型扩展 + `bot.snowluma_webui_password_override` 迁移层 | —            | 串行        | ~120 行  |
| W5   | 密码解析链路重写 + 渲染职责拆分                              | W1, W4       | 串行        | ~90 行   |
| W6   | UI 改动: 组件页加全局密码卡 / BotConfigWidget 摘字段         | W4           | 串行        | ~220 行  |
| W7   | `BotProcessManager` daemon 信号线接入 + crash 传播           | W1, W2, W3   | 串行        | ~130 行  |
| W8   | 全量测试更新 + smoke 手工验收 + phase_cleanup                | W1-W7 全完成 | 串行        | —        |

> **依赖说明**: W1 必先 (daemon 基座); W2 紧随 W1 (driver 是 daemon 的第一消费者); W3 依 W2 (poller 构造参数改动需 driver 先适配); W4 可与 W1-W3 并行规划但落地必先于 W5/W6; W5 解完 App 级密码源才能完成渲染拆分; W7 串联 daemon 崩溃信号 → driver → manager 三层, 最后集成; W8 收尾.
>
> 选 L 而非 XL 的具体证据见上方 "internal grade decision".

---

## 跨 wave 不变量 (Invariants)

- **I1** `process_changed_signal` / `notification_signal` / `snowluma_login_state_signal` 3 个对外 signal 签名与发射语义保持不变 (BotCard / BotLogPage 接收方零改动)
- **I2** `SnowLumaWebUIClient` 公开 API 8 方法签名不变 (`wait_ready` / `login` / `logout` / `list_processes` / `list_qq_instances` / `load_process` / `unload_process` / `get_auth_state` / `change_password`); 实例拥有者从 per-Bot → daemon, 但客户端代码不动
- **I3** `render_onebot_json(snowluma_path, qqid, *, connect, music_sign_url)` 签名与语义不变; 仅调用点从 `SnowLumaDriver._render_configs` 迁到 `SnowLumaBotSession.render_onebot()`
- **I4** `HookProcessInfo` / `OneBotInstanceInfo` dataclass 不变
- **I5** NapCat 路径 (`NapCatDriver` / `napcat_driver.py` / `remote_*`) 零改动; grep `NapCat` 在 git diff 里应只出现在删减 `SnowLumaProcessModel.node_process` 等共生注释里 (如有)
- **I6** 每个 Wave 独立 commit, 单 Wave 可 `git revert` 回滚 (除 W1-W2 联锁, 回滚必联动)
- **I7** `config/snowluma-session.json` 含明文, `.gitignore` 与 `script/build_scripts/collection_filters.py` 已有排除规则不变
- **I8** 不新增 httpx 长连接 / 单例 `httpx.Client`; 所有 SL WebUI 调用延续短连接语义
- **I9** 本计划**不写**新架构文档 (用户选择 Q4 = xl_plan only); requirement doc §2 已含全部 API 与文件变更说明
- **I10** 用户侧手工 smoke 是 §4.3 的 5 项验收; 开发者在 W8 完成并附证据 (截图 / 日志) 到 PR

---

## W1 — `snowluma_daemon.py` 新建 + 单例注册 + 单测

### Owner boundary

- **新增**: `src/core/runtime/snowluma_daemon.py` (~400 行)
- **新增**: `script/test/test_snowluma_daemon.py` (~250 行)
- **修改**: `src/core/runtime/__init__.py` (加 `from .snowluma_daemon import SnowLumaDaemon` 导出)
- **不动**: `snowluma_driver.py` / `snowluma_status_poller.py` / `bot_process_manager.py` / `config_model.py` / 任何 UI 文件

### 实现要点

1. **`DaemonState` enum**: `STOPPED` / `STARTING` / `READY` / `STOPPING` / `CRASHED`
2. **`SnowLumaDaemon(QObject)` 类**:
   - 字段: `_node_process: QProcess | None`, `_webui_client: SnowLumaWebUIClient | None`, `_ref_count: int = 0`, `_state: DaemonState = STOPPED`, `_start_lock: threading.Lock`, `_dead_event: threading.Event`
   - 信号:
     - `crashed(str)` — node.exe 意外 finished 时 emit (payload 是 `exit_code` / `errorString`)
     - `ready()` — daemon 首次进入 `READY` 时 emit (供未来 UI 状态可视化, 本期可不接)
   - API:
     - `ensure_running() -> SnowLumaWebUIClient` (**同步版**, 供测试与远端路径; 主线程 ≤ 10s):
       - 拿 `_start_lock`: 若 `READY` 直接 `_ref_count += 1; return _webui_client`
       - 若 `STOPPED`: state → `STARTING`; 调 `_spawn_node()` (复用 `SnowLumaDriver.build_node_process` 搬过来) + `waitForStarted` + `SnowLumaWebUIClient.wait_ready(timeout=30)` + `client.login()`; 成功 state → `READY`, `_ref_count = 1`, emit `ready`, return client; 失败 state → `STOPPED`, raise
       - 若 `STARTING`: 阻塞等 `_state` 变为 `READY` 或 `STOPPED` (最多 35s), 然后复检 `_ref_count += 1` / raise
       - 若 `CRASHED`: 抛 `RuntimeError("daemon 已崩溃, 请手动重启")` (本期不自动重启, 留给 W7 manager 层决策)
     - `ensure_running_async() -> (bool, QThread)` — Phase A-like, 主线程启非阻塞 spawn, 后台 worker 跑 wait_ready + login, 完成后 emit `ready` (本期 W1 可先只实现同步版, async 版 placeholder; W2 driver 集成时再补)
     - `release()`: `_ref_count -= 1`; 若 `== 0` 且 state `READY`: 走 `_shutdown()` 路径 (`_webui_client.logout()` fire-and-forget + `terminate_async(_node_process)` + state → `STOPPED`)
     - `webui_client() -> SnowLumaWebUIClient`: state != `READY` 时 `raise RuntimeError`; 否则返回 `_webui_client`
     - `is_running() -> bool`: `_state == READY`
   - 内部槽:
     - `_on_node_finished(exit_code, exit_status)`: state 变为 `CRASHED`, emit `crashed(msg)`, 清 client / `_ref_count = 0` (让 manager 层接信号做后续清理)
3. **配置渲染职责**:
   - 新增模块级函数 `render_daemon_globals(snowluma_path: Path) -> str` (返回生效密码, 供 UI / Bot session 查询): 内部调 `render_runtime_json(snowluma_path, webui_port=_SNOWLUMA_WEBUI_PORT)` + `resolve_effective_password(override=app_config.snowluma.webui_password_override)` + `render_webui_json(snowluma_path, password=..., must_change=False)` + `update_last_rendered(session)`
   - `ensure_running()` 开头调 `render_daemon_globals()` 拿到 password 传给 `SnowLumaWebUIClient`
4. **`creart` 单例注册**: 复用 `bot_process_manager.py` 现有 `AbstractCreator` 模板; `CreateTargetInfo` 指向 `SnowLumaDaemon`
5. **主线程亲和性注释**: docstring 明确 `QProcess` 必须在主线程创建; `ensure_running` 同步路径只在主线程 / 测试线程使用

### 单测覆盖 (`test_snowluma_daemon.py`)

- `test_ensure_running_first_call_spawns_node` — mock QProcess, 验证 state 转移 STOPPED → STARTING → READY, ref_count=1
- `test_ensure_running_second_call_reuses_client` — 连调 2 次, 只 spawn 1 次, ref_count=2
- `test_release_decrements_ref_count_without_shutdown_if_still_used` — ref_count=2 时 release → ref_count=1, node 仍在跑
- `test_release_zero_triggers_shutdown` — ref_count=1 时 release → node terminate, state=STOPPED
- `test_node_finished_emits_crashed` — 模拟 QProcess.finished 槽, 验证 `crashed` 信号 + state=CRASHED
- `test_ensure_running_on_crashed_raises` — CRASHED 状态调用 ensure_running 应 RuntimeError
- `test_render_daemon_globals_uses_app_override` — mock `AppConfig.snowluma.webui_password_override` 非空, 验证 `render_webui_json` 收到的 password 是 override 值

### 风险与回滚

- **R**: `threading.Lock` + `QTimer` + `threading.Event` 跨线程组合若处理不当可能死锁. **缓解**: 锁只在 `ensure_running` / `release` 的状态机转移段持有, 不包住任何 `QProcess` 调用 (QProcess 必须主线程); 参考现有 `SnowLumaProcessModel.dead_event` 的 pattern
- **回滚**: `git revert` 本 Wave 一个 commit; `snowluma_driver.py` 未改动, 不影响现有单 Bot 启动

### 验证命令

```powershell
cd d:\NapCat-Project\NapCatQQ-Desktop-V1
uv run pytest script/test/test_snowluma_daemon.py -v
uv run pytest script/test/test_snowluma_driver.py -v  # 回归 (应全绿, 本 Wave 不动 driver)
```

### 预计 commit 消息

```
feat(runtime): add SnowLumaDaemon singleton for shared WebUI/node lifecycle

W1 of snowluma-daemon-refactor: introduces SnowLumaDaemon as the single owner
of SnowLuma's node.exe child process and WebUI session. Prepares the ground
for subsequent waves that remove the per-Bot node spawn and lift the 1-Bot
hard limit.

Refs: docs/requirements/2026-05-11-snowluma-daemon-refactor.md §2.1
```

---

## W2 — `SnowLumaDriver` 去单实例守护 + 使用 daemon + 模型裁剪

### Owner boundary

- **修改**: `src/core/runtime/snowluma_driver.py` (~500 行 diff)
- **修改**: `script/test/test_snowluma_driver.py` (~80 行 — 去掉 "1 Bot limit" assert, 加 "2 Bots concurrent" 断言)
- **不动**: poller / manager / config / UI (留给后续 Wave)

### 实现要点

1. **删除单实例守护** (`@...\snowluma_driver.py:318-323`): 去掉 `if self._processes: raise RuntimeError(...)` 与对应 log
2. **`SnowLumaProcessModel` 字段裁剪**:
   - 保留: `qq_id`, `qq_process` (optional, COLD only), `qq_pid`, `state`, `started_at`, `dead_event`
   - **删除**: `node_process`, `webui_client`, `auth_token`, `effective_password`
   - **新增**: `uin: str = ""` (由 poller 写回), `ancillary_pids: set[int] = field(default_factory=set)` (由 poller 写回)
3. **`start_async` 重写** (保留签名: `(config, *, start_mode, attach_pid) -> tuple[ProcessHandle, worker, session]`):
   - Phase A (主线程): `daemon = it(SnowLumaDaemon)`; COLD 则构造 QQ.exe QProcess (不 start); HOT 则 validate `attach_pid`; 构造 `SnowLumaProcessModel`; `self._processes[qq_id] = model`
   - `_start_phase_a_processes` 改只启 QQ.exe (HOT 则跳过); **不再** spawn node
   - Phase B-C (Worker): 改为 `_SnowLumaPhaseCWorker` (去掉 Phase B): `client = daemon.ensure_running()` (同步阻塞 worker 线程 ≤ 35s) → `client.load_process(model.qq_pid)` → emit `succeeded(client)`. 失败 emit `failed(str)`
   - ProcessHandle 返回: `primary_process = model.qq_process or None` (HOT 下 QQ 不归 Desktop 拥有, primary 为 None, 这点需 manager 兼容检查; 若 `ProcessHandle.primary_process=None` 在既有 manager 路径会出问题, W7 里补兼容)
4. **`stop(qq_id)` 重写**:
   - 停 poller (同现 `detach_poller`)
   - `client = self._daemon_webui_client()` (若 daemon 未就绪直接跳过 unload)
   - `client.unload_process(model.qq_pid)` fire-and-forget (复用 `_SnowLumaStopHttpWorker`, 但把 `webui_client` 参数改成 daemon 的 client)
   - COLD: `terminate_async(model.qq_process)`; HOT: 不动用户 QQ
   - `daemon.release()` — 若本 Bot 是最后一个, daemon 自行 shutdown
   - `self._processes.pop(qq_id)`
5. **`_render_configs` 拆除**: 删除 runtime.json / webui.json 渲染 (归 daemon 了); 只保留 `onebot_<uin>.json` 渲染; 方法更名为 `_render_onebot_config(config)`
6. **`is_running` / `get_status_poller` / `get_process_model` / `list_processes` / `remove_process_model` / `attach_poller` / `detach_poller`** 签名不变, 内部字典操作不变
7. **`build_node_process` 方法**: 迁到 `snowluma_daemon.py` (W1 已迁入); 本 Wave 在 driver 侧删除

### 测试更新 (`test_snowluma_driver.py`)

- `test_start_second_bot_while_first_running_succeeds` — 新增, 期望两次 start_async 都成功, daemon 只起 1 次
- `test_start_async_uses_shared_daemon` — 新增, mock `SnowLumaDaemon.ensure_running`, 验证被调 N 次 (N = Bot 数)
- `test_stop_bot_releases_daemon_ref_count` — 新增, 验证 `daemon.release` 被调
- 旧 `test_start_refuses_second_bot` **删除** (不再适用)
- 旧 `test_start_spawns_node_for_each_bot` **删除** (不再适用)
- 回归测试: `test_render_onebot_json_receives_connect_config` 保持绿 (§I3 不变量)

### 风险与回滚

- **R1**: `_SnowLumaPhaseCWorker` 从 worker 线程调 `daemon.ensure_running()` (同步 ≤ 35s): 若并发 2 个 Bot 同时触发首次 `ensure_running`, `_start_lock` 会序列化, 第 2 个 worker 多等 35s. **缓解**: W1 已写 `ensure_running` 支持 "`STARTING` 状态下等待转 `READY`" 的协作等待; 不会重复 spawn
- **R2**: `ProcessHandle.primary_process` 可能为 None (HOT 模式): 既有 `BotProcessManager._handle_process_finished` 依赖 primary_process 接 `finished` 信号. **缓解**: W7 里给 manager 加 None check, HOT 模式下接 `model.qq_process is None` 分支跳 QProcess 路径, 依赖 poller 的 `disconnected` 信号
- **回滚**: `git revert` 单 commit; daemon 模块仍在但暂无消费者 (W1 测试依然绿)

### 验证命令

```powershell
uv run pytest script/test/test_snowluma_driver.py script/test/test_snowluma_daemon.py -v
uv run pytest script/test/test_snowluma_status_poller.py -v  # 回归 (poller 本 Wave 不动)
```

---

## W3 — `SnowLumaStatusPoller` 改按 UIN 聚合 + 新 `pid_set_changed`

### Owner boundary

- **修改**: `src/core/runtime/snowluma_status_poller.py` (~180 行 diff)
- **修改**: `script/test/test_snowluma_status_poller.py` (~120 行 — 新场景: 同 UIN 多 PID 聚合)
- **不动**: driver (W2 已完成) / manager (W7 再接) / UI / config

### 实现要点

1. **构造签名**: `(qq_id: str, initial_pid: int, webui_client: SnowLumaWebUIClient, parent=None)`; 参数名 `qq_pid → initial_pid` 语义化
2. **内部状态新增**: `self._uin: str = ""` (locked 后持久), `self._last_pid_set: set[int] = set()`
3. **新信号**: `pid_set_changed = Signal(str, list)` — payload `(qq_id, list[int])`
4. **`_on_processes` 重写** (对齐 requirement §2.3):
   ```
   if self._uin == "":
       # 首次探测: 用 initial_pid 找 UIN, fallback qq_instances[0].uin
       info = next((p for p in processes if p.pid == self._initial_pid), None)
       candidate_uin = info.uin if info else (qq_instances[0].uin if qq_instances else "")
       if _is_real(candidate_uin):
           self._uin = candidate_uin
           self.uin_detected.emit(self._qq_id, self._uin)
   
   # 后续所有 tick: 按 UIN 聚合
   matched = [p for p in processes if p.uin == self._uin] if self._uin else []
   statuses = {p.status for p in matched}
   if "online" in statuses:
       new_state = SNOWLUMA_STATE_LOGGED_IN
   elif "loaded" in statuses:
       new_state = SNOWLUMA_STATE_WAITING_FOR_QR_SCAN
   elif any(s in statuses for s in ("available", "loading", "connecting")):
       new_state = SNOWLUMA_STATE_STARTING
   elif statuses and statuses <= {"error", "disconnected"}:
       new_state = SNOWLUMA_STATE_DISCONNECTED
   else:
       # matched 空但 qq_instances 匹 UIN 有 → W7 fallback 语义保留
       new_state = SNOWLUMA_STATE_LOGGED_IN if any(q.uin == self._uin for q in qq_instances) else None
   
   if new_state is not None and new_state != self._last_state:
       self.state_changed.emit(self._qq_id, new_state)
       self._last_state = new_state
   
   # pid_set 变化感知
   new_pid_set = {p.pid for p in matched}
   if new_pid_set != self._last_pid_set:
       self.pid_set_changed.emit(self._qq_id, sorted(new_pid_set))
       self._last_pid_set = new_pid_set
   ```
5. **`_translate_status`** 方法与 `_STATUS_TRANSLATION_TABLE` 删除 (因为现在按 UIN 聚合, 不再走单条 hook_status → desktop_state 映射表; 映射移到 `_on_processes` 内联的集合语义中, 语义等价)
6. **`_is_real` uin 判定**: 抽模块级函数, 与 hook-manager.ts 规则保持一致 (非空 + != "0" + 全数字 + len >= 5)

### 测试更新 (`test_snowluma_status_poller.py`)

- `test_single_pid_online_emits_logged_in` — 基线
- `test_multiple_pids_same_uin_any_online_emits_logged_in` — **新**: 两条 hook_info, PID=1001 status=loading, PID=1002 status=online, UIN 同, 期望 `logged_in`
- `test_multiple_pids_all_loaded_emits_waiting_for_qr_scan` — **新**
- `test_pid_set_changed_emits_on_watcher_expansion` — **新**: 首次 tick 只 1 条 PID, 第 2 tick 出现 2 条 (watcher 新发现), 期望 `pid_set_changed` emit 一次
- `test_qq_instances_fallback_when_processes_empty` — 保留现 W7 场景; 期望 fallback 到 `logged_in`
- `test_uin_locked_after_first_detect` — **新**: UIN 第一次 lock 后, 即使 `initial_pid` 消失, 也能继续用 UIN 匹配
- `test_translate_status_table_removed` — **新**: `_STATUS_TRANSLATION_TABLE` 不应再是公开可 import 符号

### 风险与回滚

- **R**: 集合语义替换映射表, 有可能漏 `error` 状态场景 (单条 PID 状态是 error, 但 UIN 也在其他 PID 上 online). **缓解**: 测试用例 `test_mixed_error_and_online` 覆盖该分支, 预期 `logged_in` 胜出 (任一 online 即在线)
- **回滚**: 单 commit 回滚即可; driver (W2) 的 poller 调用点若已改构造参数签名需一起回滚

---

## W4 — 配置模型扩展 + `bot.snowluma_webui_password_override` 迁移层

### Owner boundary

- **修改**: `src/core/config/config_model.py` (~100 行 diff)
- **修改**: `script/test/test_config_model.py` (~80 行 — 新迁移场景)
- **不动**: driver / poller / manager / session / UI (等 W5 / W6)

### 实现要点

1. **新增模型**:
   ```python
   class AppSnowLumaConfig(BaseModel):
       """App 级 SnowLuma 全局设置 (daemon 共享, 不挂 per-Bot)."""
       webui_password_override: str = Field(
           default="",
           description="SnowLuma WebUI 全局密码 override (空=走 snowluma-session.json 自动生成)",
       )
   ```
2. **挂到 `AppConfig`**:
   ```python
   class AppConfig(BaseModel):
       # ... 现有字段 ...
       snowluma: AppSnowLumaConfig = Field(default_factory=AppSnowLumaConfig)
   ```
   若项目当前无 `AppConfig` (仅有 `Config`), 本 Wave 需同步建 `AppConfig` 模型并挂到 `Config.app: AppConfig = Field(default_factory=AppConfig)` (落地时按现状调整)
3. **删除 `BotConfig.snowluma_webui_password_override`** (`@...\config_model.py:531-534`)
4. **`_migrate_legacy_*` 新增一条迁移规则** (在 `_migrate_legacy_bot_config` 或等价位置):
   ```python
   # 2026-05-11: bot.snowluma_webui_password_override → app.snowluma.webui_password_override
   legacy_override = (bot_raw.pop("snowluma_webui_password_override", "") or "").strip()
   if legacy_override:
       _DEFERRED_APP_OVERRIDES.append(legacy_override)  # 模块级列表, load_app_config 时消费
       rules_applied.append(
           "bot.snowluma_webui_password_override migrated to app.snowluma.webui_password_override"
       )
   ```
   `load_app_config()` 在读 `config/config.json` 时消费 `_DEFERRED_APP_OVERRIDES`: 取第一个非空值; 后续非空值 log warn ("多个 Bot 曾设置不同 SnowLuma 密码 override, 选 '...' 作为全局值; 其余将丢弃")
5. **`BOT_CONFIG_COMPAT_VERSION` 升版**: `v2.0 → v2.1`, 在 `_migrate_legacy_*` 顶部加版本 gate
6. **`.gitignore` 决策点 (Open Question)**:
   - **选项 A**: `AppConfig.snowluma.webui_password_override` 直接放到 `config/config.json`. 若现有 `config/config.json` **已入 git**, 则明文密码入 git → **不可接受**
   - **选项 B**: 新建 `config/snowluma-app.json` 存储 `AppSnowLumaConfig`, 加入 `.gitignore` (与 `snowluma-session.json` 同列)
   - **W4 落地前需先确认**: `git check-ignore config/config.json` 看当前是否入 git (若入 git 取 B, 否则取 A)

### 测试更新

- `test_legacy_bot_override_migrated_to_app` — 给老 bot.json 加 `snowluma_webui_password_override: "foo"`, 启动后该字段消失 + `AppConfig.snowluma.webui_password_override == "foo"`
- `test_multiple_legacy_overrides_first_wins` — 2 份 bot.json 分别是 "foo" 与 "bar", 期望 AppConfig 值 = "foo" + 1 条 warn 日志
- `test_new_bot_config_rejects_snowluma_webui_password_override` — 显式传字段应触发 pydantic `ValidationError` (字段已删)
- 回归: `test_bot_config_backward_compat` — v2.0 bot.json 应能 load (经迁移后)

### 风险与回滚

- **R**: `_DEFERRED_APP_OVERRIDES` 模块级列表在测试并发时可能泄漏. **缓解**: 用 `contextvars.ContextVar` 或 pytest fixture 在每个 test case 清空
- **R**: `BOT_CONFIG_COMPAT_VERSION` 升版可能破坏既有 CI 快照测试 (如 `test_bot_config_serialize_snapshot.py`). **缓解**: W4 同时更新快照文件
- **回滚**: 单 commit 回滚; 已迁移的用户需手动把密码搬回 bot.json (不过项目未上线, 开发期无用户数据, 此代价为 0)

---

## W5 — 密码解析链路重写 + 渲染职责拆分

### Owner boundary

- **修改**: `src/core/runtime/snowluma_session.py` (~40 行 diff)
- **修改**: `src/core/runtime/snowluma_daemon.py` (W1 新建的文件, 本 Wave 补齐 `render_daemon_globals` 读 App 配置)
- **修改**: `src/core/runtime/snowluma_config_renderer.py` (若 `render_webui_json` / `render_runtime_json` 签名需调整; 预期不动)
- **修改**: `script/test/test_snowluma_session.py` (~30 行)
- **不动**: driver (W2 已不再 `_render_configs`, 只渲染 onebot_<uin>.json) / poller / manager / UI

### 实现要点

1. **`resolve_effective_password` 签名重写**:
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
2. **daemon 内调用**:
   ```python
   from src.core.config import cfg
   override = cfg.app.snowluma.webui_password_override  # W4 新字段
   password = resolve_effective_password(override=override)
   render_webui_json(snowluma_path, password=password, must_change=False)
   ```
3. **UI 预览按钮** (W6 里做, 本 Wave 不动 UI): card.py `@...\card.py:556-562` 改读 `cfg.app.snowluma.webui_password_override` — W6 Owner boundary 里包含

### 测试更新

- `test_resolve_effective_password_with_override` — 直接传 override 参数
- `test_resolve_effective_password_fallback_to_session` — override="", 验证 load_session / create_session 路径
- 删除 `test_resolve_effective_password_with_bot_config_override` (签名已变)

### 风险与回滚

- **R**: `resolve_effective_password` 有单测也有实际调用点, 搜索遗漏调用点会 type error. **缓解**: W5 落地前先 `grep -r resolve_effective_password src/` 全量列出调用点, 逐一修改
- **回滚**: 单 commit; W1 daemon 的 `render_daemon_globals` 会 import 失败, 联动回滚

---

## W6 — UI 改动: 组件页加全局密码卡 / BotConfigWidget 摘字段

### Owner boundary

- **修改**: `src/ui/page/bot_page/widget/config.py` (~80 行 diff — 删字段卡片)
- **修改**: `src/ui/page/bot_page/widget/card.py` (~30 行 diff — WebUI 按钮改读源)
- **修改**: 组件页 SnowLuma tab (路径待确认, 估 `src/ui/page/setup_page/component_page.py` 或 `src/ui/page/component_page/`) (~110 行 new widget)
- **不动**: poller / driver / manager / config (W4 已改)

### 实现要点

1. **`BotConfigWidget` 删 per-Bot 密码卡**:
   - `config.py:288-314`: 删除 `snowluma_webui_password_override` 的 `save_config` 条目与 `fill_value` 反填
   - 删除 `snowluma_webui_password_card` 的构造 + 布局挂载代码 (约 40 行 UI code)
2. **`BotCard.webui_button_clicked`** (`card.py:556-562`): 改读源:
   ```python
   from src.core.config import cfg
   override = (cfg.app.snowluma.webui_password_override or "").strip()
   if override:
       effective_password = override
       password_source = "app.snowluma.webui_password_override"
   else:
       session = load_session()
       if session is not None:
           effective_password = session.password
           password_source = "snowluma-session.json"
       else:
           effective_password = None
           password_source = ""
   # 其余 UI 逻辑不变
   ```
3. **组件页 SnowLuma tab 加全局密码卡** (具体文件路径 W6 落地时 grep 确认):
   - 一张 `LineEdit` 卡片, label `"SnowLuma WebUI 密码 override"`, placeholder `"留空 = 自动生成随机密码"`
   - 双向绑定 `cfg.app.snowluma.webui_password_override`
   - tooltip: "daemon 启动时读一次; 若 daemon 已在跑, 改完需重启所有 SnowLuma Bot 生效"
4. **`AdvancedConfigWidget` / `ChooseConfigTypeDialog` 若有相关显隐规则** (`@docs/plans/2026-05-10-snowluma-bot-form-backend-aware-execution-plan.md` §W6 里的 SnowLuma 表单显隐): 搜索所有 `snowluma_webui_password` 引用点, 若在 SnowLuma-only 显隐表里, 一并摘除

### 测试更新

- 若有 UI 快照测试 (如 `test_bot_config_widget.py`): 更新快照, 去掉密码卡片
- 组件页新卡片的单测: mock cfg, 验证填入值被持久化到 `app.snowluma.webui_password_override`

### 风险与回滚

- **R**: 组件页具体路径未在本 requirement 里锁定, 需要在 W6 落地时先 grep 找到现有 SnowLuma tab (可能还没有, 则本 Wave 顺带新建)
- **R**: UI 改动面跨多文件, 漏改一处会出现 "BotConfigWidget 里还看到该 row" 的回归. **缓解**: W6 收尾用 `grep -r snowluma_webui_password src/ui/` 检查, 预期 0 结果
- **回滚**: 单 commit

---

## W7 — `BotProcessManager` daemon 信号线接入 + crash 传播

### Owner boundary

- **修改**: `src/core/runtime/bot_process_manager.py` (~130 行 diff)
- **修改**: `script/test/test_bot_process_manager_snowluma.py` (或等价文件) (~60 行)
- **不动**: daemon / driver / poller / config / UI (前 6 Wave 全落地)

### 实现要点

1. **`BotProcessManager.__init__` 增 daemon 接入**:
   ```python
   self._snowluma_daemon = it(SnowLumaDaemon)  # W1 注册的 creart 单例
   self._snowluma_daemon.crashed.connect(self._on_snowluma_daemon_crashed)
   self._snowluma_daemon.ready.connect(self._on_snowluma_daemon_ready)  # 可选, 本期不消费
   ```
2. **新增槽 `_on_snowluma_daemon_crashed(message: str)`**:
   - 遍历 `self._snowluma_driver._processes` 所有 qq_id:
     - emit `process_changed_signal(qq_id, QProcess.ProcessState.NotRunning)`
     - `self._stop_snowluma_status_poller(qq_id)`
     - `self._snowluma_driver.remove_process_model(qq_id)`
     - COLD 模式: `terminate_async(model.qq_process)` (若 qq_process 仍在跑)
   - emit `notification_signal("error", f"SnowLuma daemon 崩溃: {message}. 所有 SnowLuma Bot 已停止, 请查看日志后重启.")`
3. **`ProcessHandle.primary_process=None` 兼容** (W2 R2 风险):
   - `_handle_process_finished` 入参原是 `(qq_id, exit_code, exit_status)` (来自 QProcess.finished 信号). HOT 模式下 Desktop 不拥有 QQ.exe, manager 不再能通过 QProcess.finished 感知 QQ 崩溃
   - **解决**: poller 的 `state_changed(qq_id, "disconnected")` 已经是 HOT 模式下 QQ 崩溃的检测通道, manager 接此信号后走既有 `_handle_process_finished` 等价清理 (抽方法 `_handle_snowluma_bot_terminated(qq_id, reason)`)
   - 若 HOT 模式下 QQ 未崩仅用户手动 stop, 主流程 `stop_bot(qq_id)` 仍走 driver.stop → daemon.release, 无需经 QQ.exe.finished 触发
4. **`_connect_snowluma_poller_signal` 调整**: 新接 `poller.pid_set_changed(qq_id, list)` → 写回 `model.ancillary_pids`:
   ```python
   poller.pid_set_changed.connect(self._on_snowluma_pid_set_changed)
   
   def _on_snowluma_pid_set_changed(self, qq_id: str, pids: list[int]) -> None:
       model = self._snowluma_driver.get_process_model(qq_id)
       if model is None: return
       primary = model.qq_pid
       model.ancillary_pids = {p for p in pids if p != primary}
       logger.trace(f"SnowLuma UIN ancillary PIDs 更新 (QQID: {qq_id}, primary={primary}, ancillary={model.ancillary_pids})")
   ```
5. **`stop_bot(qq_id)` 路径 (NapCat + SnowLuma)**: SnowLuma 分支原本就是 `self._snowluma_driver.stop(qq_id)`, 本 Wave 无新增 (W2 已在 driver 内部接入 daemon.release)
6. **远端 SSH Bot 路径**: daemon 是本地概念, 远端 SnowLuma Bot 目前未覆盖 (远端仍走 `remote_backend.start_napcat` 路径, 本期只重构本地). 远端 SnowLuma 支持列为**未来需求**

### 测试更新

- `test_daemon_crash_propagates_to_all_bots` — 2 个 Bot 跑, 模拟 `daemon.crashed.emit`, 验证 2 条 `process_changed_signal(NotRunning)` + 1 条 error notification
- `test_pid_set_changed_updates_ancillary_pids` — poller emit `pid_set_changed(qq_id, [1001, 1002])`, 验证 `model.ancillary_pids == {1002}` (假设 primary=1001)
- 回归: `test_bot_process_manager_snowluma_uin_mismatch` 保持绿 (§1.5 Q2 UIN 匹配语义不变)

### 风险与回滚

- **R**: 信号线并发顺序: `daemon.crashed` 与 `poller.state_changed` 可能先后到达, manager 若两处都走清理路径会双清. **缓解**: 在 `_handle_snowluma_bot_terminated` 前先 check `_processes.get(qq_id)` 是否还在, None 则 early return
- **回滚**: 单 commit

---

## W8 — 全量测试更新 + smoke 手工验收 + phase_cleanup

### Owner boundary

- **执行**: 不新增代码变更, 只跑 regression + smoke
- **维护**: `outputs/runtime/vibe-sessions/<session-id>/` 下的 phase cleanup 受托件

### 执行步骤

1. **跑全量测试**:
   ```powershell
   uv run pytest script/test/ -v
   ```
   预期: 所有测试绿; 已迁移的 `test_snowluma_*` 全绿; `test_bot_process_manager*` 全绿; 既有 NapCat / 远端 SSH 测试零回归
2. **静态检查** (若项目有 mypy / ruff 配置):
   ```powershell
   uv run ruff check src/
   uv run mypy src/core/runtime/snowluma_daemon.py src/core/runtime/snowluma_driver.py src/core/runtime/snowluma_status_poller.py
   ```
3. **手工 smoke** (requirement §4.3 5 项):
   - S1: 单 Bot 冷启动 → `logged_in` → 停止 → daemon 回收 (观察 `Get-Process node` 确认 node.exe 消失)
   - S2: 双 Bot 并发冷启动 → 两条卡都 `logged_in` → `Get-NetTCPConnection -LocalPort 5099` 应只 1 个 owning PID
   - S3: 双 Bot 混合 (COLD + HOT) → 两条卡都 `logged_in`
   - S4: 密码迁移: 准备一份含 `snowluma_webui_password_override` 的旧 bot.json → 启动应用 → 观察日志 + `Get-Content config/config.json` (或 `config/snowluma-app.json`, 按 W4 Open Question 选择) 含迁移值; 旧 bot.json 该字段应消失
   - S5: daemon crash 恢复: 双 Bot 跑期间 `Stop-Process -Name node` → 两条 Bot 同时标红 + notification 错误 → 再启任一 Bot → daemon 重新拉起 → 注入成功
4. **phase_cleanup 受托件** (vibe 契约 stage 6):
   - 写 `outputs/runtime/vibe-sessions/<session-id>/cleanup-receipt.json`: `{ "requirement_frozen": true, "waves_completed": ["W1"..."W8"], "smoke_passed": true, "files_changed_net_lines": <N>, "rollback_safe_commits": [<sha1>, ...] }`
   - 清理临时文件: 无 (本次改动无临时脚手架文件)
   - 归档日志: 把 smoke S1-S5 的关键日志行 (启动 / 停止 / crash / 迁移) 拼进 PR description
5. **delivery-acceptance report**:
   - AC1-AC8 全部绿 → "delivery: full completion" 许可 (vibe 契约要求)
   - 任何 AC 失败 → "delivery: partial, blockers: [AC-x, ...]" + 回滚对应 Wave

### 验证命令摘要

```powershell
# 完整回归
uv run pytest script/test/ -v --tb=short

# 特定模块
uv run pytest script/test/test_snowluma_daemon.py script/test/test_snowluma_driver.py script/test/test_snowluma_status_poller.py -v

# 配置迁移
uv run pytest script/test/test_config_model.py -k "migrate" -v

# manager 信号线
uv run pytest script/test/test_bot_process_manager_snowluma.py -v

# 启动应用 (手工 smoke)
uv run python main.py
```

---

## Open Questions (待 `plan_execute` 阶段前解决)

1. **OQ1**: `AppConfig.snowluma.webui_password_override` 落到 `config/config.json` 还是新建 `config/snowluma-app.json`? 取决于 `config/config.json` 是否入 git (W4 落地前先 `git check-ignore` 确认)
2. **OQ2**: 组件页的实际路径. 从 `list_dir` 看 `src/ui/page/` 结构确认 SnowLuma tab 是否已存在; 若不存在, W6 是否扩出单独 tab 还是并入现有 `setup_page`?
3. **OQ3**: 远端 SSH 路径的 SnowLuma 支持 (远端 daemon) 是否在本期需求内? requirement §3 已列为 Non-Goal, 但如果用户后续有需求, 应单独起一份 requirement
4. **OQ4**: daemon 的 `ensure_running_async` 是否本期必做? W1 备注里写了同步版优先, async 版在 W2 driver 集成时再补. 若 Phase A 对主线程卡顿敏感 (ensure_running 首次 ~35s), 需在 W2 里落 async 版本
5. **OQ5**: `SnowLumaWebUIClient` 是否需要在 daemon 内部加 `refresh_token` / reconnect 语义? 当前 client 有 401 auto-retry, 短连接, 单次 `login` 持 token. 长跑 daemon 数小时后 token 是否过期? 若过期, `list_processes` 会触发 401 auto-retry → 重 login, 这是既有行为, 本期不改

---

## 设计变更日志 (Post-AC, 实施期发现)

- **DC-1 (2026-05-11): daemon 生命周期从 "ref-counted 自动 terminate" 改为 "持久 daemon (App 级)"**

  - **原设计 (按 requirement AC3 字面意思)**: ``release()`` 把 ``ref_count`` 降到 0 时自动 terminate node.exe; 下次有 Bot 启动再 spawn.
  - **实测问题**:
    - 反复启停 Bot 时, 每次起首个 Bot 都要等 ~30s daemon spawn (wait_ready 30s);
    - 用户停最后一个 Bot 后立即又起 Bot, ensure_running 命中 ``STOPPING`` 状态报错;
    - 与上游 SnowLuma "一个服务多 QQ 挂" 的设计也不完全贴合 (上游 daemon 通常持久驻留).
  - **改后语义**:
    - ``SnowLumaDaemon.release()``: 仅 ``ref_count -= 1``, **不**触发 terminate; daemon 保持 ``READY`` 直到 App 退出.
    - ``SnowLumaDaemon.shutdown()`` 新增方法: 显式 terminate (logout + terminate_async + state=STOPPED). 一般由 ``QApplication.aboutToQuit`` 钩子调.
    - ``main.py`` 注册 ``app.aboutToQuit`` → ``daemon.shutdown()`` (延迟 import + 异常静默, 即使 daemon 从未启过也安全).
  - **取舍**:
    - 优势: 反复启停 Bot 不再等 spawn; ``STOPPING`` 状态下的 race 消失.
    - 代价: daemon 启过一次后约 100MB 常驻到 App 退出; 改全局 WebUI 密码需手动重启 App 才生效 (与 AC6 之前的语义一致, 用户体验仅"重启 App"而非"停所有 Bot 后再起").
  - **覆盖**: ``test_snowluma_daemon.py::TestRelease::test_release_to_zero_does_not_shutdown_in_persistent_mode`` + ``test_shutdown_*`` (3 新测试).
  - **AC3 字面修订**: 原 AC3 "停止第 2 个 Bot 后 node.exe 自动退出 (ref_count 归 0 回收)" 改为 "停止所有 SnowLuma Bot 后 node.exe **保持运行**; App 退出时 node.exe 由 ``QApplication.aboutToQuit`` 钩子触发 ``daemon.shutdown()`` 优雅退出". 需要相应更新 requirement doc.

---

## 已知预存在缺陷 (Pre-existing, 非本 wave 引入)

- **PE-1 (test isolation)**: ``script/test/test_bot_card.py`` 在**模块顶层** (line 48) 调 ``load_card_module()``, 该函数直接覆盖 ``sys.modules["src.ui.page"]``, ``["src.ui.page.bot_page"]``, ``["src.ui.page.bot_page.widget"]`` 为裸 ``ModuleType`` stub. 由于发生在 pytest collection 阶段, 影响**整个** test session 内所有后续测试. 后续 (按 pytest 字母排序) 的 ``test_bot_config_widget.py``, ``test_advanced_config_widget.py``, ``test_bot_config_page.py``, ``test_bot_list_page.py``, ``test_bot_page_batch_mode.py`` 等加载完整 widget 包时, 命中 stub → ``ImportError: cannot import name 'HttpClientConfigCard' from 'src.ui.page.bot_page.widget' (unknown location)``, 或在运行时 ``self.auto_restart_dialog_card.fill_value(...)`` → ``MainWindow`` → ``from src.ui.page import ApiDebugPage`` 失败. **基线对比** (已 stash 验证): master HEAD 无任何 W1-W6 改动时, 同一 ``pytest script/test/`` 调用下 19 fail / 851 pass; 含 W1-W6 改动后 18 fail / 978 pass (新增 127 测试, fail 数量未上升). 修复路径 (脱离本期范围): 在 ``load_card_module()`` 用 ``monkeypatch.setitem`` 替代 ``sys.modules[...] = ...`` 让 pytest 自动 teardown; 或在测试结束时手动 ``sys.modules.pop(name, None)``.

- **PE-2 (broken-module imports)**: 3 个测试 collection-time 失败, 引用了已不存在的模块:
  - ``test_home_version_card.py``: ``src.ui.page.home_page.version_card``
  - ``test_setup_desktop_log_page.py``: ``src.ui.page.setup_page.desktop_log``
  - ``test_update_log_card.py``: ``src.ui.page.component_page.base``

  本期跑测命令需 ``--ignore=`` 这三个文件. 修复路径 (脱离本期范围): 重写或删除这些过期 tests.

- **PE-3 (test_stacked_widget abort)**: ``test_stacked_widget.py::test_transparent_stacked_widget_uses_soft_animation`` 会触发 Python interpreter abort (Qt 相关). 本期跑测命令 ``--ignore=script/test/test_stacked_widget.py`` 绕开. 修复路径 (脱离本期范围): 调查 ``TransparentStackedWidget`` 在 ``QT_QPA_PLATFORM=offscreen`` 下的崩溃.

---

## 合规与交付语言

- **completion language policy**: 完成时声明 "vibe stage=`xl_plan` output delivered"; **不声称** `plan_execute` / `phase_cleanup` 完成 (本 wrapper `vibe-how` 不覆盖)
- **re-entry**: 用户批准后, 通过 `/vibe` 或 `/vibe-do` 入口, 附 `$vibe` 前缀在任何 sub-agent 提示尾, 继承 requirement doc + 本 plan 作为冻结上下文
- **delegation-envelope**: 若后续 XL 切 child lane, 父 lane 需写 `delegation-envelope.json` 指向本 plan id; 本计划不自动生成

---

## 执行路线总览 (一句话)

> `snowluma_daemon.py` 新建 (W1) → 驱 driver 改造 (W2) → 驱 poller 按 UIN 聚合 (W3) → 驱 配置迁 (W4) → 驱 密码链重写 (W5) → 驱 UI 摘字段 (W6) → 驱 manager 接 daemon 信号 (W7) → 验 (W8).
