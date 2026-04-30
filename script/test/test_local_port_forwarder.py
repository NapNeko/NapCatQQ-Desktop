# -*- coding: utf-8 -*-
"""[`LocalPortForwarder`](src/core/remote/tunnel.py) 生命周期测试 (P2.5).

仅验证 ``start`` / ``stop`` 不依赖真实 SSH transport 的部分:
- ``start()`` 绑定本地 loopback + 返回随机端口
- ``stop()`` 幂等; 释放本地端口
- 重复 ``start()`` 复用同一端口

完整的 ``direct-tcpip`` 转发链路测试需要真实 paramiko Transport, 留给集成测试覆盖.
"""

from __future__ import annotations

import socket

import pytest

from src.core.remote.tunnel import LocalPortForwarder


class _NoopTransport:
    """paramiko.Transport 替身, 不会被 start/stop 路径调用到."""

    def open_channel(self, *args, **kwargs):  # pragma: no cover - 不会触发
        raise AssertionError("不应在 start/stop 阶段被调用")


@pytest.fixture
def forwarder() -> LocalPortForwarder:
    fwd = LocalPortForwarder(_NoopTransport(), "127.0.0.1", 6099, label="test")  # type: ignore[arg-type]
    yield fwd
    fwd.stop()


def test_start_returns_bound_local_port(forwarder: LocalPortForwarder) -> None:
    port = forwarder.start()
    assert isinstance(port, int) and port > 0
    assert forwarder.local_port == port
    assert forwarder.is_running is True

    # 端口确实被监听
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.settimeout(1.0)
        # 应能成功 connect (即使后端没有真正接受)
        probe.connect(("127.0.0.1", port))


def test_start_is_idempotent_returns_same_port(forwarder: LocalPortForwarder) -> None:
    port1 = forwarder.start()
    port2 = forwarder.start()
    assert port1 == port2


def test_stop_releases_port_and_marks_not_running(forwarder: LocalPortForwarder) -> None:
    port = forwarder.start()
    forwarder.stop()
    assert forwarder.is_running is False
    assert forwarder.local_port is None

    # 二次 stop 是幂等的
    forwarder.stop()


def test_context_manager_start_stop() -> None:
    with LocalPortForwarder(_NoopTransport(), "127.0.0.1", 6099, label="ctx") as fwd:  # type: ignore[arg-type]
        assert fwd.is_running is True
        assert fwd.local_port is not None
    assert fwd.is_running is False
