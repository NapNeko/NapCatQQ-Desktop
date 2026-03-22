# -*- coding: utf-8 -*-
from __future__ import annotations

from abc import ABC
from typing import TYPE_CHECKING, Self

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

from src.ui.common.style_sheet import PageStyleSheet
from src.ui.components.background import DottedBackground
from .widget import HelloCard, NoticeCard, OccupancyPanel, VersionCardsPanel

if TYPE_CHECKING:
    from src.ui.window.main_window import MainWindow


class _HomeMainColumn(QWidget):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.hello_card = HelloCard()
        self.notice_card = NoticeCard()
        self._set_layout()

    def _set_layout(self) -> None:
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(12)
        self.v_box_layout.addWidget(self.hello_card)
        self.v_box_layout.addWidget(self.notice_card)


class _HomeSidebar(QWidget):
    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.version_cards = VersionCardsPanel()
        self.occupancy_panel = OccupancyPanel()
        self._set_layout()

    def _set_layout(self) -> None:
        self.v_box_layout = QVBoxLayout(self)
        self.v_box_layout.setContentsMargins(0, 0, 0, 0)
        self.v_box_layout.setSpacing(12)
        self.v_box_layout.addWidget(self.version_cards)
        self.v_box_layout.addWidget(self.occupancy_panel)


class HomeWidget(DottedBackground):
    def __init__(self) -> None:
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        self._create_widgets()
        self._init_widget(parent)
        self._set_layout()

        PageStyleSheet.HOME.apply(self)
        self.main_column.hello_card.attach_floating_icon(self)
        return self

    def _create_widgets(self) -> None:
        self.main_column = _HomeMainColumn(self)
        self.sidebar = _HomeSidebar(self)

    def _init_widget(self, parent: "MainWindow") -> None:
        self.setParent(parent)
        self.setObjectName("home_page")

    def _set_layout(self) -> None:
        self.h_box_layout = QHBoxLayout(self)
        self.h_box_layout.setContentsMargins(24, 48, 24, 24)
        self.h_box_layout.setSpacing(12)
        self.h_box_layout.addWidget(self.main_column, 7)
        self.h_box_layout.addWidget(self.sidebar, 5)


class HomePageCreator(AbstractCreator, ABC):
    """Home 页面创建器"""

    targets = (
        CreateTargetInfo(
            module="src.ui.page.home_page",
            identify="HomeWidget",
            humanized_name="Home 页面",
            description="NapCatQQ Desktop 主页面",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断 Home 页面模块是否可用"""
        return exists_module("src.ui.page.home_page")

    @staticmethod
    def create(create_type):
        """创建 Home 页面实例"""
        return create_type()


add_creator(HomePageCreator)

__all__ = ["HomePageCreator", "HomeWidget"]
