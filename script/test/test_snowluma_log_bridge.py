# -*- coding: utf-8 -*-
"""SnowLuma 日志按钮修复 (2026-05-11): ``SnowLumaDaemonProcessLog`` + manager 集成测试.

历史背景: 用户反馈 SnowLuma Bot 卡片点【日志】按钮**没有任何输出**.
- COLD 模式: ``QQ.exe`` 用 ``ForwardedChannels``, stdout 不进 pipe → 旧 ``NapCatQQProcessLog``
  ``readyReadStandardOutput`` 永远拿不到数据.
- HOT 模式: ``primary_process is None``, 旧 manager 干脆不挂 log → 用户看到 "未找到对应的日志信息".

修复:
- ``SnowLumaDaemon`` 加 ``readyReadStandardOutput`` 槽 + ``_node_log_storage`` deque +
  ``node_log_output_signal``, 读 node.exe (业务日志真正源头) stdout 缓存到全局.
- ``SnowLumaDaemonProcessLog`` (manager 模块) 桥接 daemon 信号 + 暴露
  ``output_log_signal`` / ``get_log_content`` 让 ``BotLogPage`` 复用 NapCat 路径.
- ``ManagerNapCatQQLog.create_snowluma_log(config, daemon)`` 注册到 ``napcat_log_dict[qq_id]``.

参见: ``src/core/runtime/snowluma_daemon.py`` ``_on_node_stdout_ready`` /
``get_node_log_content`` / ``node_log_output_signal``;
``src/core/runtime/bot_process_manager.py`` ``SnowLumaDaemonProcessLog`` /
``ManagerNapCatQQLog.create_snowluma_log``.
"""
from __future__ import annotations

import os

import pytest
from PySide6.QtCore import QObject, Signal
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


# ==================== Fake daemon (避免真起 node.exe) ====================
class _FakeDaemon(QObject):
    """``SnowLumaDaemon`` 测试替身, 仅暴露 ``SnowLumaDaemonProcessLog`` 用到的 API."""

    node_log_output_signal = Signal(str)

    def __init__(self) -> None:
        super().__init__()
        self._content = ""

    def append(self, text: str) -> None:
        """模拟 daemon 收到 node.exe stdout: 追加到内部 + emit 信号."""
        self._content += text
        self.node_log_output_signal.emit(text)

    def get_node_log_content(self) -> str:
        return self._content


# ==================== SnowLumaDaemonProcessLog ====================
class TestSnowLumaDaemonProcessLog:
    """日志桥接类与 ``NapCatQQProcessLog`` API 对齐."""

    def test_get_log_content_proxies_daemon(self) -> None:
        """``get_log_content`` 应直接返回 daemon 的累积输出."""
        from src.core.runtime.bot_process_manager import SnowLumaDaemonProcessLog

        daemon = _FakeDaemon()
        daemon.append("[INFO] startup\n")
        daemon.append("[DEBUG] ready\n")

        bridge = SnowLumaDaemonProcessLog(daemon)
        assert bridge.get_log_content() == "[INFO] startup\n[DEBUG] ready\n"

    def test_output_log_signal_forwards_daemon_signal(self) -> None:
        """daemon 的 ``node_log_output_signal`` 应通过 bridge 的 ``output_log_signal`` 转发出去."""
        from src.core.runtime.bot_process_manager import SnowLumaDaemonProcessLog

        daemon = _FakeDaemon()
        bridge = SnowLumaDaemonProcessLog(daemon)

        received: list[str] = []
        bridge.output_log_signal.connect(received.append)

        daemon.append("first chunk\n")
        daemon.append("second chunk\n")

        assert received == ["first chunk\n", "second chunk\n"]

    def test_clear_is_noop(self) -> None:
        """``clear()`` 是 no-op (与 NapCatQQProcessLog API 对齐, 但不清 daemon 共享缓冲)."""
        from src.core.runtime.bot_process_manager import SnowLumaDaemonProcessLog

        daemon = _FakeDaemon()
        daemon.append("preserved")
        bridge = SnowLumaDaemonProcessLog(daemon)
        bridge.clear()
        assert bridge.get_log_content() == "preserved"

    def test_get_log_content_swallows_daemon_exception(self) -> None:
        """daemon 在 shutdown 期可能抛错; bridge ``get_log_content`` 静默返回空串."""
        from src.core.runtime.bot_process_manager import SnowLumaDaemonProcessLog

        class _BrokenDaemon(QObject):
            node_log_output_signal = Signal(str)

            def get_node_log_content(self) -> str:
                raise RuntimeError("daemon shutdown")

        bridge = SnowLumaDaemonProcessLog(_BrokenDaemon())
        assert bridge.get_log_content() == ""


# ==================== ManagerNapCatQQLog 集成 ====================
class TestManagerSnowLumaIntegration:
    """``ManagerNapCatQQLog.create_snowluma_log`` 注册 + ``get_log`` 取回."""

    def test_create_snowluma_log_registers_bridge(self) -> None:
        """``create_snowluma_log(config, daemon)`` 后 ``get_log(qq_id)`` 返回 bridge."""
        from unittest.mock import MagicMock

        from src.core.runtime.bot_process_manager import (
            ManagerNapCatQQLog,
            SnowLumaDaemonProcessLog,
        )

        manager = ManagerNapCatQQLog()
        daemon = _FakeDaemon()

        config = MagicMock()
        config.bot.QQID = 12345

        manager.create_snowluma_log(config, daemon)
        log = manager.get_log("12345")
        assert isinstance(log, SnowLumaDaemonProcessLog)

    def test_log_returns_daemon_content(self) -> None:
        """通过 manager 拿到的 log, ``get_log_content`` 返回 daemon 的输出 (端到端验证)."""
        from unittest.mock import MagicMock

        from src.core.runtime.bot_process_manager import ManagerNapCatQQLog

        manager = ManagerNapCatQQLog()
        daemon = _FakeDaemon()
        daemon.append("daemon log line 1\n")

        config = MagicMock()
        config.bot.QQID = 99999

        manager.create_snowluma_log(config, daemon)
        log = manager.get_log("99999")
        assert log is not None
        assert "daemon log line 1" in log.get_log_content()

        # 后续追加也能取到
        daemon.append("line 2\n")
        assert "line 2" in log.get_log_content()

    def test_remove_log_cleans_bridge(self) -> None:
        """``remove_log`` 后 ``get_log`` 返回 None."""
        from unittest.mock import MagicMock

        from src.core.runtime.bot_process_manager import ManagerNapCatQQLog

        manager = ManagerNapCatQQLog()
        daemon = _FakeDaemon()

        config = MagicMock()
        config.bot.QQID = 77777

        manager.create_snowluma_log(config, daemon)
        assert manager.get_log("77777") is not None

        manager.remove_log("77777")
        assert manager.get_log("77777") is None

    def test_create_snowluma_log_replaces_existing(self) -> None:
        """同 qq_id 多次调 ``create_snowluma_log`` 应替换 (不泄漏旧 bridge)."""
        from unittest.mock import MagicMock

        from src.core.runtime.bot_process_manager import ManagerNapCatQQLog

        manager = ManagerNapCatQQLog()
        daemon = _FakeDaemon()

        config = MagicMock()
        config.bot.QQID = 33333

        manager.create_snowluma_log(config, daemon)
        first_log = manager.get_log("33333")

        manager.create_snowluma_log(config, daemon)
        second_log = manager.get_log("33333")

        assert first_log is not second_log  # 替换为新实例
