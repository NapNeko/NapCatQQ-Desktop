# 远程 SSH P3 — 启动流程异步化与状态可见性 (requirement)

> 关联文档:
> - [`docs/general/remote_ssh_p3_plan.md`](../general/remote_ssh_p3_plan.md)
> - [`docs/general/remote_ssh_progress.md`](../general/remote_ssh_progress.md)
>
> 运行模式: `vibe interactive_governed → high_autonomy`
> 终止阶段: `phase_cleanup`

## 1. 背景

P3 W1/W2/W3 已完成核心功能 (SSH 持久连接、UI 维护入口、迁移服务、远端日志收尾)。
进入"体验优化阶段"后，用户实测发现：

1. **远端 Bot 启动期间 (~10s) UI 没有可见反馈**
   - `_create_remote_process` 已 emit `ProcessState.Starting` 信号；
   - 但 `BotCard.slot_process_changed_button` 不区分 `Starting`，按钮仍显示"启动"，看起来"按了没反应"。

2. **保存远端 Bot 配置时主线程仍会卡顿**
   - `update_config` / `delete_config` 在主线程内同步调用 `_sync_bot_runtime_config_to_remote` /
     `_delete_bot_runtime_config_from_remote`，这两条路径直接发起 SSH `write_text` × 2 / `remove`，
     在网络抖动时主线程会冻结数百 ms ~ 数秒。

3. **本地 Bot 启动同样阻塞主线程**
   - `create_napcat_process` 调用 `process.waitForStarted(5000)`，
     最多阻塞 5s/Bot；`_auto_start_bots` 串行多个 Bot 时累计可达 20s+，
     拖慢应用首屏。

4. **多个后台任务并发时缺乏全局可见性**
   - 启动 / 停止 / 部署 / 迁移 / 配置同步等 SSH 任务都走 `QThreadPool`，
     UI 没有"当前有 N 个后台任务在跑"的统一状态指示，
     用户无从判断"是不是又卡了"。

## 2. In-Scope (本次必须交付)

| 子项 | 标签 | 简述 |
| --- | --- | --- |
| **A** | BotCard `Starting` 状态可视化 | `slot_process_changed_button` 区分三态；`Starting` 时按钮 disable + 文案改为"启动中…"，旁边附 `IndeterminateProgressBar`。本地 + 远端均生效。 |
| **B** | 配置远端同步异步化 | `update_config` / `delete_config` 的远端钩子改为派发到 `QThreadPool`；BotConfig 保存按钮在派发期间显示"正在同步到远端…"提示，完成后 InfoBar 反馈。 |
| **C** | 本地 QProcess 启动去阻塞 | 移除 `waitForStarted(5000)` 主线程等待；改用 `process.started` / `process.errorOccurred` 信号驱动状态切换 + 通知。 |
| **D** | 全局后台任务状态条 | 新增 `BackgroundTaskCenter` 单例聚合所有进行中的 SSH/QRunnable 任务；BotPage Header 添加状态条，任意一项进行中即显示进度环 + "后台任务: N"。 |

## 3. Out-of-Scope (本期不做)

- 任务取消按钮 (停止运行中的 SSH worker) — paramiko 中途中断成本高，留给后续。
- 全局错误聚合面板 — InfoBar / Bot 内日志 已经足够。
- ServerEditDialog 的"测试连接"按钮 — 当前实现没有同步 SSH，仅校验字段，不在本次范围。
- 重做 `RemoteBotOperationRunnable` 内部的 `backend.connect()` 调用语义 (P3 W1 已经引入 `ensure_alive`，不重复)。
- 取消已经在跑的 SSH 任务 / 强制超时。

## 4. 验收标准 (acceptance criteria)

每个子项独立可验收，任一失败本次优化不通过：

### A. BotCard `Starting` 状态可视化

- BotCard 在 `process_changed_signal(qq_id, ProcessState.Starting)` 后：
  - `run_button` 隐藏 (或 `disable + 文案改为"启动中"`)
  - `stop_button` 隐藏
  - 显示 `IndeterminateProgressBar` (或等价的 `ProgressRing` 指示)
- `Running` / `NotRunning` 切换：进度指示恢复隐藏，按钮按现有逻辑显隐。
- 切换在主线程 ≤ 1 帧 (16ms) 内完成；不引入新的 timer。
- 测试: `script/test/test_bot_card_starting_state.py` 新增。

### B. 配置远端同步异步化

- `update_config(config)` 在主线程仅做本地写盘 + 调用 `_dispatch_remote_sync(config)`；
  后者把 SSH 写入派发到 `QThreadPool` 的 `_RemoteConfigSyncRunnable`。
- 同步完成后通过 `RemoteConfigSyncCenter` (单例) 发出 `sync_finished_signal(qq_id, ok, message)`；
  BotConfig 页订阅后用 InfoBar / 状态条反馈。
- 兼容现有 244 P2 + P3 用例：测试中默认走"内联同步"模式 (无 QApplication)，避免线程依赖。
- `delete_config` 同上。
- 测试: `script/test/test_operate_config_remote_sync_async.py` 新增。

### C. 本地 QProcess 启动去阻塞

- `create_napcat_process` 不再调用 `process.waitForStarted`；改为：
  - 立即 emit `process_changed_signal(qq_id, ProcessState.Starting)`
  - 把 `process` 挂到 `napcat_process_dict` (state=`Starting`)
  - `process.started` / `process.errorOccurred` 信号驱动 `Running` / `NotRunning`
- `_auto_start_bots` 自然受益：N 个 Bot 同步发起，主线程不再因每个 Bot 阻塞 0~5s。
- 失败语义：`errorOccurred` 触发后弹错误 InfoBar + emit `NotRunning`，与原 `waitForStarted=False` 路径等价。
- 测试: `script/test/test_local_process_async_start.py` 新增 + `test_run_napcat.py` 既有用例适配。

### D. 全局后台任务状态条

- 新增 `src/core/runtime/background_tasks.py`：
  ```python
  class BackgroundTaskCenter(QObject):
      task_started_signal = Signal(str, str)   # task_id, label
      task_finished_signal = Signal(str)        # task_id
      count_changed_signal = Signal(int)        # 当前任务数

      def begin(self, task_id: str, label: str) -> None: ...
      def end(self, task_id: str) -> None: ...
      def active_count(self) -> int: ...
      def active_tasks(self) -> list[tuple[str, str]]: ...
  ```
- 现有 Runnable 在 `run()` 入口 / 出口包装 `begin/end`：
  - `RemoteBotOperationRunnable` (start/stop/poll)
  - `_RemoteLogTailRunnable` (拉取日志，频次太高 -> **不上报** 给 Center，只算"自治轮询")
  - `BotMigrationRunnable`
  - `RedetectVersionsRunnable` / `ForceUpdateRunnable` / `RollbackRunnable` (P3.W2)
  - `_RemoteConfigSyncRunnable` (本次新增)
- BotPage Header 嵌入 `BackgroundTaskBar` widget：`count > 0` 时可见，显示 `IndeterminateProgressBar` + "后台任务 N · {首项 label}"。
- 测试: `script/test/test_background_task_center.py` 新增。

## 5. 非目标 (明确不做)

- 不替换现有 `QThreadPool` 为自定义 worker pool；
- 不改 `RemoteBackend` / `SSHClient` 公共 API；
- 不引入 `asyncio` / `qasync`；
- BackgroundTaskBar 不展示具体 progress 数值，只展示"任务数 + 首条 label"，避免 UI 噪音。

## 6. 完成语言策略 (delivery truth contract)

- 每个子项 PR 通过新增单测 + 全量 P2/P3 既有用例回归无衰退后，方可声称"完成"。
- 验收文档写在 `docs/general/remote_ssh_p3_perf_acceptance.md`。
- 网络相关 SSH 路径无法在 CI 跑，通过 `monkeypatch` 化的 backend mock 验证；标注"网络相关需手动验证"。

## 7. 风险与回滚

| 风险 | 应对 |
| --- | --- |
| `update_config` 异步化后测试时序变化 | 提供 `inline=True` 同步模式开关 + `wait_for_remote_sync()` 测试辅助 |
| `process.errorOccurred` 信号在某些 Linux QProcess 版本上偶发未触发 | fallback：`QTimer.singleShot(8000, _check_started)` 超时兜底 emit `NotRunning` |
| `BackgroundTaskCenter` 多线程 emit 竞态 | 内部 `threading.Lock` 互斥；信号自动跨线程 (Qt::QueuedConnection) |
| BotCard `Starting` 渲染因 `IndeterminateProgressBar` 资源占用过大 | 全局 1 个 Bar，按 qq_id 复用；或用 `ProgressRing` 替代 (CPU 占用低) |
