# -*- coding: utf-8 -*-
from abc import ABC
from typing import TYPE_CHECKING, Self

from creart import AbstractCreator, CreateTargetInfo, it, add_creator, exists_module
from PySide6.QtCore import Qt
from qfluentwidgets import ScrollArea
from PySide6.QtWidgets import QWidget, QVBoxLayout

from src.Ui.StyleSheet import StyleSheet
from src.Ui.common.Netwrok import NapCatUpdateCard
from src.Ui.UpdatePage.UpdateTopCard import UpdateTopCard

if TYPE_CHECKING:
    from src.Ui import MainWindow


class UpdateWidget(QWidget):

    def __init__(self) -> None:
        super().__init__()
        self.ncUpdateCard = None
        self.cardView = None
        self.topCard = None
        self.vBoxLayout = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        # 设置控件
        self.setParent(parent)
        self.setObjectName("update_view")

        # 创建布局和控件
        self.vBoxLayout = QVBoxLayout(self)
        self.topCard = UpdateTopCard(self)
        self.cardView = CardView(self)
        self.ncUpdateCard = NapCatUpdateCard(self)

        # 调用方法
        self.cardView.addWidget(self.ncUpdateCard)
        self.cardView.viewLayout.addStretch(1)
        self._setLayout()

        # 应用样式表
        StyleSheet.BOT_LIST_WIDGET.apply(self)

        return self

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.cardView)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)


class CardView(ScrollArea):
    """
    ## 更新卡片视图
    """

    def __init__(self, parent):
        super().__init__(parent=parent)

        # 创建控件和布局
        self.view = QWidget(self)
        self.viewLayout = QVBoxLayout(self.view)

        # 设置控件
        self.view.setObjectName("UpdateViewWidget_CardView")
        self.setWidget(self.view)
        self.setWidgetResizable(True)

        # 设置布局
        self.viewLayout.setContentsMargins(0, 0, 10, 0)

        # 应用样式表
        StyleSheet.HOME_WIDGET.apply(self)

    def addWidget(self, widget):
        """
        ## 添加控件到 viewLayout
        """
        self.viewLayout.addWidget(widget, 0, Qt.AlignmentFlag.AlignTop)


class UpdateWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.UpdatePage", "UpdateWidget"),)

    # 静态方法available()，用于检查模块"BotListWidget"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.UpdateWidget")

    # 静态方法create()，用于创建BotListWidget类的实例，返回值为BotListWidget对象。
    @staticmethod
    def create(create_type: [UpdateWidget]) -> UpdateWidget:
        return UpdateWidget()


add_creator(UpdateWidgetClassCreator)
