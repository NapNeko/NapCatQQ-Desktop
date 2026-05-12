# -*- coding: utf-8 -*-
"""SnowLuma 远端 daemon 控制器 (W9).

与本地 :class:`src.core.runtime.snowluma_daemon.SnowLumaDaemon` 对偶: 本地用
``QProcess`` 直接 spawn node.exe; 远端通过 :class:`SnowLumaLauncherCommands` 通过
SSH 间接控制远端 daemon 进程组 + 通过 :class:`SnowLumaTunnelManager` 暴露 WebUI /
noVNC HTTP 给 Desktop.

聚合关系::

  RemoteSnowLumaDaemon
    ├─ SSHClient                                (已建立的 SSH 会话)
    ├─ SnowLumaLauncherCommands                 (生成 start/stop/status 命令)
    ├─ SnowLumaRemoteRuntimeService             (cat status_daemon.json 解析)
    ├─ SnowLumaTunnelManager                    (双隧道 WebUI + noVNC)
    └─ Qt 信号 (state_changed / crashed / ready)

状态机 (与本地 :class:`DaemonState` 子集对齐):

- ``STOPPED``: 远端 status JSON 缺失 / running=false
- ``STARTING``: ``ensure_running`` 期间, 远端 daemon 已起但 webui 未通
- ``READY``: 隧道与 webui 双就绪 (ensure_running 返回的状态)
- ``CRASHED``: 隧道 watchdog 检测到任一隧道挂; 或远端 status running=true 但
  pid 不存活. 触发 :attr:`crashed` 信号供 :class:`BotProcessManager` 弹错并
  把所有挂在本 daemon 上的 Bot 状态置为 stopped.
- ``STOPPING``: ``release()`` 触发回收期间, 仅供调试观察.

线程安全: ``ensure_running`` / ``release`` / ``is_alive`` 用内部 lock 序列化;
Qt 信号通过 ``Qt.QueuedConnection`` 跨线程 (调用方需用 ``moveToThread`` 安置在主线程).
"""

from __future__ import annotations

import enum
import threading
from collections.abc import Callable
from dataclasses import dataclass
from typing import TYPE_CHECKING

from PySide6.QtCore import QObject, Signal

from src.core.logging import LogSource, LogType, logger

from ..execution_backend import RemoteExecutionBackend
from .launcher import SnowLumaLauncherCommands
from .paths import SnowLumaRemotePaths
from .status import (
    SnowLumaRemoteDaemonState,
    SnowLumaRemoteDaemonStatus,
    SnowLumaRemoteRuntimeService,
)
from .tunnels import (
    SnowLumaTunnelBundle,
    SnowLumaTunnelManager,
    TunnelLabel,
)

if TYPE_CHECKING:
    from ..ssh_client import SSHClient


# ==================== 默认参数 ====================
_ENSURE_RUNNING_TIMEOUT_SEC: float = 55.0
"""``ensure_running`` 总超时 (秒); 含 daemon launcher start (~30s wait-ready 内置)
+ 隧道建立 (~5s) + 缓冲."""

_STATUS_POLL_INTERVAL_SEC: float = 1.0
"""``ensure_running`` 内部探测远端 status JSON 的间隔."""


class RemoteDaemonState(enum.Enum):
    """远端 daemon 状态机 (与本地 :class:`DaemonState` 对齐).

    比 :class:`SnowLumaRemoteDaemonState` 多 ``STOPPING`` 状态 (本地 daemon 也有);
    专用于 :class:`RemoteSnowLumaDaemon` 与 :class:`BotProcessManager` 协议.
    """

    STOPPED = "stopped"
    STARTING = "starting"
    READY = "ready"
    STOPPING = "stopping"
    CRASHED = "crashed"


# ==================== ready payload ====================
@dataclass(slots=True, frozen=True)
class RemoteDaemonReadyInfo:
    """``ensure_running`` 成功后返回的就绪信息.

    Attributes:
        tunnels: 两条隧道端点 (供 Desktop UI 构造 noVNC URL / WebUI 跳转)
        status: 最新一次 ``status_daemon.json`` 解析结果
    """

    tunnels: SnowLumaTunnelBundle
    status: SnowLumaRemoteDaemonStatus


# ==================== 异常 ====================
class RemoteDaemonStartTimeout(RuntimeError):
    """``ensure_running`` 超时."""


class RemoteDaemonStartFailed(RuntimeError):
    """``ensure_running`` 启动失败 (launcher start 返非 0 退出码或 status 异常)."""


# ==================== 主类 ====================
class RemoteSnowLumaDaemon(QObject):
    """SnowLuma 远端 daemon 控制器 (W9).

    Args:
        ssh_client: 已建立 SSH 会话的 :class:`SSHClient` (调用方拥有连接生命周期)
        paths: SL 远端目录布局
        parent: Qt parent

    Signals:
        state_changed: ``(RemoteDaemonState,)`` — 状态机转移; UI 用 queued 连接
        ready: ``()`` — 首次进入 READY 时 emit (供 BotCard 自动 unlock 启动按钮)
        crashed: ``(str,)`` — 隧道 / daemon 进程挂掉; payload 是人类可读 error message

    Examples:
        >>> daemon = RemoteSnowLumaDaemon(ssh_client, paths)
        >>> info = daemon.ensure_running(timeout=55.0)
        >>> webbrowser.open(info.tunnels.novnc.local_url + "/vnc.html?...")
        >>> # ... Bot 任务结束
        >>> daemon.release()
    """

    state_changed = Signal(object)
    ready = Signal()
    crashed = Signal(str)

    def __init__(
        self,
        ssh_client: "SSHClient",
        paths: SnowLumaRemotePaths,
        *,
        parent: QObject | None = None,
    ) -> None:
        super().__init__(parent)
        self._ssh_client = ssh_client
        self._paths = paths

        # P9 (review): __init__ 不做 IO. ``ssh_client.transport`` 是 property,
        # 未连接会抛 ``SSHConnectionError``; 我们让构造期总是成功 (即使 SSH 还没起),
        # 把 ``RemoteExecutionBackend`` / ``SnowLumaTunnelManager`` 的实际依赖
        # transport 延迟到 :meth:`ensure_running` 第一次调用时再绑.
        self._exec_backend = RemoteExecutionBackend(ssh_client)
        self._launcher_cmds = SnowLumaLauncherCommands(paths)
        self._runtime_service = SnowLumaRemoteRuntimeService(self._exec_backend, paths)
        self._tunnel_manager: SnowLumaTunnelManager | None = None

        self._state: RemoteDaemonState = RemoteDaemonState.STOPPED
        self._ref_count: int = 0
        self._lock = threading.RLock()

    def _ensure_tunnel_manager_locked(self) -> SnowLumaTunnelManager:
        """惰性构造 :class:`SnowLumaTunnelManager`; 调用方持有 ``self._lock``.

        SSH 必须已 connect (否则 ``ssh_client.transport`` raise). 把构造时机
        从 ``__init__`` 推迟到 ``ensure_running``, 解决 P9 review: ``__init__``
        不应做 IO, 也不应在 SSH 还没连接时抛错.
        """
        if self._tunnel_manager is None:
            self._tunnel_manager = SnowLumaTunnelManager(
                self._ssh_client.transport,
                on_crash=self._on_tunnel_crash,
            )
        return self._tunnel_manager

    # ==================== 公开属性 ====================
    @property
    def state(self) -> RemoteDaemonState:
        with self._lock:
            return self._state

    @property
    def ref_count(self) -> int:
        with self._lock:
            return self._ref_count

    @property
    def paths(self) -> SnowLumaRemotePaths:
        return self._paths

    @property
    def tunnel_manager(self) -> SnowLumaTunnelManager:
        """暴露给 W10 ``open_snowluma_vnc`` 直接拿端点构造 URL.

        P9 (review): 惰性构造 — 首次访问时 SSH 必须已 connect, 否则
        ``ssh_client.transport`` raise. UI 调用者应该在 ``ensure_running()``
        返回后才访问本 property.
        """
        with self._lock:
            return self._ensure_tunnel_manager_locked()

    @property
    def runtime_service(self) -> SnowLumaRemoteRuntimeService:
        """暴露给 BotCard 读 bot 状态 / tail 日志."""
        return self._runtime_service

    @property
    def launcher_commands(self) -> SnowLumaLauncherCommands:
        """暴露给 driver 调 bot start/stop 等子命令."""
        return self._launcher_cmds

    # ==================== ensure_running / release ====================
    def ensure_running(
        self,
        *,
        timeout: float = _ENSURE_RUNNING_TIMEOUT_SEC,
    ) -> RemoteDaemonReadyInfo:
        """启动远端 daemon (若未起) + 引用计数 +1, 返回隧道端点与状态.

        Args:
            timeout: 总超时秒数; 含远端 launcher start (内置 30s wait-ready) +
                隧道建立 + 状态轮询缓冲.

        Returns:
            :class:`RemoteDaemonReadyInfo` (含两条隧道端点 + 状态 JSON).

        Raises:
            RemoteDaemonStartTimeout: 超时仍未 ready
            RemoteDaemonStartFailed: launcher start 退出码非 0 或状态文件解析失败
        """
        with self._lock:
            # P9 (review): 首次调用时惰性构造 tunnel manager (依赖 SSH 已 connect)
            tunnel_mgr = self._ensure_tunnel_manager_locked()
            self._ref_count += 1
            if self._state == RemoteDaemonState.READY:
                bundle = tunnel_mgr.get_endpoints()
                status = self._runtime_service.get_daemon_status()
                if bundle is not None:
                    return RemoteDaemonReadyInfo(tunnels=bundle, status=status)
                # tunnel 端点丢失 (不应该发生; 视为 CRASHED 走重启)
                self._set_state_locked(RemoteDaemonState.CRASHED)

            try:
                self._set_state_locked(RemoteDaemonState.STARTING)
                info = self._do_start_locked(timeout=timeout)
                self._set_state_locked(RemoteDaemonState.READY)
                return info
            except Exception:
                # 启动失败 → 回滚 ref + 状态置 STOPPED, 让下次重试
                self._ref_count -= 1
                self._set_state_locked(RemoteDaemonState.STOPPED)
                tunnel_mgr.stop()
                raise

    def release(self) -> None:
        """引用计数 -1; 归 0 时执行远端 daemon stop + 关闭隧道. 幂等."""
        with self._lock:
            if self._ref_count == 0:
                return
            self._ref_count -= 1
            if self._ref_count == 0:
                self._shutdown_locked()

    def is_alive(self) -> bool:
        """隧道存活 + 远端 status running=true 才算 alive.

        与 :meth:`state == READY` 略有差别: ``state`` 是缓存值,
        ``is_alive`` 主动探测一次 (适合 watchdog / 心跳)
        """
        with self._lock:
            if self._state != RemoteDaemonState.READY:
                return False
            # READY 状态下 tunnel_manager 必已构造 (由 ensure_running 完成)
            if self._tunnel_manager is None or not self._tunnel_manager.is_alive():
                return False
        # 远端 status 探测放在 lock 外 (SSH 调用可能慢)
        try:
            status = self._runtime_service.get_daemon_status()
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"is_alive 探测远端 status 异常: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
            return False
        return status.running

    # ==================== 内部 ====================
    def _do_start_locked(self, *, timeout: float) -> RemoteDaemonReadyInfo:
        """实际执行 daemon launcher start + 隧道建立 + 状态轮询.

        调用方必须持有 ``self._lock``.
        """
        # 1. 调远端 launcher start
        start_cmd = self._launcher_cmds.daemon_start_cmd()
        # W10b-Driver: launcher 内部 wait-ready 阶段最长 30s, 加 dbus/Xvfb/x11vnc/websockify 启动
        # 时间 ~3s, 总计可达 33s; 默认 SSH timeout (20s) 不够, 显式 60s 给余量
        result = self._exec_backend.run(start_cmd, timeout=60.0, check=False)
        if not result.ok:
            # W10b-Driver: launcher 在 return 5 (wait-ready 超时) 时主动 dump 诊断信息到
            # stderr (含 node 进程存活检查 / 端口监听 / daemon.log tail). 不再截断让用户看到完整根因.
            stderr_text = result.stderr.strip()
            # 上限 4000 字符, 防 stderr 失控 (理论上 daemon.log tail 50 行 ~ 几 KB).
            if len(stderr_text) > 4000:
                stderr_text = stderr_text[:4000] + "\n... (stderr 截断, 完整日志请 SSH 远端查看)"
            raise RemoteDaemonStartFailed(
                f"daemon launcher start 返非 0 退出码: exit={result.exit_status}\n"
                f"stderr:\n{stderr_text}"
            )

        # 2. 建隧道 (优先固定端口, 失败回退随机)
        # P9: ensure_running 入口已 _ensure_tunnel_manager_locked, 这里直接 assert
        assert self._tunnel_manager is not None, "_do_start_locked 调用前未构造 tunnel manager"
        try:
            tunnels = self._tunnel_manager.acquire()
        except Exception as exc:
            raise RemoteDaemonStartFailed(f"隧道建立失败: {exc}") from exc

        # 3. 轮询远端 status_daemon.json.ready = true
        deadline = threading.Event()
        elapsed = 0.0
        last_status: SnowLumaRemoteDaemonStatus | None = None
        while elapsed < timeout:
            try:
                status = self._runtime_service.get_daemon_status()
            except Exception as exc:  # noqa: BLE001
                logger.warning(
                    f"ensure_running 探测 status 异常: {exc}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                status = SnowLumaRemoteDaemonStatus.stopped()
            last_status = status
            if status.state == SnowLumaRemoteDaemonState.READY:
                # P10 (2026-05-12 fix): launcher 的 wait-ready 只检测 ``/dev/tcp`` TCP listen,
                # 但 Node.js SL daemon 在 webui middleware 加载完之前 socket 已 listen, 此时
                # 通过 SSH 隧道 connect 5099 会被 daemon 拒 → ``ChannelException(2, Connect failed)``.
                # 用户表现: 首次启 Bot 点 WebUI 按钮浏览器打不开, 停掉重启就 OK (daemon 已完全就绪).
                # 修复: 在 status.ready=true 之后, 额外通过本地隧道做一次真 HTTP 探测, retry 几秒
                # 直到 webui 真能响应, 与 launcher 的 TCP 探测互补.
                self._verify_webui_http_reachable(tunnels.webui.local_port)
                logger.info(
                    f"SnowLuma 远端 daemon ready: tunnels="
                    f"webui:{tunnels.webui.local_port}, novnc:{tunnels.novnc.local_port}",
                    LogType.NETWORK,
                    LogSource.CORE,
                )
                return RemoteDaemonReadyInfo(tunnels=tunnels, status=status)
            deadline.wait(_STATUS_POLL_INTERVAL_SEC)
            elapsed += _STATUS_POLL_INTERVAL_SEC

        raise RemoteDaemonStartTimeout(
            f"daemon 在 {timeout}s 内未 ready; 最后状态={last_status.state.value if last_status else 'unknown'}"
        )

    def _verify_webui_http_reachable(
        self,
        local_port: int,
        *,
        retries: int = 40,
        interval: float = 0.5,
    ) -> None:
        """通过本地隧道做 HTTP HEAD 探测 WebUI 真可响应; retry 直到成功或耗尽次数.

        与 launcher 的 ``bash /dev/tcp`` TCP listen 探测互补: Node.js 进程 ``listen(5099)``
        之后, 直到 webui middleware 初始化完才能真响应 HTTP. 此处通过 SSH 隧道发请求,
        端到端验证 "浏览器能拿到响应".

        任何 HTTP 状态码 (含 401/302/404) 都视为成功 — 只要 server 回了 byte 就证明
        全链路 (浏览器 → 本地 forwarder → SSH channel → daemon HTTP) 贯通.

        P10 (2026-05-12 fix): 首次冷启动时 SnowLuma framework 需要初始化 SQLite 数据库、
        加载插件等, HTTP 响应时间可能超过 10s. 将 retry 从 20 次 (10s) 提升到 40 次 (20s),
        覆盖首次冷启动场景.

        Args:
            local_port: 隧道在 Desktop 本地的 listen 端口 (典型 47099)
            retries: 最大重试次数, 默认 40 (× 0.5s = 20s)
            interval: 每次重试间隔秒数

        Raises:
            RemoteDaemonStartFailed: 全部 retry 用完仍无响应 — daemon listen 了但不响应,
                此时 ensure_running 的 ref_count + tunnel 已由调用方在 except 分支回滚.
        """
        import http.client
        import socket
        import time

        last_exc: BaseException | None = None
        for attempt in range(retries):
            try:
                conn = http.client.HTTPConnection("127.0.0.1", local_port, timeout=2.0)
                try:
                    # HEAD 比 GET 省带宽, /api/status 是 SL 标准端点 (会返 401, 但有响应即 OK)
                    conn.request("HEAD", "/api/status")
                    resp = conn.getresponse()
                    resp.read()  # 必须读 body 才能让 keep-alive 干净 close
                    return
                finally:
                    conn.close()
            except (OSError, socket.timeout, http.client.HTTPException) as exc:
                last_exc = exc
                # ConnectionResetError / channel 被 SSH 关 都属此类, retry 等 daemon 真就绪
                time.sleep(interval)

        # 全部 retry 失败: daemon listen 但 HTTP 不响应, 很可能 framework 内部 bug
        raise RemoteDaemonStartFailed(
            f"daemon HTTP 探测失败 (隧道 127.0.0.1:{local_port} 经 {retries} 次 retry 仍无响应; "
            f"last_exc={type(last_exc).__name__ if last_exc else 'None'}: {last_exc}); "
            "TCP listen 但 webui 未响应, 请去远端 SSH 检查 daemon.log"
        )

    def _shutdown_locked(self) -> None:
        """release 触发的清理: 远端 launcher stop + 隧道关闭. 调用方持有 lock."""
        self._set_state_locked(RemoteDaemonState.STOPPING)
        try:
            stop_cmd = self._launcher_cmds.daemon_stop_cmd()
            self._exec_backend.run(stop_cmd, check=False)
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"daemon launcher stop 异常 (静默忽略, 仍清理隧道): {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )
        if self._tunnel_manager is not None:
            self._tunnel_manager.stop()
        self._set_state_locked(RemoteDaemonState.STOPPED)

    def _set_state_locked(self, new_state: RemoteDaemonState) -> None:
        """状态机转移 + emit ``state_changed`` / ``ready`` 信号. 调用方持有 lock."""
        if self._state == new_state:
            return
        old = self._state
        self._state = new_state
        logger.info(
            f"RemoteSnowLumaDaemon 状态: {old.value} → {new_state.value}",
            LogType.NETWORK,
            LogSource.CORE,
        )
        # 信号 emit 不能在 lock 内做 (slot 可能回调 daemon 方法导致死锁);
        # 这里实际是 RLock, 但仍按规范处理: 用 thread 派发
        try:
            self.state_changed.emit(new_state)
            if new_state == RemoteDaemonState.READY:
                self.ready.emit()
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"state_changed signal emit 异常: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )

    def _on_tunnel_crash(self, label: TunnelLabel, error_message: str) -> None:
        """:class:`SnowLumaTunnelManager` 的 watchdog 回调.

        策略: 不自动重连, 直接置 CRASHED + emit ``crashed`` 信号; 让 UI / driver
        决定下一步 (典型: 弹错给用户 + 停止所有挂在本 daemon 上的 Bot).
        """
        with self._lock:
            if self._state in (RemoteDaemonState.STOPPED, RemoteDaemonState.STOPPING):
                # 主动关闭期间隧道 stop 是预期的, 不算 crash
                return
            self._set_state_locked(RemoteDaemonState.CRASHED)
        full_msg = f"远端 {label} 隧道挂了: {error_message}"
        try:
            self.crashed.emit(full_msg)
        except Exception as exc:  # noqa: BLE001
            logger.warning(
                f"crashed signal emit 异常: {exc}",
                LogType.NETWORK,
                LogSource.CORE,
            )


__all__ = [
    "RemoteSnowLumaDaemon",
    "RemoteDaemonState",
    "RemoteDaemonReadyInfo",
    "RemoteDaemonStartTimeout",
    "RemoteDaemonStartFailed",
]
