# -*- coding: utf-8 -*-
""":class:`src.core.operation.remote_snowluma_backend.RemoteSnowLumaBackend` 单元测试 (W10b-Driver).

策略: 与 [`test_remote_backend_process`](script/test/test_remote_backend_process.py)
相同模式 - 把 ``ssh_client`` / ``_exec_backend`` / ``_runtime`` 替换为 fake,
另外 mock ``_daemon`` 让其不实际跑 SSH; 完全绕开真实远端通讯, 仅验证:

- 启动序: ``_verify_launcher_present`` → ``daemon.ensure_running`` → 首次打开 noVNC →
  ``bot launcher start`` → ``status_bot_<qq_id>.json`` 探测
- 失败路径: launcher 失败 / status 显示未运行 → 必须 ``daemon.release()`` 回滚引用
- 停止序: ``bot launcher stop`` → ``daemon.release()`` (try/finally 保证 release)
- WebUI 端点: daemon 隧道存在时返 ``WebUIEndpoint``; 未建立返 ``None``
- close(): 把残留 ref_count 全部释放
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any
from unittest.mock import MagicMock

import pytest

import src.resource.resource  # noqa: F401  - 让 Qt 资源系统就绪 (脚本读模板)

from src.core.operation.backend import WebUIEndpoint
from src.core.operation.remote_snowluma_backend import RemoteSnowLumaBackend
from src.core.remote.errors import RemoteCommandError
from src.core.remote.models import RemoteCommandResult, SSHCredentials
from src.core.remote.snowluma import SnowLumaRemotePaths
from src.core.remote.snowluma.daemon import RemoteDaemonReadyInfo
from src.core.remote.snowluma.status import SnowLumaRemoteBotStatus
from src.core.remote.snowluma.tunnels import (
    SnowLumaTunnelBundle,
    SnowLumaTunnelEndpoint,
)
from src.core.runtime.snowluma_webui_client import (
    HookProcessInfo,
    SnowLumaWebUIError,
)


# ==================== Fakes ====================
@dataclass
class _FakeSSHClient:
    """最小 SSH 客户端替身; 仅暴露 backend 实际调用到的方法."""

    is_connected: bool = True
    remote_exists_result: bool = True
    remote_exists_paths: list[str] = field(default_factory=list)
    read_text_responses: dict[str, str] = field(default_factory=dict)

    def connect(self) -> None:
        self.is_connected = True

    def close(self) -> None:
        self.is_connected = False

    def remote_exists(self, path: str) -> bool:
        self.remote_exists_paths.append(path)
        return self.remote_exists_result

    def read_text(self, path: str) -> str:
        return self.read_text_responses.get(path, "")


@dataclass
class _FakeExecBackend:
    """RemoteExecutionBackend 替身."""

    history: list[tuple[str, dict[str, Any]]] = field(default_factory=list)
    canned: list[RemoteCommandResult] = field(default_factory=list)

    def run(
        self,
        command: str,
        *,
        timeout: float | None = None,
        check: bool = False,
    ) -> RemoteCommandResult:
        self.history.append((command, {"timeout": timeout, "check": check}))
        result = self.canned.pop(0) if self.canned else RemoteCommandResult(
            command=command, exit_status=0
        )
        if check and not result.ok:
            raise RemoteCommandError(
                command=command,
                exit_status=result.exit_status,
                stderr=result.stderr,
            )
        return result


@dataclass
class _FakeRuntime:
    """SnowLumaRemoteRuntimeService 替身."""

    status_responses: dict[str, SnowLumaRemoteBotStatus] = field(default_factory=dict)
    queried_qq_ids: list[str] = field(default_factory=list)

    def get_bot_status(self, qq_id: str) -> SnowLumaRemoteBotStatus:
        self.queried_qq_ids.append(qq_id)
        return self.status_responses.get(
            qq_id,
            SnowLumaRemoteBotStatus.stopped(qq_id),
        )

    def tail_bot_log(self, qq_id: str, lines: int = 200) -> str:  # noqa: ARG002
        return ""


@dataclass
class _FakeTunnelManager:
    """SnowLumaTunnelManager 替身; 控制 ``get_endpoints`` 返回值."""

    endpoints: SnowLumaTunnelBundle | None = None

    def get_endpoints(self) -> SnowLumaTunnelBundle | None:
        return self.endpoints


class _FakeWebUIClient:
    """:class:`SnowLumaWebUIClient` 替身; 拦截 ``load_process`` 让测试不实际打 httpx.

    每个实例化收集 ``host`` / ``port`` / ``password``, 测试可 assert 注入路径
    使用了**正确的隧道端口**与**有效密码**.

    通过类属性 ``_load_process_impl`` 控制 ``load_process`` 行为:

    - ``None`` (默认): 返回 ``HookProcessInfo(status="loaded")`` 模拟成功
    - 可调用: 调入参 ``pid`` 后返回 ``HookProcessInfo`` 或 raise (供失败路径测试)
    """

    _load_process_impl: "Any" = None
    instances: "list[_FakeWebUIClient]" = []

    def __init__(self, *, host: str, port: int, password: str) -> None:
        self.host = host
        self.port = port
        self.password = password
        self.load_process_calls: list[int] = []
        _FakeWebUIClient.instances.append(self)

    def load_process(self, pid: int) -> HookProcessInfo:
        self.load_process_calls.append(pid)
        if _FakeWebUIClient._load_process_impl is not None:
            return _FakeWebUIClient._load_process_impl(pid)
        return HookProcessInfo(
            pid=pid,
            name="qq",
            path="/opt/QQ/qq",
            uin="",
            status="loaded",
            error="",
        )

    @classmethod
    def reset(cls) -> None:
        cls._load_process_impl = None
        cls.instances = []


@dataclass
class _FakeDaemon:
    """RemoteSnowLumaDaemon 替身; 仅暴露 backend 用到的接口."""

    tunnels: SnowLumaTunnelBundle = field(
        default_factory=lambda: SnowLumaTunnelBundle(
            webui=SnowLumaTunnelEndpoint(label="webui", local_port=47099, remote_port=5099),
            novnc=SnowLumaTunnelEndpoint(label="novnc", local_port=47609, remote_port=6081),
        )
    )
    ensure_running_calls: int = 0
    release_calls: int = 0
    ref_count: int = 0
    raise_on_ensure: bool = False
    _tunnel_manager: _FakeTunnelManager = field(default_factory=_FakeTunnelManager)

    def __post_init__(self) -> None:
        self._tunnel_manager.endpoints = self.tunnels

    @property
    def tunnel_manager(self) -> _FakeTunnelManager:
        return self._tunnel_manager

    def ensure_running(self, *, timeout: float = 60.0) -> RemoteDaemonReadyInfo:  # noqa: ARG002
        self.ensure_running_calls += 1
        if self.raise_on_ensure:
            raise RuntimeError("fake daemon: ensure_running 故意失败")
        self.ref_count += 1
        # 隧道在 ensure_running 后才视为已建立
        self._tunnel_manager.endpoints = self.tunnels
        return RemoteDaemonReadyInfo(
            tunnels=self.tunnels,
            status=MagicMock(),  # bot backend 不读 status 字段, 占位即可
        )

    def release(self) -> None:
        self.release_calls += 1
        if self.ref_count > 0:
            self.ref_count -= 1
            if self.ref_count == 0:
                self._tunnel_manager.endpoints = None


# ==================== Fixtures ====================
@pytest.fixture
def credentials() -> SSHCredentials:
    return SSHCredentials(host="192.0.2.10", username="ubuntu", auth_method="password", password="x")


@pytest.fixture
def sl_paths() -> SnowLumaRemotePaths:
    return SnowLumaRemotePaths.from_base("/opt/sl")


@pytest.fixture
def backend(
    credentials: SSHCredentials,
    sl_paths: SnowLumaRemotePaths,
    monkeypatch: pytest.MonkeyPatch,
) -> RemoteSnowLumaBackend:
    """构造 backend + 替换全部 IO/daemon 依赖, 不触发真实 SSH/浏览器/httpx."""
    bk = RemoteSnowLumaBackend(credentials, sl_paths)
    bk.ssh_client = _FakeSSHClient()  # type: ignore[assignment]
    bk._exec_backend = _FakeExecBackend()  # type: ignore[attr-defined]
    bk._runtime = _FakeRuntime()  # type: ignore[attr-defined]
    bk._daemon = _FakeDaemon()  # type: ignore[attr-defined]
    # 预填 effective password, 让 _inject_remote_qq 走快路径 (无需读 webui.secret).
    # 生产路径上由 _ensure_remote_webui_password 在 daemon.ensure_running 之前渲染.
    bk._cached_webui_password = "test-cached-password"  # type: ignore[attr-defined]

    # 拦截 open_snowluma_vnc 避免真打开浏览器; 记录调用次数供断言
    open_calls: list[Any] = []

    def _fake_open(_backend, _paths, _novnc_endpoint) -> tuple[bool, str]:
        open_calls.append((_backend, _paths, _novnc_endpoint))
        # 与生产签名一致 (W10): ``(ok, message)``
        return True, _novnc_endpoint.local_url

    monkeypatch.setattr(
        "src.core.remote.snowluma.vnc_launcher.open_snowluma_vnc",
        _fake_open,
    )

    # 拦截 SnowLumaWebUIClient 避免 _inject_remote_qq 真打 httpx; 默认 stub 返成功.
    # 个别 case 通过 _FakeWebUIClient._load_process_impl = ... 改返回值/抛错.
    _FakeWebUIClient.reset()
    monkeypatch.setattr(
        "src.core.runtime.snowluma_webui_client.SnowLumaWebUIClient",
        _FakeWebUIClient,
    )

    bk._test_vnc_open_calls = open_calls  # type: ignore[attr-defined]
    bk._test_webui_clients = _FakeWebUIClient.instances  # type: ignore[attr-defined]
    return bk


# ==================== start_napcat 成功路径 ====================
class TestStartNapcatSuccess:
    def test_full_chain_returns_running_status(self, backend: RemoteSnowLumaBackend) -> None:
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456",
            uin="123456",
            pid=4242,
            running=True,
            started_at="2026-05-12T00:00:00Z",
        )

        result = backend.start_napcat("123456", MagicMock())

        assert result.running is True
        assert result.pid == 4242
        # 首次启动自动打开 noVNC
        assert len(backend._test_vnc_open_calls) == 1  # type: ignore[attr-defined]
        assert backend._novnc_browser_opened is True
        # daemon.ensure_running 被调
        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        assert daemon.ensure_running_calls == 1
        assert daemon.release_calls == 0  # 成功路径不应回滚
        # bot launcher start 命令被发出
        exec_backend: _FakeExecBackend = backend._exec_backend  # type: ignore[attr-defined]
        bot_start_cmds = [c for c, _ in exec_backend.history if "snowluma_bot_launcher.sh" in c and "start" in c]
        assert len(bot_start_cmds) == 1
        assert "123456" in bot_start_cmds[0]
        # 自动注入: WebUIClient 被构造 1 次, 用 webui 隧道本地端口 + cached password 调 load_process
        clients: list[_FakeWebUIClient] = backend._test_webui_clients  # type: ignore[attr-defined]
        assert len(clients) == 1
        assert clients[0].host == "127.0.0.1"
        assert clients[0].port == 47099  # _FakeDaemon 默认 webui 本地端口
        assert clients[0].password == "test-cached-password"
        assert clients[0].load_process_calls == [4242]
        # extra 透出注入结果
        assert result.extra["inject_status"] == "loaded"
        assert result.extra["inject_error"] is None

    def test_second_start_does_not_reopen_browser(self, backend: RemoteSnowLumaBackend) -> None:
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=1, running=True
        )
        runtime.status_responses["654321"] = SnowLumaRemoteBotStatus(
            qq_id="654321", uin="654321", pid=2, running=True
        )

        backend.start_napcat("123456", MagicMock())
        backend.start_napcat("654321", MagicMock())

        # 浏览器只打开一次 (daemon 已经 READY, 同一 backend 后续 Bot 不重复)
        assert len(backend._test_vnc_open_calls) == 1  # type: ignore[attr-defined]


# ==================== start_napcat 失败路径 ====================
class TestStartNapcatFailure:
    def test_missing_launcher_raises_file_not_found(self, backend: RemoteSnowLumaBackend) -> None:
        backend.ssh_client.remote_exists_result = False  # type: ignore[attr-defined]

        with pytest.raises(FileNotFoundError, match="snowluma_bot_launcher.sh"):
            backend.start_napcat("123456", MagicMock())

        # daemon 未被启 (launcher 缺失早于 ensure_running)
        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        assert daemon.ensure_running_calls == 0

    def test_launcher_failure_releases_daemon_ref(self, backend: RemoteSnowLumaBackend) -> None:
        # bot launcher 跑 exit 非 0
        exec_backend: _FakeExecBackend = backend._exec_backend  # type: ignore[attr-defined]
        exec_backend.canned.append(
            RemoteCommandResult(
                command="bash bot.sh start",
                exit_status=2,
                stderr="bot launcher 失败",
            )
        )

        with pytest.raises(RemoteCommandError):
            backend.start_napcat("123456", MagicMock())

        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        assert daemon.ensure_running_calls == 1
        # 失败必须 release 一次, 让 ref_count 不漏增
        assert daemon.release_calls == 1

    def test_status_not_running_releases_daemon_ref(self, backend: RemoteSnowLumaBackend) -> None:
        # launcher 返 0 但 status 文件显示未运行 (竞态场景)
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus.stopped("123456")

        with pytest.raises(RemoteCommandError, match="status_bot"):
            backend.start_napcat("123456", MagicMock())

        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        assert daemon.release_calls == 1


# ==================== start_napcat 自动注入 (修复: 远端不再要求手动 Load) ====================
class TestStartNapcatInject:
    """覆盖 :meth:`RemoteSnowLumaBackend._inject_remote_qq` 与 ``start_napcat`` 的集成.

    设计目标 (与本地 :meth:`SnowLumaDriver._do_phase_c_inject` 行为对齐):

    - 成功路径已由 :class:`TestStartNapcatSuccess` 覆盖 (extra.inject_status="loaded")
    - 失败路径**绝不能**阻断 Bot 启动: QQ 已在远端跑, 用户可去 WebUI 手动 Load 兜底
    - 各种失败模式 (网络异常 / SL 返 error / pid 缺失) 都需要在 extra.inject_error
      里塞可读字符串供 UI 提示
    """

    def test_webui_error_does_not_block_bot_start(
        self, backend: RemoteSnowLumaBackend
    ) -> None:
        """``SnowLumaWebUIError`` (网络/认证) 不应阻断 Bot 启动."""
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=4242, running=True
        )

        def _raise(_pid: int) -> HookProcessInfo:
            raise SnowLumaWebUIError(0, "fake network error")

        _FakeWebUIClient._load_process_impl = _raise

        # 不抛: Bot 启动成功
        result = backend.start_napcat("123456", MagicMock())

        assert result.running is True
        assert result.pid == 4242
        # 注入失败被记录到 extra
        assert result.extra["inject_status"] is None
        assert "fake network error" in result.extra["inject_error"]
        # daemon ref 没漏扣 (注入失败不触发 release, 因为 Bot 仍 running)
        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        assert daemon.release_calls == 0

    def test_hook_returns_status_error_does_not_block(
        self, backend: RemoteSnowLumaBackend
    ) -> None:
        """SL framework 返 ``status="error"`` (注入逻辑失败) 同样不应阻断 Bot 启动."""
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=4242, running=True
        )

        def _return_error(pid: int) -> HookProcessInfo:
            return HookProcessInfo(
                pid=pid,
                name="qq",
                path="/opt/QQ/qq",
                uin="",
                status="error",
                error="dlopen failed: libfoo.so.1 not found",
            )

        _FakeWebUIClient._load_process_impl = _return_error

        result = backend.start_napcat("123456", MagicMock())

        assert result.running is True
        assert result.extra["inject_status"] is None
        assert "dlopen failed" in result.extra["inject_error"]

    def test_missing_pid_skips_injection(self, backend: RemoteSnowLumaBackend) -> None:
        """``status_bot.pid`` 缺失时直接跳过注入并写入提示, 不抛."""
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        # status 标 running 但 pid 字段为 None (理论上不该出现, 但 status 文件被改坏时会)
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=None, running=True
        )

        result = backend.start_napcat("123456", MagicMock())

        assert result.running is True
        assert result.pid is None
        # 没构造 WebUIClient
        clients: list[_FakeWebUIClient] = backend._test_webui_clients  # type: ignore[attr-defined]
        assert len(clients) == 0
        # extra 显式标注跳过原因
        assert result.extra["inject_status"] is None
        assert "无 pid" in result.extra["inject_error"]

    def test_inject_uses_correct_webui_local_port(
        self, backend: RemoteSnowLumaBackend
    ) -> None:
        """daemon 给的不是默认 47099 时, 注入也得用 daemon 实际报的端口 (随机回退场景)."""
        # 改 daemon 的隧道端口模拟随机回退
        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        daemon.tunnels = SnowLumaTunnelBundle(
            webui=SnowLumaTunnelEndpoint(
                label="webui", local_port=51234, remote_port=5099
            ),
            novnc=SnowLumaTunnelEndpoint(
                label="novnc", local_port=51235, remote_port=6081
            ),
        )

        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=4242, running=True
        )

        backend.start_napcat("123456", MagicMock())

        clients: list[_FakeWebUIClient] = backend._test_webui_clients  # type: ignore[attr-defined]
        assert len(clients) == 1
        assert clients[0].port == 51234

    def test_inject_falls_back_to_webui_secret_when_no_cache(
        self, backend: RemoteSnowLumaBackend
    ) -> None:
        """``_cached_webui_password`` 为空时回退读 ``webui.secret`` 文件."""
        # 清掉缓存让 _inject_remote_qq 走 _resolve_remote_webui_password
        backend._cached_webui_password = None  # type: ignore[attr-defined]
        # 模拟 webui.secret 内容
        backend.ssh_client.read_text_responses[backend.sl_paths.webui_secret] = (  # type: ignore[attr-defined]
            "secret-from-file" + "\n"
        )

        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=4242, running=True
        )

        result = backend.start_napcat("123456", MagicMock())

        assert result.extra["inject_status"] == "loaded"
        clients: list[_FakeWebUIClient] = backend._test_webui_clients  # type: ignore[attr-defined]
        assert len(clients) == 1
        assert clients[0].password == "secret-from-file"


# ==================== stop_napcat ====================
class TestStopNapcat:
    def test_normal_stop_runs_launcher_and_releases(self, backend: RemoteSnowLumaBackend) -> None:
        # 先 start 让 daemon 有 ref
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=1, running=True
        )
        backend.start_napcat("123456", MagicMock())

        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        assert daemon.ref_count == 1

        backend.stop_napcat("123456")

        # ref 回到 0
        assert daemon.ref_count == 0
        # bot launcher stop 命令发出
        exec_backend: _FakeExecBackend = backend._exec_backend  # type: ignore[attr-defined]
        stop_cmds = [c for c, _ in exec_backend.history if "snowluma_bot_launcher.sh" in c and " stop " in c]
        assert len(stop_cmds) == 1

    def test_stop_releases_even_on_launcher_failure(self, backend: RemoteSnowLumaBackend) -> None:
        # 先 start
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=1, running=True
        )
        backend.start_napcat("123456", MagicMock())

        # stop 时让 launcher 失败 (远端 SSH 闪断 / launcher 自检挂)
        exec_backend: _FakeExecBackend = backend._exec_backend  # type: ignore[attr-defined]
        exec_backend.canned.append(
            RemoteCommandResult(
                command="bash bot.sh stop",
                exit_status=1,
                stderr="远端崩了",
            )
        )

        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        with pytest.raises(RemoteCommandError):
            backend.stop_napcat("123456")

        # 即使 launcher 失败, daemon ref 仍被 release
        assert daemon.release_calls == 1


# ==================== get_process_status ====================
class TestGetProcessStatus:
    def test_returns_running_when_status_file_says_so(self, backend: RemoteSnowLumaBackend) -> None:
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["123456"] = SnowLumaRemoteBotStatus(
            qq_id="123456", uin="123456", pid=9999, running=True, started_at="2026-01-01T00:00:00Z"
        )
        result = backend.get_process_status("123456")
        assert result.running is True
        assert result.pid == 9999
        assert result.started_at == "2026-01-01T00:00:00Z"
        assert result.extra.get("uin") == "123456"

    def test_returns_not_running_when_status_missing(self, backend: RemoteSnowLumaBackend) -> None:
        result = backend.get_process_status("123456")
        assert result.running is False
        assert result.pid is None


# ==================== get_webui_endpoint ====================
class TestGetWebUIEndpoint:
    def test_returns_none_when_daemon_not_started(
        self, credentials: SSHCredentials, sl_paths: SnowLumaRemotePaths
    ) -> None:
        # 不走 backend fixture 是因为 fixture 会塞 _daemon; 这里要测 daemon=None 路径
        bk = RemoteSnowLumaBackend(credentials, sl_paths)
        bk.ssh_client = _FakeSSHClient()  # type: ignore[assignment]
        # 不动 _daemon, 保持 None
        assert bk.get_webui_endpoint("123456") is None

    def test_returns_endpoint_when_tunnels_alive(self, backend: RemoteSnowLumaBackend) -> None:
        # 该测试验证 webui.secret 文件回退路径; 清掉 fixture 预填的 cache 强走 resolve.
        backend._cached_webui_password = None  # type: ignore[attr-defined]
        # 模拟 webui.secret 文件内容
        backend.ssh_client.read_text_responses[backend.sl_paths.webui_secret] = "abc123def456" + "\n"  # type: ignore[attr-defined]
        # daemon 已经持有 tunnel endpoints
        endpoint = backend.get_webui_endpoint("123456")
        assert endpoint is not None
        assert isinstance(endpoint, WebUIEndpoint)
        assert endpoint.base_url == "http://127.0.0.1:47099"
        # webui.secret 末尾换行符被 strip
        assert endpoint.token == "abc123def456"


# ==================== deployment property (W10b-Maintenance) ====================
class TestDeploymentProperty:
    """``RemoteSnowLumaBackend.deployment`` 必须返 ``SnowLumaDeployment`` 实例,
    供 ``ServerManager.rollback_server`` 调 ``clean_environment`` (与 NC backend 同语义).
    """

    def test_lazy_construct_returns_snowluma_deployment(
        self,
        credentials: SSHCredentials,
        sl_paths: SnowLumaRemotePaths,
    ) -> None:
        from src.core.remote.snowluma.deployment import SnowLumaDeployment

        bk = RemoteSnowLumaBackend(credentials, sl_paths)
        # 首次访问应惰性构造
        assert bk._deployment is None  # type: ignore[attr-defined]
        deployment = bk.deployment
        assert isinstance(deployment, SnowLumaDeployment)
        # paths 透传
        assert deployment.paths is sl_paths
        # 二次访问命中缓存
        assert bk.deployment is deployment


# ==================== close() ====================
class TestClose:
    def test_close_drains_daemon_refs(self, backend: RemoteSnowLumaBackend) -> None:
        # 启 2 个 Bot
        runtime: _FakeRuntime = backend._runtime  # type: ignore[attr-defined]
        runtime.status_responses["111111"] = SnowLumaRemoteBotStatus(
            qq_id="111111", uin="111111", pid=1, running=True
        )
        runtime.status_responses["222222"] = SnowLumaRemoteBotStatus(
            qq_id="222222", uin="222222", pid=2, running=True
        )
        backend.start_napcat("111111", MagicMock())
        backend.start_napcat("222222", MagicMock())

        daemon: _FakeDaemon = backend._daemon  # type: ignore[attr-defined]
        assert daemon.ref_count == 2

        # close 不 stop bot (那是 BotProcessManager 的事), 但要 drain daemon ref 让 SSH 安全关
        backend.close()

        assert daemon.ref_count == 0
        assert backend.ssh_client.is_connected is False  # type: ignore[attr-defined]
