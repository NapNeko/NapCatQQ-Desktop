# SnowLuma 后端并列适配 — 执行计划

- **关联需求**：`docs/requirements/2026-05-10-snowluma-backend-adapter.md`
- **内部执行级别 (Internal Grade)**：**L** — 单 agent 串行执行；wave 之间存在线性依赖链，XL fan-out 收益不抵合并风险
- **runtime**：`interactive_governed` (`root_governed` lane)

---

## Wave 结构

| Wave | 名称                                                 | 依赖       | 串行 / 并行                             |
| ---- | ---------------------------------------------------- | ---------- | --------------------------------------- |
| W1   | 数据层：BackendType 枚举 + PathFunc + BotConfig 字段 | —          | 串行                                    |
| W2   | 配置渲染器：`snowluma_config_renderer`               | W1         | 串行                                    |
| W3   | 进程管理：`_create_snowluma_process` + 状态机分流    | W1, W2     | 串行                                    |
| W4   | 状态轮询：`snowluma_status_poller` + 登录态扩展      | W3         | 串行                                    |
| W5   | 安装器与版本服务：urls + versioning + installer      | W1         | 串行（与 W2-W4 无文件交集，但保守串行） |
| W6   | UI 适配：add_bot / card / setting                    | W1, W3, W5 | 串行                                    |
| W7   | 单元测试                                             | W1-W6      | 串行                                    |
| W8   | 验证 + phase_cleanup                                 | W7         | 串行                                    |

> 内部 grade 选 L 而非 XL：W2/W3/W4 共享对 `PathFunc` 与 `ManagerNapCatQQProcess` 的写权限，W6 又跨多页面文件，多 agent fan-out 的合并冲突概率高于关键路径压缩收益。

---

## 跨 wave 不变量 (Invariants)

- 任何 wave 都**不允许**修改 NapCat 既有启动路径的可观察行为（`_create_napcat_process` / `_get_env_variable` / `_write_load_script` 函数体保持现状）
- 所有 SnowLuma 新增模块禁止在 `src/core/runtime/napcat.py` 内部直接 import；走 `PathFunc` 与 `BackendType` 解耦
- W2 渲染器只在「进程启动前」被调用一次；W3 启动逻辑不得在子进程 Running 后再 emit 渲染调用
- `BotConfig.backend_type` 字段必须默认 `BackendType.NAPCAT`；旧 bot.json 反序列化路径**不**因新字段而失败

---

## W1 — `src/core/runtime/backend_type.py` + `paths.py` + `config_model.py`

**Owner boundary**：

- 新建 `src/core/runtime/backend_type.py`
- 修改 `src/core/runtime/paths.py`（仅 `PathFunc.__init__` 与 `path_validator`，新增 4 个 getter）
- 修改 `src/core/config/config_model.py`（仅 `BotConfig` 加字段 + 兼容反序列化）

**1.1 `backend_type.py` 输出**：

```python
from enum import Enum

class BackendType(str, Enum):
    NAPCAT = "napcat"
    SNOWLUMA = "snowluma"

    @classmethod
    def from_str(cls, value: str | None) -> "BackendType":
        if not value:
            return cls.NAPCAT
        try:
            return cls(value)
        except ValueError:
            return cls.NAPCAT  # 兼容未知字符串，降级到 NAPCAT

    @property
    def display_name(self) -> str:
        return {self.NAPCAT: "NapCat", self.SNOWLUMA: "SnowLuma"}[self]
```

**1.2 `paths.py` 改动**：

```python
class PathFunc:
    def __init__(self) -> None:
        ...
        self.napcat_path = self.runtime_path / "NapCatQQ"
        self.snowluma_path = self.runtime_path / "SnowLuma"  # ← 新增
        ...

    def path_validator(self) -> None:
        paths_to_validate = [
            (self.tmp_path, "Tmp"),
            (self.config_dir_path, "config"),
            (self.napcat_path, "NapCat"),
            (self.snowluma_path, "SnowLuma"),  # ← 新增
        ]
        ...

    # ↓ 新增 4 个 getter
    def get_snowluma_node_executable(self) -> Path | None:
        node = self.snowluma_path / "node.exe"
        return node if node.exists() else None

    def get_snowluma_entry(self) -> Path:
        return self.snowluma_path / "index.mjs"

    def get_snowluma_config_dir(self) -> Path:
        return self.snowluma_path / "config"

    def get_snowluma_data_dir(self) -> Path:
        return self.snowluma_path / "data"
```

**1.3 `config_model.py` 改动**：

```python
class BotConfig(BaseModel):
    ...
    backend_type: BackendType = BackendType.NAPCAT  # ← 新增
    ...

    @field_validator("backend_type", mode="before")
    @classmethod
    def _coerce_backend_type(cls, v):
        return BackendType.from_str(v) if isinstance(v, str) else v
```

**验证命令**：
```pwsh
.venv\Scripts\python.exe -c "from src.core.runtime.backend_type import BackendType; print(list(BackendType), BackendType.from_str('snowluma'), BackendType.from_str(None))"
.venv\Scripts\python.exe -c "from src.core.runtime.paths import PathFunc; from creart import it; p=it(PathFunc); print(p.snowluma_path, p.get_snowluma_entry())"
.venv\Scripts\python.exe -c "from src.core.config.config_model import BotConfig; b=BotConfig.model_validate({'QQID':10000}); print(b.backend_type)"
```

**期望输出**：
- `[<BackendType.NAPCAT: 'napcat'>, <BackendType.SNOWLUMA: 'snowluma'>] BackendType.SNOWLUMA BackendType.NAPCAT`
- `<runtime_path>/SnowLuma <runtime_path>/SnowLuma/index.mjs`
- `BackendType.NAPCAT`

---

## W2 — `src/core/runtime/snowluma_config_renderer.py`

**Owner boundary**：仅新建该文件，不改其他文件。

**API**：

```python
def render_runtime_json(snowluma_path: Path, *, webui_port: int = 5099) -> None: ...

def render_webui_json(
    snowluma_path: Path,
    *,
    password: str | None = None,
    must_change: bool = False,
) -> None: ...
# 当 password is None 时：若 webui.json 已存在则不覆盖；不存在则什么都不做（让 SnowLuma 自首次启动时生成）。
# 当 password 非空时：强制写入新的 hash + salt（用 SHA-512 + 16 字节随机 salt，与上游 webui.json 字段对齐）。

def render_onebot_json(
    snowluma_path: Path,
    qqid: int,
    *,
    http_port: int = 3000,
    ws_port: int = 3001,
    access_token: str = "",
    message_format: Literal["array", "string"] = "array",
    report_self_message: bool = False,
    music_sign_url: str = "",
) -> None: ...
# 路径：snowluma_path / "config" / f"onebot_{qqid}.json"
# 结构按上游 SnowLuma-v1.7.5-win-x64/config/onebot_2550419068.json 1:1 复刻：
#   { "networks": { "httpServers": [...], "httpClients": [], "wsServers": [...], "wsClients": [] }, "musicSignUrl": "" }

def read_existing_onebot_json(snowluma_path: Path, qqid: int) -> dict | None: ...
# 如果文件存在则返回原 dict，让上层做 dict.update 后再 render；不存在返回 None。
```

**关键实现要点**：

- 使用 `json.dump(..., ensure_ascii=False, indent=2)` 与上游格式对齐
- 写入前先 `mkdir(parents=True, exist_ok=True)`，目录就是 `snowluma_path/config/`
- access_token 默认空串而不是 None（与上游样本一致）；调用方负责生成

**验证命令**：

```pwsh
.venv\Scripts\python.exe -c "from pathlib import Path; from src.core.runtime.snowluma_config_renderer import render_runtime_json, render_onebot_json; tmp=Path('runtime/_smoke_snowluma'); render_runtime_json(tmp); render_onebot_json(tmp, 10000, access_token='abc'); import json; print(json.loads((tmp/'config'/'runtime.json').read_text()), json.loads((tmp/'config'/'onebot_10000.json').read_text()))"
```

**期望输出**：包含 `'webuiPort': 5099` 与 `'accessToken': 'abc'`、`'port': 3000`、`'port': 3001` 字样。

---

## W3 — `src/core/runtime/napcat.py` 进程管理分流

**Owner boundary**：仅修改 `ManagerNapCatQQProcess` 类内部；不改其对外 public 方法签名。

**改动点**：

1. `create_napcat_process(config)` 入口 in-place 增加 backend 分支（`if not config.bot.is_remote:` 内部新增 `if config.bot.backend_type == BackendType.SNOWLUMA: ...`）
2. 新增 `_create_snowluma_process(config: Config) -> QProcess`：

```python
def _create_snowluma_process(self, config: Config) -> QProcess:
    path_func = it(PathFunc)
    node_exe = path_func.get_snowluma_node_executable()
    if node_exe is None:
        raise FileNotFoundError(
            "未检测到 SnowLuma node.exe，请在组件页 (Component) 的 SnowLuma tab 下先安装 SnowLuma 后再启动"
        )

    # 启动前渲染配置（一次性）
    from src.core.runtime.snowluma_config_renderer import (
        render_runtime_json,
        render_onebot_json,
        render_webui_json,
    )
    render_runtime_json(path_func.snowluma_path, webui_port=5099)
    render_webui_json(path_func.snowluma_path)  # 不覆盖既有
    render_onebot_json(
        path_func.snowluma_path,
        config.bot.QQID,
        http_port=config.bot.http_port,
        ws_port=config.bot.ws_port,
        access_token=config.bot.access_token,
        message_format=config.bot.message_format,
        report_self_message=config.bot.report_self_message,
    )

    process = QProcess()
    process.setProgram(str(node_exe))
    process.setArguments([str(path_func.get_snowluma_entry())])
    process.setWorkingDirectory(str(path_func.snowluma_path))
    process.setProcessChannelMode(QProcess.ProcessChannelMode.MergedChannels)
    # 不设 environment：使用 systemEnvironment，避免误注入 NAPCAT_*
    return process
```

3. 在 `create_napcat_process` 的 backend 分流处调用：

```python
if config.bot.backend_type == BackendType.SNOWLUMA:
    try:
        process = self._create_snowluma_process(config)
    except FileNotFoundError as exc:
        logger.error(str(exc), LogType.FILE_FUNC, LogSource.CORE)
        self.notification_signal.emit("error", str(exc))
        return
else:
    if (qq_path := path_func.get_qq_path()) is None:
        ...  # 现有逻辑
    process = self._create_napcat_process(config, qq_path)
```

**关键约束**：

- `BotConfig` 上的 `http_port` / `ws_port` / `access_token` / `message_format` / `report_self_message` 字段如尚未存在，**本 wave 不引入**；W2 内部默认值兜底，W6 UI 阶段再决定是否要把这些字段暴露到 UI（暴露与否走单独需求，本期 deliverable 第 2.6 节没列）
- `_handle_process_state_changed` / `_handle_process_finished` / `_handle_local_start_error` 不动；它们对 `QProcess` 的处理在两种 backend 下行为相同
- `stop_process` 不动（已有的 `psutil.Process(pid).children(recursive=True)` 方案对 `node.exe` 同样有效）

**验证命令**：

```pwsh
.venv\Scripts\python.exe -c "from PySide6.QtCore import QProcess; from src.core.runtime.napcat import ManagerNapCatQQProcess; print(hasattr(ManagerNapCatQQProcess, '_create_snowluma_process'))"
```

**期望输出**：`True`

---

## W4 — `src/core/runtime/snowluma_status_poller.py` + 登录态扩展

**Owner boundary**：

- 新建 `src/core/runtime/snowluma_status_poller.py`
- 修改 `ManagerNapCatQQLoginState` 增加 `WaitingForQRScan` 态（**不**破坏现有 `LoggedIn` / `Disconnected` 行为）

**4.1 Poller 设计**：

```python
class SnowLumaStatusPoller(QObject):
    """通过 OneBot WS get_status 接口轮询 SnowLuma 登录态。

    与 NapCat 的 status.json 落盘读取语义对齐：
    - 进程未运行 → 不轮询
    - WS 连不通 / 鉴权失败 → 视为 WaitingForQRScan（容忍 SnowLuma 启动早期）
    - get_status.online == True → LoggedIn
    - get_status.online == False → WaitingForQRScan
    """

    state_changed = Signal(str, str)  # qq_id, state_name (LoggedIn / WaitingForQRScan)
    _POLL_INTERVAL_MS = 5000
```

实现要点：

- 使用 `QTimer` + `httpx` (短超时 2s) 调 OneBot WS 的 HTTP 端点等价 API（SnowLuma 的 OneBot WS 同时暴露 HTTP 接口在 `http_port=3000`，`POST /get_status`）
- 失败仅 `logger.trace(...)`，不污染主日志
- 启动入口在 `_handle_process_state_changed(qq_id, Running)` 里：当 backend_type=SNOWLUMA 且 state=Running 时启动该 qq_id 的 poller；finished 时停掉

**4.2 `ManagerNapCatQQLoginState` 扩展**：

```python
class LoginPhase(str, Enum):
    DISCONNECTED = "disconnected"
    LOGGED_IN = "logged_in"
    WAITING_FOR_QR_SCAN = "waiting_for_qr_scan"  # ← 新增，仅 SnowLuma 用
```

`get_login_state(qq_id)` 已有；新增 `set_phase(qq_id, LoginPhase)` 由 poller 调用。

**验证命令**：

```pwsh
.venv\Scripts\python.exe -c "from src.core.runtime.snowluma_status_poller import SnowLumaStatusPoller; print(SnowLumaStatusPoller._POLL_INTERVAL_MS)"
```

**期望输出**：`5000`

---

## W5 — `src/core/network/urls.py` + `versioning/service.py` + `installation/installers.py`

**Owner boundary**：

- 修改 `src/core/network/urls.py`（仅追加常量）
- 修改 `src/core/versioning/service.py`（`VersionSnapshot` 追加 `snowluma_*`；`RemoteVersionTask.execute` 多拉一份；`LocalVersionTask` 补 `get_snowluma_version`）
- 修改 `src/core/installation/installers.py`（在 `NapCatInstall` 旁追加 `SnowLumaInstall` 类，不新建独立目录）

**5.1 `urls.py` 追加**：

```python
# SnowLuma 相关地址
SNOWLUMA_REPO = QUrl("https://github.com/SnowLuma/SnowLuma")
SNOWLUMA_ISSUES = QUrl("https://github.com/SnowLuma/SnowLuma/issues")
SNOWLUMA_REPO_API = QUrl("https://api.github.com/repos/SnowLuma/SnowLuma/releases/latest")
SNOWLUMA_REPO_API_FALLBACK = QUrl("https://api.github.com/repos/SnowLuma/SnowLuma/releases/latest")
SNOWLUMA_DOWNLOAD_TEMPLATE = "https://github.com/SnowLuma/SnowLuma/releases/download/{tag}/SnowLuma-{tag}-win-x64.zip"
```

> 镜像站位为空时主/备同 URL；后续如有镜像追加只改 `SNOWLUMA_REPO_API`。

**5.2 `versioning/service.py` 改动**：

```python
@dataclass
class VersionSnapshot:
    napcat_version: str | None
    qq_version: str | None
    ncd_version: str | None
    snowluma_version: str | None        # ← 新增
    qq_download_url: str | None
    napcat_update_log: str | None
    ncd_update_log: str | None
    snowluma_update_log: str | None     # ← 新增

class RemoteVersionTask(VersionTaskBase):
    def execute(self) -> VersionSnapshot:
        napcat_info = self._get_version_with_fallback(...)
        qq_version = self._get_version(...)
        ncd_version = self._get_version_with_fallback(...)
        snowluma_info = self._get_version_with_fallback(  # ← 新增
            Urls.SNOWLUMA_REPO_API.value,
            Urls.SNOWLUMA_REPO_API_FALLBACK.value,
            "SnowLuma",
            self._parse_github_response,
        )
        return VersionSnapshot(
            napcat_version=napcat_info["version"],
            qq_version=qq_version["version"],
            ncd_version=ncd_version["version"],
            snowluma_version=snowluma_info["version"],
            qq_download_url=qq_version["download_url"],
            napcat_update_log=napcat_info["update_log"],
            ncd_update_log=ncd_version["update_log"],
            snowluma_update_log=snowluma_info["update_log"],
        )

    def _get_error_value(self, name):
        error_values = {
            ...,
            "SnowLuma": {"version": None, "update_log": None},  # ← 新增
        }

class LocalVersionTask(VersionTaskBase):
    def execute(self) -> VersionSnapshot:
        return VersionSnapshot(
            napcat_version=self.get_napcat_version(),
            qq_version=self.get_qq_version(),
            ncd_version=self.get_ncd_version(),
            snowluma_version=self.get_snowluma_version(),  # ← 新增
            qq_download_url=None,
            napcat_update_log=None,
            ncd_update_log=None,
            snowluma_update_log=None,                       # ← 新增
        )

    def get_snowluma_version(self) -> str | None:           # ← 新增
        """读 <snowluma_path>/package.json 的 version 字段。

        上游发布包中的 package.json 结构为:
            {"name": "@snowluma/runtime", "version": "0.1.0", ...}
        注意: 该 version 是内部版本, 不是 release tag (eg v1.7.5)。
        本期仅用于 "是否已安装" 与 "是否需要更新" 的启启判断,
        上游跳动 package.json.version 实践存在时需同步调整代码.
        """
        try:
            package_json = it(PathFunc).snowluma_path / "package.json"
            if not package_json.exists():
                return None
            payload = json.loads(package_json.read_text(encoding="utf-8"))
            value = payload.get("version") if isinstance(payload, dict) else None
            if not isinstance(value, str):
                return None
            return value.strip() or None
        except (OSError, json.JSONDecodeError):
            return None
```

**5.3 `installation/installers.py` 追加 `SnowLumaInstall` 类**（不新建独立目录）：

```python
# 与现有 NapCatInstall 同文件、同语义、同 signal 名。
# 让 SnowLumaPage 能完全复刻 NapCatPage 的信号连接模式。
class SnowLumaInstall(InstallerBase):
    """SnowLuma 发布包解压安装器. """

    status_label_signal = Signal(str)
    error_finish_signal = Signal()
    progress_ring_toggle_signal = Signal(int)   # ProgressRingStatus
    install_finish_signal = Signal()

    def __init__(self, tag: str) -> None:
        super().__init__()
        self._tag = tag
        self._zip_path = it(PathFunc).tmp_path / f"SnowLuma-{tag}-win-x64.zip"
        self._target = it(PathFunc).snowluma_path

    def run(self) -> None:
        try:
            self.status_label_signal.emit(self.tr("正在解压 SnowLuma..."))
            self._extract()
            self._verify()
            self.install_finish_signal.emit()
        except Exception as exc:  # noqa: BLE001
            logger.error(f"SnowLuma 安装失败: {type(exc).__name__}: {exc}", log_source=LogSource.CORE)
            self.error_finish_signal.emit()
        finally:
            self._zip_path.unlink(missing_ok=True)

    def _extract(self) -> None:
        # 关键：覆盖文件，但保留 target/config 与 target/data 子目录已有文件
        self._target.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(self._zip_path) as zf:
            for member in zf.namelist():
                # zip 内的顶层目录是 SnowLuma-vX.Y.Z-win-x64/，需要去掉首段
                stripped = member.split("/", 1)[1] if "/" in member else member
                if not stripped:
                    continue
                out = self._target / stripped
                if member.endswith("/"):
                    out.mkdir(parents=True, exist_ok=True)
                    continue
                if stripped.startswith(("config/", "data/")) and out.exists():
                    # 已有配置/数据：保留用户运行时修改
                    continue
                out.parent.mkdir(parents=True, exist_ok=True)
                with zf.open(member) as src, out.open("wb") as dst:
                    shutil.copyfileobj(src, dst)

    def _verify(self) -> None:
        for required in ("node.exe", "index.mjs", "package.json"):
            if not (self._target / required).exists():
                raise RuntimeError(f"安装产物缺失: {required}")
```

> `SnowLumaPage.handle_install_requested` 会 `QThreadPool.globalInstance().start(SnowLumaInstall(tag=remote_version))`，连接 4 个信号到 `update_operation_status_text` / `handle_operation_failed` / `update_operation_progress_ring` / `handle_install_finished`，完全复用 `NapCatPage` 同名槽。

**验证命令**：

```pwsh
.venv\Scripts\python.exe -c "from src.core.network.urls import Urls; print(Urls.SNOWLUMA_DOWNLOAD_TEMPLATE.format(tag='v1.7.5'))"
.venv\Scripts\python.exe -c "from src.core.installation.installers import SnowLumaInstall; print(SnowLumaInstall.__name__)"
.venv\Scripts\python.exe -c "from src.core.versioning.service import LocalVersionTask; print(hasattr(LocalVersionTask, 'get_snowluma_version'))"
```

**期望输出**：

- `https://github.com/SnowLuma/SnowLuma/releases/download/v1.7.5/SnowLuma-v1.7.5-win-x64.zip`
- `SnowLumaInstall`
- `True`

---

## W6 — UI 适配

**Owner boundary**：

- 修改 `src/ui/page/add_bot_page/...`（具体文件名以仓库实际为准；本 wave 第一步先 `grep` 出 `QQID` 与 `BotConfig` 表单字段所在文件）
- 修改 `src/ui/page/bot_page/widget/card.py`
- **新建** `src/ui/page/component_page/sub_page/snowluma_page.py`、**修改** `src/ui/page/component_page/sub_page/__init__.py`、**修改** `src/ui/page/component_page/__init__.py`
- **不动** `src/ui/page/setup_page/...`（该页是 Desktop 自身设置，与组件安装无关）

**6.1 `add_bot_page`**：

新增 backend_type 单选（`SegmentedWidget` 或 RadioButton），选项 NapCat / SnowLuma；默认 NAPCAT。提交时写入 `BotConfig.backend_type`。

SnowLuma 选项在以下情况禁用 + tooltip 提示：

- `it(PathFunc).get_snowluma_node_executable() is None`（未安装 SnowLuma）
- 当前平台非 win-x64（`platform.system() != 'Windows' or platform.machine() != 'AMD64'`）

**6.2 `card.py`**：

```python
def slot_web_ui_button(self) -> None:
    if self._config.bot.backend_type == BackendType.SNOWLUMA:
        # SnowLuma：读 runtime.json 拿 webuiPort，外开浏览器（无 token，让用户在 WebUI 输密码）
        runtime_json = it(PathFunc).get_snowluma_config_dir() / "runtime.json"
        webui_port = 5099
        if runtime_json.exists():
            try:
                webui_port = int(json.loads(runtime_json.read_text(encoding="utf-8")).get("webuiPort", 5099))
            except (OSError, json.JSONDecodeError, ValueError, TypeError):
                pass
        web_ui_url = f"http://127.0.0.1:{webui_port}/"
        QDesktopServices.openUrl(QUrl(web_ui_url))
        logger.info(f"已打开 SnowLuma WebUI(QQID: {mask_qqid(str(self._config.bot.QQID))}, url={web_ui_url})", log_source=LogSource.UI)
        return

    # NapCat 走原逻辑
    qq_id = str(self._config.bot.QQID)
    login_state = it(ManagerNapCatQQLoginState).get_login_state(qq_id)
    ...  # 原 354-372 行不动
```

`slot_qr_code_button`：在 `backend_type == SNOWLUMA` 时调 `info_bar` 提示「请在 SnowLuma WebUI 内完成扫码登录」并 `return`，不弹 NapCat 的 QRCodeDialog。

BotCard 顶部加 backend_type 徽标（`PixmapLabel` 或 `BodyLabel`，文本 "NapCat" / "SnowLuma"，色彩走 ThemeColor 区分）。

**6.3 `component_page` 增 SnowLuma tab**（**范本是 `napcat_page.py`**，1:1 复刻）：

**6.3.1 新建 `src/ui/page/component_page/sub_page/snowluma_page.py`**：

```python
class SnowLumaPage(PageBase):
    """SnowLuma 核心库的安装, 更新和管理页面. """

    def __init__(self, parent) -> None:
        super().__init__(parent=parent)
        self.setObjectName("UnitSnowLumaPage")
        self.downloader = None
        self.installer = None
        self.app_card.set_name("SnowLuma")
        self.app_card.set_hyper_label_name(self.tr("仓库地址"))
        self.app_card.set_hyper_label_url(Urls.SNOWLUMA_REPO.value)
        self.log_card.set_loading(True)
        self.log_card.set_url(Urls.SNOWLUMA_REPO.value.url())

        # 连接信号槽 (与 NapCatPage 同名同语义)
        self.app_card.install_button.clicked.connect(self.handle_download_requested)
        self.app_card.update_button.clicked.connect(self.handle_download_requested)
        self.app_card.pause_button.clicked.connect(self.handle_pause_requested)
        self.app_card.cancel_button.clicked.connect(self.handle_cancel_requested)
        self.app_card.open_folder_button.clicked.connect(
            lambda: QDesktopServices.openUrl(QUrl.fromLocalFile(it(PathFunc).snowluma_path))
        )

    @Slot()
    def apply_remote_version_data(self, version_data: VersionSnapshot) -> None:
        if version_data.snowluma_version is None or version_data.snowluma_update_log is None:
            self.remote_version = None
            self.remote_log = self.tr("获取 SnowLuma 更新日志失败")
        else:
            self.remote_version = version_data.snowluma_version
            self.remote_log = version_data.snowluma_update_log
        self.mark_remote_version_loaded()
        self.refresh_page_if_ready()

    @Slot()
    def apply_local_version_data(self, version_data: VersionSnapshot) -> None:
        self.local_version = version_data.snowluma_version
        self.mark_local_version_loaded()
        self.refresh_page_if_ready()

    @Slot()
    def handle_download_requested(self) -> None:
        # 与 NapCatPage.handle_download_requested 同样的骨架: has_running_bot 检查 + AskBox + _start_download
        ...

    def _start_download(self) -> None:
        from src.core.network.downloader import GithubDownloader
        url = Urls.SNOWLUMA_DOWNLOAD_TEMPLATE.format(tag=self.remote_version)
        downloader = GithubDownloader(QUrl(url))
        self.downloader = downloader
        # 信号连接与 NapCatPage._start_download 一致
        ...

    @Slot()
    def handle_install_requested(self) -> None:
        from src.core.installation.installers import SnowLumaInstall
        installer = SnowLumaInstall(tag=self.remote_version)
        self.installer = installer
        installer.status_label_signal.connect(self.update_operation_status_text)
        installer.error_finish_signal.connect(self.handle_operation_failed)
        installer.progress_ring_toggle_signal.connect(self.update_operation_progress_ring)
        installer.install_finish_signal.connect(self.handle_install_finished)
        QThreadPool.globalInstance().start(installer)

    @Slot()
    def handle_install_finished(self) -> None:
        self.end_operation()
        self.downloader = None
        self.installer = None
        self.local_version = LocalVersionTask().get_snowluma_version()
        self.refresh_page_view()
        QTimer.singleShot(300, self._refresh_version_state_after_install)
```

**6.3.2 `src/ui/page/component_page/sub_page/__init__.py` 追加导出**：

```python
from .snowluma_page import SnowLumaPage  # ← 新增
__all__ = [..., "SnowLumaPage"]
```

**6.3.3 `src/ui/page/component_page/__init__.py` 注册**：

```python
from .sub_page import DesktopPage, NapCatPage, QQPage, SnowLumaPage   # ← 补 SnowLumaPage

class ComponentPage(QWidget):
    def __init__(self) -> None:
        super().__init__()
        ...
        self.napcat_page = NapCatPage(self)
        self.qq_page = QQPage(self)
        self.desktop_page = DesktopPage(self)
        self.snowluma_page = SnowLumaPage(self)              # ← 新增

    def _create_view(self) -> None:
        self.view.setObjectName("UpdateView")
        self.view.addWidget(self.napcat_page)
        self.view.addWidget(self.qq_page)
        self.view.addWidget(self.desktop_page)
        self.view.addWidget(self.snowluma_page)              # ← 新增

        self.top_card.pivot.addItem(
            routeKey=self.napcat_page.objectName(),
            text=self.tr("NapCat"),
            onClick=lambda: self.view.setCurrentWidget(self.napcat_page),
        )
        self.top_card.pivot.addItem(
            routeKey=self.qq_page.objectName(),
            text=self.tr("QQ"),
            onClick=lambda: self.view.setCurrentWidget(self.qq_page),
        )
        self.top_card.pivot.addItem(
            routeKey=self.desktop_page.objectName(),
            text=self.tr("Desktop"),
            onClick=lambda: self.view.setCurrentWidget(self.desktop_page),
        )
        self.top_card.pivot.addItem(                          # ← 新增 SnowLuma tab
            routeKey=self.snowluma_page.objectName(),
            text=self.tr("SnowLuma"),
            onClick=lambda: self.view.setCurrentWidget(self.snowluma_page),
        )
        ...

    def _connect_signals(self) -> None:
        ...
        self.version_service.remote_versions_loaded.connect(self.snowluma_page.apply_remote_version_data)  # ← 新增
        self.version_service.local_versions_loaded.connect(self.snowluma_page.apply_local_version_data)    # ← 新增

    def refresh_versions(self) -> None:
        ...
        self.snowluma_page.begin_version_refresh()           # ← 新增
        self.snowluma_page.log_card.set_loading(True)        # ← 新增
```

**平台限制**（与需求第 10 节推断对齐）：非 win-x64 时 `SnowLumaPage.app_card.install_button` / `update_button` 需 disable + tooltip；可在 `SnowLumaPage._setup_widget_properties` 中实现，与 `NapCatPage` 达成一致的平台限制处理风格。

**验证命令**：

```pwsh
.venv\Scripts\python.exe -c "from src.ui.page.bot_page.widget.card import BotCard; import inspect; src=inspect.getsource(BotCard.slot_web_ui_button); print('SNOWLUMA' in src and 'webuiPort' in src)"
.venv\Scripts\python.exe -c "from src.ui.page.component_page.sub_page import SnowLumaPage; from src.ui.page.component_page import ComponentPage; import inspect; print('SnowLumaPage' in inspect.getsource(ComponentPage))"
```

**期望输出**：`True` + `True`

---

## W7 — 单元测试

**测试 1**：`script/test/test_backend_type_model.py`

- `BackendType.from_str(None) is BackendType.NAPCAT`
- `BackendType.from_str("snowluma") is BackendType.SNOWLUMA`
- `BackendType.from_str("garbage") is BackendType.NAPCAT`（降级）
- `BotConfig.model_validate({"QQID": 10000})` 的 `backend_type == BackendType.NAPCAT`（旧配置兼容）
- `BotConfig.model_validate({"QQID": 10001, "backend_type": "snowluma"}).backend_type == BackendType.SNOWLUMA`

**测试 2**：`script/test/test_snowluma_config_renderer.py`

- `render_runtime_json(tmp)` 后 `runtime.json == {"webuiPort": 5099}`
- `render_onebot_json(tmp, 10000, access_token="abc")` 输出与 `SnowLuma-v1.7.5-win-x64/config/onebot_2550419068.json` 同结构（断言 `networks.wsServers[0].port == 3001`、`networks.httpServers[0].accessToken == "abc"`、`musicSignUrl == ""`）
- `render_webui_json(tmp)` 在文件不存在时**不**创建（让 SnowLuma 自治）
- `render_webui_json(tmp, password="x")` 写入含 salt（hex 32 字符）+ hash（hex 128 字符）的 json
- 用上游真实样本 `C:\Users\QIAO\Desktop\SnowLuma-v1.7.5-win-x64\config\onebot_2550419068.json` 作 golden（仅在该文件存在时跑该断言，CI 上跳过）

**测试 3**：`script/test/test_snowluma_process_construction.py`

- mock `PathFunc.get_snowluma_node_executable` 返回临时 `node.exe` 桩
- 调 `mgr._create_snowluma_process(config)` 后断言：
  - `process.program() == str(<tmp>/node.exe)`
  - `process.arguments() == [str(<snowluma_path>/index.mjs)]`
  - `process.workingDirectory() == str(<snowluma_path>)`
  - 不实际 `process.start()`
- `get_snowluma_node_executable() is None` 时 `_create_snowluma_process` 抛 `FileNotFoundError`

**测试 4**：`script/test/test_snowluma_installer.py`

- mock `httpx.Client.stream` 返回内存 zip 字节流
- 该 zip 顶层目录为 `SnowLuma-v1.7.5-win-x64/`，含 `node.exe`、`index.mjs`、`package.json`、`config/runtime.json`
- 在 target 已有 `config/runtime.json` 的情况下：`SnowLumaInstall` 运行后该文件**未**被覆盖
- `node.exe` / `index.mjs` 缺失时 `_verify` 抛 `RuntimeError`
- 临时 zip 在异常路径上被清理
- 验证 `SnowLumaInstall.install_finish_signal` / `error_finish_signal` 信号在成功与失败路径上各 emit 一次

**测试 5**：`script/test/test_versioning_snowluma.py`

- mock `httpx.Client.get` 返回 `{"tag_name": "v1.7.5", "body": "release notes"}`
- `RemoteVersionTask.execute()` 的 `VersionSnapshot.snowluma_version == "v1.7.5"`
- mock 镜像 5xx + fallback 200 时仍能拿到 SnowLuma 版本（验证 fallback 链路）
- `LocalVersionTask().get_snowluma_version()`：mock `<snowluma_path>/package.json` 存在 + 含 `"version": "0.1.0"` 时返回 `"0.1.0"`；文件不存在时返回 `None`；JSON 损坏时返回 `None`（不报错）

**验证命令**：

```pwsh
.venv\Scripts\python.exe -m pytest script\test\test_backend_type_model.py script\test\test_snowluma_config_renderer.py script\test\test_snowluma_process_construction.py script\test\test_snowluma_installer.py script\test\test_versioning_snowluma.py -q
```

**期望**：退出码 0；5 个测试文件全绿。

---

## W8 — 验证 + phase_cleanup

**8.1 全量回归基线**：

```pwsh
.venv\Scripts\python.exe -m pytest script\test -q -k "backend or napcat or config or version or remote or installer or path"
```

要求：现有 `test_*napcat*` / `test_*backend*` / `test_*remote*` / `test_*config*` 套件**不**回归。

**8.2 启动 smoke**：

```pwsh
.venv\Scripts\python.exe main.py
```

人工核对：

- 主窗口正常打开
- BotPage 现有 NapCat Bot 仍能 Start/Stop（零回归）
- 「新建 Bot」表单可见 backend_type 单选；当前未装 SnowLuma 时 SnowLuma 选项 disabled + tooltip
- 组件页 (`ComponentPage`) 顶部 pivot 多出一个 `SnowLuma` tab，进入后与 NapCat tab 同款 (相同的 install/update/pause/cancel/openFolder 按钮与 左边 app_card / 右边 update log card 布局)

**8.3 grep 兜底**：

```pwsh
findstr /s /n /i "NAPCAT_PATCH_PACKAGE\|NAPCAT_LOAD_PATH\|NAPCAT_INJECT_PATH" src\
```

要求结果**仅**在 `src/core/runtime/napcat.py` 内的 `_get_env_variable` 与文档/测试中命中（即 SnowLuma 启动路径绝不能误触这些 env 变量）。

**8.4 阶段清理产物**（与 ssh-distro-expansion 对齐）：

- `outputs/runtime/vibe-sessions/<run-id>/skeleton-receipt.json`
- `outputs/runtime/vibe-sessions/<run-id>/intent-contract.json`
- `outputs/runtime/vibe-sessions/<run-id>/phase-W1-W2.json`
- `outputs/runtime/vibe-sessions/<run-id>/phase-W3-W4.json`
- `outputs/runtime/vibe-sessions/<run-id>/phase-W5.json`
- `outputs/runtime/vibe-sessions/<run-id>/phase-W6.json`
- `outputs/runtime/vibe-sessions/<run-id>/phase-W7-W8.json`
- `outputs/runtime/vibe-sessions/<run-id>/cleanup-receipt.json`

---

## 完成语言规则

- 所有 wave **未通过**对应验证命令前，对外不能用「全部完成」「交付完成」
- W7 任一新增测试退出码非 0 → 必须报告失败 stage 与 stderr，不得使用完成性措辞
- W8 grep 命中 SnowLuma 启动路径误注入 NAPCAT_* env → 不能声明 W3/W6 已完成
- 人工 8.2 smoke 未跑过 → 仅可声明「代码层完成，待真机验收」

## 回滚规则

- W1 失败：删除 `backend_type.py`；回滚 `paths.py` 与 `config_model.py`（git checkout）
- W2 失败：删除 `snowluma_config_renderer.py`
- W3 失败：仅 `napcat.py` 的 backend 分流块与 `_create_snowluma_process` 方法 git checkout 还原；NapCat 路径不动
- W4 失败：删除 `snowluma_status_poller.py` + 还原 login_state 改动；W3 仍可独立成立（SnowLuma 启动后只是不显示登录态，BotCard 退化为「Running 但状态未知」）
- W5 失败：还原 `urls.py` / `versioning/service.py` 改动；从 `installation/installers.py` 中删除 `SnowLumaInstall` 类；同时 W6 的 `component_page/__init__.py` 与 `sub_page/__init__.py` 中的 SnowLumaPage 注册需同步还原 (否则会 import 报错)
- W6 失败：UI 文件 git checkout；现有 NapCat UI 不受影响
- W7 失败：保留代码，标记测试不通过，不写 `cleanup-receipt.json` 的 success 字段

## 阶段清理预期

- W1-W8 完成后产出 8.4 节列出的全部受信件 + W7 五份 pytest 报告（保存至 `outputs/runtime/vibe-sessions/<run-id>/test-reports/`）
- 临时脚本 / 调试 print **不**应残留在 `src/`、`script/test/`
- `runtime/_smoke_snowluma/`（W2 验证产物）需在 W8 清理
- 任何在 `runtime/SnowLuma/` 下因 W8 smoke 引入的临时文件需保留（属于真实安装产物，归用户运行时数据）
