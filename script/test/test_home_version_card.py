# -*- coding: utf-8 -*-

from types import SimpleNamespace

from PySide6.QtWidgets import QApplication

from src.core.home import VersionSummary, home_version_refresh_bus
from src.ui.page.home_page.version_card import VersionCardsPanel


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_version_cards_panel_refreshes_when_refresh_event_emitted() -> None:
    """主页版本卡片收到刷新通知后应重新读取本地版本。"""
    ensure_qapp()
    panel = VersionCardsPanel()
    panel._version_service = SimpleNamespace(
        get_summary=lambda: VersionSummary(
            napcat_version="v9.9.9",
            qq_version="9.9.99",
            desktop_version="v1.0.0",
        )
    )

    home_version_refresh_bus.request_refresh()

    assert panel.napcat_card.version_label.text() == "v9.9.9"
    assert panel.qq_card.version_label.text() == "9.9.99"
