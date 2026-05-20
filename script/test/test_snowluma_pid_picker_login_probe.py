# -*- coding: utf-8 -*-
"""``SnowLumaPidPickerDialog`` 登录探测渲染回归测试.

复刻自上游 SnowLuma#56 思路: 在 PID picker 上直接显示每个 QQ.exe 当前登录的 uin,
用户多开 QQ 时能精准选到对应账号, 降低 cross-bot 误注入概率.

参见: ``src/ui/page/bot_page/widget/snowluma_start_dialog.py`` ``QQProcessInfo``
``_PidPickerCard`` ``SnowLumaPidPickerDialog`` ``EnumerateQQProcessesWorker``.
"""
from __future__ import annotations

import os
from dataclasses import replace
from unittest.mock import patch

import pytest
from PySide6.QtWidgets import QApplication, QWidget

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")


@pytest.fixture(autouse=True)
def _qapp() -> QApplication:
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _make_info(pid: int, *, login_uin: str = "", login_probed: bool = False):
    from src.ui.page.bot_page.widget.snowluma_start_dialog import QQProcessInfo

    return QQProcessInfo(
        pid=pid,
        create_time_iso="2026-05-20 09:00:00",
        memory_mb=512,
        login_uin=login_uin,
        login_probed=login_probed,
    )


# ==================== _PidPickerCard 渲染 ====================
class TestPidPickerCardRendering:
    def test_logged_in_card_shows_uin(self) -> None:
        from src.ui.page.bot_page.widget.snowluma_start_dialog import _PidPickerCard
        from qfluentwidgets import RadioButton

        info = _make_info(12345, login_uin="498600841", login_probed=True)
        card = _PidPickerCard(RadioButton(""), info)

        assert card.login_label is not None
        assert "498600841" in card.login_label.text()
        assert "已登录" in card.login_label.text()

    def test_probed_but_not_logged_in_shows_label(self) -> None:
        """探测过但 uin 为空 → 显示 "未登录" (而不是省略整个 label)."""
        from src.ui.page.bot_page.widget.snowluma_start_dialog import _PidPickerCard
        from qfluentwidgets import RadioButton

        info = _make_info(12345, login_uin="", login_probed=True)
        card = _PidPickerCard(RadioButton(""), info)

        assert card.login_label is not None
        assert "未登录" in card.login_label.text()

    def test_unprobed_card_omits_login_label(self) -> None:
        """没探测过 (旧测试 / 主线程构造) → 不显示登录行, 保持向后兼容."""
        from src.ui.page.bot_page.widget.snowluma_start_dialog import _PidPickerCard
        from qfluentwidgets import RadioButton

        info = _make_info(12345)  # login_probed=False
        card = _PidPickerCard(RadioButton(""), info)

        assert card.login_label is None


# ==================== SnowLumaPidPickerDialog 提示文案 ====================
class TestPidPickerDialogHint:
    def test_hint_mentions_qqid_match_when_any_logged_in(self) -> None:
        """有候选已登录 → 提示文案改成"选择与 Bot 配置 QQID 一致的进程"."""
        from src.ui.page.bot_page.widget.snowluma_start_dialog import (
            SnowLumaPidPickerDialog,
        )

        host = QWidget()
        candidates = [
            _make_info(11111, login_uin="498600841", login_probed=True),
            _make_info(22222, login_uin="", login_probed=True),
        ]
        dialog = SnowLumaPidPickerDialog(parent=host, candidates=candidates)

        assert "QQID" in dialog.hint_label.text()

    def test_hint_falls_back_when_no_candidate_logged_in(self) -> None:
        """所有候选都未登录 → 退化到旧的"启动时间最久"提示."""
        from src.ui.page.bot_page.widget.snowluma_start_dialog import (
            SnowLumaPidPickerDialog,
        )

        host = QWidget()
        candidates = [
            _make_info(11111, login_uin="", login_probed=True),
            _make_info(22222, login_uin="", login_probed=True),
        ]
        dialog = SnowLumaPidPickerDialog(parent=host, candidates=candidates)

        assert "启动时间" in dialog.hint_label.text()


# ==================== EnumerateQQProcessesWorker.run 探测注入 ====================
class TestWorkerProbesLogin:
    def test_worker_attaches_login_info_to_results(self, monkeypatch) -> None:
        """worker.run 应对每个候选调 ``probe_qq_login``, 把 uin 填进 QQProcessInfo."""
        from src.core.runtime.q_port_probe import QqPortLoginInfo
        from src.ui.page.bot_page.widget import snowluma_start_dialog
        from src.ui.page.bot_page.widget.snowluma_start_dialog import (
            EnumerateQQProcessesWorker,
        )

        raw_results = [
            _make_info(11111),
            _make_info(22222),
        ]
        monkeypatch.setattr(
            snowluma_start_dialog, "enumerate_qq_processes", lambda: raw_results
        )

        def fake_probe(pid: int) -> QqPortLoginInfo | None:
            if pid == 11111:
                return QqPortLoginInfo(
                    port=9210, uin="498600841", logged_in=True
                )
            return None

        monkeypatch.setattr(
            snowluma_start_dialog, "probe_qq_login", fake_probe
        )

        worker = EnumerateQQProcessesWorker()
        emitted: list[list] = []
        worker.finished.connect(lambda payload: emitted.append(payload))

        worker.run()

        assert len(emitted) == 1
        results = emitted[0]
        assert len(results) == 2
        # PID 11111 探测命中, 标 logged in + uin
        assert results[0].login_uin == "498600841"
        assert results[0].login_probed is True
        # PID 22222 探测返回 None, 但仍然标 probed=True (区分"未探测")
        assert results[1].login_uin == ""
        assert results[1].login_probed is True

    def test_probe_exception_does_not_break_worker(self, monkeypatch) -> None:
        """单个 PID 探测抛异常不应影响 worker 整体, 该项 login_uin 退化为空."""
        from src.ui.page.bot_page.widget import snowluma_start_dialog
        from src.ui.page.bot_page.widget.snowluma_start_dialog import (
            EnumerateQQProcessesWorker,
        )

        monkeypatch.setattr(
            snowluma_start_dialog,
            "enumerate_qq_processes",
            lambda: [_make_info(11111)],
        )

        def boom(_pid: int):
            raise RuntimeError("simulated probe failure")

        monkeypatch.setattr(snowluma_start_dialog, "probe_qq_login", boom)

        worker = EnumerateQQProcessesWorker()
        emitted: list[list] = []
        worker.finished.connect(lambda payload: emitted.append(payload))

        worker.run()

        assert len(emitted) == 1
        results = emitted[0]
        assert len(results) == 1
        assert results[0].login_uin == ""
        assert results[0].login_probed is True
