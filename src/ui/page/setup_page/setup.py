# -*- coding: utf-8 -*-
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self

# 第三方库导入
from creart import AbstractCreator, CreateTargetInfo, add_creator, exists_module
from PySide6.QtWidgets import QVBoxLayout, QWidget

# 项目内模块导入
from src.ui.common.style_sheet import PageStyleSheet
from src.ui.components.code_editor import CodeExibit
from src.ui.components.stacked_widget import TransparentStackedWidget
from src.ui.page.setup_page.general import General
from src.ui.page.setup_page.personalization import Personalization
from src.ui.page.setup_page.setup_top_card import SetupTopCard

if TYPE_CHECKING:
    # 项目内模块导入
    from src.ui.window.main_window import MainWindow


class SetupWidget(QWidget):
    """设置页面"""

    top_card: SetupTopCard
    v_box_layout: QVBoxLayout
    infoWidget: CodeExibit

    view: TransparentStackedWidget
    personalization: Personalization

    def __init__(self):
        super().__init__()

    def initialize(self, parent: "MainWindow") -> Self:
        """初始化"""
        # 创建控件
        self.top_card = SetupTopCard(self)
        self.v_box_layout = QVBoxLayout()
        self._create_view()

        # 调整控件
        self.setParent(parent)
        self.setObjectName("setup_page")
        self.view.setObjectName("SetupView")

        # 设置布局
        self.v_box_layout.addWidget(self.top_card)
        self.v_box_layout.addWidget(self.view)
        self.v_box_layout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.v_box_layout)

        # 应用样式表
        PageStyleSheet.SETUP.apply(self)
        return self

    def _create_view(self) -> None:
        """创建并配置 QStackedWidget"""
        # 创建控件
        self.view = TransparentStackedWidget(self)
        self.general = General(self)
        self.personalization = Personalization(self)

        # 设置控件
        self.view.addWidget(self.personalization)
        self.view.addWidget(self.general)

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

        # 连接信号并初始化当前标签页
        self.view.setCurrentWidget(self.personalization)
        self.top_card.pivot.setCurrentItem(self.personalization.objectName())


class SetupPageCreator(AbstractCreator, ABC):
    """设置页面创建器"""

    targets = (
        CreateTargetInfo(
            module="src.ui.page.setup_page.setup",
            identify="SetupWidget",
            humanized_name="设置页面",
            description="NapCatQQ Desktop 设置页面",
        ),
    )

    @staticmethod
    def available() -> bool:
        """判断设置页面模块是否可用"""
        return exists_module("src.ui.page.setup_page.setup")

    @staticmethod
    def create(create_type: type[SetupWidget]) -> SetupWidget:
        """创建设置页面实例"""
        return create_type()


add_creator(SetupPageCreator)
