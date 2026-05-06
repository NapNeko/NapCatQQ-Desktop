# 远程 SSH P3 — 启动流程异步化与状态可见性 (execution plan)

> 关联需求: [`docs/requirements/2026-05-06-remote-ssh-p3-perf.md`](../requirements/2026-05-06-remote-ssh-p3-perf.md)
> 内部 grade: **L** (serial native execution from frozen plan)
> 入口模式: `vibe interactive_governed → high_autonomy`，终止于 `phase_cleanup`

## 1. 内部 Grade 决策

**Grade = L (native serial execution).**

理由：

- 子项 D (全局任务中心) 是 A/B 上报的"宿主"，必须先建；
- A/B/C 之间无相互依赖，可在 D 之上各自落地；
- 改动总量约 4 个文件 + 2~3 个新增测试文件，单 agent serial 完全可控；
- 无 wave 间并行需求，避免引入 XL 编排开销。

## 2. 波次结构

```
Wave 1 (Center) → Wave 2 (UI) → Wave 3 (Async) → Wave 4 (Verify)
```

| Wave | 子项 | 关键产物 | 验证 |
| --- | --- | --- | --- |
| W1 | **D** BackgroundTaskCenter + Header 状态条 | 新建 `src/core/runtime/background_tasks.py`；BotPage HeaderWidget 嵌入 `BackgroundTaskBar` | `test_background_task_center.py` |
| W2 | **A** BotCard Starting 渲染 + **C** 本地 QProcess 去阻塞 | 改 `card.py:slot_process_changed_button`；改 `napcat.py:create_napcat_process`；现有 Runnable 接 Center | `test_bot_card_starting_state.py`、`test_local_process_async_start.py` |
| W3 | **B** 配置同步异步化 | 改 `operate_config.py:update_config / delete_config`；新增 `_RemoteConfigSyncRunnable`；BotConfig UI 接信号 | `test_operate_config_remote_sync_async.py` |
| W4 | 全量回归 + 验收文档 + cleanup | `docs/general/remote_ssh_p3_perf_acceptance.md` + 进度更新 | 全量 P2 + P3 + 本期新增用例全绿 |

## 3. ownership boundaries (写入域)

每个 wave 写入域明确，禁止越界：

### W1 写入域

- 新建 `src/core/runtime/background_tasks.py`
- `src/ui/page/bot_page/widget/__init__.py` (导出 BackgroundTaskBar；≤10 行)
- 新建 `src/ui/page/bot_page/widget/background_task_bar.py`
- `src/ui/page/bot_page/widget/header.py` 嵌入 BackgroundTaskBar (≤30 行)
- 新建 `script/test/test_background_task_center.py`

### W2 写入域

- `src/ui/page/bot_page/widget/card.py` (slot_process_changed_button 三态化；≤40 行)
- `src/core/runtime/napcat.py`
  - `create_napcat_process` 去 `waitForStarted` (≤30 行)
  - 现有 Runnable 在 `run()` 包装 BackgroundTaskCenter `begin/end` (≤30 行)
- 新建 `script/test/test_bot_card_starting_state.py`
- 新建 `script/test/test_local_process_async_start.py`
- 既有 `script/test/test_run_napcat.py` 适配 (≤20 行)

### W3 写入域

- `src/core/config/operate_config.py` (sync 钩子改为 dispatch；≤80 行)
- 新建 `src/core/operation/config_sync.py` (`_RemoteConfigSyncRunnable` + `RemoteConfigSyncCenter`)
- `src/ui/page/bot_page/sub_page/bot_config.py` (订阅 sync_finished_signal；≤30 行)
- 新建 `script/test/test_operate_config_remote_sync_async.py`

### W4 写入域

- `docs/general/remote_ssh_p3_perf_acceptance.md` (新建)
- `docs/general/remote_ssh_progress.md` (更新进度章节)

## 4. 验证命令

每个 wave 结束跑一次基线回归 + 本 wave 新增用例：

```bash
# 全 P2 + P3 既有回归
python -m pytest \
  script/test/test_config_model.py \
  script/test/test_operate_config.py \
  script/test/test_legacy_import.py \
  script/test/test_config_load.py \
  script/test/test_local_backend.py \
  script/test/test_backend_resolver.py \
  script/test/test_remote_backend_process.py \
  script/test/test_local_port_forwarder.py \
  script/test/test_remote_deploy_runner.py \
  script/test/test_remote_deploy_probe.py \
  script/test/test_server_manager_deploy.py \
  script/test/test_server_registry.py \
  script/test/test_run_napcat.py \
  script/test/test_remote_process_manager.py \
  script/test/test_remote_log_buffer.py \
  script/test/test_ssh_line_splitter.py \
  script/test/test_path_func.py \
  script/test/test_bot_config_widget.py \
  script/test/test_bot_log_page.py \
  script/test/test_qr_code_dialog.py \
  script/test/test_connect_dialogs.py \
  script/test/test_ssh_client_persistent.py \
  script/test/test_remote_page_actions.py \
  script/test/test_bot_migration.py \
  -q

# P3 perf 新增 (按 wave 累加)
python -m pytest script/test/test_background_task_center.py -q              # W1
python -m pytest script/test/test_bot_card_starting_state.py \
                 script/test/test_local_process_async_start.py -q           # W2
python -m pytest script/test/test_operate_config_remote_sync_async.py -q    # W3
```

## 5. 完成语言规则

- W1/W2/W3 完成后只说 "Wave N done, regression green"，不写"P3 优化完成"；
- 仅在 W4 phase_cleanup 后才允许 "P3 启动流程优化已通过验收"；
- 任何 wave 部分子项失败 → 用 "blocked / partial" 措辞，不得宣称完成。

## 6. 回滚规则

- W1 (D) 出问题：`background_tasks.py` 与 BackgroundTaskBar 局部回退；其他子项不依赖 D 的副作用。
- W2 (A) 出问题：BotCard `slot_process_changed_button` 三态化的代码可整段 revert，回到原 `Running/else` 逻辑。
- W2 (C) 出问题：`waitForStarted` 改回 `True` 通过；保留 errorOccurred 信号挂接的代码作为额外鲁棒性。
- W3 (B) 失败：`update_config` 内部 `inline=True` 强制同步路径作为 fallback；本地写盘语义不变。

## 7. delivery acceptance plan

W4 通过条件：

1. 全量 P2/P3 用例 + 本期新增用例 100% 绿。
2. 手动验证 (网络相关无法 CI)：
   - 启动远端 Bot：BotCard 立即看到"启动中"指示 + 进度条；BotPage Header 状态条 +1。
   - 同时启动 2 个远端 Bot：状态条显示"后台任务 2 · {首项 label}"。
   - 保存远端 Bot 配置：保存按钮一瞬间响应；状态条显示"后台任务 1 · 同步配置"。
   - 启动多个本地 Bot：BotPage 仍可点击其他 tab，无明显冻结。
3. `docs/general/remote_ssh_p3_perf_acceptance.md` 已落盘。
4. `docs/general/remote_ssh_progress.md` 已更新 perf 章节。

## 8. phase_cleanup 期望

- 删除任何调试用临时文件；
- 提交按 wave 拆 commit，commit message 走中文 atomic commit 规范；
- 验收文档 `remote_ssh_p3_perf_acceptance.md` 落盘；
- `remote_ssh_progress.md` 更新到 "P3 启动流程优化已完成"。

## 9. 实现细节锚点

### 9.1 W1 — `BackgroundTaskCenter`

`src/core/runtime/background_tasks.py`:

```python
@dataclass(frozen=True)
class BackgroundTask:
    task_id: str
    label: str

class BackgroundTaskCenter(QObject):
    task_started_signal = Signal(str, str)   # task_id, label
    task_finished_signal = Signal(str)        # task_id
    count_changed_signal = Signal(int)

    def __init__(self) -> None:
        super().__init__()
        self._tasks: dict[str, BackgroundTask] = {}
        self._lock = threading.Lock()

    def begin(self, task_id: str, label: str) -> None: ...
    def end(self, task_id: str) -> None: ...
    def active_count(self) -> int: ...
    def active_tasks(self) -> list[BackgroundTask]: ...
```

入口约定：

- `task_id` 用 `uuid4().hex[:8]` 或 `f"{action}-{qq_id}"` 等可读形式；
- `label` 例：`"启动 Bot 12345"`、`"停止 Bot 12345"`、`"同步配置 12345"`、`"部署 NapCat (server01)"`。

### 9.2 W1 — `BackgroundTaskBar`

`src/ui/page/bot_page/widget/background_task_bar.py`:

```python
class BackgroundTaskBar(QWidget):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._progress = IndeterminateProgressBar(self)
        self._label = CaptionLabel("", self)
        self.hide()
        it(BackgroundTaskCenter).count_changed_signal.connect(self._on_count_changed)

    def _on_count_changed(self, count: int) -> None:
        if count <= 0:
            self.hide()
            return
        tasks = it(BackgroundTaskCenter).active_tasks()
        head_label = tasks[0].label if tasks else ""
        self._label.setText(self.tr(f"后台任务 {count} · {head_label}"))
        self.show()
```

### 9.3 W2 — BotCard `Starting` 状态

```python
def slot_process_changed_button(self, qq_id: str, state: QProcess.ProcessState) -> None:
    if qq_id != str(self._config.bot.QQID):
        return
    if state == QProcess.ProcessState.Starting:
        self.run_button.setEnabled(False)
        self.run_button.setText(self.tr("启动中…"))
        self.run_button.show()
        self.stop_button.hide()
        self._show_inline_spinner(True)
        return
    self._show_inline_spinner(False)
    self.run_button.setEnabled(True)
    self.run_button.setText(self.tr("启动"))
    if state == QProcess.ProcessState.Running:
        self.run_button.hide()
        self.stop_button.show()
        self.log_button.show()
        self.web_ui_button.show()
    else:
        self.run_button.show()
        self.stop_button.hide()
        self.log_button.hide()
        self.web_ui_button.hide()
```

`_show_inline_spinner(bool)` 在 BotCard 内通过 `IndeterminateProgressBar` 实现，附在 headerLayout 末尾。

### 9.4 W2 — 本地 QProcess 去阻塞

```python
process.stateChanged.connect(...)  # 原有
process.errorOccurred.connect(
    lambda err, emitted_qq_id=qq_id, emitted_process=process:
        self._handle_local_start_error(emitted_qq_id, emitted_process, err)
)
process.finished.connect(...)        # 原有

it(ManagerNapCatQQLog).create_log(config, process)
self.napcat_process_dict[qq_id] = NapCatProcessModel(
    qq_id=qq_id, process=process, state=QProcess.ProcessState.Starting,
    started_at=monotonic(),
)
self.process_changed_signal.emit(qq_id, QProcess.ProcessState.Starting)

process.start()
# 不再 waitForStarted
```

`_handle_process_state_changed` 已经会把 `Running` 同步到 `napcat_process_dict[qq_id].state` 并 emit；`errorOccurred` 单独清理。

### 9.5 W3 — `_RemoteConfigSyncRunnable`

`src/core/operation/config_sync.py`:

```python
class RemoteConfigSyncCenter(QObject):
    sync_finished_signal = Signal(str, str, bool, str)  # qq_id, action, ok, message

class _RemoteConfigSyncRunnable(QObject, QRunnable):
    def __init__(self, config: Config, action: str) -> None: ...
    # action ∈ {"write", "delete"}

    def run(self) -> None:
        task_id = f"cfg-sync-{action}-{qq_id}"
        it(BackgroundTaskCenter).begin(task_id, f"同步 Bot {qq_id} 配置")
        try:
            backend = resolve_backend_for_bot(self._config)
            if action == "write":
                backend.write_bot_runtime_config(self._config)
            elif action == "delete":
                backend.delete_bot_runtime_config(str(self._config.bot.QQID))
            it(RemoteConfigSyncCenter).sync_finished_signal.emit(qq_id, action, True, "")
        except Exception as exc:
            ...
        finally:
            it(BackgroundTaskCenter).end(task_id)
```

`operate_config.py` 改写：

```python
def update_config(...):
    ...
    _apply_json_transaction(...)
    _dispatch_remote_sync(config_to_save, action="write")
    return True
```

测试时通过 `_dispatch_remote_sync(..., inline=True)` 走原同步路径，不依赖 QApplication。
