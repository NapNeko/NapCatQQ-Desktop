# -*- coding: utf-8 -*-
"""SnowLuma 远端状态服务 (W5).

读取远端 ``status_daemon.json`` / ``status_bot_<qq_id>.json`` 与日志, 给 Desktop UI
提供与本地 :class:`src.core.runtime.snowluma_daemon.SnowLumaDaemon` 同步的状态视图.

数据流:

  远端                                                     Desktop
  ----                                                     -------
  daemon launcher start
    └→ 写 $RUNTIME_DIR/status_daemon.json (W2 契约)        ▲
  bot launcher start <qq_id>                              │
    └→ 写 $RUNTIME_DIR/status_bot_<qq_id>.json            │
                                                          │
                                                       SnowLumaRemoteRuntimeService.
                                                          get_daemon_status() / get_bot_status()
                                                          (轮询 cat 远端 json)

状态枚举与本地对齐:

- daemon: ``STOPPED`` / ``STARTING`` (running 但未 ready) / ``READY`` / ``CRASHED`` (pid 不存活)
- bot: ``STOPPED`` / ``RUNNING`` / ``CRASHED`` (pid 不存活但 status 标 running)
"""

from __future__ import annotations

import enum
import json
import re
from dataclasses import dataclass, field
from typing import Any

from ..execution_backend import ExecutionBackend
from .paths import SnowLumaRemotePaths


# ==================== 状态枚举 ====================
class SnowLumaRemoteDaemonState(enum.Enum):
    """SL 远端 daemon 状态. 与本地 :class:`DaemonState` 子集对齐.

    远端没有 ``STARTING`` 与本地的"客户端发起 spawn 等待"语义; 远端简化为:

    - ``STOPPED``: status JSON 缺失 / running=false
    - ``STARTING``: running=true 但 ready=false (Xvfb/node 启动中, webui 未通)
    - ``READY``: running=true 且 ready=true (webui /api/status 探测通过)
    - ``CRASHED``: status 标 running=true 但远端实际 pid 不存活 (定期 probe 触发)
    - ``UNKNOWN``: 远端命令失败 / SSH 中断
    """

    STOPPED = "stopped"
    STARTING = "starting"
    READY = "ready"
    CRASHED = "crashed"
    UNKNOWN = "unknown"


class SnowLumaRemoteBotState(enum.Enum):
    """SL 远端 Bot (qq.exe) 状态."""

    STOPPED = "stopped"
    RUNNING = "running"
    CRASHED = "crashed"
    UNKNOWN = "unknown"


# ==================== status payload ====================
@dataclass(slots=True)
class SnowLumaRemoteDaemonStatus:
    """远端 daemon 状态 (从 ``status_daemon.json`` 解析).

    Attributes:
        running: 整体是否在跑
        ready: WebUI ``/api/status`` 是否通
        started_at: ISO 8601 启动时间 (UTC); ``None`` 表示未启动
        pids: ``{"xvfb": 1234, "fluxbox": 1235, ...}`` 5 个辅助进程 pid; 值可为 ``None``
        ports: ``{"vnc": 5900, "novnc": 6081, "webui": 5099}``
        display: ``":0"`` 之类
        raw: 解析前的原始 dict (调试用 / UI 展开)
    """

    running: bool = False
    ready: bool = False
    started_at: str | None = None
    pids: dict[str, int | None] = field(default_factory=dict)
    ports: dict[str, int] = field(default_factory=dict)
    display: str | None = None
    raw: dict[str, Any] = field(default_factory=dict)

    @property
    def state(self) -> SnowLumaRemoteDaemonState:
        """计算派生状态枚举."""
        if not self.running:
            return SnowLumaRemoteDaemonState.STOPPED
        if self.ready:
            return SnowLumaRemoteDaemonState.READY
        return SnowLumaRemoteDaemonState.STARTING

    @classmethod
    def from_json(cls, text: str) -> "SnowLumaRemoteDaemonStatus":
        """解析远端 ``status_daemon.json`` 文本.

        Raises:
            ValueError: JSON 不合法或缺关键字段.
        """
        try:
            data = json.loads(text)
        except json.JSONDecodeError as exc:
            raise ValueError(f"status_daemon.json 不是合法 JSON: {exc}") from exc
        if not isinstance(data, dict):
            raise ValueError(f"status_daemon.json 顶层不是 object: {type(data).__name__}")
        return cls(
            running=bool(data.get("running", False)),
            ready=bool(data.get("ready", False)),
            started_at=data.get("started_at") or None,
            pids=dict(data.get("pids") or {}),
            ports=dict(data.get("ports") or {}),
            display=data.get("display") or None,
            raw=data,
        )

    @classmethod
    def stopped(cls) -> "SnowLumaRemoteDaemonStatus":
        """构造 STOPPED 状态 (远端 status 缺失时使用)."""
        return cls(running=False, ready=False)


@dataclass(slots=True)
class SnowLumaRemoteBotStatus:
    """远端单 Bot 状态 (从 ``status_bot_<qq_id>.json`` 解析).

    Attributes:
        qq_id: SnowLuma 会话 id (常等于 uin); 与 status 文件名后缀一致
        uin: 用户 QQ 号; ``None`` 表示未登录或未设置
        pid: qq.exe 进程 pid; ``None`` = stopped
        running: status JSON 中的 running 标志
        started_at: ISO 8601
        raw: 原始 dict
    """

    qq_id: str
    uin: str | None = None
    pid: int | None = None
    running: bool = False
    started_at: str | None = None
    raw: dict[str, Any] = field(default_factory=dict)

    @property
    def state(self) -> SnowLumaRemoteBotState:
        if not self.running:
            return SnowLumaRemoteBotState.STOPPED
        if self.pid is None:
            # running 标 true 但没 pid: status 文件被错乱写入, 视为 CRASHED
            return SnowLumaRemoteBotState.CRASHED
        return SnowLumaRemoteBotState.RUNNING

    @classmethod
    def from_json(cls, qq_id: str, text: str) -> "SnowLumaRemoteBotStatus":
        try:
            data = json.loads(text)
        except json.JSONDecodeError as exc:
            raise ValueError(f"status_bot_{qq_id}.json 不是合法 JSON: {exc}") from exc
        if not isinstance(data, dict):
            raise ValueError(
                f"status_bot_{qq_id}.json 顶层不是 object: {type(data).__name__}"
            )
        # qq_id 一致性校验 (防 status 文件被改错文件名)
        json_qq = str(data.get("qq_id") or "").strip()
        if json_qq and json_qq != qq_id:
            raise ValueError(
                f"status_bot_{qq_id}.json 中 qq_id 字段不一致: file={qq_id} json={json_qq}"
            )
        raw_pid = data.get("pid")
        pid = int(raw_pid) if isinstance(raw_pid, int) else None
        return cls(
            qq_id=qq_id,
            uin=str(data["uin"]) if data.get("uin") else None,
            pid=pid,
            running=bool(data.get("running", False)),
            started_at=data.get("started_at") or None,
            raw=data,
        )

    @classmethod
    def stopped(cls, qq_id: str) -> "SnowLumaRemoteBotStatus":
        return cls(qq_id=qq_id, running=False)


# ==================== runtime service ====================
# status 文件名解析: status_bot_<qq_id>.json
_STATUS_BOT_FILE_PATTERN = re.compile(r"status_bot_(\d+)\.json$")


class SnowLumaRemoteRuntimeService:
    """远端状态查询服务 (Desktop ↔ remote 单向只读).

    通过 :class:`ExecutionBackend` 调远端 ``cat`` / ``ls`` / ``tail`` 等命令实现;
    任何 SSH 异常都被静默捕获为 ``UNKNOWN`` 状态, 不阻塞 UI.

    Args:
        backend: SSH 后端
        paths: SL 远端路径; 决定从哪里读 status 文件与日志

    Examples:
        >>> service = SnowLumaRemoteRuntimeService(backend, paths)
        >>> status = service.get_daemon_status()
        >>> if status.state == SnowLumaRemoteDaemonState.READY:
        ...     bots = service.list_bots()
    """

    def __init__(
        self,
        backend: ExecutionBackend,
        paths: SnowLumaRemotePaths,
    ) -> None:
        self.backend = backend
        self.paths = paths

    # ==================== daemon ====================
    def get_daemon_status(self) -> SnowLumaRemoteDaemonStatus:
        """读 ``status_daemon.json``; 文件缺失或解析失败返 ``STOPPED``."""
        result = self.backend.run(
            f'cat "{self.paths.status_daemon}" 2>/dev/null || true',
            check=False,
        )
        if not result.ok or not result.stdout.strip():
            return SnowLumaRemoteDaemonStatus.stopped()
        try:
            return SnowLumaRemoteDaemonStatus.from_json(result.stdout)
        except ValueError:
            # 文件存在但损坏 (e.g. 写一半被中断); 视为 STOPPED, 让上层重启 daemon
            return SnowLumaRemoteDaemonStatus.stopped()

    def tail_daemon_log(self, lines: int = 200) -> str:
        """读远端 daemon 日志最后 N 行; 文件不存在返空串."""
        if lines <= 0:
            raise ValueError(f"lines 必须 > 0: {lines}")
        result = self.backend.run(
            f'tail -n {lines} "{self.paths.log_daemon}" 2>/dev/null || true',
            check=False,
        )
        return result.stdout if result.ok else ""

    # ==================== bot ====================
    def get_bot_status(self, qq_id: str) -> SnowLumaRemoteBotStatus:
        """读单个 Bot status; 文件缺失返 ``STOPPED``."""
        if not qq_id or not qq_id.isdigit():
            raise ValueError(f"qq_id 必须是非空数字字符串: {qq_id!r}")
        result = self.backend.run(
            f'cat "{self.paths.status_bot(qq_id)}" 2>/dev/null || true',
            check=False,
        )
        if not result.ok or not result.stdout.strip():
            return SnowLumaRemoteBotStatus.stopped(qq_id)
        try:
            return SnowLumaRemoteBotStatus.from_json(qq_id, result.stdout)
        except ValueError:
            return SnowLumaRemoteBotStatus.stopped(qq_id)

    def list_bots(self) -> list[SnowLumaRemoteBotStatus]:
        """枚举远端所有 ``status_bot_*.json`` 并解析.

        Returns:
            按 qq_id 升序排列的状态列表; 空目录返空列表.
        """
        result = self.backend.run(
            f'ls -1 "{self.paths.runtime_dir}"/status_bot_*.json 2>/dev/null || true',
            check=False,
        )
        if not result.ok or not result.stdout.strip():
            return []

        statuses: list[SnowLumaRemoteBotStatus] = []
        for raw_line in result.stdout.splitlines():
            line = raw_line.strip()
            match = _STATUS_BOT_FILE_PATTERN.search(line)
            if match is None:
                continue
            qq_id = match.group(1)
            try:
                statuses.append(self.get_bot_status(qq_id))
            except ValueError:
                # qq_id 非数字 (理论上 regex 已挡掉, 双保险)
                continue

        statuses.sort(key=lambda s: int(s.qq_id))
        return statuses

    def tail_bot_log(self, qq_id: str, lines: int = 200) -> str:
        """读单个 Bot 日志最后 N 行."""
        if not qq_id or not qq_id.isdigit():
            raise ValueError(f"qq_id 必须是非空数字字符串: {qq_id!r}")
        if lines <= 0:
            raise ValueError(f"lines 必须 > 0: {lines}")
        result = self.backend.run(
            f'tail -n {lines} "{self.paths.log_bot(qq_id)}" 2>/dev/null || true',
            check=False,
        )
        return result.stdout if result.ok else ""


__all__ = [
    "SnowLumaRemoteDaemonState",
    "SnowLumaRemoteDaemonStatus",
    "SnowLumaRemoteBotState",
    "SnowLumaRemoteBotStatus",
    "SnowLumaRemoteRuntimeService",
]
