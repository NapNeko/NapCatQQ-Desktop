# 远程 SSH P4 — 高级能力 (execution plan)

> 关联需求: [`docs/requirements/2026-05-06-remote-ssh-p4.md`](../requirements/2026-05-06-remote-ssh-p4.md)
> 内部 grade: **L** (serial native execution from frozen plan)
> 入口模式: `vibe interactive_governed`，终止于 `phase_cleanup`
> 起草日期: 2026-05-06

## 1. 内部 Grade 决策

**Grade = L (native serial execution).**

理由:

- 6 个子项 (F2/F3/F4/F5/F6/F7) 之间存在显式数据依赖 (F4 消费 F3 采样, F7 修改 F6 已经触及的流式路径), 串行可控.
- 单 wave 内部多个独立小子项 (W1 的 F2 + F5 四件套, W2 的 F3 + F4) 在严格 ownership 切片下可视为
  step-level 可并行候选; 但 P3 perf 实测 L grade 单 agent 已足够, **本期不开启 XL 子代理派发**,
  保留 step-level 可并行的描述供后续编排参考.
- 改动总量约 8 个新增模块 + 12 个修改文件 + 12 个新增测试文件, L grade 单 agent serial 可控.

## 2. 波次结构

```
Wave 1 (F2 批量 + F5 体验)
  → Wave 2 (F3 资源监控 → F4 聚合面板/首页卡片)
  → Wave 3 (F6 持久数据迁移)
  → Wave 4 (F7 流式重连 + 全量回归 + 验收)
```

| Wave | 子项 | 关键产物 | 主要验证 |
| --- | --- | --- | --- |
| **W1** | **F2** 批量 Bot 管理 + **F5** 体验细节 (4 子项) | `batch_dispatcher.py`、BotPage 批量模式、`host_key_policy.py` + 对话框、`credential_store.py`、私钥拖拽、`friendly_errors.py` | `test_batch_bot_dispatcher.py`、`test_bot_page_batch_mode.py`、`test_ssh_host_key_policy.py`、`test_host_key_confirm_dialog.py`、`test_credential_keyring.py`、`test_server_edit_dialog_remember.py`、`test_server_edit_dialog_drag_drop.py`、`test_friendly_errors.py` |
| **W2** | **F3** 资源监控 → **F4** 聚合面板 / 首页卡片 | `resource_monitor.py` + `RemoteBackend.sample_resources`、`StatusOverviewDialog`、`RemoteSummaryCard` | `test_remote_backend_sample_resources.py`、`test_resource_monitor_service.py`、`test_status_overview_dialog.py`、`test_remote_summary_card.py` |
| **W3** | **F6** NapCat 持久数据迁移 | `MigrationService.transfer_persistent_data`、字节级 `migration_progress_signal`、`.partial` 续传逻辑、`MigrationDialog` 解锁勾选项 | `test_migration_persistent_data.py`、`test_migration_dialog_persistent_flag.py` |
| **W4** | **F7** `exec_stream` 中途断开自动重连 + 全量回归 + 验收文档 + cleanup | `_iter_stream_with_resume`、`install_napcat` / log tail 接入、`docs/general/remote_ssh_p4_acceptance.md`、进度文档同步 | `test_exec_stream_resume.py` + 全量 P0/P1/P2/P3/P4 回归 |

## 3. ownership boundaries (写入域)

每个 wave 写入域明确, 禁止越界. 任何越界改动需回到本计划修订.

### W1 写入域

新增:
- `src/core/operation/batch_dispatcher.py` (F2)
- `src/core/remote/host_key_policy.py` (F5.1)
- `src/core/remote/credential_store.py` (F5.2)
- `src/core/remote/friendly_errors.py` (F5.4)
- `src/ui/components/host_key_confirm_dialog.py` (F5.1)
- `script/test/test_batch_bot_dispatcher.py`
- `script/test/test_bot_page_batch_mode.py`
- `script/test/test_ssh_host_key_policy.py`
- `script/test/test_host_key_confirm_dialog.py`
- `script/test/test_credential_keyring.py`
- `script/test/test_server_edit_dialog_remember.py`
- `script/test/test_server_edit_dialog_drag_drop.py`
- `script/test/test_friendly_errors.py`

修改:
- `src/ui/page/bot_page/__init__.py` 批量模式入口与工具条 (≤120 行)
- `src/ui/page/bot_page/widget/card.py` 复选框态 (≤30 行)
- `src/core/remote/ssh_client.py` 接入 `host_key_policy` (≤40 行, F5.1)
- `src/ui/page/remote_page/server_edit_dialog.py` 拖拽 + 记住密码 (≤80 行, F5.2 + F5.3)
- `pyproject.toml` / `requirements.txt` 添加 `keyring >= 24` (Windows-only marker)
- 各 RemotePage / BotPage / ServerEditDialog 的错误展示路径切换到 `to_friendly` (≤60 行散点, F5.4)

### W2 写入域

新增:
- `src/core/remote/resource_monitor.py` (F3)
- `src/ui/components/status_overview_dialog.py` (F4)
- `src/ui/components/remote_summary_card.py` (F4)
- `script/test/test_remote_backend_sample_resources.py`
- `script/test/test_resource_monitor_service.py`
- `script/test/test_status_overview_dialog.py`
- `script/test/test_remote_summary_card.py`

修改:
- `src/core/remote/execution_backend.py` 增加 `sample_resources()` 抽象方法 (≤20 行)
- `src/core/remote/server_manager.py` 在 `connect_server` / `disconnect_server` 钩子中
  `attach`/`detach` `ResourceMonitorService` (≤40 行)
- `src/ui/page/remote_page/__init__.py` 工具栏新增"状态总览"按钮 (≤30 行)
- `src/ui/page/home_page/__init__.py` (或对应模块) 嵌入 `RemoteSummaryCard` (≤40 行)

### W3 写入域

新增:
- `script/test/test_migration_persistent_data.py`
- `script/test/test_migration_dialog_persistent_flag.py`

修改:
- `src/core/config/operate_config.py` `MigrationService` 增加 `transfer_persistent_data` 分支 (≤200 行)
- `src/core/remote/execution_backend.py` 增加 `read_bytes` / `write_bytes` / `stat` 流式接口
  (LocalBackend / RemoteBackend 同步实现; ≤120 行合计)
- `src/ui/components/migration_dialog.py` 解锁原 future flag + 字节级进度条 (≤60 行)

### W4 写入域

新增:
- `script/test/test_exec_stream_resume.py`
- `docs/general/remote_ssh_p4_acceptance.md`

修改:
- `src/core/remote/ssh_client.py` 增加 `_iter_stream_with_resume` (≤80 行, F7)
- `src/core/remote/deployment.py` `install_napcat` 进度流接入 (≤30 行, F7)
- `src/core/runtime/napcat.py` `RemoteNapCatQQLog._tail_loop` 接入 (≤30 行, F7)
- `docs/general/remote_ssh_progress.md` 标记 P4 完成

## 4. 验证命令

每个 wave 结束跑一次基线回归 + 本 wave 新增用例.

### 4.1 基线回归 (每 wave 必跑)

```bash
# P0 / P1 / P2 / P3 + P3 perf 全量
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
  script/test/test_background_task_center.py \
  script/test/test_bot_card_starting_state.py \
  script/test/test_local_process_async_start.py \
  script/test/test_operate_config_remote_async.py \
  script/test/test_runnable_background_task_wiring.py \
  script/test/test_progress_info_bar_bridge.py \
  -q
```

### 4.2 P4 各 wave 新增 (按 wave 累加)

```bash
# W1
python -m pytest \
  script/test/test_batch_bot_dispatcher.py \
  script/test/test_bot_page_batch_mode.py \
  script/test/test_ssh_host_key_policy.py \
  script/test/test_host_key_confirm_dialog.py \
  script/test/test_credential_keyring.py \
  script/test/test_server_edit_dialog_remember.py \
  script/test/test_server_edit_dialog_drag_drop.py \
  script/test/test_friendly_errors.py \
  -q

# W2
python -m pytest \
  script/test/test_remote_backend_sample_resources.py \
  script/test/test_resource_monitor_service.py \
  script/test/test_status_overview_dialog.py \
  script/test/test_remote_summary_card.py \
  -q

# W3
python -m pytest \
  script/test/test_migration_persistent_data.py \
  script/test/test_migration_dialog_persistent_flag.py \
  -q

# W4
python -m pytest script/test/test_exec_stream_resume.py -q
```

## 5. 完成语言规则

- W1/W2/W3 完成后只说 "Wave N done, regression green", 不写 "P4 完成";
- 任何 wave 部分子项失败 → 用 "blocked / partial" 措辞, 不得宣称完成;
- 仅在 W4 phase_cleanup 后 + `docs/general/remote_ssh_p4_acceptance.md` 落盘 + 进度文档同步,
  方可声称 "P4 高级能力已通过验收".
- F5.2 keyring / F5.1 host key 等涉及真实凭据的路径, 测试通过后仍需在 acceptance 文档
  标注 "需手动 Windows 真机验证", 不得仅凭 mock 通过即声称交付完成.

## 6. 回滚规则

按 wave 隔离的回滚边界:

- **W1**
  - F2: BotPage 批量入口 + `batch_dispatcher.py` 一键 revert; BotCard 复选框态以 feature flag (`_BATCH_MODE_ENABLED`) 关闭即可恢复.
  - F5.1: `SSHClient` 改回 `AutoAddPolicy` (单行切换); 对话框模块隔离, 不影响其他路径.
  - F5.2: `credential_store.py` 提供 `KeyringDisabled` fallback 常量, 出问题时直接走配置文件.
  - F5.3 / F5.4: 纯 UI / 文案改动, 整段 revert 即可.
- **W2**
  - F3: `ResourceMonitorService.attach` 内部第一行 short-circuit 即可关停采样, 不影响 ServerManager 主路径.
  - F4: `RemoteSummaryCard` / `StatusOverviewDialog` 模块隔离, 整段 revert; 入口按钮挂在工具栏, 一行隐藏即可.
- **W3**
  - F6: `transfer_persistent_data` 默认走 `False` (即 P3 既有行为) 作为 fallback; 失败时 `MigrationDialog` 取消勾选项即等同回滚.
  - 已落盘的 `.partial` 文件对运行时无副作用 (NapCat 不会读取), 用户可手动清理.
- **W4**
  - F7: `_iter_stream_with_resume` 提供 `_RESUME_ENABLED` 常量开关, 关闭后退化为现有 `exec_stream` 直读路径, P3·C 入口前探活行为不受影响.

## 7. delivery acceptance plan

W4 通过条件:

1. 全量 P0/P1/P2/P3/P3-perf/P4 用例 100% 绿; 失败用例数 = 0; xfail 不增加.
2. 手动验证 (网络相关无法 CI) 必跑清单:
   - **F2**: 进入 BotPage 批量模式, 选 3 个 Bot (含 1 远端 + 2 本地), 批量启动 → 全部 Starting → 全部 Running; 主窗口右上 ProgressInfoBar 聚合展示; 失败聚合 InfoBar 详情可展开.
   - **F3**: 远端服务器连接成功后, 状态总览面板 30s 内出现首条采样; 模拟 stress 进程让 CPU 飙至 95% 持续 30s, 观察阈值 InfoBar 一次性弹出 + 5min 内不重复.
   - **F4**: HomePage `RemoteSummaryCard` 显示服务器 / 在线 Bot 数; 点击跳转 RemotePage 选中正确服务器; 状态总览面板三栏数据与 RemotePage / BotPage 实时一致.
   - **F5.1**: 添加新服务器 → 弹 host key 确认对话框, 显示 SHA256 指纹; 选 "信任并保存" 后再次连接不再弹窗; 篡改 `known_hosts` 后下次连接弹红色警告对话框.
   - **F5.2**: ServerEditDialog 勾选 "记住密码" 保存 → 重启 Desktop → 再次打开仍可连接; 配置文件中 `password` 字段为空, `password_source: "keyring"`; 在 Windows Credential Manager 中可见 `napcat-desktop:ssh` 条目.
   - **F5.3**: 拖拽 `id_rsa` 文件到私钥路径输入框 → 自动填充; 拖拽多文件 / 目录 → 弹"请拖入单个私钥文件".
   - **F5.4**: 故意输错密码 → InfoBar 显示 "用户名或密码错误"; 故意填错端口 → "目标端口拒绝连接".
   - **F6**: Bot 在远端有 NapCat 数据库后切换 runtime_target 至本地 → MigrationDialog 勾选"搬运 NapCat 持久数据" → 进度条字节级推进; 中途断网, 重试时跳过已完成分片; 完成后本地 NapCat 数据完整可用.
   - **F7**: 部署 NapCat 期间手动断开 SSH (`ifconfig ssh down` 或路由切换 1s) → 部署不报错, 进度从断点续接; 远端 log tail 期间断网 1s → 日志面板恢复后不丢失新行.
3. `docs/general/remote_ssh_p4_acceptance.md` 已落盘, 含上述手动验证执行记录.
4. `docs/general/remote_ssh_progress.md` 已更新 P4 章节为 "已完成".
5. `pyproject.toml` 中 `keyring >= 24` 依赖以 `; sys_platform == "win32"` 标记或同等条件, 不污染 Linux/Mac 开发环境.

## 8. phase_cleanup 期望

- 删除任何调试用临时文件 (e.g. 抓包脚本 / 手动验证截图临存); `runtime/` 下不留遗留日志.
- 提交按 wave 拆 commit, 每 wave 内按子项拆 atomic commit, commit message 走中文 atomic commit 规范 (无 emoji, 无 Co-authored-by).
- 验收文档 `docs/general/remote_ssh_p4_acceptance.md` 落盘.
- `docs/general/remote_ssh_progress.md` 更新到 "P4 高级能力已完成"; P3 推迟项 (D / G / 持久数据迁移 / `exec_stream` 重连) 显式标记 "已由 P4 兑现".
- 输出 vibe runtime artifacts:
  - `outputs/runtime/vibe-sessions/<run-id>/skeleton-receipt.json`
  - `outputs/runtime/vibe-sessions/<run-id>/intent-contract.json`
  - 各 wave 的 `phase-N.json`
  - `cleanup-receipt.json`

## 9. 实现细节锚点

### 9.1 W1 — `BatchDispatcher` (F2)

`src/core/operation/batch_dispatcher.py`:

```python
@dataclass(frozen=True)
class BatchOutcome:
    qq_id: str
    ok: bool
    error: str | None

class BatchDispatcher(QObject):
    progress_signal = Signal(int, int)              # done, total
    finished_signal = Signal(list)                  # list[BatchOutcome]

    def dispatch(
        self,
        action: Literal["start", "stop", "migrate", "delete"],
        configs: list[Config],
        *,
        sequential: bool = False,                   # migrate/delete 必须 True
        target_runtime: str | None = None,          # migrate 用
    ) -> str:                                       # 返回 batch_id
        ...
```

- 内部派发到 `QThreadPool.globalInstance()`; `sequential=True` 时排成 1 worker 链式 next-on-finish.
- 单一 batch_id 在 `BackgroundTaskCenter` 上报为 1 个聚合任务 (label 含 `(N)`), 不重复登记 N 个子任务.
- 完成后 `finished_signal` 携带全部 `BatchOutcome`, BotPage 据此聚合 InfoBar.

### 9.2 W1 — `host_key_policy` (F5.1)

`src/core/remote/host_key_policy.py`:

```python
class InteractiveHostKeyPolicy(paramiko.MissingHostKeyPolicy):
    def __init__(self, known_hosts_path: Path, ui_callback: Callable[[str, str, str], HostKeyDecision]) -> None: ...
    # ui_callback(hostname, key_type, fingerprint_sha256_b64) -> trust_save / trust_once / reject
    # 主线程 invokeMethod 调度回 UI; 在 worker 线程同步等待 UI 决策

    def missing_host_key(self, client, hostname, key) -> None:
        decision = self._invoke_ui_blocking(hostname, key.get_name(), self._fingerprint(key))
        if decision is HostKeyDecision.REJECT:
            raise paramiko.SSHException("用户拒绝信任主机指纹")
        if decision is HostKeyDecision.TRUST_SAVE:
            self._known_hosts.add(hostname, key)
            self._known_hosts.save()
```

- `known_hosts` 路径: `%LOCALAPPDATA%/NapCatQQ-Desktop/ssh/known_hosts` (复用现有 user data 目录解析).
- 已知主机指纹变化由 paramiko 默认 `BadHostKeyException` 路径触发, UI 单独捕获弹红色警告对话框, 不允许"信任并覆盖" (强制需用户手动从 known_hosts 删除条目).

### 9.3 W1 — `credential_store` (F5.2)

`src/core/remote/credential_store.py`:

```python
class CredentialStore:
    def __init__(self, *, namespace: str = "napcat-desktop:ssh") -> None: ...

    def store_password(self, server_id: str, password: str) -> bool:        # 失败返回 False
    def load_password(self, server_id: str) -> str | None:
    def delete_password(self, server_id: str) -> None:
    def is_available(self) -> bool:                                         # 仅 win32 + keyring 可用时 True
```

- 内部 `try: import keyring; keyring.get_keyring()`; 非 Windows 或 keyring 异常时
  `is_available() -> False`, ServerEditDialog 自动隐藏 "记住密码" 勾选项.

### 9.4 W2 — `ResourceMonitorService` (F3)

`src/core/remote/resource_monitor.py`:

```python
class ResourceMonitorService(QObject):
    sample_arrived = Signal(str, ResourceSample)
    threshold_breached = Signal(str, str, float)   # server_id, metric, value

    INTERVAL_OK = 10.0
    INTERVAL_BACKOFF = (10.0, 30.0, 60.0)
    BREACH_COOLDOWN = 300.0

    def attach(self, server_id: str) -> None:
        if server_id in self._workers:
            return
        worker = _SamplerWorker(server_id, self._on_sample, self._on_failure)
        QThreadPool.globalInstance().start(worker)
        self._workers[server_id] = worker

    def detach(self, server_id: str) -> None:
        worker = self._workers.pop(server_id, None)
        if worker is not None:
            worker.stop()
```

- 阈值连续 3 个采样点 (CPU 30s) 才触发, 用滑动窗口实现, 减少抖动.
- 同一 (server_id, metric) breach 5 分钟内只发 1 次信号; UI 层接到信号后弹一次性 InfoBar.

### 9.5 W2 — `StatusOverviewDialog` 数据装配 (F4)

```python
class StatusOverviewDialog(MaskDialogBase):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._wire_signals()
        self._refresh_full()    # 一次性拉取当前快照

    def _wire_signals(self) -> None:
        sm = it(ServerManager)
        sm.server_status_changed.connect(self._on_server_changed)
        it(ResourceMonitorService).sample_arrived.connect(self._on_sample)
        it(BackgroundTaskCenter).count_changed_signal.connect(self._on_tasks_changed)
        # 远端 Bot 状态走 ManagerNapCatQQProcess.process_changed_signal
```

- 三栏 ListView 数据全部由 ServerManager / ResourceMonitorService / BackgroundTaskCenter / ManagerNapCatQQProcess 既有信号驱动, 不引入新单例 / 不写持久化.

### 9.6 W3 — `MigrationService.transfer_persistent_data` (F6)

```python
def migrate(
    self,
    bot: Config,
    target_runtime: str,
    *,
    transfer_persistent_data: bool = False,
    progress_cb: Callable[[int, int], None] | None = None,
) -> MigrationResult:
    ...
    if transfer_persistent_data:
        for src_root, rel_paths in self._whitelist_for(bot):
            for rel in rel_paths:
                self._copy_with_resume(src_backend, dst_backend, src_root / rel, ..., progress_cb)
```

`_copy_with_resume(...)` 关键约定:

- 1 MiB chunk; 每 chunk 完成 `progress_cb(chunk_done, total)`.
- 目标 `path.partial` 已存在时, 读其大小作为 resume offset.
- 完成后 `rename(.partial → 真名)`.
- 远端 → 本地走 `RemoteBackend.read_bytes(path, offset, chunk_size)`; 本地 → 远端反向; 双远端不做 (P4 不支持远端互拷).

### 9.7 W4 — `_iter_stream_with_resume` (F7)

`src/core/remote/ssh_client.py`:

```python
def _iter_stream_with_resume(
    self,
    open_stream: Callable[[int], Iterator[bytes]],
    progress_marker: Callable[[bytes], int | None],
    *,
    max_retries: int = 3,
) -> Iterator[bytes]:
    """
    open_stream(offset_or_step) 每次重连时调用, 由调用方决定 'offset 是字节还是 PROGRESS step';
    progress_marker(chunk) 解析每个 chunk 的进度, 返回新的 'resume key' 或 None;
    """
    delays = (1.0, 3.0, 8.0)
    last_resume = 0
    for attempt in range(max_retries + 1):
        try:
            for chunk in open_stream(last_resume):
                key = progress_marker(chunk)
                if key is not None:
                    last_resume = key
                yield chunk
            return
        except (paramiko.SSHException, OSError) as exc:
            if attempt == max_retries:
                raise RemoteConnectionLost("流式命令重连耗尽") from exc
            self.ensure_alive()
            time.sleep(delays[min(attempt, len(delays) - 1)])
```

- `install_napcat`: `progress_marker` 解析 `[PROGRESS] N` 行, `open_stream` 启动脚本时把 `RESUME_FROM=N` 注入环境 (脚本本身幂等, `< N` 步骤跳过).
- `RemoteNapCatQQLog._tail_loop`: `progress_marker` 用 chunk 累加字节数, `open_stream` 用 `tail -c +{offset} -F {log}`.

## 10. 风险与变更控制

| 风险 | 触发判据 | 应对 |
| --- | --- | --- |
| W1 keyring 在某些 Windows 用户场景写入失败 | `test_credential_keyring.py` mock 通过但真机弹安全提示 | 自动降级 + InfoBar 提示, 不阻塞保存; 若真机失败率 >10% 升级为单独子项排查 |
| W2 多服务器 (>= 5) 时 10s 轮询累计 SSH 负荷过大 | 真机观察连接数 / CPU | 每服务器 1 worker 复用持久 SSHClient (P3·C 已实现); 失败采样指数退避; 必要时拓 ServerEditDialog "关闭监控" 开关 |
| W3 大文件 (>500 MiB) 搬运超过 30 分钟引起用户体验问题 | 手动验证含一次大体量搬运 | UI 显示估算大小 + 已传 / 总大小 + 速率; 若实测 < 5 MiB/s 触发 P5 优化项 (chunk 大小 / 并发上传) |
| W4 F7 流式重连导致部署脚本"幂等"假设破裂 | install_napcat 真机断网测试出现重复 dpkg 安装 | 立即关闭 `_RESUME_ENABLED`, 退化为现状; 单独修复脚本幂等性后再启用 |
| 各 wave 写入域越界 | wave 提交前 grep 比对实际 diff 路径 | 越界提交需走本计划修订并重提 PR; phase-N.json 中显式标记 |

## 11. step-level 可并行候选 (本期不启用)

L grade 单 agent serial 即可交付. 若后续编排需要 XL 子代理派发, 以下子项可视为 step 内独立单元:

- **W1 内部**: F2 / F5.1 / F5.2 / F5.3 / F5.4 五条独立轨道, 写入域无重叠.
- **W2 内部**: F3 (core) 与 F4 (UI) 在接口冻结后可并行; F4 可先用 mock `ResourceMonitorService` 推进.

如果未来开启 XL, 子代理 prompt 末尾必须保留 `$vibe`, 且不得新增 `docs/requirements/` / `docs/plans/` 下的文档.
