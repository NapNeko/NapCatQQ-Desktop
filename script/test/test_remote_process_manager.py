# -*- coding: utf-8 -*-
"""[`BotProcessManager`](src/core/runtime/napcat.py) 远端 Bot 管理路径测试 (P2.6).

通过 monkeypatch 把 [`QThreadPool`](https://doc.qt.io/qt-6/qthreadpool.html) 的 ``start``
改为同步执行, 把 [`resolve_backend_for_bot`](src/core/operation/resolver.py) 替换
为 fake backend, 即可在不依赖真实 SSH 的情况下验证完整状态机.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import pytest
from PySide6.QtCore import QProcess, QRunnable

import src.core.runtime.bot_process_manager as run_napcat
from src.core.operation.backend import ProcessStatus, WebUIEndpoint


@dataclass
class _FakeBackend:
    """``RemoteBackend`` 替身, 仅暴露 worker 触达的方法."""

    start_responses: list[ProcessStatus] = field(default_factory=list)
    stop_calls: list[str] = field(default_factory=list)
    poll_responses: list[tuple[ProcessStatus, WebUIEndpoint | None]] = field(default_factory=list)
    webui_endpoints: dict[str, WebUIEndpoint | None] = field(default_factory=dict)
    closed_tunnels: list[str] = field(default_factory=list)

    def connect(self) -> None:
        pass

    def start_napcat(self, qq_id: str, config) -> ProcessStatus:
        if self.start_responses:
            return self.start_responses.pop(0)
        return ProcessStatus(qq_id=qq_id, running=True, pid=1234, memory_rss_bytes=10 * 1024 * 1024)

    def stop_napcat(self, qq_id: str) -> None:
        self.stop_calls.append(qq_id)

    def get_process_status(self, qq_id: str) -> ProcessStatus:
        if self.poll_responses:
            return self.poll_responses[0][0]
        return ProcessStatus(qq_id=qq_id, running=True, pid=1234, memory_rss_bytes=10 * 1024 * 1024)

    def get_webui_endpoint(self, qq_id: str) -> WebUIEndpoint | None:
        return self.webui_endpoints.get(qq_id)

    def close_webui_tunnel(self, qq_id: str) -> None:
        self.closed_tunnels.append(qq_id)


class _SyncThreadPool:
    """让 QThreadPool.start(runnable) 立刻同步执行 ``run()`` 而非派发到后台线程."""

    @staticmethod
    def globalInstance() -> "_SyncThreadPool":
        return _SyncThreadPool()

    @staticmethod
    def start(runnable: QRunnable) -> None:
        # QRunnable subclasses define ``run()``
        runnable.run()  # type: ignore[attr-defined]


@pytest.fixture
def remote_backend() -> _FakeBackend:
    return _FakeBackend()


@pytest.fixture
def manager(monkeypatch: pytest.MonkeyPatch, remote_backend: _FakeBackend) -> run_napcat.BotProcessManager:
    """构造一个干净的 BotProcessManager + 全套 monkeypatch."""
    # 同步执行 worker. P3 perf W4: 远端 SSH runnable 通过 ``run_napcat.remote_ssh_pool``
    # 派发, 而 ``GetAuthStatusRunnable`` / ``GetLoginStatusRunnable`` 仍走 ``QThreadPool``;
    # 两条路径都需要 patch 成同步执行, 否则 ``RemoteBotOperationRunnable`` 不会真正跑.
    monkeypatch.setattr(run_napcat, "QThreadPool", _SyncThreadPool)
    monkeypatch.setattr(run_napcat, "remote_ssh_pool", lambda: _SyncThreadPool())

    # 让 resolve_backend_for_bot 始终返回 fake
    from src.core.operation import resolver as resolver_module

    monkeypatch.setattr(resolver_module, "resolve_backend_for_bot", lambda config, **_: remote_backend)

    # 屏蔽 logger 副作用
    for level in ("trace", "info", "warning", "error", "exception"):
        monkeypatch.setattr(run_napcat.logger, level, lambda *a, **k: None)

    # 替换需要 creart 的依赖管理器 (避免真正的 it(ManagerAutoRestartProcess) / login state / log mgr)
    class _NoopMgr:
        def __init__(self) -> None:
            self.create_calls: list[tuple] = []
            self.remove_calls: list[tuple] = []

        def create_auto_restart_timer(self, *a, **k): pass
        def remove_auto_restart_timer(self, *a, **k): pass

        def create_login_state(self, *a, **k):
            self.create_calls.append((a, k))

        def remove_login_state(self, *a, **k):
            self.remove_calls.append((a, k))

        # P3 远端日志管理 mock
        def create_remote_log(self, *a, **k):
            self.create_calls.append((a, k))

        def remove_log(self, *a, **k):
            self.remove_calls.append((a, k))

    fake_auto = _NoopMgr()
    fake_login = _NoopMgr()
    fake_log_mgr = _NoopMgr()

    def fake_it(cls):
        if cls.__name__ == "ManagerAutoRestartProcess":
            return fake_auto
        if cls.__name__ == "ManagerNapCatQQLoginState":
            return fake_login
        if cls.__name__ == "ManagerNapCatQQLog":
            return fake_log_mgr
        raise AssertionError(f"未预期的 it({cls.__name__})")

    monkeypatch.setattr(run_napcat, "it", fake_it)

    mgr = run_napcat.BotProcessManager()
    mgr._fake_login = fake_login  # type: ignore[attr-defined]
    mgr._fake_log_mgr = fake_log_mgr  # type: ignore[attr-defined]
    return mgr


def _make_remote_config(config_factory, server_id: str = "srv-1", qqid: int = 1145141919):
    config = config_factory(qqid=qqid)
    config.bot.runtime_target = server_id
    return config


# ==================== 启动流程 ====================
def test_create_remote_process_emits_running_state(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(
        ProcessStatus(qq_id=qq_id, running=True, pid=4321, memory_rss_bytes=20 * 1024 * 1024)
    )
    # 让初次 poll 触达 fake 中的 (running, no endpoint) 状态
    remote_backend.poll_responses.append(
        (ProcessStatus(qq_id=qq_id, running=True, pid=4321, memory_rss_bytes=20 * 1024 * 1024), None)
    )

    states: list[QProcess.ProcessState] = []
    manager.process_changed_signal.connect(lambda _qq, st: states.append(st))

    manager.start_bot(config)

    # Starting -> Running 两步; (poll 不会改变 state)
    assert QProcess.ProcessState.Starting in states
    assert QProcess.ProcessState.Running in states

    record = manager.remote_process_dict[qq_id]
    assert record.state == QProcess.ProcessState.Running
    assert record.last_memory_rss_bytes == 20 * 1024 * 1024


def test_create_remote_process_handles_failure(
    manager: run_napcat.BotProcessManager, config_factory, monkeypatch: pytest.MonkeyPatch
) -> None:
    config = _make_remote_config(config_factory)

    # 让 fake backend.start_napcat 抛错
    from src.core.operation import resolver as resolver_module

    class _ExplodingBackend(_FakeBackend):
        def start_napcat(self, qq_id: str, cfg):
            raise RuntimeError("simulated SSH failure")

    monkeypatch.setattr(resolver_module, "resolve_backend_for_bot", lambda c, **_: _ExplodingBackend())

    states: list[QProcess.ProcessState] = []
    manager.process_changed_signal.connect(lambda _qq, st: states.append(st))

    manager.start_bot(config)

    assert QProcess.ProcessState.NotRunning in states
    assert str(config.bot.QQID) not in manager.remote_process_dict


# ==================== 停止流程 ====================
def test_stop_remote_process_clears_record(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(
        ProcessStatus(qq_id=qq_id, running=True, pid=4321)
    )
    remote_backend.poll_responses.append(
        (ProcessStatus(qq_id=qq_id, running=True, pid=4321), None)
    )

    manager.start_bot(config)
    assert qq_id in manager.remote_process_dict

    manager.stop_bot(qq_id)

    assert qq_id not in manager.remote_process_dict
    assert remote_backend.stop_calls == [qq_id]
    assert remote_backend.closed_tunnels == [qq_id]


def test_stop_remote_process_removes_login_state_before_ssh_stop(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    """回归: 用户点 "停止" 的瞬间必须立即 ``remove_login_state`` 关掉 3s HTTP 轮询定时器,
    不能等到 ``backend.stop_napcat`` 返回 (那是 ~4s 之后, 期间会持续打出
    ``ConnectError: 由于目标计算机积极拒绝`` 噪音).
    """
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(ProcessStatus(qq_id=qq_id, running=True, pid=4321))
    remote_backend.poll_responses.append(
        (ProcessStatus(qq_id=qq_id, running=True, pid=4321), None)
    )

    manager.start_bot(config)
    fake_login = manager._fake_login  # type: ignore[attr-defined]

    # 启动期间不会触发 remove_login_state(create 流程内只有 create / publish)
    pre_stop_remove_count = len(fake_login.remove_calls)

    # 在 stop_napcat 被调用前记录 remove_login_state 的调用数, 用以断言顺序.
    pre_ssh_stop_remove_count: list[int] = []
    original_stop_napcat = remote_backend.stop_napcat

    def _spy_stop_napcat(stopped_qq_id: str) -> None:
        pre_ssh_stop_remove_count.append(len(fake_login.remove_calls))
        original_stop_napcat(stopped_qq_id)

    remote_backend.stop_napcat = _spy_stop_napcat  # type: ignore[assignment]

    manager.stop_bot(qq_id)

    # 在 backend.stop_napcat 调用之前, remove_login_state 已经被触发过至少一次.
    assert pre_ssh_stop_remove_count, "stop_napcat 应该被调用过 (sync thread pool)"
    assert pre_ssh_stop_remove_count[0] > pre_stop_remove_count, (
        "remove_login_state 必须先于 backend.stop_napcat 被调用, "
        "否则 NapCatQQLoginState 的定时器在 SSH stop 期间继续 fire HTTP 请求, "
        "命中已关闭的 SSH 隧道并报 ConnectError."
    )


def test_late_poll_after_stop_does_not_republish_login_state(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    """回归: 用户点 "停止" 后, 若一个 in-flight 的 poll worker 才返回结果,
    且 record 因 ``stop_all_processes`` 等原因仍存在但 ``state=NotRunning``,
    必须丢弃该结果, 不能再走 ``_publish_remote_login_state`` 重建定时器.
    """
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)

    # 手工构造一个停止中的 record (跳过完整 create 流程, 直接模拟边界态)
    from src.core.runtime.bot_process_manager import RemoteProcessRecord

    record = RemoteProcessRecord(
        qq_id=qq_id,
        config=config,
        state=QProcess.ProcessState.NotRunning,
    )
    record.login_state_published = False
    record.login_state_port = None
    manager.remote_process_dict[qq_id] = record

    fake_login = manager._fake_login  # type: ignore[attr-defined]
    pre_create_count = len(fake_login.create_calls)

    # 模拟一个迟到的 poll 结果 (running=True + endpoint), 此时 record.state=NotRunning
    late_status = ProcessStatus(qq_id=qq_id, running=True, pid=4321)
    late_endpoint = WebUIEndpoint(base_url="http://127.0.0.1:51234", token="abc")
    manager._handle_remote_poll_result(qq_id, (late_status, late_endpoint))

    # 不应该再触发 create_login_state
    assert len(fake_login.create_calls) == pre_create_count


# ==================== P3 远端日志 ====================
def test_create_remote_process_creates_log_buffer(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    """启动远端 Bot 时必须立刻创建日志缓冲, 否则用户开 ``BotLogPage`` 会显示
    "未找到对应的日志信息".
    """
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(ProcessStatus(qq_id=qq_id, running=True, pid=1))
    remote_backend.poll_responses.append((ProcessStatus(qq_id=qq_id, running=True, pid=1), None))

    fake_log_mgr = manager._fake_log_mgr  # type: ignore[attr-defined]
    pre_create_count = len(fake_log_mgr.create_calls)

    manager.start_bot(config)

    assert len(fake_log_mgr.create_calls) == pre_create_count + 1


def test_stop_remote_process_removes_log_buffer(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    """停止远端 Bot 时必须释放日志缓冲, 关闭 SSH ``tail`` 轮询计时器,
    否则会持续在后台拉日志/打 trace.
    """
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(ProcessStatus(qq_id=qq_id, running=True, pid=1))
    remote_backend.poll_responses.append((ProcessStatus(qq_id=qq_id, running=True, pid=1), None))

    manager.start_bot(config)

    fake_log_mgr = manager._fake_log_mgr  # type: ignore[attr-defined]
    pre_remove_count = len(fake_log_mgr.remove_calls)

    manager.stop_bot(qq_id)

    assert len(fake_log_mgr.remove_calls) >= pre_remove_count + 1


# ==================== 状态聚合 ====================
def test_get_process_returns_remote_record(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(ProcessStatus(qq_id=qq_id, running=True, pid=1))
    remote_backend.poll_responses.append((ProcessStatus(qq_id=qq_id, running=True, pid=1), None))

    manager.start_bot(config)
    record = manager.get_process(qq_id)
    assert record is not None
    assert isinstance(record, run_napcat.RemoteProcessRecord)
    assert record.state == QProcess.ProcessState.Running


def test_has_running_bot_includes_remote(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    config = _make_remote_config(config_factory)
    remote_backend.start_responses.append(ProcessStatus(qq_id=str(config.bot.QQID), running=True, pid=1))
    remote_backend.poll_responses.append(
        (ProcessStatus(qq_id=str(config.bot.QQID), running=True, pid=1), None)
    )

    assert manager.has_running_bot() is False
    manager.start_bot(config)
    assert manager.has_running_bot() is True


def test_get_memory_usage_for_remote_uses_cached_rss(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(
        ProcessStatus(qq_id=qq_id, running=True, pid=1, memory_rss_bytes=42 * 1024 * 1024)
    )
    remote_backend.poll_responses.append(
        (ProcessStatus(qq_id=qq_id, running=True, pid=1, memory_rss_bytes=42 * 1024 * 1024), None)
    )

    manager.start_bot(config)
    assert manager.get_memory_usage(qq_id) == 42


# ==================== 轮询 / 离线检测 ====================
def test_poll_detects_offline_and_clears_record(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(ProcessStatus(qq_id=qq_id, running=True, pid=1))
    # 启动后立即 poll 一次, 但模拟"已离线"
    remote_backend.poll_responses.append(
        (ProcessStatus(qq_id=qq_id, running=False, pid=None), None)
    )

    manager.start_bot(config)

    # 启动成功后立刻发起一次 poll, fake 返回 not running -> 应清理 record
    assert qq_id not in manager.remote_process_dict


def test_poll_publishes_login_state_on_endpoint(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    config = _make_remote_config(config_factory)
    qq_id = str(config.bot.QQID)
    remote_backend.start_responses.append(ProcessStatus(qq_id=qq_id, running=True, pid=1))
    endpoint = WebUIEndpoint(base_url="http://127.0.0.1:51234", token="abc123")
    remote_backend.poll_responses.append(
        (ProcessStatus(qq_id=qq_id, running=True, pid=1), endpoint)
    )
    remote_backend.webui_endpoints[qq_id] = endpoint

    manager.start_bot(config)

    fake_login = manager._fake_login  # type: ignore[attr-defined]
    assert fake_login.create_calls, "endpoint 拿到后应当触发 create_login_state"
    args, kwargs = fake_login.create_calls[-1]
    assert kwargs["port"] == 51234
    assert kwargs["token"] == "abc123"

    record = manager.remote_process_dict[qq_id]
    assert record.login_state_published is True
    assert record.login_state_port == 51234


# ==================== 上限 ====================
def test_remote_process_capacity_limit(
    manager: run_napcat.BotProcessManager, config_factory, remote_backend: _FakeBackend
) -> None:
    # 直接塞 4 个 record 模拟已达上限
    for i in range(4):
        manager.remote_process_dict[f"100000000{i}"] = run_napcat.RemoteProcessRecord(
            qq_id=f"100000000{i}",
            config=_make_remote_config(config_factory, qqid=int(f"100000000{i}")),
            state=QProcess.ProcessState.Running,
        )

    notifications: list[tuple[str, str]] = []
    manager.notification_signal.connect(lambda level, msg: notifications.append((level, msg)))

    config = _make_remote_config(config_factory, qqid=2222222222)
    manager.start_bot(config)

    assert str(config.bot.QQID) not in manager.remote_process_dict
    assert any(level == "error" for level, _ in notifications)
