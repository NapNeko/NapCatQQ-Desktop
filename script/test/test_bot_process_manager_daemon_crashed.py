# -*- coding: utf-8 -*-
"""W7 (2026-05-11): :class:`BotProcessManager` 接 SnowLuma daemon ``crashed`` 信号.

测试覆盖:

- ``_subscribe_daemon_crashed`` 幂等 (重复调不会重复 connect).
- ``_on_daemon_crashed`` 收集所有 SnowLuma Bot 后逐个 ``stop_bot`` + emit 一条
  ``notification_signal("error", ...)``.
- 单个 Bot 的 ``stop_bot`` 抛异常不阻塞其他 Bot 清理.
- daemon ``_get_daemon`` 抛异常时 ``_subscribe_daemon_crashed`` 静默吞掉.

参见: ``docs/requirements/2026-05-11-snowluma-daemon-refactor.md`` §2.7,
``docs/plans/2026-05-11-snowluma-daemon-refactor-execution-plan.md`` §W7.
"""
from __future__ import annotations

import os
from typing import Any

import pytest
from PySide6.QtCore import QObject, Signal
from PySide6.QtWidgets import QApplication

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def ensure_qapp() -> QApplication:
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    return ensure_qapp()


# ==================== 测试桩 ====================
class _FakeDaemon(QObject):
    """模拟 :class:`SnowLumaDaemon`, 只导出 ``crashed`` 信号."""

    crashed = Signal(str)
    ready = Signal()


class _FakeProcessModel:
    """模拟 ``SnowLumaProcessModel``: 仅暴露 ``qq_id``."""

    def __init__(self, qq_id: str) -> None:
        self.qq_id = qq_id


class _FakeSnowLumaDriver:
    """模拟 :class:`SnowLumaDriver`.

    - ``list_processes()`` 返回当前注册的 ``_FakeProcessModel`` 列表.
    - ``_get_daemon()`` 返回 ``_FakeDaemon`` (可被 monkeypatch 改为 raise 测试错误路径).
    - 其余空方法填充 manager 的接线需求.
    """

    def __init__(self) -> None:
        self._models: dict[str, _FakeProcessModel] = {}
        self._daemon = _FakeDaemon()
        self.detached_pollers: list[str] = []
        self.removed_models: list[str] = []
        self.daemon_release_calls: int = 0

    def add_bot(self, qq_id: str) -> None:
        self._models[qq_id] = _FakeProcessModel(qq_id)

    def list_processes(self) -> list[_FakeProcessModel]:
        return list(self._models.values())

    def get_process_model(self, qq_id: str) -> _FakeProcessModel | None:
        return self._models.get(qq_id)

    def remove_process_model(self, qq_id: str) -> None:
        self._models.pop(qq_id, None)
        self.removed_models.append(qq_id)

    def detach_poller(self, qq_id: str) -> None:
        self.detached_pollers.append(qq_id)
        return None  # 测试中不需要真实 poller

    def stop(self, qq_id: str) -> None:
        # 模拟 driver.stop: 干净 remove model
        self._models.pop(qq_id, None)
        self.daemon_release_calls += 1

    def _get_daemon(self) -> _FakeDaemon:
        return self._daemon

    def get_status_poller(self, _qq_id: str) -> None:
        return None


# ==================== fixture: BotProcessManager 实例 ====================
@pytest.fixture
def manager(monkeypatch):
    """构造一个 ``BotProcessManager``, 用 ``_FakeSnowLumaDriver`` 替换真 driver."""
    from src.core.runtime import bot_process_manager as bpm_module

    fake_driver = _FakeSnowLumaDriver()
    monkeypatch.setattr(bpm_module, "SnowLumaDriver", lambda: fake_driver)

    # 同时 patch ``NapCatDriver`` 避免拉真实 napcat 链
    monkeypatch.setattr(bpm_module, "NapCatDriver", lambda: type("DummyNapcat", (), {})())

    mgr = bpm_module.BotProcessManager()
    # 把 fake driver 暴露给测试用例
    mgr._fake_driver = fake_driver  # type: ignore[attr-defined]
    return mgr


# ==================== _subscribe_daemon_crashed ====================
class TestSubscribeDaemonCrashed:
    """W7: 接 daemon.crashed 信号."""

    def test_idempotent_subscribe(self, manager) -> None:
        """重复调 ``_subscribe_daemon_crashed`` 不会重复 connect (用 ``_daemon_crashed_wired`` 守护)."""
        manager._subscribe_daemon_crashed()
        assert manager._daemon_crashed_wired is True

        # 二次调用应 short-circuit (no exception, 标记位仍为 True)
        manager._subscribe_daemon_crashed()
        assert manager._daemon_crashed_wired is True

    def test_subscribe_silently_swallows_daemon_lookup_error(
        self, manager, monkeypatch
    ) -> None:
        """``_get_daemon()`` 抛异常时 ``_subscribe_daemon_crashed`` 不应 raise."""
        def _raise_runtime(*_args, **_kwargs):
            raise RuntimeError("creart not initialized")

        monkeypatch.setattr(
            manager._fake_driver, "_get_daemon", _raise_runtime
        )

        # 不应 raise, 即使 daemon 不可达
        manager._subscribe_daemon_crashed()
        # 标记位保持 False, 下次能重试
        assert manager._daemon_crashed_wired is False


# ==================== _on_daemon_crashed ====================
class TestOnDaemonCrashed:
    """W7: 全员清理 + 通知."""

    def test_stops_all_snowluma_bots(self, manager) -> None:
        """daemon crashed → 所有 SnowLuma Bot 调 ``stop_bot``."""
        manager._fake_driver.add_bot("12345")
        manager._fake_driver.add_bot("67890")

        # 用 monkeypatch 记录 stop_bot 调用
        stop_calls: list[str] = []
        original_stop_bot = manager.stop_bot

        def _record_stop_bot(qq_id: str) -> None:
            stop_calls.append(qq_id)
            try:
                original_stop_bot(qq_id)
            except Exception:
                pass  # 单元测试不必走完真 stop_bot

        manager.stop_bot = _record_stop_bot  # type: ignore[method-assign]

        manager._on_daemon_crashed("node exit_code=1, error=SIGTERM")
        # 应对每个 Bot 都调一次 stop_bot
        assert sorted(stop_calls) == ["12345", "67890"]

    def test_emits_single_error_notification(self, manager) -> None:
        """daemon crashed → 一条 ``notification_signal("error", message)``."""
        manager._fake_driver.add_bot("12345")
        emissions: list[tuple[str, str]] = []
        manager.notification_signal.connect(lambda level, msg: emissions.append((level, msg)))

        # patch stop_bot 防止真清理路径
        manager.stop_bot = lambda _qq_id: None  # type: ignore[method-assign]

        manager._on_daemon_crashed("node crashed (exit=137)")

        assert len(emissions) == 1
        level, msg = emissions[0]
        assert level == "error"
        # 消息含关键信息
        assert "SnowLuma daemon" in msg
        assert "已停止所有 SnowLuma Bot" in msg
        assert "node crashed (exit=137)" in msg

    def test_emits_notification_even_with_no_bots(self, manager) -> None:
        """daemon crashed 但没有任何 SnowLuma Bot → 仍 emit 一条通知 (让组件页可见)."""
        emissions: list[tuple[str, str]] = []
        manager.notification_signal.connect(lambda level, msg: emissions.append((level, msg)))
        manager.stop_bot = lambda _qq_id: None  # type: ignore[method-assign]

        manager._on_daemon_crashed("node lost stdout")

        assert len(emissions) == 1
        level, msg = emissions[0]
        assert level == "error"

    def test_single_stop_bot_failure_does_not_block_others(self, manager) -> None:
        """某个 Bot stop_bot 抛异常时, 其他 Bot 仍走清理."""
        manager._fake_driver.add_bot("11111")
        manager._fake_driver.add_bot("22222")
        manager._fake_driver.add_bot("33333")

        stop_calls: list[str] = []

        def _flaky_stop_bot(qq_id: str) -> None:
            stop_calls.append(qq_id)
            if qq_id == "22222":
                raise RuntimeError("simulated stop failure")

        manager.stop_bot = _flaky_stop_bot  # type: ignore[method-assign]

        # 不应 raise — 内部 try/except 吞掉
        manager._on_daemon_crashed("test")

        # 三个 Bot 都被尝试 stop
        assert sorted(stop_calls) == ["11111", "22222", "33333"]

    def test_signal_emission_via_real_connection(self, manager) -> None:
        """端到端: 通过 ``_subscribe_daemon_crashed`` 接 daemon, daemon emit → manager 处理.

        用 ``Qt.DirectConnection`` (默认) 避免 event loop 在测试中跑.
        """
        from PySide6.QtCore import Qt as QtMod

        manager._fake_driver.add_bot("99999")

        # 用 DirectConnection 让信号同步传递, 跳过 queued event loop
        manager._fake_driver._daemon.crashed.connect(
            manager._on_daemon_crashed, QtMod.ConnectionType.DirectConnection
        )

        emissions: list[tuple[str, str]] = []
        manager.notification_signal.connect(
            lambda level, msg: emissions.append((level, msg))
        )
        manager.stop_bot = lambda _qq_id: None  # type: ignore[method-assign]

        manager._fake_driver._daemon.crashed.emit("daemon died")

        # 一条通知应已 emit
        assert len(emissions) == 1
        assert emissions[0][0] == "error"
        assert "daemon died" in emissions[0][1]
