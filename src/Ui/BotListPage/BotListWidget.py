# -*- coding: utf-8 -*-

"""
机器人列表
"""
# 标准库导入
from abc import ABC
from typing import TYPE_CHECKING, Self, Optional

# 第三方库导入
from creart import add_creator, exists_module
from creart.creator import AbstractCreator, CreateTargetInfo
from PySide6.QtWidgets import QWidget, QVBoxLayout, QStackedWidget

# 项目内模块导入
from src.Ui.StyleSheet import StyleSheet
from src.Ui.BotListPage.BotList import BotList
from src.Ui.BotListPage.BotTopCard import BotTopCard

if TYPE_CHECKING:
    # 项目内模块导入
    from src.Ui.MainWindow import MainWindow


class BotListWidget(QWidget):

    def __init__(self) -> None:
        super().__init__()
        self.view: Optional[QStackedWidget] = None
        self.topCard: Optional[BotTopCard] = None
        self.botList: Optional[BotList] = None
        self.vBoxLayout: Optional[QVBoxLayout] = None

    def initialize(self, parent: "MainWindow") -> Self:
        """
        初始化
        """
        self.vBoxLayout = QVBoxLayout(self)

        self.topCard = BotTopCard(self)
        self.view = QStackedWidget(self)
        self.botList = BotList(self.view)

        # 设置 QWidget
        self.setParent(parent),
        self.setObjectName("BotListPage")
        self.view.setObjectName("BotListStackedWidget")
        self.view.addWidget(self.botList)
        self.view.setCurrentWidget(self.botList)

        # 调用方法
        self._setLayout()

        # 应用样式表
        StyleSheet.BOT_LIST_WIDGET.apply(self)

        return self

    def _setLayout(self) -> None:
        """
        ## 对内部进行布局
        """
        self.vBoxLayout.addWidget(self.topCard)
        self.vBoxLayout.addWidget(self.view)
        self.vBoxLayout.setContentsMargins(24, 20, 24, 10)
        self.setLayout(self.vBoxLayout)

    def runAllBot(self) -> None:
        """
        ## 运行所有机器人
        """
        # 项目内模块导入
        from src.Ui.BotListPage import BotListWidget
        from src.Ui.BotListPage.BotWidget import BotWidget

        for card in self.botList.botCardList:

            if card.botWidget is None:
                card.botWidget = BotWidget(card.config)
                self.view.addWidget(card.botWidget)

            card.botWidget.runButtonSlot()

    def stopAllBot(self):
        """
        ## 停止所有 card
        """
        for card in self.botList.botCardList:
            if not card.botWidget:
                continue
            if card.botWidget.isRun:
                card.botWidget.stopButton.click()

    def getBotIsRun(self):
        """
        ## 获取是否有 card 正在运行
        """
        for card in self.botList.botCardList:
            if not card.botWidget:
                # 如果没有创建则表示没有运行
                continue
            if card.botWidget.isRun:
                return True


class BotListWidgetClassCreator(AbstractCreator, ABC):
    # 定义类方法targets，该方法返回一个元组，元组中包含了一个CreateTargetInfo对象，
    # 该对象描述了创建目标的相关信息，包括应用程序名称和类名。
    targets = (CreateTargetInfo("src.Ui.BotListPage.BotListWidget", "BotListWidget"),)

    # 静态方法available()，用于检查模块"BotListWidget"是否存在，返回值为布尔型。
    @staticmethod
    def available() -> bool:
        return exists_module("src.Ui.BotListPage.BotListWidget")

    # 静态方法create()，用于创建BotListWidget类的实例，返回值为BotListWidget对象。
    @staticmethod
    def create(create_type: [BotListWidget]) -> BotListWidget:
        return BotListWidget()


add_creator(BotListWidgetClassCreator)
