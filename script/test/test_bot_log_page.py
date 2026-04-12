# -*- coding: utf-8 -*-

import os

from PySide6.QtWidgets import QApplication

from src.desktop.ui.page.bot_page.sub_page.bot_log import BotLogPage


def ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _build_log_lines(count: int) -> str:
    return "\n".join(f"line {index}" for index in range(count))


def test_append_log_keeps_reader_position_when_not_pinned_to_bottom() -> None:
    ensure_qapp()
    page = BotLogPage()
    page.resize(520, 280)
    page.show()
    page.slot_set_log_view(_build_log_lines(300))
    QApplication.processEvents()

    scroll_bar = page.log_view.verticalScrollBar()
    middle_value = max(0, scroll_bar.maximum() // 2)
    scroll_bar.setValue(middle_value)
    QApplication.processEvents()

    before_value = scroll_bar.value()
    page.slot_insert_log_view("\nnew tail line")
    QApplication.processEvents()

    assert scroll_bar.value() == before_value
    page.close()


def test_append_log_follows_tail_when_reader_is_at_bottom() -> None:
    ensure_qapp()
    page = BotLogPage()
    page.resize(520, 280)
    page.show()
    page.slot_set_log_view(_build_log_lines(300))
    QApplication.processEvents()

    scroll_bar = page.log_view.verticalScrollBar()
    scroll_bar.setValue(scroll_bar.maximum())
    QApplication.processEvents()

    page.slot_insert_log_view("\nnew tail line")
    QApplication.processEvents()

    assert scroll_bar.value() == scroll_bar.maximum()
    page.close()
