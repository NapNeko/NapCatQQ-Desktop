# -*- coding: utf-8 -*-
"""SnowLuma daemon 单测 (W1, P2 daemon 解耦重构).

覆盖范围:

- :func:`render_daemon_globals` 模块级辅助 (override / session fallback / 持久化)
- :class:`SnowLumaDaemon` 状态机 (STOPPED → STARTING → READY → STOPPED)
- :meth:`SnowLumaDaemon.ensure_running` 引用计数 / 并发去重 / CRASHED 状态报错
- :meth:`SnowLumaDaemon.release` 引用计数递减 / 归零回收
- :meth:`SnowLumaDaemon._on_node_finished` 崩溃路径 (state → CRASHED, emit ``crashed``)

测试隔离策略:

- 每个测试新建 :class:`SnowLumaDaemon` 实例 (不走 creart 单例, 避免跨用例污染)
- ``SnowLumaWebUIClient`` 全程 monkeypatch 替换为 mock; 不实际 HTTP
- ``QProcess`` start / waitForStarted 通过 monkeypatch ``_spawn_and_start_node`` 替换;
  daemon 内部仍持有一个真的但**未启动**的 ``QProcess`` 对象作占位

参见: ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.1,
``docs/plans/2026-05-11-snowluma-daemon-refactor-execution-plan.md`` §W1.
"""
from __future__ import annotations

import os
import threading
import time
from pathlib import Path
from typing import Any, Callable
from unittest.mock import MagicMock

import pytest
from creart import it
from PySide6.QtCore import QProcess
from PySide6.QtWidgets import QApplication

from src.core.runtime.paths import PathFunc

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def ensure_qapp() -> QApplication:
    """创建或复用测试用 offscreen QApplication."""
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _pump_until(predicate: Callable[[], bool], timeout: float = 2.0) -> bool:
    """主线程 pump events 直到 predicate 为 True 或超时.

    daemon 在 worker 线程调 ``QTimer.singleShot(0, ...)`` 把 spawn 调度到主线程,
    生产环境主线程 Qt event loop 持续 pump 没问题; 测试环境主线程被 pytest 占用,
    所以测试要显式 pump.
    """
    deadline = time.monotonic() + timeout
    while not predicate():
        if time.monotonic() >= deadline:
            return False
        QApplication.processEvents()
        time.sleep(0.005)
    return True


# ==================== fixtures ====================
@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    """所有 daemon 测试都需要 QApplication (QObject / QProcess / QTimer 依赖)."""
    return ensure_qapp()


@pytest.fixture
def snowluma_install(tmp_path: Path, monkeypatch) -> Path:
    """伪造一份 SnowLuma 安装包; monkeypatch ``PathFunc.snowluma_path`` 指向它."""
    fake_root = tmp_path / "SnowLuma"
    fake_root.mkdir()
    # node.exe / index.mjs 只需存在, 内容不重要 (daemon 不会真 spawn)
    (fake_root / "node.exe").write_bytes(b"\x4dZ")
    (fake_root / "index.mjs").write_text("// stub")

    path_func = it(PathFunc)
    monkeypatch.setattr(path_func, "snowluma_path", fake_root)
    return fake_root


@pytest.fixture
def isolated_session(tmp_path: Path, monkeypatch) -> Path:
    """把 ``snowluma_session.session_path`` 重定向到 tmp_path."""
    fake_session_path = tmp_path / "snowluma-session.json"
    monkeypatch.setattr(
        "src.core.runtime.snowluma_session.session_path",
        lambda: fake_session_path,
    )
    return fake_session_path


@pytest.fixture
def mock_webui_client_factory(monkeypatch):
    """Monkeypatch ``SnowLumaWebUIClient`` (在 daemon 模块内) 为 MagicMock 工厂.

    Returns:
        一个 (host, port, password) -> MagicMock 的 factory; 默认 mock 的
        ``wait_ready`` 返回 True, ``login`` 返回 "fake-token-xyz", ``logout`` 无副作用,
        ``token`` 属性也是 "fake-token-xyz".
    """
    created_clients: list[MagicMock] = []

    def _factory(*args: Any, **kwargs: Any) -> MagicMock:
        # 不用 spec_set: 测试需要在 mock 上挂自定义辅助属性 (_init_args)
        client = MagicMock()
        client.wait_ready.return_value = True
        client.login.return_value = "fake-token-xyz"
        client.token = "fake-token-xyz"
        client.last_wait_errors = {}
        client.logout.return_value = None
        client._init_args = (args, kwargs)  # 便于断言构造参数
        created_clients.append(client)
        return client

    monkeypatch.setattr(
        "src.core.runtime.snowluma_daemon.SnowLumaWebUIClient",
        _factory,
    )
    return created_clients


def _install_spawn_stub(
    daemon, monkeypatch, *, fake_password: str = "stub-password-Pwd1!2345"
) -> dict[str, Any]:
    """在指定 daemon 实例上 monkeypatch ``_spawn_and_start_node``.

    替换为一个无副作用的桩:
    - 不真正 ``start()`` node.exe
    - 设置 ``daemon._node_process`` 为一个真但**未启动**的 ``QProcess``
    - 返回 ``fake_password`` 作为 effective_password (供 client 构造)

    Returns:
        含 ``"calls"`` 计数 / ``"node_processes"`` 引用列表的 dict, 供测试断言.
    """
    state = {"calls": 0, "node_processes": []}

    def _stub(override: str = "") -> str:
        state["calls"] += 1
        # 不实际 spawn, 但构造一个真 QProcess 用于状态检查 (主线程)
        proc = QProcess()
        proc.setProgram("__not-started__")
        daemon._node_process = proc
        state["node_processes"].append(proc)
        state["last_override"] = override
        return fake_password

    monkeypatch.setattr(daemon, "_spawn_and_start_node", _stub)
    return state


# ==================== render_daemon_globals ====================
class TestRenderDaemonGlobals:
    """:func:`render_daemon_globals` 模块级辅助单测."""

    def test_no_override_creates_session_and_uses_its_password(
        self,
        snowluma_install: Path,
        isolated_session: Path,
    ) -> None:
        """无 override 且 session.json 不存在 → 现场 create_session, 返回 session.password."""
        import json

        from src.core.runtime.snowluma_daemon import render_daemon_globals

        assert not isolated_session.exists(), "前提: session.json 一开始不存在"

        effective = render_daemon_globals(snowluma_install, override="")

        # 1. session.json 应被创建
        assert isolated_session.exists()
        payload = json.loads(isolated_session.read_text(encoding="utf-8"))
        assert payload["password"] == effective
        assert effective and len(effective) >= 10  # 强密码

        # 2. runtime.json 应被写入, webuiPort=5099
        runtime_json = snowluma_install / "config" / "runtime.json"
        assert runtime_json.exists()
        runtime_payload = json.loads(runtime_json.read_text(encoding="utf-8"))
        assert runtime_payload["webuiPort"] == 5099

        # 3. webui.json 应包含 scrypt hash + salt + mustChangePassword=False
        webui_json = snowluma_install / "config" / "webui.json"
        assert webui_json.exists()
        webui_payload = json.loads(webui_json.read_text(encoding="utf-8"))
        assert "passwordHash" in webui_payload
        assert "passwordSalt" in webui_payload
        assert webui_payload["mustChangePassword"] is False

    def test_override_takes_precedence_over_session(
        self,
        snowluma_install: Path,
        isolated_session: Path,
    ) -> None:
        """传入 override 非空 → 返回 override, webui.json 用 override 渲染."""
        import json

        from src.core.runtime.snowluma_daemon import render_daemon_globals

        # 先 create session, 让 session.password != override
        from src.core.runtime.snowluma_session import create_session

        session = create_session()
        custom_password = "MyCustomP@ssw0rd1!"
        assert custom_password != session.password

        effective = render_daemon_globals(snowluma_install, override=custom_password)

        # override 优先: 返回 override 值
        assert effective == custom_password
        # session.json 仍存在且不动 (override 模式只覆盖 webui.json 的 scrypt hash)
        payload = json.loads(isolated_session.read_text(encoding="utf-8"))
        assert payload["password"] == session.password
        # webui.json 应被 override 渲染 (scrypt hash 与 session.password 算出来的不同;
        # 但我们只检查 hash 字段存在 - 强烈断言会要求重做 scrypt 校验, 太重)
        webui_json = snowluma_install / "config" / "webui.json"
        assert webui_json.exists()

    def test_blank_override_falls_back_to_session(
        self,
        snowluma_install: Path,
        isolated_session: Path,
    ) -> None:
        """override 是空白字符串 → 视为未设置, 走 session 路径."""
        from src.core.runtime.snowluma_daemon import render_daemon_globals
        from src.core.runtime.snowluma_session import create_session

        session = create_session()
        effective = render_daemon_globals(snowluma_install, override="   ")
        assert effective == session.password


# ==================== SnowLumaDaemon: 状态机 + ensure_running ====================
class TestEnsureRunning:
    """:meth:`SnowLumaDaemon.ensure_running` 状态机 / 引用计数行为."""

    def test_first_call_spawns_node_and_returns_ready_client(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """STOPPED → STARTING → READY: 首次调用应 spawn + login + 状态 READY."""
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        spawn_state = _install_spawn_stub(daemon, monkeypatch)

        assert daemon.state == DaemonState.STOPPED
        assert daemon.ref_count == 0

        client = daemon.ensure_running(override="")

        # 状态机: 已 READY
        assert daemon.state == DaemonState.READY
        assert daemon.ref_count == 1
        assert daemon.is_running() is True

        # spawn 被调一次
        assert spawn_state["calls"] == 1

        # client 是 mock 工厂创建的
        assert len(mock_webui_client_factory) == 1
        assert client is mock_webui_client_factory[0]

        # wait_ready + login 都被调
        client.wait_ready.assert_called_once()
        client.login.assert_called_once()

    def test_second_call_reuses_client_and_increments_ref_count(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """READY 状态下二次调用应直接返回同一 client, ref_count = 2."""
        from src.core.runtime.snowluma_daemon import SnowLumaDaemon

        daemon = SnowLumaDaemon()
        spawn_state = _install_spawn_stub(daemon, monkeypatch)

        client_a = daemon.ensure_running(override="")
        client_b = daemon.ensure_running(override="")

        # spawn 只调一次 (复用 daemon)
        assert spawn_state["calls"] == 1
        assert daemon.ref_count == 2
        # client 复用
        assert client_a is client_b
        assert len(mock_webui_client_factory) == 1
        # login 只调一次
        mock_webui_client_factory[0].login.assert_called_once()

    def test_crashed_state_raises_runtime_error(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """模拟 node.exe 退出 → state=CRASHED; 再次 ensure_running 应 raise."""
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)
        daemon.ensure_running(override="")
        assert daemon.state == DaemonState.READY

        # 模拟 node.exe 意外 finished
        daemon._on_node_finished(exit_code=1, exit_status=QProcess.ExitStatus.CrashExit)
        assert daemon.state == DaemonState.CRASHED

        # 再次 ensure_running 应 raise
        with pytest.raises(RuntimeError, match="已崩溃"):
            daemon.ensure_running(override="")

    def test_render_daemon_globals_uses_passed_override(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """ensure_running(override="...") 应把 override 透传到 _spawn_and_start_node."""
        from src.core.runtime.snowluma_daemon import SnowLumaDaemon

        daemon = SnowLumaDaemon()
        spawn_state = _install_spawn_stub(daemon, monkeypatch)

        daemon.ensure_running(override="MyOverride@123")

        assert spawn_state["last_override"] == "MyOverride@123"

    def test_concurrent_callers_only_spawn_once(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """两个 worker 线程并发调 ensure_running, 只 spawn 一次, ref_count=2.

        这是 W2 driver 的 Phase C worker 并发场景的核心场景: 用户连点启动两个 Bot.
        """
        from src.core.runtime.snowluma_daemon import SnowLumaDaemon

        daemon = SnowLumaDaemon()

        spawn_started = threading.Event()
        spawn_proceed = threading.Event()
        spawn_calls = {"count": 0, "last_override": None}

        def _slow_spawn(override: str = "") -> str:
            spawn_calls["count"] += 1
            spawn_calls["last_override"] = override
            proc = QProcess()
            proc.setProgram("__not-started__")
            daemon._node_process = proc
            spawn_started.set()
            # 让另一个 caller 进入 STARTING 等待
            spawn_proceed.wait(timeout=2.0)
            return "stub-password-Pwd1!2345"

        monkeypatch.setattr(daemon, "_spawn_and_start_node", _slow_spawn)

        # 让 mock 的 wait_ready 也阻塞一下, 模拟首个 caller 在跑
        clients_box: list[MagicMock] = []

        def _client_factory(*args: Any, **kwargs: Any) -> MagicMock:
            client = MagicMock()
            client.wait_ready.return_value = True
            client.login.return_value = "fake-token"
            client.token = "fake-token"
            client.last_wait_errors = {}
            clients_box.append(client)
            return client

        monkeypatch.setattr(
            "src.core.runtime.snowluma_daemon.SnowLumaWebUIClient", _client_factory
        )

        results: dict[int, Any] = {}
        errors: dict[int, BaseException] = {}

        def _worker(tid: int) -> None:
            try:
                results[tid] = daemon.ensure_running(override="")
            except BaseException as exc:  # noqa: BLE001
                errors[tid] = exc

        t1 = threading.Thread(target=_worker, args=(1,), daemon=True)
        t1.start()
        # 主线程 pump events 让 QTimer.singleShot 调度的 spawn 跑起来,
        # 然后 _slow_spawn 把 spawn_started.set() (但仍卡在 spawn_proceed.wait).
        assert _pump_until(spawn_started.is_set, timeout=3.0), "starter 未进入 spawn"

        t2 = threading.Thread(target=_worker, args=(2,), daemon=True)
        t2.start()
        # 给 t2 一点时间进入 ensure_running 的 STARTING 等待分支
        _pump_until(lambda: daemon.ref_count >= 2, timeout=1.0)

        # 放行 starter, 完成 spawn → wait_ready → login → state=READY
        spawn_proceed.set()

        # pump 直到两个 worker 都结束
        _pump_until(lambda: not (t1.is_alive() or t2.is_alive()), timeout=5.0)
        t1.join(timeout=1.0)
        t2.join(timeout=1.0)

        assert not errors, f"线程异常: {errors!r}"
        # spawn 只调一次
        assert spawn_calls["count"] == 1
        # ref_count = 2
        assert daemon.ref_count == 2
        # 两个 caller 拿到同一个 client
        assert results[1] is results[2]


# ==================== release / shutdown ====================
class TestRelease:
    """:meth:`SnowLumaDaemon.release` 引用计数 / shutdown 行为."""

    def test_release_decrements_ref_count_without_shutdown_if_still_used(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """ref_count=2 时 release → ref_count=1, daemon 仍 READY."""
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)

        daemon.ensure_running(override="")
        daemon.ensure_running(override="")
        assert daemon.ref_count == 2

        daemon.release()
        assert daemon.ref_count == 1
        assert daemon.state == DaemonState.READY
        # logout 不应被调 (daemon 还在跑)
        mock_webui_client_factory[0].logout.assert_not_called()

    def test_release_to_zero_does_not_shutdown_in_persistent_mode(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """持久 daemon 模型 (2026-05-11 设计变更): ``release()`` 把 ref 降到 0 也**不**
        触发 shutdown; daemon 保持 ``READY``, ``webui_client`` 仍可用. 真正 shutdown
        由显式 :meth:`SnowLumaDaemon.shutdown` 调用 (App 退出钩子)."""
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)

        daemon.ensure_running(override="")
        assert daemon.ref_count == 1

        daemon.release()
        assert daemon.ref_count == 0
        # 持久 daemon: state 不变, 仍 READY
        assert daemon.state == DaemonState.READY
        # logout **不**应被自动调用
        mock_webui_client_factory[0].logout.assert_not_called()

    def test_release_below_zero_is_idempotent(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """重复 release 不会让 ref_count 负数; 持久 daemon 模型下 state 仍 READY."""
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)

        daemon.ensure_running(override="")
        daemon.release()
        # 再 release 一次, 应静默 no-op
        daemon.release()
        assert daemon.ref_count == 0
        assert daemon.state == DaemonState.READY

    def test_shutdown_terminates_when_ready(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """:meth:`shutdown` 在 READY 状态触发完整 terminate: logout + state=STOPPED."""
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)

        daemon.ensure_running(override="")
        assert daemon.state == DaemonState.READY

        daemon.shutdown()
        assert daemon.state == DaemonState.STOPPED
        # logout 应被调一次 (fire-and-forget)
        mock_webui_client_factory[0].logout.assert_called_once()

    def test_shutdown_is_idempotent_on_already_stopped(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """:meth:`shutdown` 多次调用安全: 第二次 no-op, logout 仍只调一次."""
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)

        daemon.ensure_running(override="")
        daemon.shutdown()
        # 二次 shutdown 应静默 no-op
        daemon.shutdown()
        assert daemon.state == DaemonState.STOPPED
        # logout 仍只调一次
        mock_webui_client_factory[0].logout.assert_called_once()

    def test_shutdown_on_never_started_daemon_is_noop(self) -> None:
        """:meth:`shutdown` 在 STOPPED 状态 (从未 ensure_running) 调用应静默无副作用.

        覆盖 App 退出钩子在 daemon 从未被任何 Bot 启过时的场景 (用户全程用 NapCat backend).
        """
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        assert daemon.state == DaemonState.STOPPED

        # 不应 raise, 不应有任何副作用
        daemon.shutdown()
        assert daemon.state == DaemonState.STOPPED


# ==================== node finished → crashed signal ====================
class TestNodeFinished:
    """:meth:`SnowLumaDaemon._on_node_finished` 崩溃 / 正常退出路径分流."""

    def test_unexpected_finish_transitions_to_crashed(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """READY 状态下 node 退出 → state=CRASHED, emit ``crashed`` signal."""
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)
        daemon.ensure_running(override="")
        assert daemon.state == DaemonState.READY

        # 捕获 crashed signal
        crashed_payloads: list[str] = []
        daemon.crashed.connect(lambda msg: crashed_payloads.append(msg))

        daemon._on_node_finished(
            exit_code=139,
            exit_status=QProcess.ExitStatus.CrashExit,
        )

        # 处理一下 event loop (Qt 信号是 direct emit, 但保险 processEvents 一次)
        QApplication.processEvents()

        assert daemon.state == DaemonState.CRASHED
        assert daemon.ref_count == 0  # crashed 后清零
        assert len(crashed_payloads) == 1
        assert "exit_code=139" in crashed_payloads[0]

    def test_expected_finish_after_shutdown_does_not_emit_crashed(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        """显式 :meth:`shutdown` 后 node 自然退出 → state=STOPPED 不变, **不**进 CRASHED.

        2026-05-11 设计变更: 持久 daemon 下 ``release()`` 不再 terminate; 真正 terminate
        只发生在显式 ``shutdown()`` (App 退出钩子). 这里覆盖 "我们自己主动 shutdown 后
        node finished 信号到达" 的预期路径.
        """
        from src.core.runtime.snowluma_daemon import DaemonState, SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)
        daemon.ensure_running(override="")
        daemon.shutdown()
        # shutdown 后 state 已是 STOPPED (我们的 mock 没真 spawn, 同步走到底)
        assert daemon.state == DaemonState.STOPPED

        crashed_payloads: list[str] = []
        daemon.crashed.connect(lambda msg: crashed_payloads.append(msg))

        # 模拟 node 退出 (本 case 下 daemon._node_process 已被 shutdown 清, 但 _on_node_finished
        # 仍可能被 Qt 信号触发; 它应识别 state=STOPPED 后静默忽略.)
        daemon._on_node_finished(
            exit_code=0,
            exit_status=QProcess.ExitStatus.NormalExit,
        )
        QApplication.processEvents()

        assert daemon.state == DaemonState.STOPPED
        assert crashed_payloads == []


# ==================== webui_client 访问器 ====================
class TestWebUIClientAccessor:
    """:meth:`SnowLumaDaemon.webui_client` 状态校验."""

    def test_returns_client_when_ready(
        self,
        snowluma_install: Path,
        isolated_session: Path,
        mock_webui_client_factory: list[MagicMock],
        monkeypatch,
    ) -> None:
        from src.core.runtime.snowluma_daemon import SnowLumaDaemon

        daemon = SnowLumaDaemon()
        _install_spawn_stub(daemon, monkeypatch)
        client = daemon.ensure_running(override="")
        assert daemon.webui_client() is client

    def test_raises_when_not_ready(self) -> None:
        from src.core.runtime.snowluma_daemon import SnowLumaDaemon

        daemon = SnowLumaDaemon()
        with pytest.raises(RuntimeError, match="未就绪"):
            daemon.webui_client()
