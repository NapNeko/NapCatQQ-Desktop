# -*- coding: utf-8 -*-

import os
from pathlib import Path

from PySide6.QtWidgets import QApplication, QWidget

from src.desktop.core.logging import LogLevel, logger
from src.desktop.ui.page.setup_page.desktop_log import DesktopLog
from src.desktop.ui.page.setup_page import setup as setup_page_module


def ensure_qapp() -> QApplication:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _build_log_lines(count: int) -> str:
    return "\n".join(f"25-03-22 12:00:{index:02d} | [INFO] | line {index}" for index in range(count))


def test_setup_widget_inserts_desktop_log_between_general_and_developer(monkeypatch) -> None:
    ensure_qapp()
    monkeypatch.setattr(setup_page_module, "is_developer_mode_enabled", lambda: True)

    host = QWidget()
    widget = setup_page_module.SetupWidget().initialize(host)

    assert widget.view.count() == 4
    assert widget.view.widget(0) is widget.personalization
    assert widget.view.widget(1) is widget.general
    assert widget.view.widget(2) is widget.desktop_log
    assert widget.view.widget(3) is widget.developer

    widget.close()
    host.close()


def test_desktop_log_stream_keeps_reader_position(tmp_path: Path, monkeypatch) -> None:
    ensure_qapp()
    log_path = tmp_path / "desktop.log"
    log_path.write_text(_build_log_lines(300), encoding="utf-8")
    monkeypatch.setattr("src.ui.page.setup_page.desktop_log.logger.log_path", log_path)
    monkeypatch.setattr("src.core.logging.log_func.logger.log_path", log_path)

    page = DesktopLog()
    page.resize(560, 320)
    page.show()
    QApplication.processEvents()

    scroll_bar = page.log_view.verticalScrollBar()
    middle_value = max(0, scroll_bar.maximum() // 2)
    scroll_bar.setValue(middle_value)
    QApplication.processEvents()

    before_value = scroll_bar.value()
    logger.info("appended line")

    QApplication.processEvents()

    assert scroll_bar.value() == before_value
    assert "appended line" in page.log_view.toPlainText()
    page.close()


def test_desktop_log_formats_preview_as_terminal_style() -> None:
    raw_text = (
        "26-03-22 13:42:22 | [WARN] | [ NONE_TYPE ] | [  UI  ] | [default > <qt>:0] | "
        "Qt message: QFont::setPointSize: Point size <= 0 (-1), must be greater than 0\n"
        "26-03-22 13:42:23 | [EROR] | [ NONE_TYPE ] | [ NONE ] | [src.core.versioning.service > service.py:139] | "
        "获取 NapCatQQ Desktop 更新策略 版本信息失败: [SSL: UNEXPECTED_EOF_WHILE_READING] EOF occurred in violation of protocol (_ssl.c:1010)\n"
        "Traceback (most recent call last):\n"
    )

    formatted = "".join(text for _, text in DesktopLog._build_preview_entries(raw_text))

    assert (
        "26-03-22 13:42:22 | [WARN] | Qt message: QFont::setPointSize: Point size <= 0 (-1), must be greater than 0\n"
        in formatted
    )
    assert (
        "26-03-22 13:42:23 | [EROR] | 获取 NapCatQQ Desktop 更新策略 版本信息失败: "
        "[SSL: UNEXPECTED_EOF_WHILE_READING] EOF occurred in violation of protocol (_ssl.c:1010)\n"
        in formatted
    )
    assert "Traceback (most recent call last):\n" in formatted
    assert "[ NONE_TYPE ]" not in formatted


def test_desktop_log_level_filter_hides_non_matching_lines(tmp_path: Path, monkeypatch) -> None:
    ensure_qapp()
    log_path = tmp_path / "desktop.log"
    log_path.write_text(
        (
            "26-03-22 13:42:22 | [WARN] | [ NONE_TYPE ] | [  UI  ] | [default > <qt>:0] | warn line\n"
            "26-03-22 13:42:23 | [EROR] | [ NONE_TYPE ] | [ NONE ] | [service > service.py:139] | error line\n"
            "26-03-22 13:42:24 | [INFO] | [ NONE_TYPE ] | [ CORE ] | [main > main.py:1] | info line\n"
        ),
        encoding="utf-8",
    )
    monkeypatch.setattr("src.ui.page.setup_page.desktop_log.logger.log_path", log_path)

    page = DesktopLog()
    page.show()
    QApplication.processEvents()

    page.level_filter_combo.setCurrentIndex(page.level_filter_combo.findData(LogLevel.EROR.name))
    QApplication.processEvents()

    preview_text = page.log_view.toPlainText()
    assert "error line" in preview_text
    assert "warn line" not in preview_text
    assert "info line" not in preview_text
    page.close()


def test_desktop_log_level_filter_combo_updates_selection_via_action_trigger(tmp_path: Path, monkeypatch) -> None:
    ensure_qapp()
    log_path = tmp_path / "desktop.log"
    log_path.write_text(
        "26-03-22 13:42:23 | [EROR] | [ NONE_TYPE ] | [ NONE ] | [service > service.py:139] | error line\n",
        encoding="utf-8",
    )
    monkeypatch.setattr("src.ui.page.setup_page.desktop_log.logger.log_path", log_path)

    page = DesktopLog()
    page.show()
    QApplication.processEvents()

    page.level_filter_combo._showComboMenu()
    QApplication.processEvents()
    page.level_filter_combo.dropMenu.actions()[1].trigger()
    QApplication.processEvents()

    assert page.level_filter_combo.currentData() == LogLevel.CRIT.name
    page.close()
