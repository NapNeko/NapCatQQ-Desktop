# -*- coding: utf-8 -*-
from __future__ import annotations

from abc import ABC
from typing import TYPE_CHECKING, Self

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtWidgets import QVBoxLayout, QWidget

from src.core.platform.runtime_args import is_developer_mode_enabled
from src.ui.common.style_sheet import PageStyleSheet
from src.ui.components.stacked_widget import TransparentStackedWidget
from .sub_page import DesktopLog, Developer, General, Personalization
from .widget import SetupTopCard

if TYPE_CHECKING:
    from src.ui.window.main_window import MainWindow


class SetupWidget(QWidget):
    """设置页面"""

    top_card: SetupTopCard
    v_box_layout: QVBoxLayout

    view: TransparentStackedWidget
    personalization: Personalization

    def __init__(self):
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化"""
        self.top_card = SetupTopCard(self)
        self.v_box_layout = QVBoxLayout()
        self._create_view()

        self.setParent(parent)
        self.setObjectName("setup_page")
        self.view.setObjectName("SetupView")

        self.v_box_layout.addWidget(self.top_card)
        self.v_box_layout.addWidget(self.view)
        self.v_box_layout.setContentsMargins(24, 20, 24, 20)
        self.setLayout(self.v_box_layout)

        PageStyleSheet.SETUP.apply(self)
        return self

    def _create_view(self) -> None:
        """创建并配置 QStackedWidget"""
        self.view = TransparentStackedWidget(self)
        self.general = General(self)
        self.personalization = Personalization(self)
        self.desktop_log = DesktopLog(self)
        self.developer = Developer(self) if is_developer_mode_enabled() else None

        self.view.addWidget(self.personalization)
        self.view.addWidget(self.general)
        self.view.addWidget(self.desktop_log)
        if self.developer is not None:
            self.view.addWidget(self.developer)

        self.top_card.pivot.addItem(
            routeKey=self.personalization.objectName(),
            text=self.tr("个性化"),
            onClick=lambda: self.view.setCurrentWidget(self.personalization),
        )
        self.top_card.pivot.addItem(
            routeKey=self.general.objectName(),
            text=self.tr("常规"),
            onClick=lambda: self.view.setCurrentWidget(self.general),
        )
        self.top_card.pivot.addItem(
            routeKey=self.desktop_log.objectName(),
            text=self.tr("日志"),
            onClick=lambda: self.view.setCurrentWidget(self.desktop_log),
        )
        if self.developer is not None:
            self.top_card.pivot.addItem(
                routeKey=self.developer.objectName(),
                text=self.tr("开发者"),
                onClick=lambda: self.view.setCurrentWidget(self.developer),
            )

        self.view.setCurrentWidget(self.personalization)
        self.top_card.pivot.setCurrentItem(self.personalization.objectName())


class SetupPageCreator(AbstractCreator, ABC):
    """设置页面创建器"""

    targets = (
        CreateTargetInfo(
            module="src.ui.page.setup_page",
            identify="SetupWidget",
            humanized_name="设置页面",
            description="NapCatQQ Desktop 设置页面",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断设置页面模块是否可用"""
        return exists_module("src.ui.page.setup_page")

    @staticmethod
    def create(create_type):
        """创建设置页面实例"""
        return create_type()


add_creator(SetupPageCreator)

__all__ = ["SetupPageCreator", "SetupWidget"]
