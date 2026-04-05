# -*- coding: utf-8 -*-

# 标准库导入
import importlib.util
import os
import sys
from pathlib import Path
from types import ModuleType
from typing import Any, cast

# 第三方库导入
import pytest
from PySide6.QtCore import Signal
from PySide6.QtWidgets import QApplication, QWidget

# 项目内模块导入
sys.modules.setdefault("qrcode", ModuleType("qrcode"))


def load_bot_list_module():
    """按文件路径加载目标模块，避免触发页面包的全量导入。"""
    project_root = Path(__file__).resolve().parents[2]
    module_name = "src.ui.page.bot_page.sub_page.bot_list"

    page_package = ModuleType("src.ui.page")
    page_package.__path__ = [str(project_root / "src" / "ui" / "page")]
    sys.modules["src.ui.page"] = page_package

    bot_page_package = ModuleType("src.ui.page.bot_page")
    bot_page_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page")]
    sys.modules["src.ui.page.bot_page"] = bot_page_package

    sub_page_package = ModuleType("src.ui.page.bot_page.sub_page")
    sub_page_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page" / "sub_page")]
    sys.modules["src.ui.page.bot_page.sub_page"] = sub_page_package

    widget_package = ModuleType("src.ui.page.bot_page.widget")
    widget_package.__path__ = [str(project_root / "src" / "ui" / "page" / "bot_page" / "widget")]
    sys.modules["src.ui.page.bot_page.widget"] = widget_package

    card_module = ModuleType("src.ui.page.bot_page.widget.card")

    class ImportBotCard(QWidget):
        remove_signal = Signal(str)

        def __init__(self, config, parent: QWidget | None = None) -> None:
            super().__init__(parent)
            self._config = config

        def update_info_card(self) -> None:
            return None

    setattr(card_module, "BotCard", ImportBotCard)
    sys.modules["src.ui.page.bot_page.widget.card"] = card_module

    sys.modules.pop(module_name, None)
    spec = importlib.util.spec_from_file_location(
        module_name,
        project_root / "src" / "ui" / "page" / "bot_page" / "sub_page" / "bot_list.py",
    )
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


bot_list_module = load_bot_list_module()


def ensure_qapp() -> QApplication:
    """创建或复用测试用 QApplication。"""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return cast(QApplication, app)


class DummyBotCard(QWidget):
    """用于替代真实 BotCard 的轻量测试控件。"""

    remove_signal = Signal(str)

    def __init__(self, config, parent: QWidget | None = None, invalid: bool = False) -> None:
        super().__init__(parent)
        self._config = config
        self._invalid = invalid
        self.info_updated = False
        self.delete_later_called = False

    def update_info_card(self) -> None:
        self.info_updated = True

    def parent(self):  # type: ignore[override]
        if self._invalid:
            raise RuntimeError("Internal C++ object (BotCard) already deleted.")
        return super().parent()

    def setParent(self, parent) -> None:  # type: ignore[override]
        if self._invalid:
            raise RuntimeError("Internal C++ object (BotCard) already deleted.")
        super().setParent(parent)

    def deleteLater(self) -> None:  # type: ignore[override]
        if self._invalid:
            raise RuntimeError("Internal C++ object (BotCard) already deleted.")
        self.delete_later_called = True
        super().deleteLater()


class FakeFlowLayout:
    """最小可用布局替身，仅记录移除操作。"""

    def __init__(self) -> None:
        self.removed_widgets: list[QWidget] = []

    def removeWidget(self, widget: QWidget) -> None:
        self.removed_widgets.append(widget)


def test_update_bot_list_rebuild_does_not_keep_stale_card_references(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """重复刷新列表后，内部卡片引用应保持与配置数量一致。"""
    ensure_qapp()
    configs = [config_factory(114514, "Alpha"), config_factory(223344, "Beta")]

    monkeypatch.setattr(bot_list_module, "BotCard", DummyBotCard)
    monkeypatch.setattr(bot_list_module, "read_config", lambda: configs.copy())

    page = bot_list_module.BotListPage()
    first_cards = page._bot_card_list.copy()

    assert len(first_cards) == 2

    page.update_bot_list()

    assert len(page._bot_card_list) == 2
    assert all(cast(DummyBotCard, card).info_updated for card in page._bot_card_list)
    assert all(card not in page._bot_card_list for card in first_cards)
    assert all(cast(DummyBotCard, card).delete_later_called for card in first_cards)


def test_remove_bot_by_qqid_skips_invalid_stale_card_and_removes_live_card(
    monkeypatch: pytest.MonkeyPatch, config_factory
) -> None:
    """删除 Bot 时应跳过已失效卡片引用，并移除仍存活的目标卡片。"""
    ensure_qapp()
    config_keep = config_factory(114514, "Alpha")
    config_delete = config_factory(223344, "Beta")
    deleted_qqids: list[str] = []

    def fake_delete_config(config) -> bool:
        deleted_qqids.append(str(config.bot.QQID))
        return True

    monkeypatch.setattr(bot_list_module, "read_config", lambda: [])
    monkeypatch.setattr(bot_list_module, "delete_config", fake_delete_config)

    page = bot_list_module.BotListPage()
    page.view_layout = cast(Any, FakeFlowLayout())
    page._bot_config_list = [config_keep, config_delete]

    stale_card = DummyBotCard(config_delete, invalid=True)
    keep_card = DummyBotCard(config_keep)
    delete_card = DummyBotCard(config_delete)
    page._bot_card_list = cast(Any, [stale_card, keep_card, delete_card])

    page.remove_bot_by_qqid(str(config_delete.bot.QQID))

    assert deleted_qqids == [str(config_delete.bot.QQID)]
    assert [str(config.bot.QQID) for config in page._bot_config_list] == [str(config_keep.bot.QQID)]
    assert page._bot_card_list == [keep_card]
    assert cast(FakeFlowLayout, page.view_layout).removed_widgets == [delete_card]
    assert delete_card.delete_later_called is True
