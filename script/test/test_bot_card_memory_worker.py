# -*- coding: utf-8 -*-
"""BotCard 内存监控异步 worker 单测 (2026-05-11 主线程卡顿修复).

历史背景: BotCard 的内存监控 ``QTimer`` 每秒在主线程直调
``BotProcessManager.get_memory_usage(qq_id)`` → ``NapCatDriver.get_memory_usage_for_pid``
→ ``psutil`` walk QQ.exe Electron 多子进程树 (5-15 个子进程, 每个一次跨进程 syscall),
累计 50-200ms 阻塞主线程, 用户实测 SnowLuma 热启动后 UI 明显卡顿.

修复: 把 walk 工作甩到 :class:`_MemoryUsageWorker` (``QThreadPool`` 后台), 完成后通过
``finished`` 信号回主线程更新 ``text_label``. ``virtual_memory().total`` (系统 RAM,
单 session 不变) 在模块初始化时缓存一次.

参见: ``src/ui/page/bot_page/widget/card.py`` ``_MemoryUsageWorker`` /
``_total_memory_mb`` / ``slot_memory_usage_start`` / ``_schedule_memory_update`` /
``_update_memory_text``.
"""
from __future__ import annotations

import importlib.util
import os
import sys
from pathlib import Path
from types import ModuleType
from unittest.mock import MagicMock

import pytest
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


def _load_card_module():
    """按文件路径加载 card 模块, 与 test_bot_card.py 共用同款 stub 策略."""
    sys.modules.setdefault("qrcode", ModuleType("qrcode"))
    project_root = Path(__file__).resolve().parents[2]
    module_name = "src.ui.page.bot_page.widget.card"

    page_package = ModuleType("src.ui.page")
    page_package.__path__ = [str(project_root / "src" / "ui" / "page")]
    sys.modules["src.ui.page"] = page_package

    bot_page_package = ModuleType("src.ui.page.bot_page")
    bot_page_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page")]
    sys.modules["src.ui.page.bot_page"] = bot_page_package

    widget_package = ModuleType("src.ui.page.bot_page.widget")
    widget_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page" / "widget")]
    sys.modules["src.ui.page.bot_page.widget"] = widget_package

    sys.modules.pop(module_name, None)
    spec = importlib.util.spec_from_file_location(
        module_name,
        project_root / "src" / "ui" / "page" / "bot_page" / "widget" / "card.py",
    )
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


card_module = _load_card_module()


# ==================== _total_memory_mb 缓存 ====================
class TestTotalMemoryCache:
    """系统 RAM 在 session 内不变, 模块级 lazy 缓存避免每秒 psutil 调用."""

    def test_first_call_populates_cache(self, monkeypatch) -> None:
        """首次调 ``_total_memory_mb`` 应调一次 psutil 并写缓存."""
        # 重置全局缓存
        monkeypatch.setattr(card_module, "_CACHED_TOTAL_MEMORY_MB", None)

        call_count = {"n": 0}

        def _fake_virtual_memory():
            call_count["n"] += 1
            return MagicMock(total=8 * 1024 * 1024 * 1024)  # 8 GiB

        monkeypatch.setattr(card_module.psutil, "virtual_memory", _fake_virtual_memory)

        first = card_module._total_memory_mb()
        assert first == 8 * 1024  # 8192 MB
        assert call_count["n"] == 1

    def test_subsequent_calls_use_cache(self, monkeypatch) -> None:
        """二次调用不应再触发 psutil."""
        monkeypatch.setattr(card_module, "_CACHED_TOTAL_MEMORY_MB", None)
        call_count = {"n": 0}

        def _fake_virtual_memory():
            call_count["n"] += 1
            return MagicMock(total=4 * 1024 * 1024 * 1024)

        monkeypatch.setattr(card_module.psutil, "virtual_memory", _fake_virtual_memory)

        card_module._total_memory_mb()
        card_module._total_memory_mb()
        card_module._total_memory_mb()
        assert call_count["n"] == 1  # 仅首次

    def test_psutil_failure_returns_zero(self, monkeypatch) -> None:
        """psutil 抛异常 → 缓存 0, 不让 UI 崩."""
        monkeypatch.setattr(card_module, "_CACHED_TOTAL_MEMORY_MB", None)

        def _raise(*_a, **_kw):
            raise OSError("psutil broke")

        monkeypatch.setattr(card_module.psutil, "virtual_memory", _raise)
        assert card_module._total_memory_mb() == 0


# ==================== _MemoryUsageWorker 异步行为 ====================
class TestMemoryUsageWorker:
    """worker 在线程池跑 psutil walk, 完成后 emit ``finished(qq_id, mem_mb)``."""

    def test_run_emits_memory_from_bot_process_manager(self, monkeypatch) -> None:
        """worker.run → BotProcessManager.get_memory_usage → emit (qq_id, value)."""
        fake_manager = MagicMock()
        fake_manager.get_memory_usage.return_value = 256
        monkeypatch.setattr(card_module, "it", lambda _cls: fake_manager)

        worker = card_module._MemoryUsageWorker("12345")
        emitted: list[tuple[str, int]] = []
        worker.finished.connect(lambda q, m: emitted.append((q, m)))

        worker.run()

        assert emitted == [("12345", 256)]
        fake_manager.get_memory_usage.assert_called_once_with("12345")

    def test_run_swallows_exceptions_and_emits_zero(self, monkeypatch) -> None:
        """worker 边界吞所有异常: get_memory_usage raise → emit (qq_id, 0)."""
        fake_manager = MagicMock()
        fake_manager.get_memory_usage.side_effect = RuntimeError("psutil exploded")
        monkeypatch.setattr(card_module, "it", lambda _cls: fake_manager)

        worker = card_module._MemoryUsageWorker("99999")
        emitted: list[tuple[str, int]] = []
        worker.finished.connect(lambda q, m: emitted.append((q, m)))

        # 不应 raise (worker 内 try/except 吞掉)
        worker.run()
        assert emitted == [("99999", 0)]

    def test_setautodelete_true_for_short_lived(self) -> None:
        """worker 短寿, ``setAutoDelete(True)`` 让 QThreadPool 跑完自动 deleteLater
        (避免 BotCard 持引用泄漏).
        """
        worker = card_module._MemoryUsageWorker("11111")
        assert worker.autoDelete() is True
