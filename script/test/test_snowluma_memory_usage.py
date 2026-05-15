# -*- coding: utf-8 -*-
"""SnowLuma 内存监控修复 (2026-05-11): ``get_memory_usage`` + ``ancillary_pids`` 同步.

历史 bug (用户实测):
- **HOT 模式 Bot 卡片内存一直显示 0**: ``snow_model.qq_process is None``, 旧版 ``get_memory_usage``
  直接返回 0.
- **COLD 模式内存与 SnowLuma WebUI 不一致**: 旧版用 ``qq_process.processId()`` 即 launcher PID,
  walk 整棵 Electron 子进程树, 含 renderer / GPU / utility 等子进程, 与 WebUI 显示的
  hooked 进程内存对不上.

修复: ``SnowLumaStatusPoller.pid_set_changed`` 信号写回 ``model.ancillary_pids``
(按 Bot UIN 聚合的 hooked QQ.exe PIDs, 与 WebUI ``/api/processes`` 一致).
``get_memory_usage`` 优先用 ``ancillary_pids`` 累加 RSS; 没有则 fallback 到
``qq_pid`` walk 进程树.

参见: ``src/core/runtime/bot_process_manager.py`` ``_on_snowluma_pid_set_changed`` /
``get_memory_usage``.
"""
from __future__ import annotations

import os
from unittest.mock import MagicMock, patch

import pytest
from PySide6.QtWidgets import QApplication

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


def _ensure_qapp() -> QApplication:
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    return _ensure_qapp()


# ==================== _on_snowluma_pid_set_changed ====================
class TestPidSetChangedHandler:
    """``BotProcessManager._on_snowluma_pid_set_changed`` 写回 ``model.ancillary_pids``."""

    def test_writes_pid_set_to_model(self) -> None:
        """poller emit ``pid_set_changed`` → manager 写到 model.ancillary_pids (set 类型)."""
        from src.core.runtime.bot_process_manager import BotProcessManager
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        manager = BotProcessManager()
        model = SnowLumaProcessModel(qq_id="11001")
        manager._snowluma_driver._processes["11001"] = model

        try:
            assert model.ancillary_pids == set()  # 初始空

            manager._on_snowluma_pid_set_changed("11001", [101, 202, 303])

            assert model.ancillary_pids == {101, 202, 303}
        finally:
            manager._snowluma_driver._processes.pop("11001", None)

    def test_unknown_qq_id_silently_ignored(self) -> None:
        """qq_id 不在 driver 字典 (Bot 已停 / 已 remove) → 静默 no-op, 不抛."""
        from src.core.runtime.bot_process_manager import BotProcessManager

        manager = BotProcessManager()
        # 不应抛
        manager._on_snowluma_pid_set_changed("nonexistent", [123])

    def test_overwrites_previous_pid_set(self) -> None:
        """连续 emit 多次, 最后一次的值生效 (覆盖, 不累加)."""
        from src.core.runtime.bot_process_manager import BotProcessManager
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        manager = BotProcessManager()
        model = SnowLumaProcessModel(qq_id="22002")
        manager._snowluma_driver._processes["22002"] = model

        try:
            manager._on_snowluma_pid_set_changed("22002", [100, 200])
            assert model.ancillary_pids == {100, 200}

            # poller 重新探测 (e.g. PID 变化), emit 新集合
            manager._on_snowluma_pid_set_changed("22002", [300, 400, 500])
            assert model.ancillary_pids == {300, 400, 500}  # 覆盖, 旧值消失
        finally:
            manager._snowluma_driver._processes.pop("22002", None)


# ==================== get_memory_usage SnowLuma 路径 ====================
class TestGetMemoryUsageSnowLuma:
    """``BotProcessManager.get_memory_usage`` SnowLuma 分支行为."""

    def test_uses_ancillary_pids_when_available(self) -> None:
        """优先用 ``ancillary_pids`` 累加 RSS - 与 SnowLuma WebUI 显示一致."""
        from src.core.runtime.bot_process_manager import BotProcessManager
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        manager = BotProcessManager()
        model = SnowLumaProcessModel(qq_id="33003", qq_pid=10001)
        model.ancillary_pids = {12345, 12346, 12347}
        manager._snowluma_driver._processes["33003"] = model

        try:
            # mock psutil.Process(pid).memory_info().rss
            mock_proc = MagicMock()
            mock_proc.memory_info.return_value = MagicMock(rss=100 * 1024 * 1024)  # 100 MB

            with patch(
                "src.core.runtime.bot_process_manager.psutil.Process",
                return_value=mock_proc,
            ):
                result = manager.get_memory_usage("33003")

            # 3 个 PID × 100 MB = 300 MB
            assert result == 300
        finally:
            manager._snowluma_driver._processes.pop("33003", None)

    def test_falls_back_to_qq_pid_walk_when_no_ancillary(self) -> None:
        """``ancillary_pids`` 空 (poller 未启 / 未首次探测) → fallback 到 ``qq_pid`` walk 树."""
        from src.core.runtime.bot_process_manager import BotProcessManager
        from src.core.runtime.napcat_driver import NapCatDriver
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        manager = BotProcessManager()
        model = SnowLumaProcessModel(qq_id="44004", qq_pid=99999)
        # ancillary_pids 留空集合 (默认)
        manager._snowluma_driver._processes["44004"] = model

        try:
            with patch.object(
                NapCatDriver, "get_memory_usage_for_pid", return_value=512
            ) as mock_walk:
                result = manager.get_memory_usage("44004")

            mock_walk.assert_called_once_with(99999)
            assert result == 512
        finally:
            manager._snowluma_driver._processes.pop("44004", None)

    def test_returns_zero_when_no_pid_at_all(self) -> None:
        """``ancillary_pids`` 空且 ``qq_pid <= 0`` (Bot 注册但还没拿到 PID) → 返 0."""
        from src.core.runtime.bot_process_manager import BotProcessManager
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        manager = BotProcessManager()
        model = SnowLumaProcessModel(qq_id="55005", qq_pid=0)
        manager._snowluma_driver._processes["55005"] = model

        try:
            assert manager.get_memory_usage("55005") == 0
        finally:
            manager._snowluma_driver._processes.pop("55005", None)

    def test_hot_mode_no_qq_process_uses_ancillary(self) -> None:
        """**关键 bug 复现**: HOT 模式 ``qq_process is None`` 但 ``ancillary_pids`` 非空 →
        旧版返 0, 现在能拿到真实内存.
        """
        from src.core.runtime.bot_process_manager import BotProcessManager
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        manager = BotProcessManager()
        # HOT 模式典型 model: qq_process=None, qq_pid=用户 attach 的 PID
        model = SnowLumaProcessModel(qq_id="66006", qq_process=None, qq_pid=88888)
        # poller 已探测到 hooked PIDs (与 attach_pid 重合或扩展)
        model.ancillary_pids = {88888, 88889}
        manager._snowluma_driver._processes["66006"] = model

        try:
            mock_proc = MagicMock()
            mock_proc.memory_info.return_value = MagicMock(rss=200 * 1024 * 1024)  # 200 MB

            with patch(
                "src.core.runtime.bot_process_manager.psutil.Process",
                return_value=mock_proc,
            ):
                result = manager.get_memory_usage("66006")

            # 2 个 PID × 200 MB = 400 MB (旧版会返 0)
            assert result == 400
        finally:
            manager._snowluma_driver._processes.pop("66006", None)

    def test_psutil_exception_silently_ignored(self) -> None:
        """``ancillary_pids`` 中某 PID psutil 失败 (进程已退出 / 权限) → 该 PID 跳过, 其他正常累加."""
        from src.core.runtime.bot_process_manager import BotProcessManager
        from src.core.runtime.snowluma_driver import SnowLumaProcessModel

        manager = BotProcessManager()
        model = SnowLumaProcessModel(qq_id="77007")
        model.ancillary_pids = {1, 2, 3}
        manager._snowluma_driver._processes["77007"] = model

        try:
            call_count = {"n": 0}

            def _fake_psutil_process(pid):
                call_count["n"] += 1
                if pid == 2:
                    raise RuntimeError("simulated NoSuchProcess")
                m = MagicMock()
                m.memory_info.return_value = MagicMock(rss=50 * 1024 * 1024)  # 50 MB
                return m

            with patch(
                "src.core.runtime.bot_process_manager.psutil.Process",
                side_effect=_fake_psutil_process,
            ):
                result = manager.get_memory_usage("77007")

            # PID 1 + PID 3 各 50 MB, PID 2 跳过 = 100 MB
            assert result == 100
            assert call_count["n"] == 3  # 3 个 PID 都被尝试
        finally:
            manager._snowluma_driver._processes.pop("77007", None)
