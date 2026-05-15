# -*- coding: utf-8 -*-
"""SnowLuma 远端双隧道管理器 (W6).

为远端 SnowLuma daemon 提供两条 SSH 本地端口转发:

- **WebUI 隧道**: Desktop ``127.0.0.1:47099`` → remote ``5099`` (SnowLuma WebUI HTTP)
- **noVNC 隧道**: Desktop ``127.0.0.1:47609`` → remote ``6081`` (websockify + noVNC)

两条隧道**共享同一个 ServerProfile 的 SSH transport**, 不引入额外认证开销.
建立顺序固定 (WebUI → noVNC), 关闭顺序逆序; 两条任一失败都视为整体失败.

设计要点 (Plan §W6, OQ6):

- **引用计数**: 多 Bot 共用同一对隧道; 计数归 0 才关闭
- **优先固定端口**: 47099 / 47609 让 noVNC URL / WebUI URL 在 Desktop 重启间稳定;
  端口占用时回退随机 (与 NC :class:`LocalPortForwarder` 完全兼容)
- **崩溃监视 (watchdog)**: 每 2s 检查两条隧道 ``is_running``, 任一为 False 则
  emit ``on_crash(label, error)`` 回调; 不自动重连 (推给 UI / 调用方决定)
- **线程安全**: ``acquire`` / ``release`` / ``reconnect`` / ``stop`` 用 ``threading.Lock``
  序列化, 防止 watchdog 与 UI 操作竞争

OQ6 决策遵循: watchdog 周期固定 2s, 不实现"立即 probe" (用户人眼分辨不出 2s 延迟).
"""

from __future__ import annotations

import threading
from collections.abc import Callable
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from src.core.logging import LogSource, LogType, logger

from ..errors import SSHConnectionError
from ..tunnel import LocalPortForwarder

if TYPE_CHECKING:
    import paramiko


# ==================== 端口约定 ====================
SNOWLUMA_PREFERRED_WEBUI_LOCAL_PORT: int = 47099
"""SnowLuma WebUI 本地优先端口; 与 :class:`SnowLumaDaemon` 本地启动用的 5099 不撞 (Bot
本地直连本机 daemon, 不经隧道). 选 47099 是因 ``net.ipv4.ip_local_port_range`` 默认上
界通常为 60999, 47000 段较冷门."""

SNOWLUMA_PREFERRED_NOVNC_LOCAL_PORT: int = 47609

SNOWLUMA_REMOTE_WEBUI_PORT: int = 5099
"""远端 SnowLuma WebUI HTTP 端口 (与 daemon launcher 默认值对齐)."""

SNOWLUMA_REMOTE_NOVNC_PORT: int = 6081
"""远端 noVNC + websockify HTTP 端口."""

WATCHDOG_INTERVAL_SEC: float = 2.0
"""watchdog 心跳间隔 (秒); OQ6 决策固定."""


TunnelLabel = Literal["webui", "novnc"]


# ==================== 端点数据 ====================
@dataclass(slots=True, frozen=True)
class SnowLumaTunnelEndpoint:
    """单条隧道的绑定信息.

    Attributes:
        label: ``"webui"`` 或 ``"novnc"``
        local_port: Desktop 端实际绑定端口 (固定首选 / 随机回退)
        remote_port: 远端端口 (固定 5099 / 6081)
    """

    label: TunnelLabel
    local_port: int
    remote_port: int

    @property
    def local_url(self) -> str:
        """供 Desktop UI 直接打开的 URL (HTTP)."""
        return f"http://127.0.0.1:{self.local_port}"


@dataclass(slots=True, frozen=True)
class SnowLumaTunnelBundle:
    """两条隧道的端点集合 (manager.acquire 返回)."""

    webui: SnowLumaTunnelEndpoint
    novnc: SnowLumaTunnelEndpoint


# ==================== 异常 ====================
class SnowLumaTunnelError(SSHConnectionError):
    """SnowLuma 隧道相关错误 (启动失败 / 崩溃监视无法恢复 / etc)."""


# ==================== Crash 回调 ====================
CrashCallback = Callable[[TunnelLabel, str], None]
"""``(label, error_message)`` 形参; ``label`` 为崩溃的隧道, ``error_message`` 是人类可读
的失败原因."""


# ==================== 主类 ====================
class SnowLumaTunnelManager:
    """SnowLuma 双隧道生命周期管理器.

    Args:
        transport: paramiko Transport (来自 ``SSHClient``); 必须已建立.
        on_crash: 可选回调; watchdog 检测到任一隧道死时触发. 不自动重连.
        webui_local_port: 覆盖 WebUI 优先本地端口 (测试用; 生产保持默认).
        novnc_local_port: 覆盖 noVNC 优先本地端口 (测试用).
        watchdog_interval: watchdog 周期 (秒); 测试可设较小值加速断言.

    Examples:
        >>> manager = SnowLumaTunnelManager(transport, on_crash=on_tunnel_crash)
        >>> bundle = manager.acquire()
        >>> webbrowser.open(bundle.novnc.local_url + "/vnc.html?...")
        >>> # ... 任务结束
        >>> manager.release()
    """

    def __init__(
        self,
        transport: "paramiko.Transport",
        *,
        on_crash: CrashCallback | None = None,
        webui_local_port: int = SNOWLUMA_PREFERRED_WEBUI_LOCAL_PORT,
        novnc_local_port: int = SNOWLUMA_PREFERRED_NOVNC_LOCAL_PORT,
        watchdog_interval: float = WATCHDOG_INTERVAL_SEC,
    ) -> None:
        self._transport = transport
        self._on_crash = on_crash
        self._webui_preferred = webui_local_port
        self._novnc_preferred = novnc_local_port
        self._watchdog_interval = watchdog_interval

        self._webui_forwarder: LocalPortForwarder | None = None
        self._novnc_forwarder: LocalPortForwarder | None = None
        self._ref_count: int = 0
        self._lock = threading.RLock()  # RLock: acquire 可调 reconnect, 都需要 lock
        self._stop_event = threading.Event()
        self._watchdog_thread: threading.Thread | None = None
        # P3 (review): 边沿触发标志; on_crash 回调每次隧道挂只 emit 一次, 直到
        # ``_start_tunnels_locked`` / ``reconnect`` 重置. 解决 watchdog 周期内
        # ``is_running=False`` 恒成立导致 ``crashed`` 信号每 ``watchdog_interval``
        # 重发刷屏 UI 的 bug.
        self._crashed_emitted: bool = False

    # ==================== 公开 API ====================
    @property
    def ref_count(self) -> int:
        """当前引用计数 (调试用; UI 不应依赖)."""
        with self._lock:
            return self._ref_count

    def is_alive(self) -> bool:
        """两条隧道都处于 ``is_running`` 状态时返 True."""
        with self._lock:
            return (
                self._webui_forwarder is not None
                and self._webui_forwarder.is_running
                and self._novnc_forwarder is not None
                and self._novnc_forwarder.is_running
            )

    def get_endpoints(self) -> SnowLumaTunnelBundle | None:
        """已建立隧道时返 :class:`SnowLumaTunnelBundle`; 未建立返 ``None``."""
        with self._lock:
            if not self.is_alive():
                return None
            assert self._webui_forwarder is not None  # type narrowing
            assert self._novnc_forwarder is not None
            webui_port = self._webui_forwarder.local_port
            novnc_port = self._novnc_forwarder.local_port
            if webui_port is None or novnc_port is None:
                return None
            return SnowLumaTunnelBundle(
                webui=SnowLumaTunnelEndpoint(
                    label="webui",
                    local_port=webui_port,
                    remote_port=SNOWLUMA_REMOTE_WEBUI_PORT,
                ),
                novnc=SnowLumaTunnelEndpoint(
                    label="novnc",
                    local_port=novnc_port,
                    remote_port=SNOWLUMA_REMOTE_NOVNC_PORT,
                ),
            )

    def acquire(self) -> SnowLumaTunnelBundle:
        """引用计数 +1; 首次调用时建立两条隧道 + 启动 watchdog.

        Returns:
            两条隧道的端点; 调用方应 :meth:`release` 配对释放.

        Raises:
            SnowLumaTunnelError: 任一隧道建立失败 (端口绑定失败 / SSH channel 失败).
                此时已建立的隧道会被回滚关闭, 不留半状态.
        """
        with self._lock:
            self._ref_count += 1
            if self._ref_count == 1:
                try:
                    self._start_tunnels_locked()
                except Exception:
                    self._ref_count = 0
                    raise

            bundle = self.get_endpoints()
            if bundle is None:
                # 极端: 上面 start 成功但 get_endpoints 拿不到; 视为 race.
                # P4 (review): 必须先停掉已经起好的 forwarder + watchdog, 否则下次
                # acquire 会再起一对, 旧的成孤儿且占用端口.
                self._ref_count = 0
                self._stop_tunnels_locked()
                raise SnowLumaTunnelError("隧道刚建立后查询端点失败 (race)")
            return bundle

    def release(self) -> None:
        """引用计数 -1; 计数归 0 时关闭隧道 + 停止 watchdog. 幂等."""
        with self._lock:
            if self._ref_count == 0:
                return  # 多次 release 容忍
            self._ref_count -= 1
            if self._ref_count == 0:
                self._stop_tunnels_locked()

    def stop(self) -> None:
        """强制关闭隧道 + 重置引用计数; 用于 ServerProfile 切换 / SSH 重连前清理.

        P15 (review): 入口立刻 ``_stop_event.set()`` (在加 lock 前), 让 watchdog
        线程在下一个 ``wait()`` 唤醒立刻 break, 避免 watchdog 还在调 ``on_crash``
        回调 (可能持外部锁) 时 join 阻塞 5s. 实际清理仍在 lock 内做.
        """
        self._stop_event.set()
        with self._lock:
            self._ref_count = 0
            self._stop_tunnels_locked()

    def reconnect(self) -> SnowLumaTunnelBundle:
        """关闭并重建两条隧道, 引用计数保持不变.

        典型场景: watchdog 触发 ``on_crash`` 后, 调用方决定立即重连.
        若引用计数为 0, raise (没有任何 Bot 持有隧道, 不应该 reconnect).
        """
        with self._lock:
            if self._ref_count == 0:
                raise SnowLumaTunnelError("无活跃引用, 不应触发 reconnect")
            self._stop_tunnels_locked()
            self._start_tunnels_locked()
            bundle = self.get_endpoints()
            if bundle is None:
                raise SnowLumaTunnelError("reconnect 后无法查询端点")
            return bundle

    # ==================== 内部 ====================
    def _start_tunnels_locked(self) -> None:
        """建立两条隧道; 调用方必须已持有 ``self._lock``.

        失败时回滚所有已建立的隧道, raise :class:`SnowLumaTunnelError`.
        """
        # P3 (review): 重置边沿触发标志, 让重启后的隧道再次 crash 时 on_crash 能 emit
        self._crashed_emitted = False
        webui_fwd = self._build_forwarder(
            label="webui",
            remote_port=SNOWLUMA_REMOTE_WEBUI_PORT,
            preferred=self._webui_preferred,
        )
        try:
            webui_fwd.start()
        except SSHConnectionError:
            # 优先端口失败 → 回退随机
            logger.warning(
                f"SnowLuma WebUI 隧道优先端口 {self._webui_preferred} 绑定失败, 回退随机",
                LogType.NETWORK,
                LogSource.CORE,
            )
            webui_fwd = self._build_forwarder(
                label="webui",
                remote_port=SNOWLUMA_REMOTE_WEBUI_PORT,
                preferred=0,
            )
            try:
                webui_fwd.start()
            except SSHConnectionError as exc:
                raise SnowLumaTunnelError(
                    f"SnowLuma WebUI 隧道建立失败 (随机端口也失败): {exc}"
                ) from exc

        novnc_fwd = self._build_forwarder(
            label="novnc",
            remote_port=SNOWLUMA_REMOTE_NOVNC_PORT,
            preferred=self._novnc_preferred,
        )
        try:
            novnc_fwd.start()
        except SSHConnectionError:
            logger.warning(
                f"SnowLuma noVNC 隧道优先端口 {self._novnc_preferred} 绑定失败, 回退随机",
                LogType.NETWORK,
                LogSource.CORE,
            )
            novnc_fwd = self._build_forwarder(
                label="novnc",
                remote_port=SNOWLUMA_REMOTE_NOVNC_PORT,
                preferred=0,
            )
            try:
                novnc_fwd.start()
            except SSHConnectionError as exc:
                # 回滚 WebUI
                try:
                    webui_fwd.stop()
                except Exception:  # noqa: BLE001
                    pass
                raise SnowLumaTunnelError(
                    f"SnowLuma noVNC 隧道建立失败 (随机端口也失败): {exc}"
                ) from exc

        self._webui_forwarder = webui_fwd
        self._novnc_forwarder = novnc_fwd

        # 启动 watchdog
        self._stop_event.clear()
        self._watchdog_thread = threading.Thread(
            target=self._watchdog_loop,
            name="snowluma-tunnel-watchdog",
            daemon=True,
        )
        self._watchdog_thread.start()

        logger.info(
            (
                f"SnowLuma 隧道已建立: "
                f"webui=127.0.0.1:{webui_fwd.local_port}→remote:{SNOWLUMA_REMOTE_WEBUI_PORT}, "
                f"novnc=127.0.0.1:{novnc_fwd.local_port}→remote:{SNOWLUMA_REMOTE_NOVNC_PORT}"
            ),
            LogType.NETWORK,
            LogSource.CORE,
        )

    def _stop_tunnels_locked(self) -> None:
        """关闭隧道 + watchdog; 调用方必须持有 lock. 幂等."""
        self._stop_event.set()

        for fwd in (self._novnc_forwarder, self._webui_forwarder):
            if fwd is not None:
                try:
                    fwd.stop()
                except Exception as exc:  # noqa: BLE001
                    logger.warning(
                        f"SnowLuma 隧道 stop 异常 (label={fwd._label}): {exc}",  # noqa: SLF001
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
        self._webui_forwarder = None
        self._novnc_forwarder = None

        if self._watchdog_thread is not None and self._watchdog_thread.is_alive():
            self._watchdog_thread.join(timeout=5.0)
        self._watchdog_thread = None

    def _build_forwarder(
        self,
        *,
        label: TunnelLabel,
        remote_port: int,
        preferred: int,
    ) -> LocalPortForwarder:
        return LocalPortForwarder(
            self._transport,
            "127.0.0.1",
            remote_port,
            label=f"snowluma-{label}",
            preferred_local_port=preferred,
        )

    def _watchdog_loop(self) -> None:
        """周期检查两条隧道存活; 任一死时触发 ``on_crash`` 回调.

        Note:
            on_crash **不在 watchdog 内重连**, 而是 emit 给调用方决定;
            避免 watchdog 内部状态机过于复杂 + reconnect 失败时的死循环风险.
            调用方典型实现: 立即调 ``reconnect()`` 重建隧道, 或 ``stop()`` 放弃.

            P3 (review): on_crash 是"边沿触发" - 第一次检测到 crash 时 emit 一次,
            然后置 ``_crashed_emitted=True`` 抑制后续重复 emit, 避免每个 watchdog
            周期持续刷屏. 标志在 ``_start_tunnels_locked`` (含 reconnect 内部) 被
            重置, 让恢复后的二次 crash 也能正常 emit.
        """
        while not self._stop_event.wait(self._watchdog_interval):
            crashed_label: TunnelLabel | None = None
            should_emit = False
            with self._lock:
                if self._webui_forwarder is not None and not self._webui_forwarder.is_running:
                    crashed_label = "webui"
                elif self._novnc_forwarder is not None and not self._novnc_forwarder.is_running:
                    crashed_label = "novnc"
                if crashed_label is not None and not self._crashed_emitted:
                    self._crashed_emitted = True
                    should_emit = True
            if crashed_label is None or not should_emit:
                continue
            logger.warning(
                f"SnowLuma {crashed_label} 隧道意外终止 (watchdog 检测)",
                LogType.NETWORK,
                LogSource.CORE,
            )
            if self._on_crash is not None:
                try:
                    self._on_crash(crashed_label, "tunnel forwarder is_running=False")
                except Exception as exc:  # noqa: BLE001
                    logger.warning(
                        f"on_crash 回调抛出异常: {exc}",
                        LogType.NETWORK,
                        LogSource.CORE,
                    )
            # watchdog 不主动 break (轻成本; 让 stop_event 控制); 后续周期靠
            # ``_crashed_emitted`` 抑制重复 emit


__all__ = [
    "SnowLumaTunnelManager",
    "SnowLumaTunnelBundle",
    "SnowLumaTunnelEndpoint",
    "SnowLumaTunnelError",
    "TunnelLabel",
    "CrashCallback",
    "SNOWLUMA_PREFERRED_WEBUI_LOCAL_PORT",
    "SNOWLUMA_PREFERRED_NOVNC_LOCAL_PORT",
    "SNOWLUMA_REMOTE_WEBUI_PORT",
    "SNOWLUMA_REMOTE_NOVNC_PORT",
]
