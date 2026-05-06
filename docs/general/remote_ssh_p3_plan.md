# 远程 SSH P3 — 体验优化与稳态收尾（requirement + plan）

> 关联文档:
> - [`docs/general/remote_ssh_plan.md`](./remote_ssh_plan.md) §7 P3 章节
> - [`docs/general/remote_ssh_p2_acceptance.md`](./remote_ssh_p2_acceptance.md) §4 (P3 范围确认)
> - [`docs/general/remote_ssh_progress.md`](./remote_ssh_progress.md)
>
> **运行模式**: `vibe interactive_governed → high_autonomy`
> （freeze 后一路推进到 phase_cleanup, 仅在硬阻塞 / 破坏性决策时打断）

---

## 1. 范围冻结 (requirement_doc)

### 1.1 In-Scope（核心 P3）

| 子项 | 标签 | 简述 |
| --- | --- | --- |
| **A** | 版本检测与更新 | RemotePage 暴露"刷新版本 / 强制更新 NapCat / 强制重装 LinuxQQ"入口；后端能力 (`redetect_versions`, `deploy_server(force_*)`) 已就绪 |
| **B** | Bot 运行位置迁移（含数据搬运） | 用户在配置页切换 `runtime_target` 后自动停源端 → 搬运配置 + NapCat 持久数据（`config/`, `data/`, `cache/`）→ 启动目标 |
| **C** | SSH 持久连接升级 | `SSHClient` 加 keepalive + 操作前活性探测 + 自动重连一次；`RemoteBackend` 在所有 SSH 调用入口统一走 `_ensure_alive()` |
| **E** | 远端 BotPage 日志面板收尾 | `RemoteNapCatQQLog` 已实现，本期补完体验：错误状态提示、标题后缀 "· 远端 (host)"、停止时清理、空内容 fallback |
| **F** | 部署失败 / 手动回滚 UI 入口 | RemotePage 新增"回滚"按钮：二次确认 + 进度回显；后端复用 `ServerManager.rollback_server` |

### 1.2 Out-of-Scope（推迟到 P3.5 / P4）

- **D**: 多服务器/Bot 状态聚合面板、首页远程状态卡片
- **G**: 体验细节（首次指纹确认对话框、密码 keyring、私钥拖拽、人话错误提示文案统一） — 单独立项
- 自动切换（本地不可用 → 远程）— P4 评估
- 远端 CPU / 磁盘资源监控 — P4

### 1.3 验收标准（acceptance criteria）

每个子项独立可验收，任一失败 P3 不通过：

1. **A**: RemotePage 选中已部署服务器后，能看到"刷新版本"按钮（异步触发 `redetect_versions`），以及"强制更新 NapCat" / "强制重装 LinuxQQ" 两个二次确认按钮（异步触发 `deploy_server(force_*=True)`）。失败有 InfoBar 提示，过程中按钮 disable。
2. **B**: 在 BotConfig 页改 `runtime_target` 后保存触发 `MigrationDialog`：
   - 用户可选"仅迁移配置" / "迁移配置 + NapCat 数据"
   - 取消则恢复原 target，不写盘
   - 确认后异步执行：停源端 Bot → 搬运 → 在新 target 不自动启动（让用户手动开）
   - 迁移失败：原 target 不动，弹错误对话框
3. **C**:
   - `SSHClient.connect` 调用 `transport.set_keepalive(30)`
   - 新增 `SSHClient.ensure_alive(reconnect=True)`：检测会话死亡时自动重连，最多 1 次
   - `run / exec_stream / read_text / write_text / open_sftp / open_local_tunnel / remote_*` 入口在网络异常时调一次 `ensure_alive(reconnect=True)` 然后重试一次
   - 重连成功 / 失败有 logger 记录
4. **E**:
   - `BotLogPage` 标题：远端 Bot 显示 `Bot 日志(QQID) · 远端 [服务器名]`
   - SSH tail 失败 N 次（默认 3）后日志缓冲区注入一行 `[ERROR] 远端日志拉取失败: …` 并停止轮询；用户重启 Bot 时重新 enqueue
   - Bot 停止 / 删除时，`RemoteNapCatQQLog._tail_timer` 必停
5. **F**: RemotePage 服务器卡片新增"回滚"按钮，仅在 `deployment_state ∈ {DEPLOYED, FAILED}` 显示。点击弹 `MessageBox` 二次确认 + "是否同时清理 LinuxQQ" 复选；执行期间复用现有 `deployment_log`/`deployment_progress`/`deployment_finished` 信号。

### 1.4 非目标（明确不做）

- 不对 `SSHClient` 内部协议做重写（只补 keepalive + 重连）
- 不引入连接池、多 channel 并发优化（paramiko 默认即可满足）
- B 不强制要求"双向数据校验"，搬运成功即算成功，不做 hash 比对（数据规模小）
- 不实现 Bot 在多服务器之间的"软迁移"（运行时无停机）；P3 接受短停机

### 1.5 完成语言策略 (delivery truth contract)

- 每个子项 PR 通过单测 + 全量 244 P2 用例回归无衰退后，方可声称"完成"
- 直接/集成 SSH 测试无法在 CI 跑（需要真实远端），通过 mock-paramiko 单测验证；标注为 "网络相关需手动验证"
- 验收文档写在 `docs/general/remote_ssh_p3_acceptance.md`

---

## 2. 内部执行计划 (xl_plan)

### 2.1 内部 Grade

**Grade = L（serial native execution from frozen plan）**

理由：
- 子项之间存在依赖序：C 是 B/E/F/A 的稳态底座，先做
- UI 改动多，难以可靠并行
- 单 agent 完全可控

### 2.2 波次结构

```
Wave 1 (底座) → Wave 2 (UI 入口能力) → Wave 3 (复杂业务) → Wave 4 (验证 + cleanup)
```

| Wave | 子项 | 关键产物 | 验证 |
| --- | --- | --- | --- |
| W1 | **C** SSHClient 持久化 | `ssh_client.py` `set_keepalive` + `ensure_alive` + 包装 `run/exec_stream/sftp/tunnel` | `test_ssh_client_persistent.py`（mock paramiko transport） |
| W2 | **F** 回滚 UI + **A** 版本/更新 UI | `remote_page/__init__.py` 增按钮 + `RollbackRunnable`/`RedetectRunnable`/`ForceUpdateRunnable` | `test_remote_page_actions.py` (Qt mock) |
| W3 | **B** 迁移服务 + 向导 + **E** 日志收尾 | `core/operation/migration.py` + `bot_page/widget/migration_dialog.py` + `BotLogPage` 远端标识 + `RemoteNapCatQQLog` 错误退避 | `test_bot_migration.py`、`test_bot_log_page.py` 扩展、`test_remote_log_buffer.py` 扩展 |
| W4 | 全量回归 + 验收文档 | `remote_ssh_p3_acceptance.md` + `phase_cleanup` 报告 | 244+ P2 + 新增 P3 用例全绿 |

### 2.3 ownership boundaries（写入域）

每个 wave 写入域明确，禁止越界：

- W1 写入域: `src/core/remote/ssh_client.py`、`src/core/remote/tunnel.py`（仅 keepalive 钩子）、`script/test/test_ssh_client_persistent.py`（新建）
- W2 写入域: `src/ui/page/remote_page/__init__.py`、新建 `src/ui/page/remote_page/maintenance_runnables.py`、新建 `script/test/test_remote_page_actions.py`
- W3 写入域:
  - 新建 `src/core/operation/migration.py`（搬运服务）
  - 新建 `src/ui/page/bot_page/widget/migration_dialog.py`
  - `src/ui/page/bot_page/widget/config.py`（捕获保存事件）
  - `src/core/runtime/napcat.py`（`RemoteNapCatQQLog` 错误退避; ≤30 行）
  - `src/ui/page/bot_page/sub_page/bot_log.py`（标题后缀; ≤20 行）
  - 新建 `script/test/test_bot_migration.py`
  - 扩展 `script/test/test_bot_log_page.py`、`script/test/test_remote_log_buffer.py`
- W4 写入域: `docs/general/remote_ssh_p3_acceptance.md`、`docs/general/remote_ssh_progress.md`（更新进度）

### 2.4 验证命令

每个 wave 结束跑一次基线回归 + 本 wave 新增用例：

```bash
# 全 P2 回归 (244 用例)
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
  -q

# P3 新增（按 wave 累加）
python -m pytest script/test/test_ssh_client_persistent.py -q              # W1
python -m pytest script/test/test_remote_page_actions.py -q                # W2
python -m pytest script/test/test_bot_migration.py -q                      # W3
```

### 2.5 完成语言规则

- W1/W2/W3 完成后只说 "Wave N done, regression green"，不写"P3 完成"
- 仅在 W4 phase_cleanup 后才允许 "P3 完成 / 已通过验收"
- 任何 wave 的部分子项失败 → 用 "blocked / partial" 措辞，不得宣称完成

### 2.6 回滚规则

- W1 (C) 出问题：`SSHClient` 改动局限在新方法 + 调用点的 try/retry 包装；任何回归失败立刻 revert 该 wave
- W2 (F/A) 出问题：UI 按钮可灰化或移除；后端能力本就独立可用
- W3 (B) 失败：`MigrationDialog` 拒绝执行，BotConfig 保存仍走原路径（仅写 `runtime_target`，不搬数据）；E 失败：fallback 到 P2 行为（无标题后缀）

### 2.7 phase_cleanup 期望

- 删除任何调试用临时文件
- 提交按 wave 拆 commit，commit message 走中文 atomic commit 规范
- 验收文档 `remote_ssh_p3_acceptance.md` 落盘
- `remote_ssh_progress.md` 更新 P3 状态为 "已完成"

---

## 3. 实现细节锚点

### 3.1 W1 — `SSHClient` 持久连接

`src/core/remote/ssh_client.py`:

```python
class SSHClient:
    DEFAULT_KEEPALIVE_INTERVAL = 30  # seconds

    def connect(self) -> None:
        ...
        transport = self._client.get_transport()
        if transport is not None:
            transport.set_keepalive(self.DEFAULT_KEEPALIVE_INTERVAL)

    def ensure_alive(self, *, reconnect: bool = True) -> bool:
        """检测会话是否仍然存活；死亡且 reconnect=True 时自动重连一次。
        Returns True 表示当前 session 可用。"""
        ...

    def _retry_on_disconnect(self, op: Callable[[], T], *, label: str) -> T:
        """run/exec_stream/SFTP 等的统一包装：捕获 SSHException/socket 错误，
        ensure_alive 后重试一次，仍失败则原样抛出。"""
```

包装范围：`run`、`exec_stream`、`upload_file`、`download_file`、`read_text`、`write_text`、`remote_exists`、`remote_listdir`、`remote_remove`、`open_local_tunnel`。

注意：流式命令（`exec_stream`）若已经在传输中途断开，重试只能从头开始；这是 paramiko 限制，重试一次后失败即抛错给上层。

### 3.2 W2 — RemotePage UI 入口

新建 `src/ui/page/remote_page/maintenance_runnables.py`：

```python
class RedetectVersionsRunnable(QObject, QRunnable):
    finished_signal = Signal(str, str, str)  # server_id, napcat_ver, qq_ver
    failed_signal = Signal(str, str)

class ForceUpdateRunnable(QObject, QRunnable):
    """复用 deploy_server(force_napcat_update=True) / force_linuxqq_reinstall=True"""
    finished_signal = Signal(str, bool, str)
    progress_signal = Signal(str, int)
    log_signal = Signal(str)

class RollbackRunnable(QObject, QRunnable):
    """复用 ServerManager.rollback_server"""
    finished_signal = Signal(str, bool, str)
```

UI 改动：在 `remote_page/__init__.py` 服务器详情区追加按钮组：
- "刷新版本"（小图标按钮，always 可见）
- "强制更新 NapCat"（仅 DEPLOYED 时可见，二次确认）
- "强制重装 LinuxQQ"（折叠在 "更多" 菜单内，强警告确认）
- "回滚"（DEPLOYED / FAILED 时可见，红色风格，二次确认 + include_qq 复选）

### 3.3 W3 — Bot 运行位置迁移

新建 `src/core/operation/migration.py`：

```python
@dataclass
class MigrationPlan:
    qq_id: str
    source_target: str
    dest_target: str
    move_persistent_data: bool  # False=只迁配置, True=连同 NapCat config/data 迁移

class BotMigrationService(QObject):
    progress_signal = Signal(str, int)   # message, percent
    finished_signal = Signal(bool, str)  # ok, message

    def execute(self, plan: MigrationPlan) -> None:
        # 1. 停源端 Bot（若运行中）
        # 2. 拷贝 bot.json 到目标 (始终)
        # 3. move_persistent_data=True 时:
        #    - 列出源 backend 的 config/{qq_id}, data/{qq_id} (NapCat 持久目录)
        #    - 通过 backend.download → desktop tmp → backend.upload 跨端搬运
        # 4. 更新 BotConfig.runtime_target 字段并写盘
        # 5. (不自动启动, 让用户决定)
```

新建 `src/ui/page/bot_page/widget/migration_dialog.py`：

```python
class MigrationDialog(MessageBoxBase):
    """运行位置迁移确认对话框。
    显示: 源 → 目标 / 是否搬运持久数据 / 风险提示
    返回: MigrationPlan | None
    """
```

`config.py` 改 `BotConfigWidget` 保存路径：
- 检测到 `runtime_target` 变化时，弹 `MigrationDialog`
- 用户取消 → fill_value 回原值，本次保存不动 target
- 用户确认 → 后台运行 `BotMigrationService`，UI 进度展示

### 3.4 W3 — RemoteNapCatQQLog 错误退避

`src/core/runtime/napcat.py` `RemoteNapCatQQLog._on_tail_error`:

- 累积错误次数 `_consecutive_errors`
- 超过阈值（默认 3）则 `_tail_timer.stop()` + 注入一行 `[ERROR] 远端日志拉取连续失败 N 次, 停止轮询...`
- 用户重启 Bot 触发重新创建 `RemoteNapCatQQLog`，自然恢复

`bot_log.py` 标题：

```python
def set_current_log_manager(self, config: Config) -> None:
    ...
    if config.bot.is_remote:
        server = it(ServerManager).get_server(config.bot.runtime_target)
        suffix = f" · 远端 [{server.name}]" if server else " · 远端"
    else:
        suffix = ""
    self.view.setTitle(self.tr(f"Bot 日志({qq_id}){suffix}"))
```

---

## 4. 风险与应对

| 风险 | 应对 |
| --- | --- |
| `SSHClient.ensure_alive` 重连过程中其他线程并发 run | 用 `threading.Lock` 互斥 connect 状态切换；RemoteBackend 已经在主线程 + QThreadPool worker 模式，并发面有限 |
| 数据搬运的中间临时目录占盘 | 用 `tempfile.TemporaryDirectory` 包装，try/finally 确保清理；进度阶段 emit 给用户 |
| 迁移途中失败导致两端都有半残数据 | 迁移失败时不更新 `runtime_target`，源端数据保留，UI 提示用户；目标端写入失败的零碎文件用 `RemoteBackend.remove(recursive=True)` 兜底回滚 |
| 强制更新过程中有 Bot 在跑 | UI 在执行前检查 `ManagerNapCatQQProcess` 是否有该 server 的活跃 Bot，提示先停止；后端不强制 |
| `transport.set_keepalive(30)` 在某些 OpenSSH 配置下被服务器拒 | 失败被 paramiko 静默吞掉，不会影响 connect；keepalive 失效时由 `ensure_alive` 兜底 |

---

## 5. 待 freeze 的开放项（执行中遇到再回填）

- 数据搬运是否压缩传输？默认 P3 不压缩；若耗时 > 30s 再考虑 `tar.gz` 中间格式
- `MigrationDialog` 是否提供"搬运后自动启动"复选？默认不勾，避免远端意外暴露 WebUI

