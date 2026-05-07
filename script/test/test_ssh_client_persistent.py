# -*- coding: utf-8 -*-
"""[`SSHClient`](src/core/remote/ssh_client.py) 持久连接 + 自动重连单元测试 (P3.W1).

回归保护点 (对应 [`docs/general/remote_ssh_p3_plan.md`](../../docs/general/remote_ssh_p3_plan.md) §3.1):

- ``connect()`` 成功后必须调用 ``transport.set_keepalive(DEFAULT_KEEPALIVE_INTERVAL)``
- ``ensure_alive()`` 探活 + 一次性自动重连语义
- ``_call_with_retry()`` 失败重试一次的语义
- 命令超时 (``transport`` 仍活着) 不应触发重试
- 多线程并发触发断线时, 实际 reconnect 只发生一次

不依赖真实 SSH; 全部通过 monkeypatch + 子类替身完成。
"""

from __future__ import annotations

import threading
from typing import TYPE_CHECKING, Any

import pytest

from src.core.remote.errors import (
    RemoteCommandError,
    SSHAuthenticationError,
    SSHConnectionError,
)
from src.core.remote.models import RemoteCommandResult, SSHCredentials
from src.core.remote.ssh_client import SSHClient

if TYPE_CHECKING:
    pass


# ==================== Fakes for connect()-path keepalive 验证 ====================
class _FakeTransport:
    def __init__(self) -> None:
        self.keepalive_calls: list[int] = []
        self._active = True

    def is_active(self) -> bool:
        return self._active

    def set_keepalive(self, seconds: int) -> None:
        self.keepalive_calls.append(seconds)


class _FakeParamikoSSHClient:
    """``paramiko.SSHClient`` 替身; 跟踪 connect/get_transport/close 调用."""

    instances: list["_FakeParamikoSSHClient"] = []

    def __init__(self) -> None:
        self._transport = _FakeTransport()
        self.connect_kwargs: dict[str, Any] | None = None
        self.closed = False
        type(self).instances.append(self)

    def load_system_host_keys(self) -> None:
        return None

    def set_missing_host_key_policy(self, _policy: Any) -> None:
        return None

    def connect(self, **kwargs: Any) -> None:
        self.connect_kwargs = kwargs

    def get_transport(self) -> _FakeTransport | None:
        return self._transport

    def close(self) -> None:
        self.closed = True
        self._transport._active = False


class _FakeParamikoModule:
    """``src.core.remote.ssh_client.paramiko`` 模块替身."""

    SSHClient = _FakeParamikoSSHClient

    class SSHException(Exception):
        pass

    class AuthenticationException(SSHException):  # noqa: N818
        pass

    class BadHostKeyException(SSHException):  # noqa: N818
        pass

    class AutoAddPolicy:  # noqa: D401
        pass

    class RejectPolicy:  # noqa: D401
        pass

    class WarningPolicy:  # noqa: D401
        pass


@pytest.fixture
def credentials() -> SSHCredentials:
    """构造测试凭据; 仅覆盖 password 模式."""
    return SSHCredentials(
        host="192.0.2.10",
        username="napcat",
        auth_method="password",
        password="x",
        host_key_policy="auto_add",
    )


@pytest.fixture
def fake_paramiko(monkeypatch: pytest.MonkeyPatch) -> type[_FakeParamikoModule]:
    """把 ``ssh_client.paramiko`` 替换为 [`_FakeParamikoModule`](script/test/test_ssh_client_persistent.py)."""
    fake = _FakeParamikoModule
    monkeypatch.setattr("src.core.remote.ssh_client.paramiko", fake)
    _FakeParamikoSSHClient.instances.clear()
    return fake


# ==================== keepalive on connect ====================
class TestConnectAppliesKeepalive:
    def test_connect_calls_set_keepalive_with_default_interval(
        self,
        fake_paramiko: type[_FakeParamikoModule],
        credentials: SSHCredentials,
    ) -> None:
        """connect() 成功后必须把 keepalive 间隔同步给 paramiko transport."""
        client = SSHClient(credentials)
        client.connect()

        paramiko_inst = _FakeParamikoSSHClient.instances[-1]
        assert paramiko_inst._transport.keepalive_calls == [
            SSHClient.DEFAULT_KEEPALIVE_INTERVAL
        ], "connect() 后未调用 transport.set_keepalive(默认间隔)"

    def test_connect_keepalive_failure_does_not_break_connection(
        self,
        fake_paramiko: type[_FakeParamikoModule],
        credentials: SSHCredentials,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """transport.set_keepalive 抛异常时, 主连接仍应建立成功."""
        original_set = _FakeTransport.set_keepalive

        def _raising_set(self: _FakeTransport, seconds: int) -> None:  # noqa: ARG001
            raise RuntimeError("simulated keepalive failure")

        monkeypatch.setattr(_FakeTransport, "set_keepalive", _raising_set)
        try:
            client = SSHClient(credentials)
            client.connect()  # 不应抛错
            assert client.is_connected is True
        finally:
            monkeypatch.setattr(_FakeTransport, "set_keepalive", original_set)


# ==================== ensure_alive / 自动重连 ====================
class _ControlledSSHClient(SSHClient):
    """可控替身: ``is_connected`` / ``connect`` / ``_run_once`` 全部由测试驱动."""

    def __init__(self, credentials: SSHCredentials) -> None:
        super().__init__(credentials)
        self.connect_calls: int = 0
        self.connect_should_fail: bool = False
        self.connect_failure: Exception = SSHConnectionError("fake connect failure")
        self._fake_alive: bool = False
        # _client 必须非 None 才不会被 _require_client 拒绝
        self._client = object()  # type: ignore[assignment]
        # _run_once 行为: 每次调用从队列里 pop 一项, 是 Exception 就抛, 否则当 result 返回
        self.run_responses: list[Any] = []
        self.run_call_count: int = 0
        # 用 sleep 模拟 connect 耗时, 让并发测试能可靠重叠
        self.connect_sleep_seconds: float = 0.0

    def connect(self) -> None:  # type: ignore[override]
        import time as _time

        self.connect_calls += 1
        if self.connect_sleep_seconds:
            _time.sleep(self.connect_sleep_seconds)
        if self.connect_should_fail:
            raise self.connect_failure
        self._client = object()  # type: ignore[assignment]
        self._fake_alive = True

    @property
    def is_connected(self) -> bool:  # type: ignore[override]
        return self._fake_alive

    def simulate_disconnect(self) -> None:
        self._fake_alive = False

    def _run_once(  # type: ignore[override]
        self,
        command: str,
        *,
        timeout: float | None,
        get_pty: bool,
        check: bool,
    ) -> RemoteCommandResult:
        self.run_call_count += 1
        if not self.run_responses:
            return RemoteCommandResult(command=command, exit_status=0, stdout="ok", stderr="")
        item = self.run_responses.pop(0)
        if isinstance(item, BaseException):
            raise item
        return item


@pytest.fixture
def controlled(credentials: SSHCredentials) -> _ControlledSSHClient:
    client = _ControlledSSHClient(credentials)
    # 默认从"已连接"状态开始
    client._fake_alive = True
    return client


class TestEnsureAlive:
    def test_returns_true_without_reconnect_when_alive(
        self, controlled: _ControlledSSHClient
    ) -> None:
        """会话健康时不应触发任何重连."""
        assert controlled.ensure_alive() is True
        assert controlled.connect_calls == 0

    def test_returns_false_when_dead_and_reconnect_disabled(
        self, controlled: _ControlledSSHClient
    ) -> None:
        controlled.simulate_disconnect()
        assert controlled.ensure_alive(reconnect=False) is False
        assert controlled.connect_calls == 0, "reconnect=False 不应调用 connect()"

    def test_reconnect_once_when_dead(
        self, controlled: _ControlledSSHClient
    ) -> None:
        controlled.simulate_disconnect()
        assert controlled.ensure_alive(reconnect=True) is True
        assert controlled.connect_calls == 1
        assert controlled.is_connected is True

    def test_returns_false_when_reconnect_fails(
        self, controlled: _ControlledSSHClient
    ) -> None:
        controlled.simulate_disconnect()
        controlled.connect_should_fail = True
        controlled.connect_failure = SSHConnectionError("network unreachable")
        assert controlled.ensure_alive(reconnect=True) is False
        assert controlled.connect_calls == 1
        assert controlled.is_connected is False

    def test_reconnect_handles_auth_failure(
        self, controlled: _ControlledSSHClient
    ) -> None:
        """凭据失效时 ensure_alive 不应反复尝试或异常上抛, 而应返回 False."""
        controlled.simulate_disconnect()
        controlled.connect_should_fail = True
        controlled.connect_failure = SSHAuthenticationError("auth failed")
        assert controlled.ensure_alive(reconnect=True) is False


# ==================== _call_with_retry / run() 集成 ====================
class TestCallWithRetry:
    def test_pass_through_on_success(self, controlled: _ControlledSSHClient) -> None:
        """正常路径: op 成功一次, 不触发任何重连."""
        result = controlled.run("echo hi")
        assert result.ok
        assert controlled.run_call_count == 1
        assert controlled.connect_calls == 0

    def test_does_not_retry_when_transport_alive(
        self, controlled: _ControlledSSHClient
    ) -> None:
        """命令超时场景: SSHConnectionError 但 is_connected 仍为 True -> 不重试."""
        # 模拟"命令超时", transport 还活着
        controlled.run_responses = [SSHConnectionError("command timed out")]
        # is_connected 在 _run_once 抛错时仍为 True
        with pytest.raises(SSHConnectionError, match="command timed out"):
            controlled.run("sleep 60")
        assert controlled.run_call_count == 1
        assert controlled.connect_calls == 0, "transport 还活着不应触发重连"

    def test_retries_once_when_transport_dies(
        self, controlled: _ControlledSSHClient
    ) -> None:
        """断线场景: 第一次抛 SSHConnectionError 同时 transport 死亡 -> 重连 + 重试 1 次."""
        disconnect_exc = SSHConnectionError("connection reset by peer")

        # 自定义 _run_once: 第一次抛错并标记断线, 第二次正常返回
        original_run_once = controlled._run_once
        call_count = {"n": 0}

        def _flaky_run_once(
            command: str,
            *,
            timeout: float | None,
            get_pty: bool,
            check: bool,
        ) -> RemoteCommandResult:
            call_count["n"] += 1
            if call_count["n"] == 1:
                controlled._fake_alive = False  # 在抛错前标记 transport 已死
                raise disconnect_exc
            return RemoteCommandResult(command=command, exit_status=0, stdout="recovered", stderr="")

        controlled._run_once = _flaky_run_once  # type: ignore[assignment]
        try:
            result = controlled.run("ls")
        finally:
            controlled._run_once = original_run_once  # type: ignore[assignment]
        assert result.stdout == "recovered"
        assert call_count["n"] == 2, "断线后应重试 _run_once 恰好一次"
        assert controlled.connect_calls == 1, "应触发一次自动重连"

    def test_propagates_after_second_failure(
        self, controlled: _ControlledSSHClient
    ) -> None:
        """重连后第二次仍失败应原样上抛."""
        first_exc = SSHConnectionError("first")
        second_exc = SSHConnectionError("second")
        call_count = {"n": 0}

        def _always_dies(
            command: str,
            *,
            timeout: float | None,
            get_pty: bool,
            check: bool,
        ) -> RemoteCommandResult:
            call_count["n"] += 1
            controlled._fake_alive = False
            raise first_exc if call_count["n"] == 1 else second_exc

        controlled._run_once = _always_dies  # type: ignore[assignment]
        with pytest.raises(SSHConnectionError, match="second"):
            controlled.run("ls")
        assert call_count["n"] == 2
        assert controlled.connect_calls == 1

    def test_propagates_when_reconnect_fails(
        self, controlled: _ControlledSSHClient
    ) -> None:
        """断线后 reconnect 自身失败 -> 抛出原始异常."""
        controlled.connect_should_fail = True

        def _dies_once(
            command: str,
            *,
            timeout: float | None,
            get_pty: bool,
            check: bool,
        ) -> RemoteCommandResult:
            controlled._fake_alive = False
            raise SSHConnectionError("transport closed")

        controlled._run_once = _dies_once  # type: ignore[assignment]
        with pytest.raises(SSHConnectionError, match="transport closed"):
            controlled.run("ls")
        assert controlled.connect_calls == 1, "重连应被尝试一次"

    def test_remote_command_error_not_retried(
        self, controlled: _ControlledSSHClient
    ) -> None:
        """退出码非 0 抛 RemoteCommandError -> 不应被 _call_with_retry 重试."""
        # _ControlledSSHClient._run_once 完全替换了真实 _run_once 的"check=True 抛 RemoteCommandError"
        # 逻辑, 因此这里直接喂 RemoteCommandError 异常去验证 _call_with_retry 的过滤行为.
        controlled.run_responses = [
            RemoteCommandError(command="false", exit_status=1, stderr="boom"),
        ]
        with pytest.raises(RemoteCommandError):
            controlled.run("false", check=True)
        assert controlled.run_call_count == 1
        assert controlled.connect_calls == 0, "RemoteCommandError 不应触发重连"


# ==================== 并发 reconnect 序列化 ====================
class TestConcurrentReconnect:
    def test_concurrent_reconnect_serialized_to_single_handshake(
        self, controlled: _ControlledSSHClient
    ) -> None:
        """多个线程同时检测到 transport 死亡时, 实际 connect() 只应执行一次."""
        controlled.simulate_disconnect()
        # 让 connect 慢一点, 增加并发覆盖窗口
        controlled.connect_sleep_seconds = 0.05

        results: list[bool] = []
        results_lock = threading.Lock()

        def _worker() -> None:
            ok = controlled.ensure_alive(reconnect=True)
            with results_lock:
                results.append(ok)

        threads = [threading.Thread(target=_worker) for _ in range(8)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=5.0)

        assert len(results) == 8
        assert all(results), "所有 worker 都应看到 ensure_alive 成功"
        assert controlled.connect_calls == 1, (
            f"并发触发的 reconnect 必须被 _reconnect_lock 序列化, 实际 connect_calls={controlled.connect_calls}"
        )
