# -*- coding: utf-8 -*-

from PySide6.QtWidgets import QApplication, QWidget

from src.ui.page.component_page.base import UpdateLogCard


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_update_log_card_switches_between_loading_and_content() -> None:
    """骨架屏应能在加载态和内容态之间切换。"""
    ensure_qapp()
    parent = QWidget()
    card = UpdateLogCard(parent)

    card.set_loading(True)
    assert card.content_stack.currentWidget() is card.skeleton

    card.set_log_markdown("## Release Notes\n\n- Fixed")
    assert card.content_stack.currentWidget() is card.log_edit
    assert "Release Notes" in card.log_edit.toPlainText()
