# -*- coding: utf-8 -*-
""":class:`RemoteSnowLumaDaemon` 单测 (W9).

覆盖:

- 状态机: STOPPED → STARTING → READY → STOPPING → STOPPED
- ``ensure_running`` 成功路径 + 引用计数 +1
- ``ensure_running`` 第二次调用复用 READY (无重复 launcher start)
- launcher start 失败 → raise + 回滚状态/计数
- 远端 ready 超时 → :class:`RemoteDaemonStartTimeout`
- ``release`` 计数归 0 时调 launcher stop + 关闭隧道
- 隧道 watchdog 触发 → 状态 CRASHED + ``crashed`` 信号
"""

from __future__ import annotations

from typing import Any
from unittest.mock import MagicMock

import pytest

from src.core.remote.models import RemoteCommandResult
from src.core.remote.snowluma import (
    RemoteDaemonStartFailed,
    RemoteDaemonStartTimeout,
    RemoteDaemonState,
    RemoteSnowLumaDaemon,
    SnowLumaRemoteDaemonState,
    SnowLumaRemoteDaemonStatus,
    SnowLumaRemotePaths,
)
from src.core.remote.snowluma import daemon as daemon_mod


# ==================== fixture ====================
@pytest.fixture
def fake_ssh_client() -> MagicMock:
    """Mock SSHClient; ``transport`` 属性返回 mock paramiko Transport."""
    client = MagicMock()
    client.is_connected = True
    fake_transport = MagicMock()
    fake_transport.is_active.return_value = True
    client.transport = fake_transport
    return client


@pytest.fixture
def paths() -> SnowLumaRemotePaths:
    return SnowLumaRemotePaths.from_base("/opt/sl")


@pytest.fixture
def patched_internals(monkeypatch: pytest.MonkeyPatch) -> dict[str, Any]:
    """把 ``RemoteExecutionBackend`` / ``SnowLumaTunnelManager`` / ``SnowLumaRemoteRuntimeService``
    都替换成可控 mock; 让 daemon 不真的发 SSH 命令也不真起隧道.
    """
    fake_exec = MagicMock()
    fake_exec.run.return_value = RemoteCommandResult(
        command="x", exit_status=0, stdout=""
    )

    fake_tunnels = MagicMock()
    fake_tunnels.is_alive.return_value = True
    fake_bundle = MagicMock()
    fake_bundle.webui.local_port = 47099
    fake_bundle.novnc.local_port = 47609
    fake_tunnels.acquire.return_value = fake_bundle
    fake_tunnels.get_endpoints.return_value = fake_bundle

    fake_runtime = MagicMock()
    ready_status = SnowLumaRemoteDaemonStatus(running=True, ready=True)
    fake_runtime.get_daemon_status.return_value = ready_status

    monkeypatch.setattr(
        daemon_mod, "RemoteExecutionBackend", lambda ssh: fake_exec
    )
    monkeypatch.setattr(
        daemon_mod,
        "SnowLumaTunnelManager",
        lambda transport, on_crash=None: fake_tunnels,
    )
    monkeypatch.setattr(
        daemon_mod, "SnowLumaRemoteRuntimeService", lambda backend, paths: fake_runtime
    )

    # P10 (2026-05-12): _verify_webui_http_reachable 在 status.READY 之后真发 HTTP 探测
    # 本地 47099, 测试环境无人 listen 会 retry 10s 卡死. 替换成 noop 让现有 daemon 测试
    # 全部走原路径. 端到端 HTTP 探测行为有专属 TestVerifyWebuiHttpReachable 单测覆盖.
    monkeypatch.setattr(
        daemon_mod.RemoteSnowLumaDaemon,
        "_verify_webui_http_reachable",
        lambda self, port, **kwargs: None,
    )

    return {
        "exec": fake_exec,
        "tunnels": fake_tunnels,
        "runtime": fake_runtime,
        "bundle": fake_bundle,
        "ready_status": ready_status,
    }


@pytest.fixture
def daemon(
    fake_ssh_client: MagicMock,
    paths: SnowLumaRemotePaths,
    patched_internals: dict[str, Any],
) -> RemoteSnowLumaDaemon:
    return RemoteSnowLumaDaemon(fake_ssh_client, paths)


# ==================== ensure_running 成功 ====================
class TestEnsureRunningSuccess:
    def test_first_call_starts_daemon(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        info = daemon.ensure_running()
        assert daemon.state == RemoteDaemonState.READY
        assert daemon.ref_count == 1
        assert info.tunnels is patched_internals["bundle"]

        # launcher start 命令被调
        patched_internals["exec"].run.assert_called()
        # 隧道 acquire 被调
        patched_internals["tunnels"].acquire.assert_called_once()

    def test_second_call_reuses(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        daemon.ensure_running()
        patched_internals["tunnels"].acquire.reset_mock()
        patched_internals["exec"].run.reset_mock()

        info2 = daemon.ensure_running()
        assert daemon.state == RemoteDaemonState.READY
        assert daemon.ref_count == 2
        # 不再触发 launcher start / acquire (复用)
        patched_internals["exec"].run.assert_not_called()
        patched_internals["tunnels"].acquire.assert_not_called()
        assert info2.tunnels is patched_internals["bundle"]


# ==================== ensure_running 失败 ====================
class TestEnsureRunningFailure:
    def test_launcher_start_nonzero_raises(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        patched_internals["exec"].run.return_value = RemoteCommandResult(
            command="x", exit_status=1, stdout="", stderr="boom"
        )
        with pytest.raises(RemoteDaemonStartFailed, match="返非 0"):
            daemon.ensure_running()
        # 回滚: 状态 STOPPED, 计数 0
        assert daemon.state == RemoteDaemonState.STOPPED
        assert daemon.ref_count == 0

    def test_remote_status_never_ready_raises_timeout(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        # 远端永远返 STARTING (running=true ready=false)
        patched_internals["runtime"].get_daemon_status.return_value = (
            SnowLumaRemoteDaemonStatus(running=True, ready=False)
        )
        with pytest.raises(RemoteDaemonStartTimeout):
            daemon.ensure_running(timeout=0.5)
        assert daemon.state == RemoteDaemonState.STOPPED
        assert daemon.ref_count == 0


# ==================== release ====================
class TestRelease:
    def test_release_decrements_ref_count(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        daemon.ensure_running()
        daemon.ensure_running()
        assert daemon.ref_count == 2

        daemon.release()
        assert daemon.ref_count == 1
        assert daemon.state == RemoteDaemonState.READY  # 仍 ready

        daemon.release()
        assert daemon.ref_count == 0
        assert daemon.state == RemoteDaemonState.STOPPED

    def test_release_at_zero_triggers_launcher_stop(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        daemon.ensure_running()
        patched_internals["exec"].run.reset_mock()

        daemon.release()
        # exec.run 至少被调一次 (daemon launcher stop)
        assert patched_internals["exec"].run.called
        # 隧道也被 stop
        patched_internals["tunnels"].stop.assert_called()

    def test_release_when_idle_is_safe(
        self,
        daemon: RemoteSnowLumaDaemon,
    ) -> None:
        daemon.release()  # 不抛
        daemon.release()  # 仍不抛
        assert daemon.ref_count == 0


# ==================== signal ====================
class TestSignals:
    def test_state_changed_emitted_on_transitions(
        self,
        daemon: RemoteSnowLumaDaemon,
    ) -> None:
        states: list[RemoteDaemonState] = []
        daemon.state_changed.connect(lambda s: states.append(s))

        daemon.ensure_running()
        # STOPPED→STARTING→READY 至少 2 个转移
        assert RemoteDaemonState.STARTING in states
        assert RemoteDaemonState.READY in states

        daemon.release()
        # 末次 release: READY→STOPPING→STOPPED
        assert RemoteDaemonState.STOPPING in states
        assert RemoteDaemonState.STOPPED in states

    def test_ready_signal_emitted_once_on_first_ready(
        self,
        daemon: RemoteSnowLumaDaemon,
    ) -> None:
        ready_count = []
        daemon.ready.connect(lambda: ready_count.append(1))
        daemon.ensure_running()
        assert sum(ready_count) == 1


# ==================== crash 桥接 ====================
class TestCrashBridge:
    def test_tunnel_crash_callback_sets_state_and_emits(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        daemon.ensure_running()
        crashes: list[str] = []
        daemon.crashed.connect(lambda msg: crashes.append(msg))

        # 模拟 SnowLumaTunnelManager 触发 on_crash 回调
        daemon._on_tunnel_crash("webui", "is_running=False")

        assert daemon.state == RemoteDaemonState.CRASHED
        assert crashes
        assert "webui" in crashes[0]

    def test_crash_during_stopping_is_ignored(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        """主动 stop 期间隧道 stop 是预期事件, 不应误判为 crash."""
        daemon.ensure_running()
        crashes: list[str] = []
        daemon.crashed.connect(lambda msg: crashes.append(msg))

        # 把状态强制设为 STOPPING (模拟 release 中)
        with daemon._lock:
            daemon._set_state_locked(RemoteDaemonState.STOPPING)

        daemon._on_tunnel_crash("webui", "shutdown")
        # 状态保持 STOPPING, 不 emit crashed
        assert daemon.state == RemoteDaemonState.STOPPING
        assert not crashes


# ==================== is_alive ====================
class TestIsAlive:
    def test_returns_false_when_idle(
        self,
        daemon: RemoteSnowLumaDaemon,
    ) -> None:
        assert not daemon.is_alive()

    def test_returns_true_when_ready_and_tunnels_alive(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        daemon.ensure_running()
        assert daemon.is_alive()

    def test_returns_false_when_tunnel_dies(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        daemon.ensure_running()
        patched_internals["tunnels"].is_alive.return_value = False
        assert not daemon.is_alive()


# ==================== 属性暴露 ====================
class TestExposedProperties:
    def test_tunnel_manager_accessor(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        assert daemon.tunnel_manager is patched_internals["tunnels"]

    def test_runtime_service_accessor(
        self,
        daemon: RemoteSnowLumaDaemon,
        patched_internals: dict[str, Any],
    ) -> None:
        assert daemon.runtime_service is patched_internals["runtime"]

    def test_launcher_commands_accessor(
        self,
        daemon: RemoteSnowLumaDaemon,
    ) -> None:
        assert daemon.launcher_commands is not None


# ==================== _verify_webui_http_reachable 专属覆盖 ====================
class TestVerifyWebuiHttpReachable:
    """覆盖 P10 加入的 HTTP 端到端探测; 直接调实现, 不走 ensure_running 编排.

    这些 case 用 ``monkeypatch`` 拦截 ``http.client.HTTPConnection`` 让 retry 行为可控,
    不真发 socket 请求.
    """

    def _make_daemon(
        self,
        fake_ssh_client: MagicMock,
        paths: SnowLumaRemotePaths,
    ) -> RemoteSnowLumaDaemon:
        # 直接构造 (不走 patched_internals fixture, 避免它把 _verify_webui_http_reachable 也 noop 了)
        return RemoteSnowLumaDaemon(fake_ssh_client, paths)

    def test_returns_when_first_attempt_succeeds(
        self,
        fake_ssh_client: MagicMock,
        paths: SnowLumaRemotePaths,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """首次 HTTP HEAD 成功立即 return, 不 retry."""
        attempts: list[int] = []

        class _FakeConn:
            def __init__(self, host: str, port: int, timeout: float = 0) -> None:
                attempts.append(port)

            def request(self, method: str, path: str) -> None:
                pass

            def getresponse(self) -> MagicMock:
                resp = MagicMock()
                resp.read.return_value = b""
                return resp

            def close(self) -> None:
                pass

        # _verify_webui_http_reachable 内 ``import http.client`` 拿到 stdlib 模块,
        # 直接 patch ``http.client.HTTPConnection`` 即可拦截.
        import http.client as _httpc

        monkeypatch.setattr(_httpc, "HTTPConnection", _FakeConn)

        d = self._make_daemon(fake_ssh_client, paths)
        d._verify_webui_http_reachable(47099, retries=5, interval=0.01)
        assert attempts == [47099]

    def test_retries_then_succeeds(
        self,
        fake_ssh_client: MagicMock,
        paths: SnowLumaRemotePaths,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """前几次抛 OSError, 第 N 次成功. 不应 raise."""
        call_count = {"n": 0}

        class _FlakyConn:
            def __init__(self, host: str, port: int, timeout: float = 0) -> None:
                call_count["n"] += 1
                if call_count["n"] < 3:
                    raise OSError("connection refused")

            def request(self, method: str, path: str) -> None:
                pass

            def getresponse(self) -> MagicMock:
                resp = MagicMock()
                resp.read.return_value = b""
                return resp

            def close(self) -> None:
                pass

        import http.client as _httpc

        monkeypatch.setattr(_httpc, "HTTPConnection", _FlakyConn)

        d = self._make_daemon(fake_ssh_client, paths)
        d._verify_webui_http_reachable(47099, retries=10, interval=0.01)
        assert call_count["n"] == 3

    def test_raises_after_exhausting_retries(
        self,
        fake_ssh_client: MagicMock,
        paths: SnowLumaRemotePaths,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        """全部 retry 失败 → raise RemoteDaemonStartFailed (含 last_exc 信息)."""

        class _AlwaysFailConn:
            def __init__(self, host: str, port: int, timeout: float = 0) -> None:
                raise OSError("hard refused")

            def close(self) -> None:
                pass

        import http.client as _httpc

        monkeypatch.setattr(_httpc, "HTTPConnection", _AlwaysFailConn)

        d = self._make_daemon(fake_ssh_client, paths)
        from src.core.remote.snowluma.daemon import RemoteDaemonStartFailed

        with pytest.raises(RemoteDaemonStartFailed, match="HTTP 探测失败"):
            d._verify_webui_http_reachable(47099, retries=3, interval=0.01)
