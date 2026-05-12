# -*- coding: utf-8 -*-
""":mod:`src.core.remote.snowluma.tunnels` 单测 (W6).

覆盖:

- ``acquire`` / ``release`` 引用计数语义 (首次启动, 末次停)
- 双隧道建立: 默认优先端口绑定, 失败时回退随机
- ``get_endpoints`` 在未启动 / 已启动两态返回正确值
- ``stop`` 强制清理 + 重置计数
- ``reconnect`` 在 ref_count > 0 时重建; ref_count == 0 时 raise
- 端口冲突时回退随机不抛错
- watchdog 触发 ``on_crash`` 回调 (含 label)
"""

from __future__ import annotations

import socket
import threading
import time
from typing import Literal
from unittest.mock import MagicMock

import pytest

from src.core.remote.errors import SSHConnectionError
from src.core.remote.snowluma import (
    SNOWLUMA_REMOTE_NOVNC_PORT,
    SNOWLUMA_REMOTE_WEBUI_PORT,
    SnowLumaTunnelBundle,
    SnowLumaTunnelEndpoint,
    SnowLumaTunnelError,
    SnowLumaTunnelManager,
)
from src.core.remote.snowluma import tunnels as tunnels_mod


# ==================== fixture: fake transport ====================
@pytest.fixture
def fake_transport() -> MagicMock:
    """paramiko Transport mock; 不会被实际调用 (LocalPortForwarder.start 只用
    Transport 在 client 连进来时开 channel; 单测里没有客户端连接, 故 mock 足够)."""
    t = MagicMock()
    t.is_active.return_value = True
    return t


# ==================== fixture: 随机可用端口 (避免与全局 47099/47609 撞测试机器) ====================
def _pick_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


@pytest.fixture
def free_webui_port() -> int:
    return _pick_free_port()


@pytest.fixture
def free_novnc_port() -> int:
    return _pick_free_port()


@pytest.fixture
def manager(
    fake_transport: MagicMock, free_webui_port: int, free_novnc_port: int
) -> SnowLumaTunnelManager:
    """避开 47099/47609 默认端口; 用随机可用端口减少跨测试机器干扰."""
    return SnowLumaTunnelManager(
        fake_transport,
        webui_local_port=free_webui_port,
        novnc_local_port=free_novnc_port,
        watchdog_interval=0.1,  # 加速 watchdog 测试
    )


# ==================== Endpoint ====================
class TestSnowLumaTunnelEndpoint:
    def test_local_url(self) -> None:
        ep = SnowLumaTunnelEndpoint(label="webui", local_port=47099, remote_port=5099)
        assert ep.local_url == "http://127.0.0.1:47099"


# ==================== acquire / release ====================
class TestAcquireRelease:
    def test_first_acquire_starts_tunnels(
        self, manager: SnowLumaTunnelManager
    ) -> None:
        bundle = manager.acquire()
        try:
            assert manager.is_alive()
            assert manager.ref_count == 1
            assert isinstance(bundle, SnowLumaTunnelBundle)
            assert bundle.webui.remote_port == SNOWLUMA_REMOTE_WEBUI_PORT
            assert bundle.novnc.remote_port == SNOWLUMA_REMOTE_NOVNC_PORT
            # 实际绑了 free port (不必是优先端口本身)
            assert bundle.webui.local_port > 0
            assert bundle.novnc.local_port > 0
        finally:
            manager.release()

    def test_second_acquire_reuses(self, manager: SnowLumaTunnelManager) -> None:
        b1 = manager.acquire()
        try:
            b2 = manager.acquire()
            try:
                assert manager.ref_count == 2
                # 同一隧道, 端点完全一致
                assert b1.webui.local_port == b2.webui.local_port
                assert b1.novnc.local_port == b2.novnc.local_port
            finally:
                manager.release()
            assert manager.ref_count == 1
        finally:
            manager.release()
        assert manager.ref_count == 0
        assert not manager.is_alive()

    def test_release_without_acquire_is_safe(
        self, manager: SnowLumaTunnelManager
    ) -> None:
        manager.release()  # 不抛
        manager.release()  # 仍不抛
        assert manager.ref_count == 0

    def test_get_endpoints_returns_none_when_idle(
        self, manager: SnowLumaTunnelManager
    ) -> None:
        assert manager.get_endpoints() is None

    def test_get_endpoints_returns_bundle_when_alive(
        self, manager: SnowLumaTunnelManager
    ) -> None:
        manager.acquire()
        try:
            bundle = manager.get_endpoints()
            assert bundle is not None
            assert bundle.webui.label == "webui"
            assert bundle.novnc.label == "novnc"
        finally:
            manager.release()


# ==================== stop ====================
class TestStop:
    def test_stop_resets_ref_count_and_kills_tunnels(
        self, manager: SnowLumaTunnelManager
    ) -> None:
        manager.acquire()
        manager.acquire()
        assert manager.ref_count == 2
        manager.stop()
        assert manager.ref_count == 0
        assert not manager.is_alive()

    def test_stop_when_idle_is_safe(
        self, manager: SnowLumaTunnelManager
    ) -> None:
        manager.stop()  # 不抛
        assert manager.ref_count == 0


# ==================== reconnect ====================
class TestReconnect:
    def test_reconnect_raises_when_idle(
        self, manager: SnowLumaTunnelManager
    ) -> None:
        with pytest.raises(SnowLumaTunnelError, match="无活跃引用"):
            manager.reconnect()

    def test_reconnect_keeps_ref_count(
        self, manager: SnowLumaTunnelManager
    ) -> None:
        manager.acquire()
        try:
            old_bundle = manager.get_endpoints()
            assert old_bundle is not None
            new_bundle = manager.reconnect()
            assert manager.ref_count == 1  # 重连不变化计数
            assert manager.is_alive()
            assert isinstance(new_bundle, SnowLumaTunnelBundle)
            # 端口可能变 (因 NC LocalPortForwarder 每次 start 都重新绑定)
        finally:
            manager.release()


# ==================== 端口回退 ====================
class TestPortFallback:
    def test_falls_back_to_random_when_preferred_busy(
        self, fake_transport: MagicMock
    ) -> None:
        """优先端口被外部进程占用时, 自动回退随机端口."""
        # 先占用一个端口模拟冲突
        blocker = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        blocker.bind(("127.0.0.1", 0))
        blocker.listen(1)
        busy_port = blocker.getsockname()[1]

        manager = SnowLumaTunnelManager(
            fake_transport,
            webui_local_port=busy_port,
            novnc_local_port=_pick_free_port(),
            watchdog_interval=0.5,
        )
        try:
            bundle = manager.acquire()
            try:
                # WebUI 拿到的不是优先端口 (因被占用), 而是随机
                assert bundle.webui.local_port != busy_port
                assert bundle.webui.local_port > 0
            finally:
                manager.release()
        finally:
            blocker.close()


# ==================== watchdog ====================
class TestWatchdog:
    def test_crash_callback_fires_when_forwarder_dies(
        self, fake_transport: MagicMock, free_webui_port: int, free_novnc_port: int
    ) -> None:
        crashes: list[tuple[str, str]] = []

        def on_crash(label: Literal["webui", "novnc"], err: str) -> None:
            crashes.append((label, err))

        manager = SnowLumaTunnelManager(
            fake_transport,
            on_crash=on_crash,
            webui_local_port=free_webui_port,
            novnc_local_port=free_novnc_port,
            watchdog_interval=0.05,  # 加速到 50ms
        )
        manager.acquire()
        try:
            # 强制 stop WebUI forwarder, 让 watchdog 检测到 is_running=False
            with manager._lock:  # noqa: SLF001
                assert manager._webui_forwarder is not None  # noqa: SLF001
                manager._webui_forwarder.stop()  # noqa: SLF001

            # 等 watchdog 心跳 (50ms * 3 = 150ms 应足够)
            deadline = time.time() + 2.0
            while not crashes and time.time() < deadline:
                time.sleep(0.05)

            assert crashes, "watchdog 未触发 on_crash"
            label, _ = crashes[0]
            assert label == "webui"
        finally:
            manager.stop()

    def test_crash_callback_edge_triggered_not_repeated(
        self, fake_transport: MagicMock, free_webui_port: int, free_novnc_port: int
    ) -> None:
        """P3 (review) 回归: 隧道挂后 on_crash 只 emit 一次, 不每个 watchdog 周期刷屏."""
        crashes: list[tuple[str, str]] = []

        def on_crash(label: Literal["webui", "novnc"], err: str) -> None:
            crashes.append((label, err))

        manager = SnowLumaTunnelManager(
            fake_transport,
            on_crash=on_crash,
            webui_local_port=free_webui_port,
            novnc_local_port=free_novnc_port,
            watchdog_interval=0.05,  # 50ms
        )
        manager.acquire()
        try:
            # 强制 stop WebUI forwarder
            with manager._lock:  # noqa: SLF001
                assert manager._webui_forwarder is not None  # noqa: SLF001
                manager._webui_forwarder.stop()  # noqa: SLF001

            # 等首次 emit
            deadline = time.time() + 2.0
            while not crashes and time.time() < deadline:
                time.sleep(0.05)
            assert crashes, "首次 watchdog 应触发 on_crash"
            initial_count = len(crashes)

            # 再等 8 个 watchdog 周期 (400ms); 如果不是边沿触发, 这里至少有 7 次额外 emit
            time.sleep(0.4)
            assert len(crashes) == initial_count, (
                f"crashed 信号应边沿触发 (只 emit 一次), 实际 emit 了 {len(crashes)} 次"
            )
        finally:
            manager.stop()
