# 远程 SSH P3 — perf 阶段验收 (acceptance)

> 关联文档:
> - [需求冻结文档](../requirements/2026-05-06-remote-ssh-p3-perf.md)
> - [P3 主计划](./remote_ssh_p3_plan.md)
> - [整体进度](./remote_ssh_progress.md)
>
> 完成日期: 2026-05-06
> 涉及子项: A / B / C / D 四项 (本期全部交付)
>
> **2026-05-06 修订**: 按用户要求把进度可视化迁移到组件库的
> [`ProgressInfoBar`](https://github.com/zhiyiYo/PyQt-Fluent-Widgets) (要求
> ``pyside6-fluent-widgets-qiao >= 2.0.17``):
> - BotPage Header 的自定义 `BackgroundTaskBar` 已删除
> - BotCard 内联的 `IndeterminateProgressBar` 已删除, Starting 仅按钮 disable + 文案
> - 全部进行中 / 完成反馈统一在主窗口右上 `ProgressInfoBar` 弹窗堆叠

## 0. 一行结论

按了"启动" / "保存配置" / "部署" 等按钮的瞬间, UI 不再卡; BotCard 立即把"启动"按钮
灰掉并改文案"启动中…", 主窗口右上同时弹出一个带旋转进度环的 `ProgressInfoBar`,
SSH 完成后自动切换到 ✅/❌ + 文案并 1.5s 淡出.

## 1. 子项 A — BotCard `Starting` 状态可视化

### 实现位置

- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/bot_page/widget/card.py:213-251`
  `slot_process_changed_button` 三态化:
  - `Starting`: `run_button.setEnabled(False)` + `setText("启动中…")`, 其他按钮隐藏;
    卡片视觉保持稳定, 不嵌入进度条
  - `Running`: 还原 `run_button` 文案后隐藏, 显示 stop / log / web_ui
  - `NotRunning`: `run_button` 重新可用, 文案恢复 "启动"
- 进度条动画与最终 ✅/❌ 反馈交由
  [`ProgressInfoBarBridge`](src/ui/components/progress_info_bar_bridge.py) 在主窗口
  右上以 `ProgressInfoBar` 展示 (子项 D)

### 验收

| 项                                             | 期望                                              | 验证                                                                                          |
| ---------------------------------------------- | ------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Starting 时按钮 disabled + 文案 "启动中…"      | run_button 可见, isEnabled=False, text=="启动中…" | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/script/test/test_bot_card_starting_state.py:115-129` |
| Running: stop/log/web_ui 显, run 隐 + 文案恢复 | 同上                                              | `test_running_state_restores_run_button_and_shows_stop`                                       |
| NotRunning: run 显且 enabled, 文案 "启动"      | 同上                                              | `test_not_running_state_restores_run_button`                                                  |
| qq_id 不匹配过滤                               | 其它 Bot 的 Starting 信号不影响本卡片             | `test_other_qq_id_does_not_trigger_render`                                                    |
| BotList 重建后保持 Starting                    | `update_info_card` 走 Starting 路径               | `test_update_info_card_reflects_starting_state`                                               |

### 验证命令

```bash
python -m pytest script/test/test_bot_card_starting_state.py -q
```

## 2. 子项 B — 配置远端同步异步化

### 实现位置

- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/config/operate_config.py:483-538`
  抽出 `_do_remote_sync_blocking(config)` (核心 SSH 写); 同名 `_sync_bot_runtime_config_to_remote`
  改为派发壳, UI 上下文走异步.
- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/config/operate_config.py:541-576`
  对称的 `_do_remote_delete_blocking(config)` + `_delete_bot_runtime_config_from_remote` 派发壳.
- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/config/operate_config.py:586-636`
  `_try_dispatch_remote_op_async(action, config)`: 检测 `QApplication.instance()` + 模块级
  `_FORCE_SYNC_REMOTE_CONFIG` toggle, 决定是否派发 `_RemoteConfigOpRunnable` 到 `QThreadPool.globalInstance()`.
- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/config/operate_config.py:666-731`
  `_RemoteConfigOpRunnable` 在 worker 内 `try/finally` 包到
  [`BackgroundTaskCenter`](../../src/core/runtime/background_tasks.py),
  自动驱动 BotPage Header 状态条出现 / 消失.

### 验收

| 项                                 | 期望                                              | 验证                                                                                                       |
| ---------------------------------- | ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| 无 QApp 同步保留                   | `test_operate_config.py` 既有 24 case 全部通过    | `python -m pytest script/test/test_operate_config.py -q -p no:cacheprovider --basetemp=C:/Temp/pytest-tmp` |
| 有 QApp 异步派发                   | `update_config` < 500ms 返回, 写发生在非主线程    | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/script/test/test_operate_config_remote_async.py:131-160`          |
| `delete_config` 对称               | 同上                                              | 同文件 `test_remote_delete_dispatches_to_qthreadpool_in_ui_context`                                        |
| `_FORCE_SYNC_REMOTE_CONFIG` toggle | autouse fixture 翻为 True 后 24 case 不受异步污染 | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/script/test/test_operate_config.py:44-55`                         |

### 验证命令

```bash
python -m pytest script/test/test_operate_config.py script/test/test_operate_config_remote_async.py -q -p no:cacheprovider --basetemp=C:/Temp/pytest-tmp
```

## 3. 子项 C — 本地 QProcess 启动去阻塞

### 实现位置

- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/napcat.py:1209-1258`
  新增 `_handle_local_start_error(qq_id, process, error)` — 区分 `FailedToStart` (清理字典 +
  emit `NotRunning` + 提示) 与运行期错误 (交给 `finished` 处理).
- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/napcat.py:1319-1361`
  `create_napcat_process` 删除 `process.waitForStarted(5000)` 同步阻塞;
  改为先把 `NapCatProcessModel(state=Starting)` 入字典 + emit `Starting`, 然后 `process.start()`,
  其余状态由 `stateChanged` (`-> Running`) 与 `errorOccurred` (`-> FailedToStart`) 异步驱动.
- `update_info_card` (`@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/bot_page/widget/card.py:163-187`)
  也识别 `Starting` 态, BotList 重建后启动中的本地 Bot 仍有指示.

### 验收

| 项                             | 期望                                             | 验证                                                                                   |
| ------------------------------ | ------------------------------------------------ | -------------------------------------------------------------------------------------- |
| 不再 `waitForStarted`          | `FakeProcess` 不再有 `waitForStarted` 方法       | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/script/test/test_run_napcat.py:63-88`         |
| 启动后立刻进入 Starting        | `process_changed_signal` 第一个事件 = `Starting` | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/script/test/test_run_napcat.py:469-516`       |
| FailedToStart 走 errorOccurred | 字典清理 + emit `NotRunning` + 提示              | 同文件 `test_create_napcat_process_emits_error_when_failed_to_start`                   |
| 异常退出仍清理                 | finished 路径不变                                | 同文件 `test_create_napcat_process_cleans_up_state_when_process_finishes_unexpectedly` |

### 验证命令

```bash
python -m pytest script/test/test_run_napcat.py -q
```

## 4. 子项 D — 全局后台任务可视化 (ProgressInfoBar 桥)

### 实现位置 (Center)

- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/background_tasks.py:38-147`
  `BackgroundTaskCenter` (`QObject` + creart 单例) 升级签名:
  - `BackgroundTask` 增加 `content` 字段 (用于 ProgressInfoBar 进行中描述)
  - 信号: `task_started_signal(str, str, str)` (task_id, label, content),
    `task_finished_signal(str)` (兼容订阅), `task_completed_signal(str, bool, str)`
    (task_id, success, message), `count_changed_signal(int)`
  - 公共 API: `begin(task_id, label, *, content="")`,
    `end(task_id, *, success=True, message="")`,
    `fail(task_id, message="")` (语义糖),
    `track(task_id, label, *, content="", success_message="")` (异常路径自动 fail)
  - 内部 `threading.Lock` 互斥, Qt 信号跨线程 `QueuedConnection` 自动派发到主线程

### 实现位置 (UI 桥)

- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/components/progress_info_bar_bridge.py:1-148`
  `ProgressInfoBarBridge`: 监听 `task_started_signal` → 在 parent 上 spawn
  不确定模式 `ProgressInfoBar.indeterminate(title=label, content=content, isClosable=False, duration=-1)`;
  监听 `task_completed_signal` → 找到对应 InfoBar 调
  `setComplete(success=, content=message, autoCloseAfter=1500)`, 自动切换 ✅/❌ 配色.
  使用 `weakref` 持有 InfoBar 引用; parent 销毁后桥自动停止 spawn.
- `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/window/main_window/window.py:44-70`
  MainWindow `initialize()` 末尾调 `_install_progress_info_bar_bridge()`,
  把桥挂到主窗口本身, 多任务 InfoBar 自动在右上角垂直堆叠.
- 旧的 `BackgroundTaskBar` widget 与 `HeaderWidget` 中的占位 **已移除** (`header.py` 高度
  恢复 48px, 不再嵌入状态条).

### 实现位置 (Runnable 接入)

| Runnable                             | 文件:行                                                                                       | task label                                                   |
| ------------------------------------ | --------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `RemoteBotOperationRunnable` (start) | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/runtime/napcat.py:457-545`                  | `启动远端 Bot {qq_id}`                                       |
| `RemoteBotOperationRunnable` (stop)  | 同上                                                                                          | `停止远端 Bot {qq_id}`                                       |
| `RemoteBotOperationRunnable` (poll)  | 同上                                                                                          | **不上报** (5s 轮询会让状态条频闪)                           |
| `BotMigrationRunnable`               | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/operation/migration.py:363-407`             | `迁移 Bot {qq_id} ({src} → {dst})`                           |
| `DeploymentRunner`                   | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/remote_page/deployment_runner.py:82-109` | `部署远端 NapCat ({server_id})` (含强制更新 / 强制重装变体)  |
| `RedetectRunner`                     | 同文件 :132-148                                                                               | `检测远端版本 ({server_id})`                                 |
| `RollbackRunner`                     | 同文件 :171-187                                                                               | `回滚远端部署 ({server_id})`                                 |
| `ConnectionTester`                   | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/ui/page/remote_page/connection_tester.py:42-54`  | `测试 SSH 连接 ({name or id})`                               |
| `_RemoteConfigOpRunnable`            | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/src/core/config/operate_config.py:691-718`           | `同步配置到远端 Bot {qq_id}` / `删除远端 Bot {qq_id} 的配置` |

### 验收

| 项                                  | 期望                                                     | 验证                                                                                                 |
| ----------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| begin/end 计数                      | 多次 begin / end 后 active_count 收敛                    | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/script/test/test_background_task_center.py:42-72`           |
| 重复 begin 仅更新 label             | 计数稳定                                                 | 同文件 `test_repeat_begin_updates_label_without_double_counting`                                     |
| 多线程并发                          | begin / end 总数无丢失                                   | 同文件 `test_concurrent_begin_end_converges`                                                         |
| `track()` 异常路径                  | 异常时仍 emit `end` + 失败文案                           | 同文件 `test_track_context_manager_marks_failure_with_exception_message`                             |
| `begin(content=...)` 透传           | content 进入 task_started_signal                         | 同文件 `test_begin_with_content_carries_through_to_started_signal`                                   |
| `end(success, message)` → completed | task_completed_signal 收到正确成败 + 文案                | 同文件 `test_end_emits_task_completed_signal_with_success_and_message`                               |
| `fail()` 是 end 的失败语义糖        | 等价 end(success=False, message=...)                     | 同文件 `test_fail_is_alias_for_end_with_success_false`                                               |
| 桥 spawn ProgressInfoBar            | begin → 在 parent 弹一个 indeterminate `ProgressInfoBar` | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/script/test/test_progress_info_bar_bridge.py:103-124`       |
| 桥完成态切 ✅/❌                      | end → InfoBar.setComplete(success, content)              | 同文件 `test_end_with_success_calls_setcomplete_on_corresponding_bar` / `..._failure_propagates_...` |
| 重复 begin 不重弹                   | 同 task_id 仅更新 title/content                          | 同文件 `test_repeat_begin_updates_existing_bar_in_place`                                             |
| 启动期回放                          | 桥构造时把 Center 已有任务也补弹                         | 同文件 `test_bridge_replays_existing_active_tasks_on_construction`                                   |
| InfoBar 构造异常容错                | logger.warning 而不让 Runnable 挂掉                      | 同文件 `test_bridge_swallows_progress_info_bar_construction_errors`                                  |
| Runnable 接入: start                | 单次会话 begin/end 一次                                  | `@/d:/NapCat-Project/NapCatQQ-Desktop-V1/script/test/test_runnable_background_task_wiring.py:88-105` |
| poll 不上报                         | 静默不进 Center                                          | 同文件 `test_remote_runnable_poll_does_not_track`                                                    |
| SSH 抛异常仍 end                    | UI 进度条不会卡死                                        | 同文件 `test_remote_runnable_end_emits_even_when_ssh_raises`                                         |
| `BotMigrationRunnable` 接入         | finished_event 收到 task_id                              | 同文件 `test_bot_migration_runnable_tracks_through_center`                                           |

### 验证命令

```bash
python -m pytest script/test/test_background_task_center.py script/test/test_progress_info_bar_bridge.py script/test/test_runnable_background_task_wiring.py -q
```

## 5. 全量回归

跨子项 + 既有 P2 / P3 用例的整合回归 (316 case + 1 expected skip):

```bash
python -m pytest \
  script/test/test_background_task_center.py \
  script/test/test_runnable_background_task_wiring.py \
  script/test/test_progress_info_bar_bridge.py \
  script/test/test_run_napcat.py \
  script/test/test_remote_process_manager.py \
  script/test/test_remote_backend_process.py \
  script/test/test_remote_page_actions.py \
  script/test/test_bot_migration.py \
  script/test/test_operate_config.py \
  script/test/test_operate_config_remote_async.py \
  script/test/test_remote_log_buffer.py \
  script/test/test_remote_runtime_status.py \
  script/test/test_remote_deploy_runner.py \
  script/test/test_remote_deploy_probe.py \
  script/test/test_server_manager_deploy.py \
  script/test/test_server_registry.py \
  script/test/test_bot_avatar.py \
  script/test/test_bot_card.py \
  script/test/test_bot_card_starting_state.py \
  script/test/test_bot_card_remove_guard.py \
  script/test/test_config_load.py \
  script/test/test_config_export.py \
  script/test/test_config_model.py \
  script/test/test_local_backend.py \
  script/test/test_local_port_forwarder.py \
  -q -p no:cacheprovider --basetemp=C:/Temp/pytest-tmp
# => 316 passed, 1 skipped, 1 warning
```

跳过的那条 (`test_remote_sync_falls_back_to_sync_when_no_qapplication`) 仅在
首个 `QApplication.instance()` 已被早期 case 创建时主动 skip; 单独跑该文件
时它会真实执行同步回退断言.

## 6. 手动验证 (网络相关)

CI 不能跑真实 SSH, 以下场景需要在本机/测试服上用 UI 验证:

| 场景                                | 期望现象                                                                                                                                                                                                                       |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 远端 Bot 点"启动"                   | 卡片"启动"按钮立刻灰掉 + 文案变 "启动中…"; 主窗口右上弹一个旋转环 ProgressInfoBar (title "启动远端 Bot {qq_id}", content "正在通过 SSH 启动…"); SSH 完成后 InfoBar 切 ✅ "Bot {qq_id} 启动成功" 并 1.5s 淡出, 卡片切到 "运行中" |
| 远端 Bot 启动失败 (强制 kill SSH)   | 右上 InfoBar 切 ❌, content 显示具体异常 (e.g. "TimeoutError: SSH timed out"); 卡片回到 "启动" 按钮可用                                                                                                                         |
| 远端 Bot 配置保存                   | 保存按钮立即响应; 右上 InfoBar "同步配置到远端 Bot {qq_id}" 旋转, 完成后切 ✅ "Bot {qq_id} 配置已同步"                                                                                                                          |
| 同时多个动作 (启动 A + 保存 B 配置) | 右上 InfoBar 自动垂直堆叠, 每个独立显示 ✅/❌; 全部完成后右上恢复干净                                                                                                                                                            |
| 本地 Bot 自动启动 N 个              | 应用启动后主线程立刻可交互, 每张卡片同步显示 "启动中…"; 不再像旧版本那样冻结 5 × N 秒                                                                                                                                          |
| 部署 / 强制更新 / 回滚              | 右上 InfoBar 显示对应 task label; 完成后 ✅/❌ 文案与底部 InfoBar 一致                                                                                                                                                           |
| 测试 SSH 连接                       | 右上 InfoBar "测试 SSH 连接 (xxx)"; 完成后切 ✅/❌ + ServerManager 返回的 message                                                                                                                                                |

## 7. 风险已知 / 后续

- **未覆盖**: 跨进程 / 跨设备多个用户同时操作同一 Bot 配置的并发. 与本次目标无关, P3 perf 不处理.
- **未实现**: BackgroundTaskCenter 任务取消按钮 (停止运行中的 SSH worker) — 与冻结文档"Out-of-Scope"一致.
- **测试边界**: `test_operate_config_remote_async.py::test_remote_sync_falls_back_to_sync_when_no_qapplication`
  在共享 pytest 进程下会被 skip; 当需要专门审查同步回退时, 单独跑该文件.

## 8. 完成声明

子项 A / B / C / D 全部满足"自动化测试通过 + 既有用例无衰退"两个条件,
本次 P3 perf 阶段交付完成.
