# -*- coding: utf-8 -*-
"""``enumerate_qq_processes`` 单测 (2026-05-11 主线程卡顿修复 v2).

历史背景: 用户实测 SnowLuma 热启动点【启动 Bot】后弹模式对话框选 HOT_START → 提交
``EnumerateQQProcessesWorker`` 后 UI **完全锁死** (鼠标拖拽 / 其它按钮全不响应).
根因: ``psutil.process_iter`` 在工作线程跑 1-3s **持续占 GIL**, 主线程几乎拿不到
GIL 时间, 即便事件循环在跑也响应不了 UI 事件.

修复: 优先 Windows ToolHelp32 ctypes 路径 (整轮枚举在 C 层做, **完全不持 GIL**,
~50ms), 主候选 (1-3 个) 才用 psutil 二次 lookup. fallback 到纯 psutil 仅在非 Windows /
ctypes 失败时. 本测试覆盖快速路径与 fallback.

参见: ``src/ui/page/bot_page/widget/snowluma_start_dialog.py``
``_enumerate_qq_processes_via_toolhelp32`` / ``enumerate_qq_processes``.
"""
from __future__ import annotations

import os
import sys
from unittest.mock import patch

import pytest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


# ==================== ToolHelp32 快速路径 ====================
class TestToolhelp32FastPath:
    """Windows ctypes 快速路径行为."""

    def test_returns_list_on_windows(self) -> None:
        """Windows 环境下应返回 list (即使没 QQ.exe 也是 [] 不是 None)."""
        from src.ui.page.bot_page.widget.snowluma_start_dialog import (
            _enumerate_qq_processes_via_toolhelp32,
        )

        if os.name != "nt":
            pytest.skip("ToolHelp32 仅 Windows 实现")

        result = _enumerate_qq_processes_via_toolhelp32()
        # 返回 list (可能为空 list 或含 QQProcessInfo)
        assert isinstance(result, list)

    def test_returns_none_on_non_windows(self, monkeypatch) -> None:
        """非 Windows 环境 ``os.name != "nt"`` 应直接返回 ``None`` 让调用方 fallback."""
        from src.ui.page.bot_page.widget import snowluma_start_dialog

        # 模拟非 Windows
        monkeypatch.setattr(snowluma_start_dialog.os, "name", "posix")
        result = snowluma_start_dialog._enumerate_qq_processes_via_toolhelp32()
        assert result is None

    def test_completes_quickly(self) -> None:
        """**关键性能断言**: ToolHelp32 路径必须 <500ms (typical ~50ms),
        证明不持 GIL 长时间. 旧 psutil.process_iter 路径在 Windows 200+ 进程时 1-3s.
        """
        import time

        from src.ui.page.bot_page.widget.snowluma_start_dialog import (
            _enumerate_qq_processes_via_toolhelp32,
        )

        if os.name != "nt":
            pytest.skip("ToolHelp32 仅 Windows 实现")

        # warm up (模块/DLL load)
        _enumerate_qq_processes_via_toolhelp32()

        t0 = time.monotonic()
        _enumerate_qq_processes_via_toolhelp32()
        elapsed = time.monotonic() - t0

        assert elapsed < 0.5, (
            f"ToolHelp32 快速路径耗时 {elapsed*1000:.1f}ms 太长, "
            f"应 <500ms (典型 ~50ms); GIL 占用问题可能未解决"
        )


# ==================== enumerate_qq_processes 入口路由 ====================
class TestEnumerateRouter:
    """``enumerate_qq_processes`` 应优先调 ToolHelp32, 失败才 fallback 到 psutil."""

    def test_uses_toolhelp32_when_available(self, monkeypatch) -> None:
        """快速路径返回 list 时, 不应调用 psutil.process_iter."""
        from src.ui.page.bot_page.widget import snowluma_start_dialog

        fake_results = []  # 假装 ToolHelp32 返回空 list
        monkeypatch.setattr(
            snowluma_start_dialog,
            "_enumerate_qq_processes_via_toolhelp32",
            lambda: fake_results,
        )

        process_iter_called = {"count": 0}

        def _spy_process_iter(*_a, **_kw):
            process_iter_called["count"] += 1
            return iter([])

        monkeypatch.setattr(
            snowluma_start_dialog.psutil, "process_iter", _spy_process_iter
        )

        result = snowluma_start_dialog.enumerate_qq_processes()
        assert result is fake_results
        assert process_iter_called["count"] == 0  # 快速路径命中, 不走 psutil

    def test_falls_back_to_psutil_when_toolhelp32_returns_none(self, monkeypatch) -> None:
        """ToolHelp32 返回 None (e.g. 非 Windows / ctypes 失败), 应走 psutil fallback."""
        from src.ui.page.bot_page.widget import snowluma_start_dialog

        monkeypatch.setattr(
            snowluma_start_dialog,
            "_enumerate_qq_processes_via_toolhelp32",
            lambda: None,
        )

        process_iter_called = {"count": 0}

        def _spy_process_iter(*_a, **_kw):
            process_iter_called["count"] += 1
            return iter([])

        monkeypatch.setattr(
            snowluma_start_dialog.psutil, "process_iter", _spy_process_iter
        )

        result = snowluma_start_dialog.enumerate_qq_processes()
        assert result == []
        assert process_iter_called["count"] == 1  # fallback 路径调了 process_iter
