# -*- coding: utf-8 -*-

"""
机器人列表
"""
from abc import ABC
from typing import TYPE_CHECKING, Self

from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo

from src.Ui.BotListPage.BotTopCard import BotTopCard
from src.Ui.StyleSheet import StyleSheet

if TYPE_CHECKING:
    from src.Ui.MainWindow import MainWindow


class BotListWidget(QWidget):

    def __init__(self):
        super().__init__()
        self.view: QStackedWidget = None
        self.topCard: BotTopCard = None
        self.vBoxLayout: QVBoxLayout = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        self.vBoxLayout = QVBoxLayout(self)

        self.topCard = BotTopCard(self)
        self.view = QStackedWidget(self)

        # 设置 ScrollArea
        self.setParent(parent),
        self.setObjectName("BotListPage")

        # 调用方法
        self._setLayout()

        # 应用样式表
        StyleSheet.BOT_LIST_WIDGET.apply(self)

        return self

    def _setLayout(self):
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(15, 10, 17, 10)
        self.setLayout(self.vBoxLayout)


class BotListWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.BotListPage.BotList", "BotListWidget"),)

    # 静态方法available()，用于检查模块"BotList"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.BotListPage.BotList")

    # 静态方法create()，用于创建BotListWidget类的实例，返回值为BotListWidget对象。
    @staticmethod
    def create(create_type: [BotListWidget]) -> BotListWidget:
        return BotListWidget()


add_creator(BotListWidgetClassCreator)
