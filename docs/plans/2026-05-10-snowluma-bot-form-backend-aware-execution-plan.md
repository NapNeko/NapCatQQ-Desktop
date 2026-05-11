# SnowLuma Bot 表单后端感知 + WebUI 客户端化 + Backend 抽象重构 — 执行计划

- **关联需求**: `docs/requirements/2026-05-10-snowluma-bot-form-backend-aware.md`
- **父需求 (P1)**: `docs/requirements/2026-05-10-snowluma-backend-adapter.md`
- **父执行计划 (P1)**: `docs/plans/2026-05-10-snowluma-backend-adapter-execution-plan.md`
- **vibe run-id**: `2026-05-10T2238-snowluma-bot-form-backend-aware`
- **runtime**: `interactive_governed` (`root_governed` lane)
- **stop target (本计划入口 wrapper)**: `xl_plan` (`vibe-how`); `plan_execute` 由后续 `vibe` / `vibe-do` 入口接管
- **内部执行级别 (Internal Grade)**: **L** — 单 agent 串行; wave 之间存在线性依赖链 (Tier I 重构必先于 Tier B/D/E/F 落点; Tier B/D 必先于 Tier E orchestration; Tier A.1 模型扩展必先于 Tier B renderer 消费); XL fan-out 收益不抵 ~3775 行机械搬移与 22 文件 rename 的合并冲突风险
- **总规模估算**: 净增 ~2120 行 / 含搬移 ~3775 行; 净新增源文件 5 个 (`bot_backend_driver.py` / `napcat_driver.py` / `snowluma_driver.py` / `bot_process_manager.py` / `snowluma_webui_client.py`); 删除 1 个 (`napcat.py`); 既有 22 文件 import 同步重命名

---

## 内部 grade 决策

选 **L** 而非 XL 的具体证据 (本节是 vibe 必填项):

- **W2 (Tier I 重构) 是单原子事件**, 22 文件机械重命名漏一处即运行时崩 (`creart` 单例查找失败 + `napcat.py` 已删 → ImportError); 不能 fan-out
- **W3 / W4a / W4b 共享对 `snowluma_driver.py` 的写权限**: W3 改 `render_onebot_json` signature → driver 调用点更新; W4a 新建 `snowluma_webui_client.py` 后 driver 持其实例; W4b 改 `SnowLumaInstall` 钩子, driver 启动时读 `snowluma-session.json`. 三者交错 fan-out 时合并冲突频繁
- **W5 (Phase A→D orchestration) 是 W3+W4a+W4b 的消费者**, 必待三者落地; 自身 ~700 行集中在 `snowluma_driver.py`, 单 agent 写更不易出错
- **W6 (UI 显隐) 跨 6 个 UI 文件**, 但每个文件只动 `setVisible` / 信号连接, 多 agent fan-out 命中力不强; 串行写 4 小时内可完成
- **W7 (测试) 涉及 9 个测试文件搬动**, 跨 import 期间易临时断; 必须串行 (Step-by-step git mv + 调整 import)

XL 替代评估: 即使把 W3 + W4a + W4b 标为可并行, 节省时间不超 30%, 但合并复杂度 (`snowluma_driver.py` 调用点冲突) 显著增加. 不选。

---

## Wave 结构

| Wave | 名称                                                           | 依赖         | 串行 / 并行 | 估算行数        |
| ---- | -------------------------------------------------------------- | ------------ | ----------- | --------------- |
| W1   | 配置模型扩展: `config_model.py` 加 SnowLuma 独有字段           | —            | 串行        | ~30 行          |
| W2   | Backend 抽象重构: 5 新文件 + 22 文件 rename + 删 napcat.py     | W1           | 串行 (原子) | ~3775 行 (搬移) |
| W3   | Renderer 重构: `render_onebot_json` 接受 `ConnectConfig`       | W1, W2       | 串行        | ~120 行         |
| W4   | WebUI 客户端 + 密码管理: `snowluma_webui_client.py` + 安装钩子 | W2           | 串行        | ~450 行         |
| W5   | Bot 启停 orchestration + Poller 重写 + 单实例守护              | W2, W3, W4   | 串行        | ~700 行         |
| W6   | UI 显隐 + ChooseConfigTypeDialog + AdvancedConfigWidget        | W1, W2, W5   | 串行        | ~250 行         |
| W7   | 测试更新与按 driver 重组                                       | W1-W6 全完成 | 串行        | ~350 行         |
| W8   | 全量验证 + smoke + phase_cleanup                               | W7           | 串行        | —               |

> 依赖箭头说明: W2 (`SnowLumaDriver` 空壳) 在 W5 之前必须存在以承载 Phase A→D 实现; W3 必须先于 W5 因为 driver `start()` 调 `render_onebot_json(connect=...)`; W4a (`SnowLumaWebUIClient`) 必须先于 W5 因为 driver 持其实例; W6 UI 表单显隐通过 `BackendType` enum 触发, W2 已稳定后再改 UI 避免连带回归.

---

## 跨 wave 不变量 (Invariants)

- 任何 wave 都**不允许**修改 NapCat 既有启动路径的可观察行为 (`_create_napcat_process` / `_get_env_variable` / `_write_load_script` 函数体保持现状; 仅由 W2 机械搬移到 `napcat_driver.py`, 函数体一字不改)
- 所有 `apply_backend_type(backend)` 调用必须**幂等** (同一 backend 多次调用结果一致)
- widget 显隐一律 `setVisible(False)`, **禁** `deleteLater()`; 持久化字段值切换 backend 后零丢失
- `BOT_CONFIG_COMPAT_VERSION` **保持 v2.0** (本期只加可选字段, 不破坏向后兼容); 旧 bot.json 反序列化路径不动
- `process_changed_signal` / `notification_signal` / `snowluma_login_state_signal` / `state_changed` 4 个 signal 名字不改; 接收方代码无感
- 所有 SnowLuma 相关 HTTP 调用走 `httpx` 短连接 (不持单例 `httpx.Client`); 401 自动 retry 一次后失败则上报 `SnowLumaWebUIError`
- 一期硬限制: 同时只能跑 1 个 SnowLuma Bot (webuiPort 5099 hardcode + SnowLuma 单工作目录设计); 守护放在 `SnowLumaDriver` 而非 `BotProcessManager`, 不影响 NapCat 多实例
- Stop SnowLuma Bot 时 **必 kill QQ.exe** (D11 决策)
- 用户在 SnowLuma WebUI 改密码会被 Desktop 在下次启动时单向覆盖 (D2 决策)
- `config/snowluma-session.json` (含明文密码) 不得入 git / 打包产物; `.gitignore` 与 `script/build_scripts/collection_filters.py` 必须覆盖排除
- 重构期间一次 commit 一个 Tier (W2 内部除外, W2 是原子事件); git 必须保持清洁状态以便快速回滚
- **不写**新测试文件 (用户决定手动验收 §5.1/§5.2/§5.3/§5.4); 既有测试因 signature / 类名变化必须随之更新保持全绿

---

## W1 — 配置模型扩展 (`src/core/config/config_model.py`)

**Owner boundary**:

- 仅修改 `src/core/config/config_model.py` 内的 `HttpServersConfig` / `HttpClientsConfig` / `WebsocketServersConfig` / `WebsocketClientsConfig` 4 个类
- **不**改 `HttpSseServersConfig` (SnowLuma 不识别 SSE; SSE 整类 SnowLuma 模式下隐藏)
- **不**改 `BotConfig` / `AdvancedConfig` / `ConnectConfig` 顶层结构
- **不**动 `BOT_CONFIG_COMPAT_VERSION` (保持 `v2.0`)
- **不**改 `_migrate_legacy_*` (新字段都是可选 + 有合理默认, pydantic 自动补齐)

**1.1 字段新增 (4 处)**:

```python
class HttpServersConfig(NetworkBaseConfig):
    host: str
    port: int
    enableCors: bool = False
    enableWebsocket: bool = False
    path: str = "/"   # ← 新增 (SnowLuma 独有, NapCat 不读); HTTP server 是前缀挂载点
```

```python
class HttpClientsConfig(NetworkBaseConfig):
    url: HttpUrl
    reportSelfMessage: bool = False
    timeoutMs: int | None = None   # ← 新增 (SnowLuma 独有, NapCat 不读); None 表示不传 → SnowLuma 默认 5000ms
```

```python
class WebsocketServersConfig(NetworkBaseConfig):
    host: str
    port: int
    reportSelfMessage: bool = False
    enableForcePushEvent: bool = False
    heartInterval: int = 30000
    path: str = "/"   # ← 新增 (SnowLuma 独有, NapCat 固定 /); WS server 是 exact match
    role: Literal["Api", "Event", "Universal"] = "Universal"   # ← 新增 (SnowLuma 独有)
```

```python
class WebsocketClientsConfig(NetworkBaseConfig):
    url: WebsocketUrl
    reportSelfMessage: bool = False
    heartInterval: int = 30000
    reconnectInterval: int = 30000
    role: Literal["Api", "Event", "Universal"] = "Universal"   # ← 新增 (SnowLuma 独有)
```

**1.2 关键约束**:

- `path` 默认 `"/"` 与 SnowLuma 上游 `makeDefaultOneBotConfig()` 一致, 在 NapCat 模式下被 NapCat 视为字段冗余但合法 (NapCat 序列化时 pydantic 不忽略)
- `role` 默认 `"Universal"`; SnowLuma 上游在未显式配置时按 URL 尾部 `/api` / `/event` 自动分类, Desktop 不依赖此自动分类, 显式写入
- `timeoutMs: int | None`: 渲染时 (W3) 仅当非 `None` 写入 SnowLuma JSON; NapCat 模式下不写入 NapCat 配置 (NapCat 本无此字段)
- `WebsocketClientsConfig.reconnectInterval` 字段名**沿用** (与 SnowLuma `reconnectIntervalMs` 同语义同单位 ms); renderer 在 W3 做名映射, 渲染时 `Math.max(1000, value)` clamp + `logger.warning`

**1.3 验证命令**:

```pwsh
# 1.A 字段默认值合规
.venv\Scripts\python.exe -c "from src.core.config.config_model import HttpServersConfig, WebsocketServersConfig, WebsocketClientsConfig, HttpClientsConfig; print(HttpServersConfig.model_fields['path'].default, WebsocketServersConfig.model_fields['path'].default, WebsocketServersConfig.model_fields['role'].default, WebsocketClientsConfig.model_fields['role'].default, HttpClientsConfig.model_fields['timeoutMs'].default)"
# 1.B 旧 bot.json 反序列化兼容 (pydantic 默认值补齐)
.venv\Scripts\python.exe -c "from src.core.config.config_model import HttpServersConfig; m=HttpServersConfig.model_validate({'name':'http-default','host':'127.0.0.1','port':3000}); print(m.path, m.enableCors)"
.venv\Scripts\python.exe -c "from src.core.config.config_model import WebsocketClientsConfig; m=WebsocketClientsConfig.model_validate({'name':'ws-client','url':'ws://localhost:8080','role':'Api'}); print(m.role, m.reconnectInterval)"
# 1.C BOT_CONFIG_COMPAT_VERSION 保持 v2.0
.venv\Scripts\python.exe -c "from src.core.config.config_model import BOT_CONFIG_COMPAT_VERSION; assert BOT_CONFIG_COMPAT_VERSION == 'v2.0', BOT_CONFIG_COMPAT_VERSION; print(BOT_CONFIG_COMPAT_VERSION)"
```

**期望输出**:

- `1.A` → `/ / Universal Universal None`
- `1.B` → `/ False` 与 `Api 30000`
- `1.C` → `v2.0`

---

## W2 — Backend 抽象重构 (Tier I, 原子事件)

**Owner boundary**:

- **新建** 5 个文件:
  - `src/core/runtime/bot_backend_driver.py` (~80 行: 抽象基类 + `ProcessHandle` 数据类)
  - `src/core/runtime/napcat_driver.py` (~1800 行: 从 `napcat.py` 搬移所有 NapCat 专有逻辑, **零逻辑改动**)
  - `src/core/runtime/snowluma_driver.py` (~250 行 W2 阶段空壳; W5 扩到 ~1100 行)
  - `src/core/runtime/bot_process_manager.py` (~500 行: dispatch + 生命周期)
- **修改** `src/core/runtime/snowluma_status_poller.py` 顶部 docstring 引用 `BotProcessManager`
- **删除** `src/core/runtime/napcat.py` **整文件**
- **批量重命名** 22 文件中 `ManagerNapCatQQProcess` → `BotProcessManager` (含 `it(...)` 调用)
- **修改** `src/core/logging/crash_bundle.py:44` 正则同时匹配新老类名
- **修改** `.gitignore` 加 `config/snowluma-session.json` 排除
- **修改** `script/build_scripts/collection_filters.py` 加 SnowLuma session 文件过滤函数
- **修改** `src/core/operation/backend.py:216` 的 `OperationBackend.start_napcat` **不动** (D-I-4 决策, 留 P3 重构)

> **W2 是单原子 commit**. 中途 `napcat.py` 已删但 22 文件 import 尚未 rename 时, `python -c "import src.core.runtime.napcat"` 会立刻 ImportError. 因此 W2 内部所有 step 必须在同一 commit 落地.

**2.1 文件设计**:

### `bot_backend_driver.py` (新)

```python
# -*- coding: utf-8 -*-
"""Backend driver 抽象基类 (Tier I, P2 SnowLuma WebUI 编程客户端化).

两个具体 driver 类型:
- NapCatDriver: NapCat 注入式 (NTQQ 加载 napcat.cjs)
- SnowLumaDriver: SnowLuma WebUI 客户端 + spawn 独立 QQ.exe + 注入

driver 持自己的 ProcessModel 字典, 由 BotProcessManager 按 backend_type dispatch.
"""
from __future__ import annotations
from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from PySide6.QtCore import QProcess
    from src.core.config.config_model import Config

@dataclass
class ProcessHandle:
    """统一进程句柄 (NapCat 单 QProcess 或 SnowLuma 双 QProcess + WebUI 客户端).

    Attributes:
        qq_id: Bot 的 QQ 号 (字符串, 与现有 dict key 对齐)
        primary_process: NapCat 路径下的 QQ.exe; SnowLuma 路径下的 QQ.exe (Desktop spawn)
        secondary_process: 仅 SnowLuma 路径有效, SnowLuma node.exe 子进程
    """
    qq_id: str
    primary_process: "QProcess"
    secondary_process: "QProcess | None" = None

class BotBackendDriver(ABC):
    """Bot 后端启动/停止/状态抽象."""

    @abstractmethod
    def start(self, config: "Config") -> ProcessHandle: ...

    @abstractmethod
    def stop(self, qq_id: str) -> None: ...

    @abstractmethod
    def is_running(self, qq_id: str) -> bool: ...

    @abstractmethod
    def get_status_poller(self, qq_id: str): ...
    """返回 SnowLumaStatusPoller 实例 (SnowLuma 路径) 或 None (NapCat 路径)."""
```

### `napcat_driver.py` (新)

`NapCatDriver(BotBackendDriver)` — 从原 `napcat.py` **机械搬移** 以下代码块, 一字不改函数体:

| 来源 (napcat.py 行号)                                  | 目标 (napcat_driver.py)                                  | 说明                                       |
| ------------------------------------------------------ | -------------------------------------------------------- | ------------------------------------------ |
| 1339 `class ManagerNapCatQQProcess` 内 NapCat 部分     | `NapCatDriver` 类                                        | 仅保留 NapCat 专有逻辑, 不含 SnowLuma 分支 |
| 1374-1392 `_write_load_script`                         | `NapCatDriver._write_load_script`                        | 一字不改                                   |
| 1394-1420 `_create_napcat_process`                     | `NapCatDriver._create_napcat_process`                    | 一字不改                                   |
| `_get_env_variable` (含此处未列行号)                   | `NapCatDriver._get_env_variable`                         | 一字不改                                   |
| 进程退出/启动失败回调里的 NapCat 分支                  | `NapCatDriver._handle_*` 系列                            | 删 SnowLuma 分支 (那些走 SnowLumaDriver)   |
| `napcat_process_dict` 字段                             | `NapCatDriver._processes: dict[str, NapCatProcessModel]` | 字段重命名为内部 `_processes`              |
| `psutil` 进程树 kill 逻辑 (`stop_process` NapCat 分支) | `NapCatDriver.stop`                                      | 包装为 `BotBackendDriver.stop` 接口        |

**新增**: `NapCatDriver.get_status_poller(qq_id) -> None` (NapCat 路径用 `ManagerNapCatQQLoginState`, 不走 poller)

**保留**: `psutil` 进程树清理 / `QProcess` 信号连接 / `it(ManagerNapCatQQLog).create_log` 调用 / `ManagerNapCatQQLoginState` 关联

### `snowluma_driver.py` (W2 仅占位空壳)

W2 阶段只产出最小可启动占位; **完整 Phase A→D 流程 + 反向 stop + Poller 接线 + 单实例守护** 全部留 W5 实施.

```python
# W2 阶段产物 (~250 行):
class SnowLumaDriver(BotBackendDriver):
    """SnowLuma 后端 driver (P2 WebUI 客户端化, Tier I).

    W2 阶段仅为占位; 完整 Phase A/B/C/D 启动序列 + 反向 stop + 单实例守护
    见 W5.
    """

    def __init__(self) -> None:
        self._processes: dict[str, "SnowLumaProcessModel"] = {}
        # W4a 引入 SnowLumaWebUIClient 后, 此处持单例工厂
        self._webui_client_factory = None  # 占位; W4a 填充

    def start(self, config: "Config") -> ProcessHandle:
        """W2 占位: 仅 spawn node.exe (复刻 P1 现行 _create_snowluma_process 行为).

        W5 改写为 Phase A→D 完整序列.
        """
        # 暂时复刻 napcat.py:1422-1475 的简化版, 把 P1 现状原地搬过来
        ...

    def stop(self, qq_id: str) -> None:
        """W2 占位: terminate node.exe + waitForFinished. W5 改写为反向序列."""
        ...

    def is_running(self, qq_id: str) -> bool: ...
    def get_status_poller(self, qq_id: str): ...
```

### `bot_process_manager.py` (新)

`BotProcessManager(QObject)` — 替代原 `ManagerNapCatQQProcess`. 核心职责:

- 持 `self._napcat_driver = NapCatDriver()` 与 `self._snowluma_driver = SnowLumaDriver()` 两个 driver 实例
- `start_bot(config: Config) -> None`: 按 `config.bot.backend_type` dispatch 到对应 driver; 异常捕获后 `notification_signal.emit("error", ...)`
- `stop_bot(qq_id: str) -> None`: 按 `config.bot.backend_type` dispatch
- `stop_all_bots() -> None`: 遍历两个 driver
- `get_process(qq_id: str) -> NapCatProcessModel | SnowLumaProcessModel | RemoteProcessRecord | None`: 联合查询本地 + 远端
- `has_running_bot() -> bool`: 复合查询 (本地 NapCat / 本地 SnowLuma / 远端)
- `get_memory_usage(qq_id: str) -> int`: 联合查询 (复用原有 `psutil` 进程树累加; 远端读 `RemoteProcessRecord.last_memory_rss_bytes`)
- 4 个 signal **保留原名**:
  - `process_changed_signal = Signal(str, QProcess.ProcessState)`
  - `notification_signal = Signal(str, str)`
  - `snowluma_login_state_signal = Signal(str, str)`
  - 来自 driver 内部 `state_changed` 转发逻辑放在 `BotProcessManager._connect_driver_signals()`

**保留**:

- `remote_process_dict: dict[str, RemoteProcessRecord]` 字段 (P2.6 远端 Bot, 与 driver 正交)
- `_create_remote_process` / `_stop_remote_process` 方法 (远端路径不走 driver)
- 4 进程上限检查 (`if len(napcat_dict) >= 4: ...`); 改为查 `_napcat_driver._processes`
- `restart_process` / `stop_all_processes` / `get_memory_usage` 公共 API; 内部按 backend dispatch

**`alias` 保留 (兼容 1 个 minor 版本)**: `create_napcat_process = start_bot` 设为类方法 alias (D-I-2 用户指令是 "机械重命名", 但为防止 22 文件之外的潜在 import 漏网, 加 1 个 alias 作为 W2 期间的临时安全网; **注**: 需求 §3 明确写"不允许保留 alias / shim"). **本计划遵循需求**: 不加 alias, 22 文件全量 rename.

### `creart` 单例迁移 (`bot_process_manager.py` 末尾)

```python
class BotProcessManagerCreator(AbstractCreator, ABC):
    targets = (CreateTargetInfo("src.core.runtime.bot_process_manager", "BotProcessManager"),)

    @staticmethod
    def available() -> bool:
        return exists_module("src.core.runtime.bot_process_manager")

    @staticmethod
    def create(create_type: type[BotProcessManager]) -> BotProcessManager:
        return create_type()

add_creator(BotProcessManagerCreator)
```

> 注: 原 `napcat.py` 内的 `ManagerNapCatQQLogManagerCreator` / `ManagerNapCatQQLoginStateCreator` / `ManagerAutoRestartProcessCreator` 三个 Creator 与 `ManagerNapCatQQProcessCreator` 一起搬到 `bot_process_manager.py` (它们的 `CreateTargetInfo` 第一参数 `"src.core.runtime.napcat"` 改为 `"src.core.runtime.bot_process_manager"`; `ManagerNapCatQQLog` / `ManagerNapCatQQLoginState` / `ManagerAutoRestartProcess` 类本身仍由原文件迁出, 类名不改).

### `NapCatProcessModel` / `SnowLumaProcessModel` 拆分 (`bot_process_manager.py` 顶部)

```python
@dataclass
class BotProcessModel:
    """本地进程模型抽象基类."""
    qq_id: str
    state: QProcess.ProcessState = QProcess.ProcessState.NotRunning
    started_at: float = 0.0

@dataclass
class NapCatProcessModel(BotProcessModel):
    """NapCat (NTQQ 注入式) 进程模型: 持 1 个 QProcess (即 QQ.exe + napcat.cjs)."""
    process: QProcess | None = None

@dataclass
class SnowLumaProcessModel(BotProcessModel):
    """SnowLuma 进程模型: 持 2 个 QProcess + WebUI 客户端 + auth token."""
    qq_process: QProcess | None = None         # Desktop spawn 的 QQ.exe
    node_process: QProcess | None = None       # SnowLuma node.exe
    webui_client: "SnowLumaWebUIClient | None" = None   # W4a 引入
    auth_token: str | None = None              # bearer token
    qq_pid: int = 0                            # spawn 之后从 QProcess.processId() 拿
```

> 22 文件中, **除** `bot_card.py` / `bot_list.py` / `card.py` 等外的 import `NapCatProcessModel` 一律保持 (行为兼容); 但凡构造 `NapCatProcessModel(...)` 的代码点 (现仅 `napcat.py:1704`, 搬到 `napcat_driver.py`) 用新 fields.

### `napcat.py` 删除 (W2 末尾)

W2 末尾执行 `Remove-Item src\core\runtime\napcat.py`. **注**: `git rm` 比 `Remove-Item` 安全, 因为有 git 索引同步.

**2.2 22 文件 import 重命名清单** (机械, 一行 sed 风格 search/replace 操作):

| 文件                                                   | hits                  |
| ------------------------------------------------------ | --------------------- |
| `src/ui/page/bot_page/widget/card.py`                  | 12                    |
| `src/core/runtime/napcat.py`                           | 7 (删)                |
| `src/ui/components/remote_summary_card.py`             | 6                     |
| `src/ui/components/status_overview_dialog.py`          | 5                     |
| `src/ui/page/bot_page/sub_page/bot_list.py`            | 4                     |
| `src/ui/page/bot_page/sub_page/bot_config.py`          | 3                     |
| `src/ui/page/component_page/sub_page/desktop_page.py`  | 3                     |
| `src/ui/page/component_page/sub_page/napcat_page.py`   | 3                     |
| `src/ui/page/component_page/sub_page/snowluma_page.py` | 3                     |
| `src/ui/page/setup_page/sub_page/developer.py`         | 3                     |
| `src/ui/window/main_window/window.py`                  | 3                     |
| `src/core/home/notice_service.py`                      | 2                     |
| `src/core/logging/crash_bundle.py`                     | 2 (字符串注释 + 正则) |
| `src/core/operation/local_backend.py`                  | 2                     |
| `src/core/runtime/snowluma_status_poller.py`           | 2 (docstring)         |
| `src/ui/page/bot_page/__init__.py`                     | 2                     |
| `src/ui/window/main_window/system_try_icon.py`         | 2                     |
| `src/core/operation/batch_dispatcher.py`               | 1                     |
| `src/core/operation/migration.py`                      | 1                     |
| `src/core/operation/remote_backend.py`                 | 1                     |
| `src/core/runtime/backend_type.py`                     | 1 (docstring)         |
| `src/core/runtime/background_tasks.py`                 | 1                     |

具体替换规则 (脚本应用):

```pwsh
# Step 1: 类名替换
Get-ChildItem src -Recurse -Include '*.py' -Exclude 'crash_bundle.py' | ForEach-Object {
    (Get-Content $_.FullName -Raw) -replace '\bManagerNapCatQQProcess\b', 'BotProcessManager' | Set-Content -Path $_.FullName -NoNewline -Encoding utf8
}
# Step 2: import 路径替换
Get-ChildItem src -Recurse -Include '*.py' | ForEach-Object {
    (Get-Content $_.FullName -Raw) -replace 'from src\.core\.runtime\.napcat import', 'from src.core.runtime.bot_process_manager import' | Set-Content -Path $_.FullName -NoNewline -Encoding utf8
}
```

> `crash_bundle.py` 单独处理 (其正则要保留 `ManagerNapCatQQProcess` 作为兼容匹配项, 见 2.3).

**2.3 `crash_bundle.py:44` 正则更新** (D-I-8 决策):

```python
# 老:
_QQID_BRACKET_PATTERN = re.compile(r"(ManagerNapCatQQProcess\[)(\d{5,12})(\])")
# 新 (兼容老类名 6+ 月):
_QQID_BRACKET_PATTERN = re.compile(r"((?:ManagerNapCatQQProcess|BotProcessManager)\[)(\d{5,12})(\])")
```

> 文件顶部注释 (line 37) `ManagerNapCatQQProcess[<qqid>]` 同步更新为 `BotProcessManager[<qqid>]` (并保留一行说明老类名兼容).

**2.4 `.gitignore` 追加**:

```diff
 ### 配置文件 ###
 # 本地配置和敏感文件
 /config/
+# Tier G: SnowLuma WebUI 密码记录, 含明文密码, 不得入 git
+# 锚定 Desktop 侧 runtime/config/, 与 W4b snowluma_session.session_path() 返回值一致
+/runtime/config/snowluma-session.json
 *.local
 *.env
 .env
```

**路径决策**: session.json 锚定 **Desktop 侧** `it(PathFunc).config_dir_path / "snowluma-session.json"` = `runtime/config/snowluma-session.json`, 与 SnowLuma `webui.json` 解耦; 即使用户重装 SnowLuma 删了 SnowLuma config 子目录, Desktop 仍能识别上次密码 (sticky). 这是 W4b `snowluma_session.session_path()` 的实际返回值.

**与现有 `.gitignore` 关系**: 第 39 行 `/config/` 锚定**仓库根** `config/`, 不覆盖 `runtime/config/`. `runtime/` 整目录默认就是运行时产物 (`runtime/log/`、`runtime/NapCatQQ/` 等本就不在 git 跟踪), 但 W4b 引入 `runtime/config/snowluma-session.json` 含密码明文, 必须**显式**加入 `.gitignore` 防止 git add . 误纳.

**2.5 `script/build_scripts/collection_filters.py` 追加过滤器**:

```python
def _filter_snowluma_session(entries) -> tuple[object, int]:
    """Tier G: 排除 snowluma-session.json (含明文密码) 进入打包产物."""
    kept: list[tuple[str, str, str]] = []
    removed = 0
    for entry in entries:
        dest, src, typecode = entry
        normalized_dest = _normalize_dest(dest)
        normalized_src = _normalize_dest(src)
        if normalized_dest.endswith("snowluma-session.json") or normalized_src.endswith("snowluma-session.json"):
            removed += 1
            continue
        kept.append((dest, src, typecode))
    return _rebuild(entries, kept), removed

# 在 apply_collection_filters() (假设入口函数) 末尾追加调用
```

> 现有文件顶部 `_filter_*` 系列函数后追加; 入口 hook 处理同样追加 `_filter_snowluma_session`.

**2.6 验证命令** (完整 W2 落地后):

```pwsh
# 2.A 5 个新文件存在 + 旧文件已删
Test-Path src\core\runtime\bot_backend_driver.py
Test-Path src\core\runtime\napcat_driver.py
Test-Path src\core\runtime\snowluma_driver.py
Test-Path src\core\runtime\bot_process_manager.py
Test-Path src\core\runtime\napcat.py    # ← 期望 False
# 2.B 抽象层导入正确
.venv\Scripts\python.exe -c "from src.core.runtime.bot_backend_driver import BotBackendDriver, ProcessHandle; from src.core.runtime.napcat_driver import NapCatDriver; from src.core.runtime.snowluma_driver import SnowLumaDriver; from src.core.runtime.bot_process_manager import BotProcessManager, NapCatProcessModel, SnowLumaProcessModel; print('OK')"
# 2.C creart 单例
.venv\Scripts\python.exe -c "from creart import it; from src.core.runtime.bot_process_manager import BotProcessManager; m=it(BotProcessManager); print(type(m).__name__, hasattr(m, 'process_changed_signal'), hasattr(m, 'notification_signal'), hasattr(m, 'snowluma_login_state_signal'))"
# 2.D 22 文件 rename 完整 (零命中或仅 crash_bundle.py:44 兼容正则一处)
Get-ChildItem src -Recurse -Include '*.py' | Select-String -Pattern '\bManagerNapCatQQProcess\b' | Where-Object { $_.Path -notmatch 'crash_bundle\.py$' } | Measure-Object
# 期望 Count = 0
Get-ChildItem src -Recurse -Include '*.py' | Select-String -Pattern 'from src\.core\.runtime\.napcat import' | Measure-Object
# 期望 Count = 0
# 2.E crash_bundle 兼容正则
.venv\Scripts\python.exe -c "import re; from src.core.logging.crash_bundle import _QQID_BRACKET_PATTERN as p; print(bool(p.search('ManagerNapCatQQProcess[123456789]')), bool(p.search('BotProcessManager[123456789]')))"
# 2.F 启动 smoke (主窗口能开)
.venv\Scripts\python.exe -c "import src.main; print('import OK')"
# 期望 stdout: 'import OK'; 任何 ImportError 即 W2 未完成
```

**期望输出**:

- `2.A` → `True True True True False`
- `2.B` → `OK`
- `2.C` → `BotProcessManager True True True`
- `2.D` → 两次 `Count : 0`
- `2.E` → `True True` (新老类名都匹配)
- `2.F` → `import OK`

**2.7 风险缓释**:

- **22 文件 rename 漏一个**: 验证命令 `2.D` 是兜底; 如果命中 != 0, 必须立即追加补漏 commit (本 wave 仍未完)
- **creart 单例 cache 时序**: 如果其他 Creator (`ManagerNapCatQQLogManagerCreator` / 等) 在 import 链上先加载, `targets[0]` 的 module 路径变化可能导致 `it(...)` 返回旧 cache. 缓释: W2 commit 前在测试 stub 里 `creart._cache.clear()` 显式清空一次
- **`SnowLumaStatusPoller` 内部对 `ManagerNapCatQQProcess` 的引用 (docstring)**: 仅文档字符串, 重命名后无运行时影响

---

## W3 — Renderer 重构 (`src/core/runtime/snowluma_config_renderer.py`)

**Owner boundary**:

- 仅修改 `src/core/runtime/snowluma_config_renderer.py` 内 `render_onebot_json` 函数 + 函数末尾的兜底默认逻辑
- **不**改 `render_runtime_json` / `render_webui_json` / `read_existing_onebot_json` / `_utc_now_iso`
- 调用方更新留 W5 (W2 期间 `snowluma_driver.py` 占位实现暂用旧 signature, W3 后切换); 测试更新留 W7

**3.1 新 signature**:

```python
def render_onebot_json(
    snowluma_path: Path,
    qqid: int,
    *,
    connect: ConnectConfig,
    music_sign_url: str = "",
) -> None:
    """渲染 SnowLuma onebot_<qqid>.json (P2 后端感知, Tier B)."""
    ...
```

**移除参数**: `http_port` / `ws_port` / `access_token` / `message_format` / `report_self_message` / `host` 6 个标量, 全部由 `connect` 字段提供.

**3.2 字段映射 (NapCat → SnowLuma)**:

| NapCat (`NetworkBaseConfig` 子类)                                                     | SnowLuma (`networks.*`)    | 备注                                                      |
| ------------------------------------------------------------------------------------- | -------------------------- | --------------------------------------------------------- |
| `enable: bool`                                                                        | `enabled: bool`            | 字段名变化                                                |
| `name: str`                                                                           | `name: str`                | 同名                                                      |
| `messagePostFormat`                                                                   | `messageFormat`            | 字段名变化, 取值域 `array` / `string` 一致                |
| `token: str`                                                                          | `accessToken: str`         | 字段名变化; 空字符串也写入                                |
| `host` / `port` / `url`                                                               | 同名                       | 透传                                                      |
| `path: str`                                                                           | `path: str`                | 默认 `/`; HTTP server 是前缀挂载点, WS server exact match |
| `role: str`                                                                           | `role: str`                | SnowLuma 独有, 默认 `Universal`                           |
| `reportSelfMessage`                                                                   | 同名                       | 透传                                                      |
| `reconnectInterval` (ms)                                                              | `reconnectIntervalMs` (ms) | 字段名变化; **clamp `max(1000, value)` + warn**           |
| `timeoutMs: int \| None`                                                              | `timeoutMs: int`           | 仅当非 None 写入                                          |
| `debug` / `enableCors` / `enableWebsocket` / `enableForcePushEvent` / `heartInterval` | (静默丢弃)                 | NapCat-only                                               |
| `httpSseServers` / `plugins`                                                          | (静默丢弃)                 | SnowLuma 不识别                                           |

**3.3 兜底默认逻辑** (用户 connect 全空时):

```python
# 当 connect.httpServers 与 connect.websocketServers 都为空, 兜底一份与
# SnowLuma makeDefaultOneBotConfig() 等价的默认值, 避免 SnowLuma 因 networks
# 全空启动失败.
import secrets
if not connect.httpServers and not connect.websocketServers:
    fallback_token = secrets.token_urlsafe(32)
    payload["networks"] = {
        "httpServers": [{"name": "http-default", "enabled": True,
                          "messageFormat": "array", "accessToken": fallback_token,
                          "reportSelfMessage": False, "host": "0.0.0.0",
                          "port": 3000, "path": "/"}],
        "wsServers": [{"name": "ws-default", "enabled": True,
                          "messageFormat": "array", "accessToken": fallback_token,
                          "reportSelfMessage": False, "host": "0.0.0.0",
                          "port": 3001, "path": "/", "role": "Universal"}],
        "httpClients": [], "wsClients": [],
    }
```

**3.4 reconnectInterval clamp 行为**:

```python
def _clamp_reconnect_ms(value: int) -> int:
    """SnowLuma 上游强制 max(1000, value), Desktop 同步 clamp 并 warn."""
    if value < 1000:
        logger.warning(
            f"用户配置 reconnectInterval={value}ms 被 clamp 到 1000ms; "
            "SnowLuma 上游 (packages/core/src/onebot/config.ts:299) 强制下限",
            log_source=LogSource.CORE,
        )
        return 1000
    return value
```

> Desktop 持久化保留用户原值 (切回 NapCat 仍是原值); renderer 仅在写 SnowLuma JSON 时 clamp.

**3.5 验证命令**:

```pwsh
# 3.A signature 变化
.venv\Scripts\python.exe -c "import inspect; from src.core.runtime.snowluma_config_renderer import render_onebot_json; sig=inspect.signature(render_onebot_json); print('connect' in sig.parameters and 'music_sign_url' in sig.parameters and 'http_port' not in sig.parameters)"
# 3.B 兜底默认
.venv\Scripts\python.exe -c "from pathlib import Path; from src.core.config.config_model import ConnectConfig; from src.core.runtime.snowluma_config_renderer import render_onebot_json; tmp=Path('runtime/_smoke_w3'); tmp.mkdir(parents=True, exist_ok=True); render_onebot_json(tmp, 99999, connect=ConnectConfig()); import json; p=json.loads((tmp/'config'/'onebot_99999.json').read_text()); print(p['networks']['httpServers'][0]['port'], p['networks']['wsServers'][0]['port'], len(p['networks']['httpServers'][0]['accessToken']) > 0)"
# 3.C 字段映射 (主要项: enable→enabled, token→accessToken, messagePostFormat→messageFormat)
.venv\Scripts\python.exe -c "from pathlib import Path; from src.core.config.config_model import ConnectConfig, HttpServersConfig; from src.core.runtime.snowluma_config_renderer import render_onebot_json; cc=ConnectConfig(httpServers=[HttpServersConfig(name='test', host='0.0.0.0', port=4000, token='ACCESS-TEST', messagePostFormat='string', enable=False, debug=True, enableCors=True, enableWebsocket=True, path='/api')]); tmp=Path('runtime/_smoke_w3'); tmp.mkdir(parents=True, exist_ok=True); render_onebot_json(tmp, 88888, connect=cc); import json; s=json.loads((tmp/'config'/'onebot_88888.json').read_text())['networks']['httpServers'][0]; print(s.get('enabled'), s.get('accessToken'), s.get('messageFormat'), s.get('path'), 'debug' not in s, 'enableCors' not in s, 'enableWebsocket' not in s)"
# 3.D reconnectInterval clamp
.venv\Scripts\python.exe -c "from pathlib import Path; from src.core.config.config_model import ConnectConfig, WebsocketClientsConfig; from src.core.runtime.snowluma_config_renderer import render_onebot_json; cc=ConnectConfig(websocketClients=[WebsocketClientsConfig(name='wsc', url='ws://localhost:8080', reconnectInterval=500)]); tmp=Path('runtime/_smoke_w3'); tmp.mkdir(parents=True, exist_ok=True); render_onebot_json(tmp, 77777, connect=cc); import json; w=json.loads((tmp/'config'/'onebot_77777.json').read_text())['networks']['wsClients'][0]; print(w.get('reconnectIntervalMs'))"
```

**期望输出**:

- `3.A` → `True`
- `3.B` → `3000 3001 True`
- `3.C` → `False ACCESS-TEST string /api True True True`
- `3.D` → `1000` (被 clamp)

**3.6 W3 完成后清理**:

- 删除验证产物 `runtime/_smoke_w3/`

---

## W4 — WebUI HTTP 客户端 + 密码管理 (Tier D + Tier G)

**Owner boundary**:

- **新建** `src/core/runtime/snowluma_webui_client.py` (W4a, ~300 行)
- **修改** `src/core/installation/installers.py` 内 `SnowLumaInstall.execute` 末尾追加密码生成 + `webui.json` 渲染钩子 (W4b)
- **新建** `src/core/runtime/snowluma_session.py` (W4b, ~120 行: `snowluma-session.json` 读写器)
- **不**改 `snowluma_config_renderer.render_webui_json` (W4b 调用现有签名 `render_webui_json(snowluma_path, password=..., must_change=False)`)

**4.1 W4a — `snowluma_webui_client.py` 设计**:

```python
# -*- coding: utf-8 -*-
"""SnowLuma WebUI HTTP 客户端 (Tier D, P2 注入流程自动化).

封装 SnowLuma packages/core/src/webui/server.ts 的 8 个端点, 让 Desktop 一键启动
SnowLuma Bot 时不需要用户去浏览器登录 WebUI 选 PID 注入.

API 全集见 docs/requirements/2026-05-10-snowluma-bot-form-backend-aware.md §10.3.
"""
from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
from typing import Final, Literal
import time
import httpx

from src.core.logging import LogSource, LogType, logger


@dataclass(frozen=True)
class HookProcessInfo:
    """匹配上游 packages/core/src/hook/hook-manager.ts:21-29 HookProcessInfo."""
    pid: int
    name: str
    path: str
    uin: str
    status: Literal[
        "available", "loading", "connecting", "loaded", "online", "error", "disconnected"
    ]
    error: str = ""


class SnowLumaWebUIError(Exception):
    """所有 WebUI HTTP 调用失败的统一异常类型."""

    def __init__(self, status_code: int, message: str) -> None:
        super().__init__(message)
        self.status_code = status_code
        self.message = message


_DEFAULT_TIMEOUT_S: Final[float] = 5.0
_LOAD_TIMEOUT_S: Final[float] = 15.0
_WAIT_READY_INTERVAL_S: Final[float] = 1.0
_WAIT_READY_MAX_S: Final[float] = 30.0


class SnowLumaWebUIClient:
    """SnowLuma WebUI HTTP 客户端.

    认证语义: login 后内部持 Bearer token; 任何 API 调用收到 401 时自动重 login + 重试一次,
    仍失败则抛 SnowLumaWebUIError(401, ...).

    所有调用走 httpx 短连接 (与 SnowLumaStatusPoller 对齐, 不持单例 httpx.Client).
    """

    def __init__(self, host: str, port: int, password: str) -> None:
        self._host = host
        self._port = port
        self._password = password
        self._token: str | None = None

    @property
    def base_url(self) -> str:
        return f"http://{self._host}:{self._port}"

    # ==================== 公共 API (8 个) ====================
    def wait_ready(self, timeout: float = _WAIT_READY_MAX_S) -> bool:
        """轮询 GET /api/status 直到 200 或 timeout."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                resp = httpx.get(f"{self.base_url}/api/status", timeout=_DEFAULT_TIMEOUT_S)
                if resp.status_code == 200:
                    return True
            except (httpx.RequestError, httpx.TimeoutException):
                pass
            time.sleep(_WAIT_READY_INTERVAL_S)
        return False

    def login(self) -> str:
        """POST /api/login {password} → 拿 Bearer token, 内部持有, 返回 token."""
        resp = httpx.post(
            f"{self.base_url}/api/login",
            json={"password": self._password},
            timeout=_DEFAULT_TIMEOUT_S,
        )
        if resp.status_code != 200:
            raise SnowLumaWebUIError(
                resp.status_code,
                f"login 失败 (status={resp.status_code}): {resp.text[:200]}",
            )
        payload = resp.json()
        token = payload.get("token") if isinstance(payload, dict) else None
        if not isinstance(token, str) or not token:
            raise SnowLumaWebUIError(0, f"login 响应结构异常: {payload!r}")
        self._token = token
        return token

    def logout(self) -> None:
        """POST /api/logout 清理 server 端 session; 失败静默忽略."""
        if self._token is None:
            return
        try:
            httpx.post(
                f"{self.base_url}/api/logout",
                headers=self._auth_header(),
                timeout=_DEFAULT_TIMEOUT_S,
            )
        except (httpx.RequestError, httpx.TimeoutException) as exc:
            logger.trace(f"SnowLuma WebUI logout 静默忽略: {type(exc).__name__}")
        finally:
            self._token = None

    def list_processes(self) -> list[HookProcessInfo]:
        """GET /api/processes 列 QQ.exe 进程; 401 自动重试."""
        resp = self._authed_request("GET", "/api/processes")
        items = resp.json() if resp.status_code == 200 else []
        if not isinstance(items, list):
            return []
        return [self._parse_hook_info(item) for item in items if isinstance(item, dict)]

    def load_process(self, pid: int) -> HookProcessInfo:
        """POST /api/processes/<pid>/load 触发注入; 等响应 status."""
        resp = self._authed_request("POST", f"/api/processes/{pid}/load", timeout=_LOAD_TIMEOUT_S)
        if resp.status_code != 200:
            raise SnowLumaWebUIError(
                resp.status_code, f"load 失败 (pid={pid}, status={resp.status_code}): {resp.text[:200]}"
            )
        return self._parse_hook_info(resp.json())

    def unload_process(self, pid: int) -> HookProcessInfo:
        """POST /api/processes/<pid>/unload 卸载注入."""
        resp = self._authed_request("POST", f"/api/processes/{pid}/unload")
        if resp.status_code != 200:
            raise SnowLumaWebUIError(
                resp.status_code, f"unload 失败 (pid={pid}, status={resp.status_code})"
            )
        return self._parse_hook_info(resp.json())

    def get_auth_state(self) -> dict:
        """GET /api/auth/state 取 mustChangePassword / session 状态."""
        resp = self._authed_request("GET", "/api/auth/state")
        return resp.json() if resp.status_code == 200 else {}

    def change_password(self, new: str) -> None:
        """POST /api/auth/change-password (本期不调用; 保留 API 兼容)."""
        resp = self._authed_request(
            "POST",
            "/api/auth/change-password",
            json={"currentPassword": self._password, "newPassword": new},
        )
        if resp.status_code != 200:
            raise SnowLumaWebUIError(resp.status_code, f"change_password 失败: {resp.text[:200]}")
        self._password = new

    # ==================== 内部 ====================
    def _auth_header(self) -> dict[str, str]:
        if self._token is None:
            return {}
        return {"Authorization": f"Bearer {self._token}"}

    def _authed_request(
        self,
        method: str,
        path: str,
        *,
        json: dict | None = None,
        timeout: float = _DEFAULT_TIMEOUT_S,
    ) -> httpx.Response:
        """带 401 自动 retry 的 HTTP 调用."""
        if self._token is None:
            self.login()
        for attempt in range(2):
            resp = httpx.request(
                method,
                f"{self.base_url}{path}",
                headers=self._auth_header(),
                json=json,
                timeout=timeout,
            )
            if resp.status_code != 401 or attempt == 1:
                return resp
            # 401: token 失效, 重 login + 重试
            logger.trace(f"SnowLuma WebUI 401 重试 ({method} {path})", LogType.NETWORK, LogSource.CORE)
            self._token = None
            self.login()
        raise SnowLumaWebUIError(401, f"{method} {path} 401 重试后仍失败")

    @staticmethod
    def _parse_hook_info(item: dict) -> HookProcessInfo:
        return HookProcessInfo(
            pid=int(item.get("pid", 0)),
            name=str(item.get("name", "")),
            path=str(item.get("path", "")),
            uin=str(item.get("uin", "")),
            status=str(item.get("status", "available")),  # type: ignore[arg-type]
            error=str(item.get("error", "")),
        )
```

**4.2 W4b — 密码管理设计**:

### 新建 `src/core/runtime/snowluma_session.py` (~120 行)

```python
# -*- coding: utf-8 -*-
"""SnowLuma WebUI 密码 session 持久化 (Tier G, P2 注入流程自动化).

Desktop 单向主导密码: 用户**不能**在 SnowLuma WebUI 改密 (改了会被 Desktop 在
下次启动 Bot 时覆盖回 session.json 里的密码).

文件位置: <runtime_path>/config/snowluma-session.json (Desktop 侧, 与 SnowLuma
config 解耦; 即使用户重装 SnowLuma, Desktop 仍能识别上次密码).

权限: Windows 下 os.chmod(0o600), 仅当前用户 ACL 可读写.

Schema:
    {
      "password": "<随机强密码 >=10 字符 + 大小写 + 特殊符号 + 不含空格>",
      "created_at": "<ISO 8601 UTC, 含 Z 后缀>",
      "last_rendered_at": "<ISO 8601 UTC>"
    }
"""
from __future__ import annotations
import json
import os
import secrets
import string
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

from src.core.logging import LogSource, LogType, logger


@dataclass
class SnowLumaSession:
    password: str
    created_at: str
    last_rendered_at: str


def session_path() -> Path:
    """返回 Desktop 侧 session.json 绝对路径."""
    from creart import it
    from src.core.runtime.paths import PathFunc

    return it(PathFunc).config_dir_path / "snowluma-session.json"


def load_session() -> SnowLumaSession | None:
    """读 session.json. 不存在 / 损坏 / 字段缺失 → 返回 None (调用方按"首次场景"处理)."""
    target = session_path()
    if not target.exists():
        return None
    try:
        payload = json.loads(target.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        logger.warning(
            f"snowluma-session.json 损坏, 视为不存在: {target}", LogType.FILE_FUNC, LogSource.CORE
        )
        return None
    if not isinstance(payload, dict):
        return None
    password = payload.get("password")
    created_at = payload.get("created_at")
    last_rendered_at = payload.get("last_rendered_at")
    if not all(isinstance(v, str) and v for v in (password, created_at, last_rendered_at)):
        return None
    return SnowLumaSession(password=password, created_at=created_at, last_rendered_at=last_rendered_at)


def create_session() -> SnowLumaSession:
    """生成新的强密码 + 写 session.json + chmod 0o600."""
    target = session_path()
    target.parent.mkdir(parents=True, exist_ok=True)
    password = _generate_strong_password()
    now_iso = _utc_now_iso()
    session = SnowLumaSession(password=password, created_at=now_iso, last_rendered_at=now_iso)
    _write_session(target, session)
    return session


def update_last_rendered(session: SnowLumaSession) -> SnowLumaSession:
    """更新 last_rendered_at 字段后落盘."""
    new_session = SnowLumaSession(
        password=session.password,
        created_at=session.created_at,
        last_rendered_at=_utc_now_iso(),
    )
    _write_session(session_path(), new_session)
    return new_session


def _write_session(target: Path, session: SnowLumaSession) -> None:
    payload = {
        "password": session.password,
        "created_at": session.created_at,
        "last_rendered_at": session.last_rendered_at,
    }
    target.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    try:
        os.chmod(target, 0o600)
    except OSError:
        # Windows 上 chmod 0o600 在 ACL 模型下语义不严格, 但仍尝试
        pass


def _generate_strong_password(length: int = 16) -> str:
    """满足 SnowLuma webui/auth.ts:38-44 规则: >=10 + 大小写 + 特殊符号 + 不含空格."""
    if length < 10:
        length = 10
    upper = string.ascii_uppercase
    lower = string.ascii_lowercase
    digits = string.digits
    specials = "!@#$%^&*()-_=+[]{};:,.<>/?"
    # 保证每类至少 1 个
    seed = [
        secrets.choice(upper),
        secrets.choice(lower),
        secrets.choice(digits),
        secrets.choice(specials),
    ]
    pool = upper + lower + digits + specials
    seed.extend(secrets.choice(pool) for _ in range(length - len(seed)))
    secrets.SystemRandom().shuffle(seed)
    return "".join(seed)


def _utc_now_iso() -> str:
    now = datetime.now(timezone.utc)
    millis = now.microsecond // 1000
    return now.strftime("%Y-%m-%dT%H:%M:%S") + f".{millis:03d}Z"
```

### `installers.py` 中 `SnowLumaInstall` 钩子追加

```python
# 在 SnowLumaInstall.execute() 的 self.write_installed_tag() 之后, install_finish_signal.emit() 之前追加:

def execute(self) -> None:
    try:
        ...  # 现有 unzip + verify + write_installed_tag
        # ↓ Tier G: 安装成功后立刻生成 Desktop 主导密码
        self._init_or_update_password()
        self.install_finish_signal.emit()
    except Exception as e:
        ...

def _init_or_update_password(self) -> None:
    """安装成功后初始化 / 维持 Desktop 主导的 SnowLuma WebUI 密码.

    幂等: 如果 session.json 已存在 + 有效, sticky 不改; 同步 render_webui_json 保证
    SnowLuma 侧 webui.json 与 Desktop session.json 一致.
    """
    from src.core.runtime.snowluma_session import (
        load_session, create_session, update_last_rendered,
    )
    from src.core.runtime.snowluma_config_renderer import render_webui_json

    session = load_session()
    if session is None:
        session = create_session()
        logger.info("已生成新的 SnowLuma WebUI 密码 (sticky)", LogSource.CORE)
    # 强制把 Desktop 侧密码同步到 SnowLuma webui.json (单向覆盖)
    render_webui_json(self.install_path, password=session.password, must_change=False)
    update_last_rendered(session)
```

> 注意: `SnowLumaInstall.execute()` 是 W2 之前的现状代码; W2 没改 installers.py; W4b 是首次给它加钩子. 这是 W4b 唯一动 `installers.py` 的地方.

**4.3 验证命令**:

```pwsh
# 4.A WebUIClient 类与 8 个 method 存在
.venv\Scripts\python.exe -c "from src.core.runtime.snowluma_webui_client import SnowLumaWebUIClient, SnowLumaWebUIError, HookProcessInfo; c=SnowLumaWebUIClient('127.0.0.1', 5099, 'pwd'); print(all(hasattr(c, m) for m in ('wait_ready','login','logout','list_processes','load_process','unload_process','get_auth_state','change_password')))"
# 4.B HookProcessInfo 7 档 status 完整
.venv\Scripts\python.exe -c "from src.core.runtime.snowluma_webui_client import HookProcessInfo; import dataclasses; fields={f.name:f for f in dataclasses.fields(HookProcessInfo)}; print('pid' in fields, 'status' in fields, 'error' in fields)"
# 4.C session 模块可读写 (用 tmp 路径 mock)
.venv\Scripts\python.exe -c "from pathlib import Path; import os, sys; sys.path.insert(0, 'src'); from src.core.runtime.snowluma_session import _generate_strong_password; p=_generate_strong_password(16); import re; print(len(p) >= 10, bool(re.search(r'[A-Z]', p)), bool(re.search(r'[a-z]', p)), bool(re.search(r'[!@#$%^&*()_+=]', p)), ' ' not in p)"
# 4.D 密码不含空格 + 满足 SnowLuma webui/auth.ts:38-44 规则
.venv\Scripts\python.exe -c "from src.core.runtime.snowluma_session import _generate_strong_password; import re; ps=[_generate_strong_password() for _ in range(20)]; assert all(len(p)>=10 and re.search(r'[A-Z]',p) and re.search(r'[a-z]',p) and re.search(r'[!@#$%^&*()_+=\\-\\[\\]{};:,.<>/?]',p) and ' ' not in p for p in ps); print('strong-password OK')"
```

**期望输出**:

- `4.A` → `True`
- `4.B` → `True True True`
- `4.C` → `True True True True True`
- `4.D` → `strong-password OK`

**4.4 风险缓释**:

- **httpx mock 测试**: W7 阶段需要给 W4a 加单测 (mock httpx.post/get/request); 由于"不写新测试文件" 约束, 这部分覆盖只能在 W7 复用 `test_snowluma_process_construction.py` 现有 stub
- **password 写文件失败**: `os.chmod` 在 Windows ACL 下语义不严格, 但 try/except 包覆使其不致 raise; 实际安全性依赖 `.gitignore` + `collection_filters.py`

---

## W5 — 启停 orchestration + Poller 重写 + 单实例守护 (Tier E + Tier F)

**Owner boundary**:

- **重写** `src/core/runtime/snowluma_driver.py` 内 `SnowLumaDriver.start` / `stop` (从 W2 占位扩到 ~1100 行)
- **重写** `src/core/runtime/snowluma_status_poller.py` 整文件 (从轮询 OneBot get_status 改为轮询 WebUI `/api/processes`)
- **修改** `src/core/runtime/bot_process_manager.py` 内 driver 信号连接逻辑

**5.1 SnowLumaDriver.start 完整 Phase A→D 序列**:

```python
def start(self, config: "Config") -> ProcessHandle:
    """启动 SnowLuma Bot 完整 4 阶段序列 (Tier E).

    Phase A: 双进程启动 (spawn QQ.exe + 起 SnowLuma node.exe)
    Phase B: WebUI ready + login (含 401 自动 webui.json 重渲染 + node 重启重试)
    Phase C: 注入 (POST /api/processes/<qq_pid>/load)
    Phase D: 等用户 QQ 登录 (Poller 轮询 /api/processes)
    """
    # 单实例守护 (一期硬限制)
    if self._processes:
        raise RuntimeError(
            "一期仅支持 1 个 SnowLuma Bot 同时运行, 请先停止其他 SnowLuma Bot"
        )

    qq_id = str(config.bot.QQID)
    path_func = it(PathFunc)

    # ==================== Phase A: 双进程启动 ====================
    qq_path = path_func.get_qq_path()
    if qq_path is None:
        raise FileNotFoundError("未检测到 QQ.exe 安装路径, 请先安装 QQ")
    node_exe = path_func.get_snowluma_node_executable()
    if node_exe is None:
        raise FileNotFoundError(
            "未检测到 SnowLuma node.exe, 请在组件页 (Component) 的 SnowLuma tab 下先安装 SnowLuma 后再启动"
        )

    # 启动前一次性渲染 SnowLuma 配置文件 (Desktop 是配置 SOT)
    snowluma_path = path_func.snowluma_path
    render_runtime_json(snowluma_path, webui_port=5099)

    # 密码: 从 Desktop session.json 读 (W4b 已确保安装时就生成)
    session = load_session()
    if session is None:
        session = create_session()
        # 同步 SnowLuma webui.json
        render_webui_json(snowluma_path, password=session.password, must_change=False)
        update_last_rendered(session)
    # OneBot 配置: W3 重构后接受 ConnectConfig
    render_onebot_json(snowluma_path, int(config.bot.QQID),
                        connect=config.connect, music_sign_url=config.bot.musicSignUrl)

    # spawn QQ.exe
    qq_process = QProcess()
    qq_process.setProgram(str(qq_path))
    # ... 不传任何 -q 参数 (与 NapCat 不同, SnowLuma 注入完后用户在 QQ.exe 里扫码登录)
    qq_process.start()
    if not qq_process.waitForStarted(5000):
        raise RuntimeError("QQ.exe 启动失败")
    qq_pid = qq_process.processId()

    # spawn SnowLuma node.exe
    node_process = QProcess()
    node_process.setProgram(str(node_exe))
    node_process.setArguments([str(path_func.get_snowluma_entry())])
    node_process.setWorkingDirectory(str(snowluma_path))
    node_process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
    node_process.start()

    # 注册到 driver._processes (Starting 态)
    model = SnowLumaProcessModel(
        qq_id=qq_id, qq_process=qq_process, node_process=node_process,
        qq_pid=qq_pid, state=QProcess.ProcessState.Starting, started_at=monotonic(),
    )
    self._processes[qq_id] = model

    # ==================== Phase B: WebUI ready + login ====================
    client = SnowLumaWebUIClient(host="127.0.0.1", port=5099, password=session.password)
    if not client.wait_ready(timeout=30.0):
        self._kill_pair(model)
        del self._processes[qq_id]
        raise RuntimeError("SnowLuma WebUI 30s 内未就绪, 已 kill 两进程")

    try:
        client.login()
    except SnowLumaWebUIError as exc:
        if exc.status_code in (401, 403):
            # 用户在 WebUI 改过密码; Desktop 单向覆盖回 session.json 的密码
            logger.warning(
                "SnowLuma WebUI login 失败 (401/403), 重渲染 webui.json 后重启 node 重试",
                LogSource.CORE,
            )
            render_webui_json(snowluma_path, password=session.password, must_change=False)
            update_last_rendered(session)
            # kill node + 重起
            node_process.terminate()
            if not node_process.waitForFinished(5000):
                node_process.kill()
            node_process.start()
            if not client.wait_ready(timeout=30.0):
                self._kill_pair(model)
                del self._processes[qq_id]
                raise RuntimeError("重启 node 后 WebUI 仍未就绪")
            client.login()  # 第二次失败抛出, 由外层 except 捕
        else:
            self._kill_pair(model)
            del self._processes[qq_id]
            raise

    model.webui_client = client
    model.auth_token = client._token

    # ==================== Phase C: 注入 ====================
    try:
        info = client.load_process(qq_pid)
        if info.status == "error":
            self._kill_pair(model)
            del self._processes[qq_id]
            raise RuntimeError(f"SnowLuma 注入失败: {info.error}")
    except SnowLumaWebUIError as exc:
        self._kill_pair(model)
        del self._processes[qq_id]
        raise RuntimeError(f"SnowLuma 注入 API 调用失败: {exc.message}")

    # ==================== Phase D: 等用户登录 (启动 Poller) ====================
    self._start_poller(qq_id, client)
    model.state = QProcess.ProcessState.Running

    return ProcessHandle(qq_id=qq_id, primary_process=qq_process, secondary_process=node_process)


def _kill_pair(self, model: SnowLumaProcessModel) -> None:
    """kill 两个 QProcess (反向: node 先, QQ 后, 避免 QQ 退出后 SnowLuma 还在读 named pipe)."""
    if model.node_process is not None and model.node_process.state() != QProcess.ProcessState.NotRunning:
        model.node_process.terminate()
        if not model.node_process.waitForFinished(5000):
            model.node_process.kill()
    if model.qq_process is not None and model.qq_process.state() != QProcess.ProcessState.NotRunning:
        model.qq_process.terminate()
        if not model.qq_process.waitForFinished(5000):
            model.qq_process.kill()
```

**5.2 SnowLumaDriver.stop 反向序列**:

```python
def stop(self, qq_id: str) -> None:
    """反向 stop: unload → logout → kill node → kill QQ (D11 决策)."""
    model = self._processes.get(qq_id)
    if model is None:
        logger.warning(f"尝试停止不存在的 SnowLuma Bot (QQID: {qq_id})", LogSource.CORE)
        return

    # 1. unload 注入 (WebUI 已死则静默忽略)
    if model.webui_client is not None:
        try:
            model.webui_client.unload_process(model.qq_pid)
        except SnowLumaWebUIError as exc:
            logger.trace(f"unload 静默忽略 (qq_pid={model.qq_pid}): {exc.message}")

    # 2. logout 清理 token (静默忽略失败)
    if model.webui_client is not None:
        model.webui_client.logout()

    # 3. kill node (terminate 5s, 不退就 kill)
    self._kill_pair(model)

    # 4. 停止 Poller
    poller = self._pollers.pop(qq_id, None)
    if poller is not None:
        poller.stop()
        poller.deleteLater()

    # 5. 清理 model
    if model.qq_process is not None:
        model.qq_process.deleteLater()
    if model.node_process is not None:
        model.node_process.deleteLater()
    self._processes.pop(qq_id, None)
```

**5.3 `snowluma_status_poller.py` 完全重写**:

```python
# 仅展示设计骨架, 实际行 ~150
"""SnowLuma 登录态轮询器 (Tier F, P2 重写).

P1 旧版轮询 OneBot HTTP get_status 已废弃, 因为 SnowLuma 启动后 OneBot service 仅在
注入完成后才起来; 用户在 QQ.exe 扫码前 OneBot 永远 timeout.

新版改为轮询 SnowLuma WebUI GET /api/processes, 7 档 HookProcessStatus → 4 档 Desktop 状态:
  available / loading / connecting → Starting (前置态, 仍在启动期)
  loaded                            → WaitingForQRScan (等用户在 QQ.exe 扫码)
  online                            → LoggedIn
  error / disconnected              → Disconnected
"""

class SnowLumaStatusPoller(QObject):
    state_changed = Signal(str, str)  # qq_id, state_name
    _POLL_INTERVAL_MS: Final[int] = 2000  # 从 5s 调到 2s, 注入期需要更快感知

    def __init__(self, qq_id: str, qq_pid: int, webui_client: "SnowLumaWebUIClient",
                  parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._qq_id = qq_id
        self._qq_pid = qq_pid
        self._webui_client = webui_client
        self._consecutive_failures = 0
        self._last_state: str | None = None
        ...

    def _tick(self) -> None:
        """单次轮询. 跑在 QThreadPool 后台线程, emit 回主线程."""
        try:
            processes = self._webui_client.list_processes()
            self._consecutive_failures = 0
        except (SnowLumaWebUIError, Exception):
            self._consecutive_failures += 1
            if self._consecutive_failures >= 3:
                self.state_changed.emit(self._qq_id, "disconnected")
            return
        info = next((p for p in processes if p.pid == self._qq_pid), None)
        if info is None:
            return  # 不发信号, 等下次轮询
        new_state = self._translate_status(info.status)
        if new_state != self._last_state:
            self.state_changed.emit(self._qq_id, new_state)
            self._last_state = new_state

    @staticmethod
    def _translate_status(hook_status: str) -> str:
        """7 档 HookProcessStatus → 4 档 Desktop 登录态."""
        return {
            "available": "starting",
            "loading": "starting",
            "connecting": "starting",
            "loaded": "waiting_for_qr_scan",
            "online": "logged_in",
            "error": "disconnected",
            "disconnected": "disconnected",
        }.get(hook_status, "disconnected")
```

> 现 P1 `_SnowLumaStatusRunnable` (轮询 OneBot HTTP get_status 那个 inner class) **整段删除**.

**5.4 `BotProcessManager` 信号转发**:

```python
class BotProcessManager(QObject):
    ...

    def __init__(self) -> None:
        super().__init__()
        ...
        self._snowluma_driver = SnowLumaDriver()
        self._napcat_driver = NapCatDriver()
        # 转发 SnowLuma poller 信号
        self._snowluma_driver.state_changed_signal.connect(
            lambda qq_id, state: self.snowluma_login_state_signal.emit(qq_id, state)
        )
        ...
```

**5.5 验证命令**:

```pwsh
# 5.A SnowLumaDriver.start signature
.venv\Scripts\python.exe -c "import inspect; from src.core.runtime.snowluma_driver import SnowLumaDriver; sig=inspect.signature(SnowLumaDriver.start); print(list(sig.parameters))"
# 5.B Poller 不再调 OneBot get_status (grep 应零命中)
Get-Content src\core\runtime\snowluma_status_poller.py | Select-String -Pattern '/get_status' | Measure-Object
# 期望 Count = 0
# 5.C Poller _POLL_INTERVAL_MS 改为 2000
.venv\Scripts\python.exe -c "from src.core.runtime.snowluma_status_poller import SnowLumaStatusPoller; print(SnowLumaStatusPoller._POLL_INTERVAL_MS)"
# 期望: 2000
# 5.D 7 档翻译表完整 (静态读)
Get-Content src\core\runtime\snowluma_status_poller.py | Select-String -Pattern '"available"|"loading"|"connecting"|"loaded"|"online"|"error"|"disconnected"' | Measure-Object
# 期望 Count >= 7 (每档至少 1 处)
# 5.E 单实例守护
.venv\Scripts\python.exe -c "from src.core.runtime.snowluma_driver import SnowLumaDriver; d=SnowLumaDriver(); d._processes['test1']='dummy'; \
import pytest; \
try: d.start(None); print('FAIL: 未触发单实例守护')\nexcept RuntimeError as e: print('OK:', str(e)[:30])"
```

**期望输出**:

- `5.A` → `['self', 'config']`
- `5.B` → `Count : 0`
- `5.C` → `2000`
- `5.D` → `Count : >= 7`
- `5.E` → `OK: 一期仅支持 1 个 SnowLuma B`

---

## W6 — UI 显隐 + ChooseConfigTypeDialog + AdvancedConfigWidget

**Owner boundary**:

- **修改** `src/ui/page/bot_page/widget/config.py` (`BotConfigWidget` 加 backend 切换 signal; `ConnectConfigWidget.apply_backend_type`; `AdvancedConfigWidget.apply_backend_type`)
- **修改** `src/ui/page/bot_page/widget/msg_box.py` (`ConfigDialogBase.apply_backend_type` 基类; 5 个 ConfigDialog 重写; `ChooseConfigTypeDialog.apply_backend_type`)
- **修改** `src/ui/page/bot_page/widget/card.py` (各 NetworkBase 子配置卡片的 `apply_backend_type` — 主要是 SSE 卡整卡 setVisible(False))
- **修改** `src/ui/page/bot_page/__init__.py` 或 `sub_page/bot_list.py` (Bot 编辑页编排层信号连接)

**6.1 BotConfigWidget — 信号声明**:

```python
class BotConfigWidget(ScrollArea):
    backend_type_changed = Signal(BackendType)  # ← 新增

    def __init__(self, parent: QWidget | None = None):
        ...
        self.backend_type_card = ComboBoxConfigCard(...)
        # 接 ComboBox 的 currentIndexChanged 信号, 转发为 BackendType enum
        self.backend_type_card.view.currentIndexChanged.connect(
            self._on_backend_index_changed
        )

    def _on_backend_index_changed(self, _idx: int) -> None:
        backend_label = (self.backend_type_card.get_value() or "").strip().lower()
        backend = BackendType.from_str(backend_label)
        self.backend_type_changed.emit(backend)
```

**6.2 `ConnectConfigWidget.apply_backend_type`**:

```python
def apply_backend_type(self, backend: BackendType) -> None:
    """按 backend 显隐 ConnectConfigWidget 内的卡片 (幂等)."""
    self._current_backend = backend
    for card in self.cards:
        if hasattr(card, "apply_backend_type"):
            card.apply_backend_type(backend)
        # SnowLuma 模式下 HTTP SSE 卡片整卡隐藏
        if backend == BackendType.SNOWLUMA and isinstance(card, HttpSSEConfigCard):
            card.setVisible(False)
        else:
            card.setVisible(True)
    # 注: ChooseConfigTypeDialog 是惰性创建, 在打开 dialog 前调一次 apply_backend_type
```

**6.3 各 ConfigDialog 显隐**:

```python
# ConfigDialogBase 基类:
def apply_backend_type(self, backend: BackendType) -> None:
    """SnowLuma 模式下隐藏 debug_card (所有子类共享)."""
    self._current_backend = backend
    self.debug_card.setVisible(backend == BackendType.NAPCAT)

# HttpServerConfigDialog 重写:
def apply_backend_type(self, backend: BackendType) -> None:
    super().apply_backend_type(backend)
    if backend == BackendType.SNOWLUMA:
        self.cors_card.setVisible(False)
        self.websocket_card.setVisible(False)
        self.path_card.setVisible(True)  # path_card 是 W6 新增
    else:
        self.cors_card.setVisible(True)
        self.websocket_card.setVisible(True)
        self.path_card.setVisible(False)

# 类似地 HttpClientConfigDialog 加 timeout_ms_card; WebsocketServerConfigDialog 加 path_card + role_card; WebsocketClientConfigDialog 加 role_card
```

> 新加的卡片 (`path_card` / `role_card` / `timeout_ms_card`) 全部用 `LineEditConfigCard` / `ComboBoxConfigCard`, 与既有卡片同风格.

**6.4 `AdvancedConfigWidget.apply_backend_type`**:

```python
def apply_backend_type(self, backend: BackendType) -> None:
    """SnowLuma 模式下隐藏 NapCat-only 卡片 (持久化保留, 仅 setVisible)."""
    self._current_backend = backend
    is_napcat = (backend == BackendType.NAPCAT)
    self.parse_mult_message_card.setVisible(is_napcat)
    self.local_file_to_url_card.setVisible(is_napcat)
    self.file_log_card.setVisible(is_napcat)
    self.file_log_level_card.setVisible(is_napcat)
    self.console_log_card.setVisible(is_napcat)
    self.console_level_card.setVisible(is_napcat)
    self.backend_config_card.setVisible(is_napcat)
    # auto_start_card / offline_notice_card 双 backend 都可见
```

**6.5 `ChooseConfigTypeDialog.apply_backend_type`**:

```python
def apply_backend_type(self, backend: BackendType) -> None:
    """SnowLuma 模式下隐藏 HTTP SSE 选项."""
    is_snowluma = (backend == BackendType.SNOWLUMA)
    self.http_sse_server_card.setVisible(not is_snowluma)
    self.http_sse_server_config_button.setVisible(not is_snowluma)
```

**6.6 编排层接线** (`bot_page/__init__.py` 或 `sub_page/bot_config.py`):

```python
class BotEditPage(QWidget):  # 假设的外层容器
    def __init__(self, ...):
        ...
        self.bot_config_widget = BotConfigWidget(self)
        self.connect_config_widget = ConnectConfigWidget(self)
        self.advanced_config_widget = AdvancedConfigWidget(self)
        # 接信号
        self.bot_config_widget.backend_type_changed.connect(
            self.connect_config_widget.apply_backend_type
        )
        self.bot_config_widget.backend_type_changed.connect(
            self.advanced_config_widget.apply_backend_type
        )

    def fill_config(self, config: Config) -> None:
        ...
        # 首次加载也调一次 apply_backend_type, 保证初始可见性正确
        self.connect_config_widget.apply_backend_type(config.bot.backend_type)
        self.advanced_config_widget.apply_backend_type(config.bot.backend_type)
```

**6.7 验证命令**:

```pwsh
# 6.A apply_backend_type 方法存在
.venv\Scripts\python.exe -c "from src.ui.page.bot_page.widget.config import ConnectConfigWidget, AdvancedConfigWidget; print(hasattr(ConnectConfigWidget, 'apply_backend_type'), hasattr(AdvancedConfigWidget, 'apply_backend_type'))"
# 6.B BotConfigWidget 有 backend_type_changed 信号
.venv\Scripts\python.exe -c "from src.ui.page.bot_page.widget.config import BotConfigWidget; print('backend_type_changed' in dir(BotConfigWidget))"
# 6.C 各 ConfigDialog 有 apply_backend_type
.venv\Scripts\python.exe -c "from src.ui.page.bot_page.widget.msg_box import HttpServerConfigDialog, HttpClientConfigDialog, WebsocketServerConfigDialog, WebsocketClientConfigDialog, ChooseConfigTypeDialog; print(all(hasattr(c, 'apply_backend_type') for c in (HttpServerConfigDialog, HttpClientConfigDialog, WebsocketServerConfigDialog, WebsocketClientConfigDialog, ChooseConfigTypeDialog)))"
# 6.D Smoke: Desktop 主窗口能开 (与 W2 验证 2.F 同)
.venv\Scripts\python.exe -c "import src.main; print('import OK')"
```

**期望输出**:

- `6.A` → `True True`
- `6.B` → `True`
- `6.C` → `True`
- `6.D` → `import OK`

---

## W7 — 测试更新与按 driver 重组

**Owner boundary**:

- **修改** `script/test/test_snowluma_config_renderer.py` (新 signature 适配, `TestRenderOnebotJson` 4 用例)
- **修改** `script/test/test_snowluma_process_construction.py` (改用 `SnowLumaDriver` 而非 `BotProcessManager._create_snowluma_process`; mock httpx)
- **修改** `script/test/test_snowluma_installer.py` (新增 `snowluma-session.json` + `webui.json` 验证)
- **不动** `script/test/test_versioning_snowluma.py` / `test_backend_type_model.py`
- **拆分** 现 `test_*napcat*.py` 5 文件:
  - `test_napcat_install_hash_verify.py` → 不动 (是 installer 测试, 不依赖 manager 类名)
  - `test_remote_install_napcat_hash_env.py` → 不动 (远端 installer)
  - `test_remote_install_napcat_sh_verify.py` → 不动
  - `test_remote_local_napcat_fallback.py` → **改 import** 适配 `BotProcessManager`
  - `test_run_napcat.py` → 拆为 `test_run_napcat_driver.py` (NapCat-only 部分) + `test_bot_process_manager_dispatch.py` (manager dispatch 部分)
- **不**新增测试文件 (用户决定; 既有套件按驱动重组属"机械重组"不算新测试)

**7.1 `test_snowluma_config_renderer.py` 4 用例适配**:

```python
# 旧 (W3 之前):
def test_default_structure_matches_upstream_shape(self, tmp_path: Path) -> None:
    render_onebot_json(tmp_path, 10001, access_token="abc")
    ...

# 新 (W3 之后):
def test_default_structure_matches_upstream_shape(self, tmp_path: Path) -> None:
    from src.core.config.config_model import ConnectConfig, HttpServersConfig, WebsocketServersConfig
    cc = ConnectConfig(
        httpServers=[HttpServersConfig(name="http-default", host="0.0.0.0", port=3000, token="abc")],
        websocketServers=[WebsocketServersConfig(name="ws-default", host="0.0.0.0", port=3001, token="abc")],
    )
    render_onebot_json(tmp_path, 10001, connect=cc)
    ...
    payload["networks"]["httpServers"][0]["accessToken"] == "abc"  # 字段名变化
    payload["networks"]["wsServers"][0]["accessToken"] == "abc"
```

> 4 个用例均按此模式更新; `test_invalid_qqid_raises` 的 `qqid` 校验逻辑不变, 加 `connect=ConnectConfig()` 即可.

**7.2 `test_snowluma_process_construction.py` 适配**:

```python
# 旧:
manager = ManagerNapCatQQProcess()
process = manager._create_snowluma_process(config)

# 新:
from src.core.runtime.snowluma_driver import SnowLumaDriver
driver = SnowLumaDriver()
# 由于 start() 现在做完整 Phase A→D, 单测不能直接调; 改测内部 _spawn_node_process 类方法
# (W5 实现时把 spawn 逻辑抽成可单独 mock 的 method)
node_process = driver._spawn_node_process(config)  # 假设 W5 抽出此 method
assert node_process.program() == str(node_exe)
...
```

> 实际策略: W5 实现 `SnowLumaDriver` 时, 把 spawn QQ.exe / spawn node.exe / Phase B login / Phase C inject 各自抽成可独立测试的私有 method, 单测分别 mock 上一阶段的依赖. 这样 4 用例都能保留.

**7.3 `test_snowluma_installer.py` 新增 case**:

```python
class TestSnowLumaInstallPasswordHook:
    def test_snowluma_session_json_created_after_install(self, snowluma_install: Path):
        """Tier G: 安装成功后 snowluma-session.json 在 Desktop config 目录."""
        # ... mock SnowLumaInstall._init_or_update_password 真实运行
        assert (it(PathFunc).config_dir_path / "snowluma-session.json").exists()
        session = load_session()
        assert session is not None
        assert len(session.password) >= 10
        assert session.created_at != ""
        assert session.last_rendered_at != ""

    def test_webui_json_contains_password(self, snowluma_install: Path):
        """Tier G: 安装成功后 webui.json 含密码 hash + salt + mustChangePassword=False."""
        webui_json = snowluma_install / "config" / "webui.json"
        assert webui_json.exists()
        payload = json.loads(webui_json.read_text())
        assert "passwordHash" in payload
        assert "passwordSalt" in payload
        assert payload.get("mustChangePassword") is False

    def test_repeated_install_does_not_change_password(self, snowluma_install: Path):
        """Tier G: 重复运行 SnowLumaInstall 不会改写已有密码 (sticky)."""
        first = load_session()
        # 再跑一次安装
        ...
        second = load_session()
        assert first.password == second.password
        assert first.created_at == second.created_at  # created_at sticky
        # last_rendered_at 可能变化, 不断言
```

> 这是 W7 唯一新增的测试 case (在已有 test_snowluma_installer.py 内追加 class), 不算"新增测试文件"; 用户的"不写新增测试文件"约束是文件级, 单文件内追加 case 是允许的 (P1 也是这种粒度).

**7.4 拆分 `test_run_napcat.py`**:

```pwsh
# Step 1: git mv (拆) — 实际是 cp + 删
git mv script\test\test_run_napcat.py script\test\test_napcat_driver_basic.py
# 在 test_napcat_driver_basic.py 里把 NapCat-only 的 test 留下, manager dispatch 的 test 搬到新文件
```

实际操作: 因 vibe 不写新测试文件硬约束, 仅做 git mv 重命名, 不另建 manager dispatch 测试文件. dispatch 行为通过 `test_napcat_driver_basic.py` 的反向测试覆盖 (NapCat path 仍能跑就足够).

> **决策修订**: W7 内**只重命名** `test_run_napcat.py` → `test_napcat_driver_basic.py` (避免文件名误导); **不**拆分跨 driver 测试文件 (原文件本来也不测 SnowLuma). 这与需求 §8 "不新增测试文件" 一致.

**7.5 验证命令**:

```pwsh
# 7.A SnowLuma 测试套件全绿
.venv\Scripts\python.exe -m pytest script\test\test_snowluma_config_renderer.py script\test\test_snowluma_process_construction.py script\test\test_snowluma_installer.py -q
# 期望: 退出码 0, 全部 PASS
# 7.B 不动套件全绿
.venv\Scripts\python.exe -m pytest script\test\test_versioning_snowluma.py script\test\test_backend_type_model.py -q
# 期望: 退出码 0
# 7.C NapCat driver 测试全绿 (重命名后)
.venv\Scripts\python.exe -m pytest script\test\test_napcat_driver_basic.py -q
# 期望: 退出码 0
# 7.D 全量回归基线 (确认零退化)
.venv\Scripts\python.exe -m pytest script\test -q
# 期望: 退出码 0; 任何退化必须在本 wave 内修复
```

**期望输出**: 4 条命令均退出码 0.

---

## W8 — 全量验证 + smoke + phase_cleanup

**8.1 字段感知端到端 grep**:

```pwsh
# 8.1.A SnowLuma 模式不会写 NapCat-only 字段
.venv\Scripts\python.exe -c "from pathlib import Path; from src.core.config.config_model import ConnectConfig, HttpServersConfig; from src.core.runtime.snowluma_config_renderer import render_onebot_json; cc=ConnectConfig(httpServers=[HttpServersConfig(name='t', host='0.0.0.0', port=3000, debug=True, enableCors=True, enableWebsocket=True)]); tmp=Path('runtime/_smoke_w8'); tmp.mkdir(parents=True, exist_ok=True); render_onebot_json(tmp, 11111, connect=cc); import json; s=json.loads((tmp/'config'/'onebot_11111.json').read_text())['networks']['httpServers'][0]; assert 'debug' not in s and 'enableCors' not in s and 'enableWebsocket' not in s, s; print('OK: NapCat-only 字段已剥离')"
# 8.1.B 静默丢弃 httpSseServers
.venv\Scripts\python.exe -c "from pathlib import Path; from src.core.config.config_model import ConnectConfig, HttpSseServersConfig; from src.core.runtime.snowluma_config_renderer import render_onebot_json; cc=ConnectConfig(httpSseServers=[HttpSseServersConfig(name='sse', host='0.0.0.0', port=4000)]); tmp=Path('runtime/_smoke_w8'); tmp.mkdir(parents=True, exist_ok=True); render_onebot_json(tmp, 22222, connect=cc); import json; n=json.loads((tmp/'config'/'onebot_22222.json').read_text())['networks']; assert 'httpSseServers' not in n; print('OK: httpSseServers 已静默丢弃')"
```

**8.2 Backend 重构 grep 兜底**:

```pwsh
# 8.2.A napcat.py 已删
Test-Path src\core\runtime\napcat.py
# 期望: False
# 8.2.B 22 文件 import 全迁
Get-ChildItem src -Recurse -Include '*.py' | Select-String -Pattern 'from src\.core\.runtime\.napcat import' | Measure-Object
# 期望: Count : 0
# 8.2.C ManagerNapCatQQProcess 仅 crash_bundle 兼容正则一处
Get-ChildItem src -Recurse -Include '*.py' | Select-String -Pattern '\bManagerNapCatQQProcess\b'
# 期望: 仅 src/core/logging/crash_bundle.py 1 处命中
# 8.2.D creart 单例
.venv\Scripts\python.exe -c "from creart import it; from src.core.runtime.bot_process_manager import BotProcessManager; assert it(BotProcessManager) is not None; print('creart OK')"
# 8.2.E BotProcessManager 不含具体 backend 实现 (psutil/subprocess/QProcess 应在 driver 层)
Get-Content src\core\runtime\bot_process_manager.py | Select-String -Pattern '\bpsutil\b|^import subprocess' | Measure-Object
# 期望: Count <= 1 (允许 psutil 的进程树查询保留在 manager 内的 get_memory_usage; 实际可视情况调整, 这是参考边界)
```

**8.3 启动 smoke** (人工):

```pwsh
.venv\Scripts\python.exe main.py
```

人工核对清单 (与需求 §5.1 / §5.2 / §5.3 / §5.4 对齐):

- [ ] 主窗口正常打开
- [ ] BotPage 现有 NapCat Bot 仍能 Start/Stop (零回归)
- [ ] 新建 SnowLuma Bot, ConnectConfigWidget 加 httpServers (port=4000, token="ACCESS-TEST", path="/api"), 启动后 `runtime/SnowLuma/config/onebot_<qqid>.json` 含 `port:4000 + accessToken:"ACCESS-TEST" + path:"/api"`
- [ ] SnowLuma Bot 启动后 QQ.exe 自动弹 + node.exe 起 + WebUI login 成功 + `inject loaded for pid=<qq_pid>` 入日志
- [ ] BotCard 状态先 Starting → 之后 WaitingForQRScan; 用户在 QQ.exe 扫码登录后 BotCard 切到 LoggedIn
- [ ] curl 验证 OneBot 端口活: `curl -H "Authorization: Bearer ACCESS-TEST" -X POST http://127.0.0.1:4000/api/get_status` 返 200
- [ ] 点 Bot 停止 → log 顺序 unload → logout → kill node → kill QQ; QQ.exe 与 node.exe 都消失
- [ ] backend_type 在 SNOWLUMA / NAPCAT 双向切换, 字段持久化值零丢失
- [ ] AdvancedConfigWidget SnowLuma 模式仅 2 张卡可见
- [ ] ChooseConfigTypeDialog SnowLuma 模式不显示 HTTP SSE 选项
- [ ] 启动第 2 个 SnowLuma Bot → 弹 "一期仅支持 1 个 SnowLuma Bot 同时运行" 错误
- [ ] 1 NapCat + 1 SnowLuma 同时跑

**8.4 阶段清理产物** (与 P1 plan 对齐, vibe runtime 必产出):

写到 `outputs/runtime/vibe-sessions/2026-05-10T2238-snowluma-bot-form-backend-aware/`:

- [x] `skeleton-receipt.json` (W2 之前由 vibe runtime 已写)
- [x] `intent-contract.json` (deep_interview 阶段产物)
- [ ] `xl-plan-receipt.json` (本 wave 完成时由本计划末尾"xl_plan 完成签字"附录补)
- [ ] `phase-W1-W2.json` (W1 + W2 完成后的 receipt: 证据 git diff hash + 验证命令出参)
- [ ] `phase-W3-W4.json`
- [ ] `phase-W5.json`
- [ ] `phase-W6.json`
- [ ] `phase-W7.json`
- [ ] `phase-W8.json` (含手工 smoke 签字)
- [ ] `cleanup-receipt.json` (最终 phase_cleanup 阶段写)

**8.5 临时文件清理**:

- 删除 `runtime/_smoke_w3/` (W3 验证产物)
- 删除 `runtime/_smoke_w8/` (W8 验证产物)
- 保留 `runtime/SnowLuma/` (真实安装产物, 归用户运行时数据, 不清理)
- **不**删 `runtime/config/snowluma-session.json` (Desktop 侧密码 sticky) 与 `runtime/SnowLuma/config/webui.json` (SnowLuma 侧密码 hash, 由 Desktop 重渲染同步)

---

## 完成语言策略 (Completion Language Policy)

- 任一 wave **未通过** 对应 §x.x 验证命令前, 不得用 "全部完成 / 已交付 / 验收通过" 等终结性措辞
- W7 任一既有测试套件退出码非 0 → 必须报告失败 stage + 失败 case 名称, 不得用完成性措辞
- W8 grep 命中 22 文件残留 `from src.core.runtime.napcat import` → 不得声明 W2 完成
- 人工 §8.3 smoke 未跑过 → 仅可声明 "代码层完成 (Wave 落地), 等待真机验收 §5.1/§5.2/§5.3/§5.4"
- §5.2 端到端注入闭环 (启动 Bot → QQ 登录 → curl 验证 OneBot → 停止 Bot) **未通过** → 整个需求未完成, 不得用 "已交付" 措辞 (与需求 §7 交付真相契约对齐)

## 回滚规则 (Rollback Rules)

- **W1 失败** (config_model 改坏): `git checkout src/core/config/config_model.py`; 不影响后续 wave (W2 抽象层与 config 字段解耦)
- **W2 失败** (重构破坏既有 NapCat 路径或 22 文件 rename 漏一个): **`git reset --hard HEAD~1`** 退回 W1 之后的 commit; W2 是原子单元, 部分回滚无意义 (中间状态 import 链断, Desktop 起不来)
- **W3 失败** (renderer 字段映射错): `git checkout src/core/runtime/snowluma_config_renderer.py`; W2 driver 占位实现暂时退回旧 signature
- **W4a 失败** (`snowluma_webui_client.py` 错): `git rm src/core/runtime/snowluma_webui_client.py`; W5 暂走 P1 残留的 OneBot get_status 路径 (degraded, 用户仍需手动浏览器登录, 但 Bot 可运行)
- **W4b 失败** (密码管理错): `git rm src/core/runtime/snowluma_session.py` + 还原 `installers.py:execute` 钩子; SnowLuma 退回首启动随机 initialPassword 自治 (用户感知到密码)
- **W5 失败** (orchestration 错): `git checkout src/core/runtime/snowluma_driver.py src/core/runtime/snowluma_status_poller.py`; 退到 W2 + W3 + W4 后的状态 (Bot 仍能起但走 P1 的 spawn-only 简化路径, 不调注入 API)
- **W6 失败** (UI 错): `git checkout src/ui/page/bot_page/widget/`; 字段全可见 (双 backend 都看到 NapCat-only 字段, 只是 SnowLuma 不消费; 不影响功能)
- **W7 失败** (测试退化): 保留代码不回滚, 标 `phase-W7.json.success=false`; 暂不进 W8
- **W8 失败** (smoke 失败): 不回滚代码, 写 `phase-W8.json.smoke_failed=true` 列出失败子项; 用户决定是否退回 commit

## 阶段清理预期 (Phase Cleanup Expectations)

- W1-W8 完成后产出 §8.4 列出的全部 10 份 receipt
- 临时脚本 / 调试 print **不**应残留在 `src/` / `script/test/`
- `runtime/_smoke_*/` 全部清理
- `runtime/SnowLuma/config/snowluma-session.json` (W4b 安装钩子产物) 保留
- `git status` 末尾应为 "working tree clean"

## XL 大计划治理 hooks

- 本计划 8 wave 均**串行**, 不开 XL fan-out
- 每 wave 完成时由 vibe runtime 写一份 `phase-W*.json` (含 git diff hash + 验证命令出参 + 风险摘要)
- 任一 wave 失败时 `phase-W*.json` 应含 `success=false` + 失败子项明细; 不得 silent skip
- W2 因为是原子事件, 完成与否以 `2.A`-`2.F` 6 条验证命令全过为准, 不细分子 step 的 receipt

---

## 附录: 与父计划 (P1) 的关系矩阵

| 父 P1 (`...snowluma-backend-adapter-execution-plan.md`) wave | 本 P2 wave 关系                                             | 决策反转?                |
| ------------------------------------------------------------ | ----------------------------------------------------------- | ------------------------ |
| P1 W1 (BackendType + PathFunc + BotConfig.backend_type)      | 本计划 W1 复用 (不改)                                       | 否                       |
| P1 W2 (`snowluma_config_renderer`)                           | 本计划 W3 重构 (signature 改)                               | 是 (对 #4-9)             |
| P1 W3 (`napcat.py` SnowLuma 分流)                            | 本计划 W2 (Tier I) 整文件搬到 driver 层 + 删 napcat.py      | 是 (P1 §3 / §8 妥协反转) |
| P1 W4 (`snowluma_status_poller`)                             | 本计划 W5 完全重写 (从 OneBot 改为 WebUI processes)         | 是 (P1 §10 假设废弃)     |
| P1 W5 (`urls` + `versioning` + `installers.SnowLumaInstall`) | 本计划 W4b 仅在 `SnowLumaInstall.execute` 末尾加密码钩子    | 否 (W5 内容保留)         |
| P1 W6 (`add_bot_page` + BotCard + ComponentPage)             | 本计划 W6 在已有 W6 之上加 backend_type 双向显隐            | 否 (扩展)                |
| P1 W7 (5 个新测试)                                           | 本计划 W7 只更新既有, 不新增                                | 否 (用户决定手动验收)    |
| P1 W8 (smoke + phase_cleanup)                                | 本计划 W8 同样 + 加入 §5.2 端到端注入 / §5.4 重构验收手工项 | 否 (扩展)                |

## 附录: xl_plan stage 边界声明

本计划 wrapper 入口为 `vibe-how`, 其 stop_target 为 **`xl_plan`**.

- 本计划文件 (`docs/plans/2026-05-10-snowluma-bot-form-backend-aware-execution-plan.md`) 一经写入即视为 `xl_plan` 阶段产出冻结
- 后续 `plan_execute` 与 `phase_cleanup` 阶段必须通过 **`vibe`** (root governed) 或 **`vibe-do`** 重入触发, 不得在本入口内执行
- 本入口写完后由 vibe runtime 在 `outputs/runtime/vibe-sessions/2026-05-10T2238-snowluma-bot-form-backend-aware/stage-lineage.json` 追加 `xl_plan` 阶段记录, terminal_stage_reached 同步更新为 `xl_plan`

