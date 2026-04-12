# -*- coding: utf-8 -*-
from __future__ import annotations

from abc import ABC
from typing import TYPE_CHECKING, Self

from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtCore import Slot
from PySide6.QtWidgets import QVBoxLayout, QWidget

from src.core.logging import LogSource, logger
from src.core.versioning import VersionService
from src.ui.common.style_sheet import PageStyleSheet
from src.ui.components.stacked_widget import TransparentStackedWidget
from .sub_page import DesktopPage, NapCatPage, QQPage
from .widget import TopWidget

if TYPE_CHECKING:
    from src.ui.window.main_window import MainWindow


class ComponentPage(QWidget):
    """组件页面控件，负责协调安装与更新相关子页面的版本刷新。"""

    def __init__(self) -> None:
        super().__init__()
        self.v_box_layout = QVBoxLayout(self)
        self.top_card = TopWidget(self)
        self.version_service = VersionService(self)
        self.view = TransparentStackedWidget()
        self.napcat_page = NapCatPage(self)
        self.qq_page = QQPage(self)
        self.desktop_page = DesktopPage(self)

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化界面控件和布局。"""
        self.setParent(parent)
        self.setObjectName("ComponentPage")

        self._create_widgets()
        self._setup_layout()
        self._connect_signals()
        self._apply_styles()

        self.refresh_versions()
        return self

    def _create_widgets(self) -> None:
        self._create_view()

    def _create_view(self) -> None:
        self.view.setObjectName("UpdateView")

        self.view.addWidget(self.napcat_page)
        self.view.addWidget(self.qq_page)
        self.view.addWidget(self.desktop_page)

        self.top_card.pivot.addItem(
            routeKey=self.napcat_page.objectName(),
            text=self.tr("NapCat"),
            onClick=lambda: self.view.setCurrentWidget(self.napcat_page),
        )
        self.top_card.pivot.addItem(
            routeKey=self.qq_page.objectName(),
            text=self.tr("QQ"),
            onClick=lambda: self.view.setCurrentWidget(self.qq_page),
        )
        self.top_card.pivot.addItem(
            routeKey=self.desktop_page.objectName(),
            text=self.tr("Desktop"),
            onClick=lambda: self.view.setCurrentWidget(self.desktop_page),
        )

        self.view.setCurrentWidget(self.napcat_page)
        self.top_card.pivot.setCurrentItem(self.napcat_page.objectName())

    def _setup_layout(self) -> None:
        self.v_box_layout.addWidget(self.top_card)
        self.v_box_layout.addWidget(self.view)
        self.v_box_layout.setContentsMargins(24, 20, 24, 20)
        self.setLayout(self.v_box_layout)

    def _connect_signals(self) -> None:
        self.view.currentChanged.connect(self.handle_current_index_changed)
        self.top_card.update_button.clicked.connect(self.refresh_versions)

        self.version_service.remote_versions_loaded.connect(self.napcat_page.apply_remote_version_data)
        self.version_service.remote_versions_loaded.connect(self.qq_page.apply_remote_version_data)
        self.version_service.remote_versions_loaded.connect(self.desktop_page.apply_remote_version_data)
        self.version_service.remote_versions_loaded.connect(lambda: self.top_card.update_button.setEnabled(True))

        self.version_service.local_versions_loaded.connect(self.napcat_page.apply_local_version_data)
        self.version_service.local_versions_loaded.connect(self.qq_page.apply_local_version_data)
        self.version_service.local_versions_loaded.connect(self.desktop_page.apply_local_version_data)

    def _apply_styles(self) -> None:
        PageStyleSheet.UNIT.apply(self)

    @Slot()
    def handle_current_index_changed(self, index: int) -> None:
        logger.trace(f"组件页标签切换: page={self.view.widget(index).objectName()}", log_source=LogSource.UI)
        self.top_card.pivot.setCurrentItem(self.view.widget(index).objectName())

    @Slot()
    def refresh_versions(self) -> None:
        logger.info("开始执行版本更新检查", log_source=LogSource.UI)
        self.napcat_page.begin_version_refresh()
        self.qq_page.begin_version_refresh()
        self.desktop_page.begin_version_refresh()
        self.napcat_page.log_card.set_loading(True)
        self.desktop_page.log_card.set_loading(True)
        self.version_service.refresh()
        self.top_card.update_button.setEnabled(False)


class ComponentPageCreator(AbstractCreator, ABC):
    """组件页面创建器"""

    targets = (
        CreateTargetInfo(
            module="src.ui.page.component_page",
            identify="ComponentPage",
            humanized_name="组件页面",
            description="NapCatQQ Desktop 组件页面",
        ),
    )

    @staticmethod
    def available() -> bool:
        return exists_module("src.ui.page.component_page")

    @staticmethod
    def create(create_type):
        return create_type()


add_creator(ComponentPageCreator)
